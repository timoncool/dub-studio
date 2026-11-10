//! Авто-распределение голосов-слотов по спикерам (#114). Юзер задаёт два списка имён голосов из voices/
//! (мужские/женские слоты). После анализа система: (1) определяет пол каждого спикера по медиане F0 его
//! реплик на чистом вокале; (2) ранжирует спикеров каждого пола по суммарной длительности речи;
//! (3) раскладывает их по слотам списка по порядку (топ-1 -> слот 1, …), спикеров сверх числа слотов —
//! ПО КРУГУ по списку; (4) записывает per-speaker голоса тем же механизмом, что ручное назначение
//! (voice.mode="voice" + позиционный CSV voice.name по лексикографически отсортированным id спикеров,
//! как читает render.rs). Пустой список пола -> спикеры этого пола остаются на клонировании (слот "-").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dub_core::Project;
use serde_json::{json, Value};

use crate::f0;

pub use crate::f0::Gender;

/// Сентинел клона в CSV voice.name (#114): позиция "-" => render клонирует голос спикера, а не берёт пак.
/// Пустая позиция сохраняет СТАРОЕ поведение render (fallback на первое имя) — сентинел не ломает паритет.
pub const CLONE_SLOT: &str = "-";

/// Сколько секунд реплик спикера собирать для замера F0 (медиана устойчива и на ~30с).
const F0_SAMPLE_SECS: f64 = 30.0;
/// Границы валидного F0 (Гц) — вне диапазона считаем замер шумом (нет пола).
const F0_MIN: f64 = 60.0;
const F0_MAX: f64 = 400.0;

/// Итог по одному спикеру (для ответа эндпоинта и записи в проект).
#[derive(Clone, Debug)]
pub struct SpeakerAssign {
    pub speaker: String,
    pub voice: Option<String>, // None => клонирование (нет слотов его пола)
    pub gender: Option<Gender>,
    pub f0: Option<f64>,
    pub total_dur: f64,
}

/// Слоты голосов: мужские / женские списки имён из voices/ (1..N каждый, может быть пуст).
pub struct Slots {
    pub male: Vec<String>,
    pub female: Vec<String>,
}

impl Slots {
    /// Из тела {male:[имена], female:[имена]}: пробелы по краям срезаются, пустые имена отбрасываются.
    pub fn from_json(body: &Value) -> Slots {
        let names_of = |k: &str| -> Vec<String> {
            body.get(k)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default()
        };
        Slots { male: names_of("male"), female: names_of("female") }
    }

    /// Имена слотов, которых нет среди голосов voices/ (`available`).
    pub fn missing_in(&self, available: &[String]) -> Vec<String> {
        self.male.iter().chain(self.female.iter()).filter(|n| !available.contains(n)).cloned().collect()
    }
}

/// Вокал анализа для замера F0: чистый vocals16_clean.wav, иначе сырой 16k vocals16.wav; None — анализа не было.
pub fn vocals_for(dir: &Path) -> Option<PathBuf> {
    [dir.join("vocals16_clean.wav"), dir.join("vocals16.wav")].into_iter().find(|p| p.is_file())
}

/// Итог раздачи для ответа: {speaker: {voice, gender, f0, seconds}}.
pub fn mapping(assigns: &[SpeakerAssign]) -> Value {
    assigns
        .iter()
        .map(|a| {
            let gender = match a.gender {
                Some(Gender::Male) => Value::from("male"),
                Some(Gender::Female) => Value::from("female"),
                None => Value::Null,
            };
            (
                a.speaker.clone(),
                json!({
                    "voice": a.voice.clone().map(Value::from).unwrap_or(Value::Null),
                    "gender": gender,
                    "f0": a.f0,
                    "seconds": a.total_dur,
                }),
            )
        })
        .collect::<serde_json::Map<String, Value>>()
        .into()
}

