//! Автокастинг облачных голосов по полу спикера + языку дубляжа. Справочник голосов (пол/возраст/русский)
//! собран роем из открытых доков провайдеров (voice_db.json, 271 голос / 12 моделей) и вшит в бинарь —
//! программа кастит сама, без обращения к сети. Мужскому спикеру — мужской голос, женскому — женский,
//! разным спикерам разные; для русского дубляжа берём только голоса, реально тянущие русский (voxtral/
//! deepgram/kokoro и пр. на русском не годятся — из справочника ru=false, отфильтруются).

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct VoiceMeta {
    pub name: String,
    pub gender: String, // male | female | neutral | unknown
    pub age: String,    // child | teen | adult | elderly | unknown
    #[serde(default)]
    pub ru: bool, // годится ли голос для русской речи
}

#[derive(Deserialize)]
pub struct ModelVoices {
    #[serde(default)]
    pub supports_russian: bool,
    pub voices: Vec<VoiceMeta>,
}

/// Встроенный справочник голосов (model_id -> метаданные). Парсим один раз.
fn db() -> &'static HashMap<String, ModelVoices> {
    static DB: OnceLock<HashMap<String, ModelVoices>> = OnceLock::new();
    DB.get_or_init(|| serde_json::from_str(include_str!("voice_db.json")).unwrap_or_default())
}

fn catalog_model(model: &str) -> &str {
    match model
        .trim_start_matches("models/")
        .trim_start_matches("google/")
    {
        "gemini-3.8-flash-lite-tts" | "gemini-3.8-flash-tts" => {
            "google/gemini-3.1-flash-tts-preview"
        }
        _ => model,
    }
}

/// Метаданные конкретного голоса модели (пол/возраст/русский). None -> нет в справочнике.
#[allow(dead_code)] // используется в тестах
pub fn voice_meta(model: &str, voice: &str) -> Option<&'static VoiceMeta> {
    db().get(catalog_model(model))?.voices.iter().find(|v| v.name.eq_ignore_ascii_case(voice))
}

/// Тянет ли модель русский (для предупреждения в UI при русском дубляже несовместимой моделью).
pub fn model_supports_russian(model: &str) -> Option<bool> {
    db().get(catalog_model(model)).map(|m| m.supports_russian)
}

/// Список голосов модели с метаданными (для дропдауна в UI). Нет в справочнике -> None.
pub fn list(model: &str) -> Option<&'static Vec<VoiceMeta>> {
    db().get(catalog_model(model)).map(|m| &m.voices)
}

/// Голоса модели «взрослого» звучания (age adult/unknown/elderly) — не teen/child. Для обычного кастинга,
/// чтобы взрослому спикеру не достался детский голос (детские зарезервированы под будущий child-детект).
fn is_adultish(v: &VoiceMeta) -> bool {
    matches!(v.age.as_str(), "adult" | "unknown" | "elderly")
}

/// speaker_id -> голос выбранной облачной TTS-модели по полу спикера и языку дубляжа. `genders`: speaker ->
/// "male"/"female" (F0-замер). `tgt_lang` — код языка дубляжа ("ru" и пр.): для русского фильтруем на ru=true.
/// Пусто -> облачный TTS уйдёт на дефолтный голос настроек. Детерминизм: спикеры отсортированы, ротация.
pub fn assign(models_root: &Path, genders: &HashMap<String, String>, tgt_lang: &str,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let model = crate::models::tts_model(models_root);
    let Some(mv) = db().get(catalog_model(&model)) else {
        return out; // модель не в справочнике -> без автокастинга (дефолтный голос настроек)
    };
    let want_ru = tgt_lang.eq_ignore_ascii_case("ru");
    // Кандидаты: для русского — только тянущие русский; иначе все. Пусто после фильтра -> все (best-effort).
    let pool: Vec<&VoiceMeta> = mv.voices.iter().filter(|v| !want_ru || v.ru).collect();
    let pool = if pool.is_empty() { mv.voices.iter().collect::<Vec<_>>() } else { pool };
    if pool.is_empty() {
        return out;
    }

    // Пул на пол: сперва взрослые голоса нужного пола, если их нет — любые голоса этого пола.
    let by_gender = |g: &str| -> Vec<String> {
        let adultish: Vec<String> =
            pool.iter().filter(|v| v.gender == g && is_adultish(v)).map(|v| v.name.clone()).collect();
        if !adultish.is_empty() {
            adultish
        } else {
            pool.iter().filter(|v| v.gender == g).map(|v| v.name.clone()).collect()
        }
    };
    let male = by_gender("male");
    let female = by_gender("female");
    let all: Vec<String> = pool.iter().map(|v| v.name.clone()).collect();

    let mut spks: Vec<String> = genders.keys().cloned().collect();
    spks.sort();
    let (mut mi, mut fi, mut ai) = (0usize, 0usize, 0usize);
    for spk in spks {
        let g = genders.get(&spk).map(String::as_str).unwrap_or("");
        let voice = if g == "male" && !male.is_empty() {
            let v = male[mi % male.len()].clone();
            mi += 1;
            v
        } else if g == "female" && !female.is_empty() {
            let v = female[fi % female.len()].clone();
            fi += 1;
            v
        } else {
            let v = all[ai % all.len()].clone();
            ai += 1;
            v
        };
        out.insert(spk, voice);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_loads_and_has_known_models() {
        assert!(voice_meta("google/gemini-3.1-flash-tts-preview", "Kore").is_some());
        assert_eq!(voice_meta("google/gemini-3.1-flash-tts-preview", "Kore").unwrap().gender, "female");
        assert_eq!(voice_meta("google/gemini-3.1-flash-tts-preview", "Puck").unwrap().gender, "male");
        // voxtral русский не тянет
        assert_eq!(model_supports_russian("mistralai/voxtral-mini-tts-2603"), Some(false));
        assert_eq!(model_supports_russian("google/gemini-3.1-flash-tts-preview"), Some(true));
        assert_eq!(model_supports_russian("openai/gpt-audio"), Some(true));
    }
}