/// Замерить медиану F0 спикера: собрать до F0_SAMPLE_SECS его реплик из вокала (вырезки по сегментам),
/// склеить сэмплы, вернуть медиану. wd — каталог для временных вырезок. Fail-safe: сбой обрезки/чтения
/// пропускается (спикер без валидного F0 -> без пола). Возвращает None, если voiced-фреймов нет.
fn speaker_f0(
    speaker: &str,
    segs: &[(f64, f64)],
    vocals: &Path,
    wd: &Path,
) -> Option<f64> {
    let mut samples: Vec<f32> = Vec::new();
    let mut sr: u32 = 0;
    let mut acc = 0.0f64;
    let tmp = wd.join(format!("f0_spk_{}.wav", sanitize(speaker)));
    for &(start, end) in segs {
        if acc >= F0_SAMPLE_SECS {
            break;
        }
        if end <= start {
            continue;
        }
        let take = (end - start).min(F0_SAMPLE_SECS - acc);
        let s_end = start + take;
        // 16k mono вырез реплики (media::trim уже даёт mono@sr); читаем сэмплы через wavio.
        if crate::media::trim(vocals, &tmp, start, s_end.max(start + 0.05), 16_000).is_err() {
            continue;
        }
        if let Ok((mut mono, s)) = crate::wavio::read_mono_f32(&tmp) {
            if sr == 0 {
                sr = s;
            }
            if s == sr {
                samples.append(&mut mono);
                acc += take;
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    if samples.is_empty() || sr == 0 {
        return None;
    }
    match f0::median_f0(&samples, sr) {
        Some(v) if (F0_MIN..=F0_MAX).contains(&v) => Some(v),
        _ => None,
    }
}

/// Разложить ранжированных спикеров одного пола по списку слотов ПО КРУГУ (топ-i -> slot[i % len]).
/// Пустой список -> все None (клонирование). ranked — id спикеров по убыванию суммарной длительности.
fn distribute(ranked: &[String], slots: &[String]) -> BTreeMap<String, Option<String>> {
    let mut out = BTreeMap::new();
    for (i, spk) in ranked.iter().enumerate() {
        let v = if slots.is_empty() {
            None
        } else {
            Some(slots[i % slots.len()].clone())
        };
        out.insert(spk.clone(), v);
    }
    out
}

/// Полное авто-распределение: plan по проекту, затем apply к нему же. Возвращает список назначений (для
/// ответа эндпоинта).
pub fn assign(proj: &mut Project, vocals: &Path, wd: &Path, slots: &Slots) -> Vec<SpeakerAssign> {
    let infos = plan(proj, vocals, wd, slots);
    apply(proj, &infos);
    infos
}

/// Спикер сегмента так, как его различает распределение: пустой speaker -> "0" (как render).
fn speaker_key(s: &dub_core::Segment) -> String {
    s.speaker.clone().unwrap_or_else(|| "0".to_string())
}

/// Спикеры проекта, между которыми plan раскладывает голоса.
pub fn speakers(proj: &Project) -> std::collections::BTreeSet<String> {
    proj.segments.iter().map(speaker_key).collect()
}

/// Распределение без записи в проект. Читает сегменты (id спикера + тайминги), меряет F0 на vocals,
/// ранжирует по длительности, раскладывает по слотам.
pub fn plan(proj: &Project, vocals: &Path, wd: &Path, slots: &Slots) -> Vec<SpeakerAssign> {
    let mut by_spk: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for s in &proj.segments {
        by_spk.entry(speaker_key(s)).or_default().push((s.start, s.end));
    }
    if by_spk.is_empty() {
        return Vec::new();
    }

    // Замер F0/пола + суммарная длительность на спикера. Более длинные (по сумме речи) реплики — раньше
    // в замер (устойчивее F0). Реплики каждого спикера сортируем по длительности убыв.
    let mut infos: Vec<SpeakerAssign> = Vec::new();
    for (spk, segs) in &by_spk {
        let total: f64 = segs.iter().map(|(a, b)| (b - a).max(0.0)).sum();
        let mut sorted = segs.clone();
        sorted.sort_by(|a, b| (b.1 - b.0).partial_cmp(&(a.1 - a.0)).unwrap_or(std::cmp::Ordering::Equal));
        let f0 = speaker_f0(spk, &sorted, vocals, wd);
        let gender = f0.map(f0::gender_of);
        infos.push(SpeakerAssign {
            speaker: spk.clone(),
            voice: None,
            gender,
            f0,
            total_dur: total,
        });
    }

    // Ранжируем каждый пол по суммарной длительности речи (убыв.). Спикер без валидного F0 -> в БОЛЬШИЙ
    // список слотов (по кругу); если оба пусты — клон (остаётся None).
    let bigger_is_male = slots.male.len() >= slots.female.len();
    let mut male_ranked: Vec<String> = Vec::new();
    let mut female_ranked: Vec<String> = Vec::new();
    // сортировка по длительности убыв. — стабильна по вставке при равенстве (id -> детерминизм ниже).
    let mut order: Vec<&SpeakerAssign> = infos.iter().collect();
    order.sort_by(|a, b| {
        b.total_dur
            .partial_cmp(&a.total_dur)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.speaker.cmp(&b.speaker))
    });
    for info in &order {
        let g = match info.gender {
            Some(Gender::Male) => Gender::Male,
            Some(Gender::Female) => Gender::Female,
            // нет пола -> в больший список слотов (там больше шансов на голос, а не клон).
            None => {
                if bigger_is_male {
                    Gender::Male
                } else {
                    Gender::Female
                }
            }
        };
        match g {
            Gender::Male => male_ranked.push(info.speaker.clone()),
            Gender::Female => female_ranked.push(info.speaker.clone()),
        }
    }

    let male_map = distribute(&male_ranked, &slots.male);
    let female_map = distribute(&female_ranked, &slots.female);
    let voice_of = |spk: &str| -> Option<String> {
        male_map
            .get(spk)
            .cloned()
            .flatten()
            .or_else(|| female_map.get(spk).cloned().flatten())
    };
    for info in &mut infos {
        info.voice = voice_of(&info.speaker);
    }
    infos
}

/// Записать назначения plan в проект тем же механизмом, что ручное назначение: voice.mode="voice" +
/// позиционный CSV voice.name по лексикографически отсортированным id спикеров (render.rs так читает), и
/// пометить сегменты dirty. Спикер без голоса (клон) -> сентинел CLONE_SLOT в его позиции. Ни одного голоса
/// из пака -> mode="clone" без пометки dirty. Пустой план проект не трогает.
pub fn apply(proj: &mut Project, infos: &[SpeakerAssign]) {
    if infos.is_empty() {
        return;
    }
    if !infos.iter().any(|i| i.voice.is_some()) {
        proj.audio.voice.mode = "clone".to_string();
        proj.audio.voice.name = None;
        return;
    }
    let assign_by_spk: BTreeMap<&str, Option<&str>> =
        infos.iter().map(|i| (i.speaker.as_str(), i.voice.as_deref())).collect();
    let csv: Vec<&str> = assign_by_spk.values().map(|v| v.unwrap_or(CLONE_SLOT)).collect();
    proj.audio.voice.mode = "voice".to_string();
    proj.audio.voice.name = Some(csv.join(","));
    // Смена голоса -> ре-синтез: метим все сегменты dirty (как op_recast/ручная смена голоса).
    for seg in &mut proj.segments {
        seg.dirty = true;
    }
}

/// Санитизация имени спикера для имени временного файла.
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribute_round_robin_over_slots() {
        // 3 мужских спикера на 2 слота -> по кругу: A->m1, B->m2, C->m1.
        let ranked = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let slots = vec!["m1".to_string(), "m2".to_string()];
        let m = distribute(&ranked, &slots);
        assert_eq!(m["A"], Some("m1".to_string()));
        assert_eq!(m["B"], Some("m2".to_string()));
        assert_eq!(m["C"], Some("m1".to_string())); // по кругу
    }

    #[test]
    fn distribute_empty_slots_all_clone() {
        let ranked = vec!["A".to_string(), "B".to_string()];
        let m = distribute(&ranked, &[]);
        assert_eq!(m["A"], None);
        assert_eq!(m["B"], None);
    }

    fn proj_with_speakers(spk_durs: &[(&str, f64)]) -> Project {
        let mut p = Project::default();
        for (i, (spk, dur)) in spk_durs.iter().enumerate() {
            p.segments.push(dub_core::Segment {
                id: format!("s{i}"),
                start: i as f64 * 100.0,
                end: i as f64 * 100.0 + dur,
                speaker: Some(spk.to_string()),
                src_text: "x".into(),
                ..Default::default()
            });
        }
        p
    }

    #[test]
    fn empty_female_list_leaves_female_on_clone() {
        // 1 муж + 1 жен спикер; женский список пуст -> женский спикер на клоне (сентинел "-"),
        // мужской на паке. F0 не мерим (нет вокала) — вручную проставим пол через прямой вызов distribute.
        let male = distribute(&["m".to_string()], &["male1".to_string()]);
        let female = distribute(&["f".to_string()], &[]);
        assert_eq!(male["m"], Some("male1".to_string()));
        assert_eq!(female["f"], None); // клонирование
    }

    #[test]
    fn assign_no_slots_stays_clone_mode() {
        // Оба списка пусты + нет вокала -> mode="clone", CSV пуст, назначений из пака нет.
        let mut p = proj_with_speakers(&[("0", 10.0), ("1", 8.0)]);
        let wd = std::env::temp_dir();
        let missing = wd.join("no_such_vocals_f0.wav");
        let slots = Slots { male: vec![], female: vec![] };
        let out = assign(&mut p, &missing, &wd, &slots);
        assert_eq!(p.audio.voice.mode, "clone");
        assert!(p.audio.voice.name.is_none());
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|a| a.voice.is_none()));
    }

    #[test]
    fn assign_writes_positional_csv_by_sorted_speaker() {
        // Вокала нет -> F0=None у всех -> все в БОЛЬШИЙ список (male, т.к. len>=). CSV позиции по
        // лексикографически отсортированным id спикеров ("0","1","2"). male=[A,B] -> ранжирование по
        // длительности, но CSV — по id. Проверяем: CSV длиной = числу спикеров, режим "voice".
        let mut p = proj_with_speakers(&[("0", 30.0), ("1", 20.0), ("2", 10.0)]);
        let wd = std::env::temp_dir();
        let missing = wd.join("no_such_vocals_f0b.wav");
        let slots = Slots { male: vec!["A".into(), "B".into()], female: vec![] };
        let out = assign(&mut p, &missing, &wd, &slots);
        assert_eq!(p.audio.voice.mode, "voice");
        let csv = p.audio.voice.name.unwrap();
        let parts: Vec<&str> = csv.split(',').collect();
        assert_eq!(parts.len(), 3); // по одной позиции на спикера (сорт. id: 0,1,2)
        // все спикеры без F0 ушли в male-список по кругу: топ-длит "0"->A, "1"->B, "2"->A (round-robin)
        let by: std::collections::HashMap<_, _> =
            out.iter().map(|a| (a.speaker.clone(), a.voice.clone())).collect();
        assert_eq!(by["0"], Some("A".to_string())); // самый длинный -> slot 0
        assert_eq!(by["1"], Some("B".to_string()));
        assert_eq!(by["2"], Some("A".to_string())); // по кругу
        // CSV позиция i = голос i-го отсортированного спикера: [0]=A, [1]=B, [2]=A
        assert_eq!(parts, vec!["A", "B", "A"]);
        // сегменты помечены dirty (ре-синтез).
        assert!(p.segments.iter().all(|s| s.dirty));
    }
}
