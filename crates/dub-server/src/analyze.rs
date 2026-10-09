//! Analyze-ядро (транскрипт-стадия). Порт ASR-части dubengine/pipeline._build_dub + сборки Project
//! из api.analyze/Project.from_artifacts, но БЕЗ перевода/vision/captions (раунд 3).
//!
//! Что покрыто: ffmpeg -> wav 16k mono; диаризация Sortformer (turns); при 1 спикере / пустой
//! диаризации — штатная single-speaker ветка (как в питоне); транскрипция со словными таймстемпами;
//! Project с сегментами (src-текст, words в extra, speaker, mode-дефолты). Поля перевода/капшенов
//! остаются пустыми (tgt_text="", subs.mode как задано, captions по умолчанию).
//!
//! Фазы прогресса (msg + stage) повторяют питон: extract_audio / diarize / asr.

use dub_core::{Meta, Project, Segment};
use serde_json::{json, Value};

use crate::localize::Localize;
use crate::media;

pub use cache::StageCache;

/// Content-addressable per-stage кэш стадий analyze (#80 «чекпоинты»).
///
/// Файл `workspace/<pid>/cache.json` хранит по стадии: `param_hash = blake3(явные параметры стадии)`
/// и список выходных артефактов `outputs[]` с размером. Стадия считается закэшированной (reuse), только
/// если хэш параметров совпал И все её выходы присутствуют непусты (паттерн Nextflow: полнота+успех).
/// Правка одного параметра стадии меняет её `param_hash` → стадия честно пересчитывается.
///
/// Запись атомарна (tmp→rename), чтобы краш между стадиями не оставил битый манифест. Модуль чистый
/// (ФС+хэш, без GPU/Tokio) — тестируемый юнитами, не тянет тяжёлые зависимости.
mod cache {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// Один выход стадии: путь (относительно work_dir, если внутри неё — иначе абсолютный) + размер в байтах.
    /// Размер — дешёвый признак «файл на месте и непуст»; полный рехэш многочасовых wav избыточен.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct StageOutput {
        pub path: String,
        #[serde(default)]
        pub size: u64,
    }

    /// Запись о посчитанной стадии.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct StageEntry {
        /// blake3(явные параметры стадии) в hex.
        pub param_hash: String,
        #[serde(default)]
        pub outputs: Vec<StageOutput>,
        /// Юникс-время записи (диагностика; не участвует в решении о reuse).
        #[serde(default)]
        pub ts: u64,
    }

    /// Манифест кэша всего проекта: стадия → запись. BTreeMap → стабильный порядок ключей в JSON.
    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    pub struct StageCache {
        #[serde(default)]
        pub stages: BTreeMap<String, StageEntry>,
        /// Неизвестные поля переживают round-trip (как extra=allow в Project).
        #[serde(flatten)]
        pub extra: BTreeMap<String, Value>,
    }

    fn cache_path(work_dir: &Path) -> PathBuf {
        work_dir.join("cache.json")
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    impl StageCache {
        /// Загрузить `cache.json` из work_dir. Отсутствие/битый файл → пустой кэш (честный пересчёт всех
        /// стадий): кэш — ускорение, а не источник истины, поэтому его потеря безопасна.
        pub fn load(work_dir: &Path) -> Self {
            match std::fs::read_to_string(cache_path(work_dir)) {
                Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        }

        /// Атомарно записать `cache.json` (tmp→rename). Ошибку не роняем наружу как фатальную — кэш
        /// вспомогательный; вызывающий может залогировать. Возвращает Result для явности.
        pub fn save(&self, work_dir: &Path) -> Result<(), String> {
            let body = serde_json::to_string_pretty(self)
                .map_err(|e| t!("analyze-serialize", what = "cache.json", error = e.to_string()))?;
            dub_core::atomic::write(&cache_path(work_dir), body.as_bytes())
        }

        /// Закэширована ли стадия: param_hash совпал И все её выходы присутствуют непусты (size>0 и файл
        /// реально на диске с тем же размером). Ручное удаление файла из workspace → miss → честный пересчёт.
        /// Запись без файловых выходов не считается кэшем: данных для восстановления стадии нет.
        pub fn stage_cached(&self, work_dir: &Path, stage: &str, param_hash: &str) -> bool {
            let Some(entry) = self.stages.get(stage) else {
                return false;
            };
            if entry.param_hash != param_hash || entry.outputs.is_empty() {
                return false;
            }
            entry.outputs.iter().all(|o| {
                let p = resolve(work_dir, &o.path);
                match std::fs::metadata(&p) {
                    Ok(m) => m.is_file() && m.len() > 0 && m.len() == o.size,
                    Err(_) => false,
                }
            })
        }

        /// Записать (или обновить) запись стадии в память кэша: param_hash + фактические размеры выходов.
        /// НЕ пишет на диск — вызвать `save` отдельно (обычно один раз в конце или после каждой стадии).
        pub fn write_stage(&mut self, work_dir: &Path, stage: &str, param_hash: &str, outputs: &[&Path]) {
            let outs: Vec<StageOutput> = outputs
                .iter()
                .map(|p| StageOutput {
                    path: rel_or_abs(work_dir, p),
                    size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                })
                .collect();
            self.stages.insert(
                stage.to_string(),
                StageEntry { param_hash: param_hash.to_string(), outputs: outs, ts: now_secs() },
            );
        }

        /// Инвалидировать стадию (удалить её запись) — например, при ручном сбросе из UI
        /// (POST /projects/{pid}/invalidate?stage=, проводка в lib.rs — вне владения этой подсистемы).
        #[allow(dead_code)] // публичное API под ручной сброс стадии; вызывающий — lib.rs (другой агент).
        pub fn invalidate(&mut self, stage: &str) {
            self.stages.remove(stage);
        }
    }

    /// Путь выхода относительно work_dir, если он внутри неё (переносимость каталога проекта); иначе абс.
    fn rel_or_abs(work_dir: &Path, p: &Path) -> String {
        match p.strip_prefix(work_dir) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => p.to_string_lossy().into_owned(),
        }
    }

    /// Обратное к rel_or_abs: относительный путь резолвим от work_dir, абсолютный — как есть.
    fn resolve(work_dir: &Path, stored: &str) -> PathBuf {
        let p = Path::new(stored);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            work_dir.join(stored)
        }
    }

    /// blake3-хэш конкатенации строк-параметров стадии → hex. Разделитель `\x1f` (unit separator) не
    /// встречается в реальных параметрах, поэтому границы полей не спутать (a|b ≠ a||b — коллизия исключена).
    pub fn hash_stage(parts: &[&str]) -> String {
        let mut h = blake3::Hasher::new();
        for p in parts {
            h.update(p.as_bytes());
            h.update(b"\x1f");
        }
        h.finalize().to_hex().to_string()
    }

    /// Быстрый префикс-хэш файла как ключ инвалидации стадий вниз по DAG: blake3(размер ‖ первые 8МБ ‖
    /// последние 8МБ). Полный рехэш многочасового wav избыточен; для реального контента совпадение
    /// размера+префикса+суффикса означает тот же файл. Нет файла → "0" (стадия честно пересчитается).
    pub fn hash_file_prefix(path: &Path) -> String {
        use std::io::{Seek, SeekFrom};
        const CHUNK: u64 = 8 * 1024 * 1024;
        let Ok(mut f) = std::fs::File::open(path) else {
            return "0".to_string();
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let mut h = blake3::Hasher::new();
        h.update(&len.to_le_bytes());
        let mut buf = vec![0u8; CHUNK as usize];
        // Первые CHUNK байт.
        let head = read_up_to(&mut f, &mut buf);
        h.update(&buf[..head]);
        // Последние CHUNK байт (если файл длиннее 2×CHUNK — иначе перекрытие с головой безвредно).
        if len > CHUNK {
            let start = len.saturating_sub(CHUNK);
            if f.seek(SeekFrom::Start(start)).is_ok() {
                let tail = read_up_to(&mut f, &mut buf);
                h.update(&buf[..tail]);
            }
        }
        h.finalize().to_hex().to_string()
    }

    pub fn hash_model(path: &Path) -> Result<String, String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)
            .map_err(|e| t!("analyze-read-model", path = path.display().to_string(), error = e.to_string()))?;
        let mut hash = blake3::Hasher::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let size = file.read(&mut buffer)
                .map_err(|e| t!("analyze-read-model", path = path.display().to_string(), error = e.to_string()))?;
            if size == 0 { break; }
            hash.update(&buffer[..size]);
        }
        Ok(hash.finalize().to_hex().to_string())
    }

    fn read_up_to(f: &mut std::fs::File, buf: &mut [u8]) -> usize {
        use std::io::Read;
        let mut filled = 0;
        while filled < buf.len() {
            match f.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(_) => break,
            }
        }
        filled
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn hash_stage_field_boundaries_distinct() {
            // "a"+"b" не должно коллидировать с "ab"+"" из-за разделителя.
            assert_ne!(hash_stage(&["a", "b"]), hash_stage(&["ab", ""]));
            // Детерминизм.
            assert_eq!(hash_stage(&["x", "y"]), hash_stage(&["x", "y"]));
        }

        #[test]
        fn stage_cached_roundtrip() {
            let dir = std::env::temp_dir().join(format!("dubcache_test_{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let out = dir.join("vocals16.wav");
            std::fs::write(&out, b"1234567890").unwrap();
            let mut c = StageCache::default();
            let key = hash_stage(&["extract", "v1"]);
            c.write_stage(&dir, "extract", &key, &[&out]);
            c.save(&dir).unwrap();

            let loaded = StageCache::load(&dir);
            assert!(loaded.stage_cached(&dir, "extract", &key));
            // Другой хэш → miss.
            assert!(!loaded.stage_cached(&dir, "extract", "deadbeef"));
            // Удалили выход → miss даже при совпавшем хэше.
            std::fs::remove_file(&out).unwrap();
            assert!(!loaded.stage_cached(&dir, "extract", &key));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn entry_without_outputs_is_not_a_hit() {
            let dir = std::env::temp_dir().join(format!("dubcache_empty_{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let mut c = StageCache::default();
            let key = hash_stage(&["diarize", "params"]);
            c.write_stage(&dir, "diarize", &key, &[]); // старые записи без файлов: восстанавливать нечего
            assert!(!c.stage_cached(&dir, "diarize", &key));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// Параметры analyze (из query POST /projects/{pid}/analyze). tgt_lang/mode/src_lang/subs/rewrite —
/// как в app.py.analyze_project. В этой стадии влияют только на mode-дефолты Project и mode/subs поля.
pub struct AnalyzeArgs {
    /// 0 — автоматическое определение; 1–8 — ожидаемое число голосов на всю запись.
    pub speaker_count: usize,
    pub tgt_lang: String,
    pub mode: String,   // auto | dub | nodub | transcribe (auto -> dub по умолчанию, до vision-стадии)
    pub src_lang: String,
    pub subs: String,   // auto | none | translate | transcribe | bilingual
    pub rewrite: String,
    pub translate_style: String, // доп-инструкция стиля перевода (#112); пусто = без стиля
    pub burn: bool,     // вжигать ли субтитры/титры на видео (композируемость; дефолт true)
    pub detect_text: bool, // OCR-детекция вшитого текста (блюр + локализация титров). Дорогая (минуты на 4K);
                           // в режиме без субтитров/блюра не нужна — галочка в UI, дефолт true.
    pub casting: bool,     // кастинг персонажей (#115): SCRFD+LVFace-скан кадров. ВЫКЛ по умолчанию —
                           // при false НИ ОДНОГО кадра не сканируется (галочка в UI, как detect_text).
    pub casting_ref: String, // slug профиля из app-библиотеки кастингов (#115): применить к этому ролику
                             // (загрузить как prev, матчить кластеры по face+voice). Пусто = без применения.
    pub content_type: String, // тип контента для кастинга: "real" (SCRFD+LVFace) | "anime" (детектор
                              // рисованных лиц + CCIP). Пусто = "real". Аниме-путь ловит мульт/аниме лица.
    pub import_translated: bool, // импортированные субтитры УЖЕ на языке перевода -> MT/vision пропустить,
                                 // tgt = импортированный текст, Даб Студио только озвучивает. Работает лишь с import_subs.
    pub align_subs: bool, // выровнять тайминги импортированных субтитров по распознанной речи (subalign):
                          // полный прогон ASR ради слов; только для субтитров на языке речи.
}

/// Пути к моделям/входу для одной джобы analyze.
pub struct AnalyzePaths {
    pub input: std::path::PathBuf,   // исходное видео (source.txt)
    pub work_dir: std::path::PathBuf, // workspace/<pid>
    pub repo_root: std::path::PathBuf, // корень репо (для app-библиотеки кастингов: <repo>/casting_library)
    pub asr: crate::models::AsrChoice, // выбранный ASR-движок (Parakeet/Whisper) на эту джобу
    pub sortformer_onnx: std::path::PathBuf, // sortformer .onnx
    pub llama_bin: std::path::PathBuf, // llama-server(.exe) — сайдкар перевода/vision
    pub mt_model: std::path::PathBuf, // Gemma GGUF
    pub mmproj: std::path::PathBuf,   // mmproj GGUF (vision-проектор)
    pub models_root: std::path::PathBuf, // корень моделей (для OCR: <root>/ocr/…)
    pub caption_fps: i32,             // частота семплинга кадров OCR
    /// Импортированные субтитры (SRT/ASS) — если есть, берём текст+тайминг отсюда вместо ASR.
    pub import_subs: Option<std::path::PathBuf>,
    /// BSRoformer (сепарация вокала) — best practices: ЧИСТЫЙ вокал до диаризации/ASR, а не сырой микс.
    pub bsroformer_cli: std::path::PathBuf,
    pub bsroformer_model: std::path::PathBuf,
}

/// Колбэк прогресса джобы (msg + произвольные поля). stage — фаза как в питоне.
pub type Progress<'a> = dyn Fn(Value) + Send + Sync + 'a;

fn emit(progress: &Progress, stage: &str, msg: &str) {
    progress(json!({ "stage": stage, "msg": msg }));
}

/// speaker-метка сегмента (Option<String>) -> i64 для dub-translate Seg. Метки бывают числовые ("0") ИЛИ
/// голосовые ("v0" после recluster_segments) — снимаем префикс 'v' перед парсом, иначе nspk схлопывается в 1
/// и «диалог из N спикеров»-хинт теряется. ЕДИНАЯ точка для translate/remix/re-lang (были 3 копии, 2
/// разъехались — ревью #3). Пусто/непарсится -> 0 (питон-дефолт speaker=0).
pub fn speaker_to_i64(speaker: Option<&str>) -> i64 {
    speaker
        .map(|x| x.trim_start_matches('v'))
        .and_then(|x| x.parse::<i64>().ok())
        .unwrap_or(0)
}

/// _has_speech (порт pipeline.py:66-78): есть ли РЕАЛЬНАЯ дубляж-годная речь? ASR галлюцинирует на
/// музыке/неподдерживаемом языке — обычно короткая фраза повторяется (заполняет таймлайн, но почти без
/// РАЗНООБРАЗИЯ слов). Гейтим по разнообразию слов + покрытию: uniq>=0.35 и coverage>=0.10. Пустой
/// транскрипт (<4 слов) -> нет речи.
fn has_speech(segs: &[Segment], total: f64) -> bool {
    let mut words: Vec<String> = Vec::new();
    let mut cjk_chars: usize = 0;
    for s in segs {
        for w in s.src_text.split(|c: char| !c.is_alphabetic()) {
            if !w.is_empty() {
                words.push(w.to_lowercase());
            }
        }
        cjk_chars += s.src_text.chars().filter(|c| is_cjk(*c)).count();
    }
    // CJK: иероглифы не разделяются пробелами — считаем символы вместо слов
    let effective_words = words.len().max(cjk_chars);
    if effective_words < 4 {
        return false;
    }
    let cov: f64 = segs.iter().map(|s| (s.end - s.start).max(0.0)).sum::<f64>() / total.max(0.1);
    let uniq = if words.is_empty() {
        1.0 // CJK-only: каждый иероглиф уникален по определению в этом контексте
    } else {
        words.iter().collect::<std::collections::HashSet<_>>().len() as f64 / words.len() as f64
    };
    cov >= 0.10 && uniq >= 0.35
}

/// Является ли символ CJK-иероглифом (китайский, японский кандзи, корейский хангыль).
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF      // CJK Unified Ideographs
        | 0x3400..=0x4DBF    // CJK Extension A
        | 0x20000..=0x2A6DF  // CJK Extension B
        | 0xAC00..=0xD7AF    // Hangul Syllables
        | 0x3040..=0x309F    // Hiragana
        | 0x30A0..=0x30FF    // Katakana
    )
}

/// Разбить mode/subs как в pipeline: dub vs subs-only vs transcribe. В транскрипт-стадии нам нужно лишь
/// решить итоговый Project.mode и Project.subs.mode. auto -> dub (флип в nodub делает has_speech-гейт
/// ниже, после ASR — как в питоне pipeline.py:124-129).
fn resolve_modes(args: &AnalyzeArgs) -> (String, String) {
    // subs=auto: для dub/transcribe режимов оставляем translate (перевод придёт в раунде 3), для
    // transcribe-режима — transcribe. Совпадает с логикой api.analyze mode-присвоения.
    let mode = match args.mode.as_str() {
        "nodub" => "nodub",
        "transcribe" => "transcribe",
        "voiceover" => "voiceover", // закадровый: переводим+TTS, но оригинал слышно приглушённым
        _ => "dub", // dub | auto -> dub
    }
    .to_string();
    let subs = match (args.subs.as_str(), mode.as_str()) {
        ("none", _) => "none",
        ("transcribe", _) | (_, "transcribe") => "transcribe",
        // перевод основной строкой + оригинал второй
        ("bilingual", _) => "bilingual",
        // dub / nodub c subs=auto|translate -> translate
        _ => "translate",
    }
    .to_string();
    (mode, subs)
}

/// Нужна ли этому анализу диаризация: всё, кроме nodub (только субтитры идут whole-clip, как питон).
pub fn wants_diarization(args: &AnalyzeArgs) -> bool {
    args.speaker_count != 1 && (args.speaker_count > 1 || args.mode != "nodub")
}

/// Спикер для импортированной реплики субтитров — по максимальному перекрытию по времени с
/// диаризацией. Нет перекрытия / нет диаризации -> "0" (тот же single-speaker контракт, что у ASR).
fn speaker_for(start: f64, end: f64, turns: &[dub_asr::Turn]) -> String {
    let mut best_spk = 0i32;
    let mut best_ov = 0.0f64;
    for t in turns {
        let ov = end.min(t.end) - start.max(t.start);
        if ov > best_ov {
            best_ov = ov;
            best_spk = t.speaker;
        }
    }
    // Нет перекрытия — назначаем ближайшего по времени спикера вместо дефолтного "0"
    if best_ov <= 0.0 {
        let mid = (start + end) / 2.0;
        let mut best_dist = f64::MAX;
        for t in turns {
            let dist = ((t.start + t.end) / 2.0 - mid).abs();
            if dist < best_dist {
                best_dist = dist;
                best_spk = t.speaker;
            }
        }
    }
    best_spk.to_string()
}

// ─── Параметры оконного драйвера (#79) и версии стадий (компонент кэш-ключа) ──────────────────
//
// Версии-строки — ручной bump при смене движка/дефолтов стадии: правка инвалидирует ТОЛЬКО свою
// стадию + downstream, а не весь фильм. Все — env-override через DUB_* для тюнинга без пересборки.

/// Порог включения оконного пути (сек). Короче → монолитный путь (байт-паритет с текущим поведением).
/// Дефолт 900с (15мин). Env DUB_CHUNK_MIN_SEC.
fn chunk_min_sec() -> f64 {
    std::env::var("DUB_CHUNK_MIN_SEC").ok().and_then(|v| v.parse().ok()).unwrap_or(900.0)
}
/// Целевой размер окна (сек) для оконного ASR/диаризации. Дефолт 600с. Env DUB_WIN_TARGET_SEC.
fn win_target_sec() -> f64 {
    std::env::var("DUB_WIN_TARGET_SEC").ok().and_then(|v| v.parse().ok()).unwrap_or(600.0)
}

const EXTRACT_VER: &str = "extract-16kmono-sync-v2";
/// В тексте сегмента есть хоть одна буква или цифра: сегмент из одной пунктуации («.») — остаток
/// ASR на хвосте тишины, переводить и озвучивать его нельзя.
fn has_speech_text(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

const DIAR_VER: &str = "nemotron3-diar-v3 · offline · merge_gap=0.8 · min_spk=10%[1.5..2.5] · out-v2";
const ASR_VER: &str = "asr-v4-abbrev-speaker-split";

/// Speech of this length is enough to tell a language Parakeet does not know: speech runs at 2-3 words
/// a second, and Parakeet given another language returns a stray word or two for minutes of it.
const LANGUAGE_CHECK_SPEECH: f64 = 15.0;
const LANGUAGE_CHECK_WORDS_PER_SECOND: f64 = 0.3;

fn unrecognised_language(speech_seconds: f64, words: usize) -> bool {
    speech_seconds >= LANGUAGE_CHECK_SPEECH && (words as f64) < speech_seconds * LANGUAGE_CHECK_WORDS_PER_SECOND
}
const TRANSLATE_VER: &str = "gemma-ctx-v4-json-glossary";
const OCR_VER: &str = "ppocr-onnx-v2";
const CAST_VER: &str = "casting-v1";

// Durable-выходы тяжёлых стадий: пишутся атомарно сразу после стадии, повторный analyze на том же
// проекте берёт их из кэша (stage_cached по ключу параметров) и стадию пропускает.
const DIAR_FILE: &str = "diar.json";
const TRANSCRIPT_FILE: &str = "transcript.json";
const TRANSLATED_FILE: &str = "translated.json";
const OCR_FILE: &str = "ocr.json";
const CASTING_FILE: &str = "casting.json";
#[cfg(test)]
pub(crate) const STAGE_FILES: [&str; 5] = [DIAR_FILE, TRANSCRIPT_FILE, TRANSLATED_FILE, OCR_FILE, CASTING_FILE];

#[derive(serde::Serialize, serde::Deserialize)]
struct DiarOut {
    turns: Vec<(f64, f64, i32)>,
    n_speakers: usize,
    ref_windows: Vec<(i32, f64, f64)>,
}

impl DiarOut {
    fn of(d: &dub_asr::DiarTurns) -> Self {
        let mut ref_windows: Vec<(i32, f64, f64)> = d.ref_windows.iter().map(|(k, w)| (*k, w.0, w.1)).collect();
        ref_windows.sort_by_key(|w| w.0);
        DiarOut {
            turns: d.turns.iter().map(|t| (t.start, t.end, t.speaker)).collect(),
            n_speakers: d.n_speakers,
            ref_windows,
        }
    }

    fn into_turns(self) -> dub_asr::DiarTurns {
        dub_asr::DiarTurns {
            turns: self
                .turns
                .into_iter()
                .map(|(start, end, speaker)| dub_asr::Turn { start, end, speaker })
                .collect(),
            n_speakers: self.n_speakers,
            ref_windows: self.ref_windows.into_iter().map(|(k, a, b)| (k, (a, b))).collect(),
        }
    }
}

/// Транскрипт после слияния огрызков, ДО голосовой переразметки кастинга (она пересчитывается).
#[derive(serde::Serialize, serde::Deserialize)]
struct TranscriptOut {
    segments: Vec<Segment>,
    n_spk: usize,
}

/// Всё, что меняет стадия перевода/vision: tgt по id сегмента + ctx-словарь + стиль/позиция субтитров.
#[derive(serde::Serialize, serde::Deserialize)]
struct TranslatedOut {
    tgt: std::collections::BTreeMap<String, String>,
    raw_ctx: serde_json::Map<String, Value>,
    sub_style: Option<dub_core::SubStyle>,
    sub_y: Option<i64>,
    brands: Vec<dub_core::Brand>,
    content_type: String,
}

impl TranslatedOut {
    fn of(p: &Project) -> Self {
        TranslatedOut {
            tgt: p.segments.iter().map(|s| (s.id.clone(), s.tgt_text.clone())).collect(),
            raw_ctx: p.raw_ctx.clone(),
            sub_style: p.captions.sub_style.clone(),
            sub_y: p.captions.sub_y,
            brands: p.captions.brands.clone(),
            content_type: p.audio.content_type.clone(),
        }
    }

    fn apply(self, p: &mut Project) {
        for s in &mut p.segments {
            if let Some(t) = self.tgt.get(&s.id) {
                s.tgt_text = t.clone();
            }
        }
        p.raw_ctx = self.raw_ctx;
        p.captions.sub_style = self.sub_style;
        p.captions.sub_y = self.sub_y;
        p.captions.brands = self.brands;
        p.audio.content_type = self.content_type;
    }
}

/// Сырые детекции OCR (кадры + DBNet/CRNN) — дорогая часть стадии, зависит только от видео и моделей.
#[derive(serde::Serialize, serde::Deserialize)]
struct OcrOut {
    regions: Vec<(String, i64, i64, i64, i64, f32, f32)>,
    raw: Vec<(String, f32, f32, f32, f32, f32)>,
}

impl OcrOut {
    fn of(regions: &[dub_ocr::Region], raw: &[dub_ocr::RawDet]) -> Self {
        OcrOut {
            regions: regions.iter().map(|r| (r.text.clone(), r.x, r.y, r.w, r.h, r.t0, r.t1)).collect(),
            raw: raw.iter().map(|d| (d.text.clone(), d.x, d.y, d.w, d.h, d.t)).collect(),
        }
    }

    fn into_parts(self) -> (Vec<dub_ocr::Region>, Vec<dub_ocr::RawDet>) {
        (
            self.regions
                .into_iter()
                .map(|(text, x, y, w, h, t0, t1)| dub_ocr::Region { text, x, y, w, h, t0, t1 })
                .collect(),
            self.raw
                .into_iter()
                .map(|(text, x, y, w, h, t)| dub_ocr::RawDet { text, x, y, w, h, t })
                .collect(),
        )
    }
}

/// Выход стадии из кэша: ключ совпал и файл на месте -> данные. Файл не читается — сообщение и пересчёт.
fn stage_load<T: serde::de::DeserializeOwned>(
    cache: &StageCache,
    work_dir: &std::path::Path,
    stage: &str,
    key: &str,
    file: &str,
    progress: &Progress,
) -> Option<T> {
    if !cache.stage_cached(work_dir, stage, key) {
        return None;
    }
    let p = work_dir.join(file);
    let loaded = std::fs::read_to_string(&p)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
    match loaded {
        Ok(v) => Some(v),
        Err(e) => {
            emit(progress, stage, &t!("analyze-stage-cache-unreadable", stage = stage.to_string(), error = e.to_string()));
            None
        }
    }
}

/// Записать durable-выход стадии атомарно и зарегистрировать его в cache.json. Сбой записи кэша не
/// валит анализ (результат уже в памяти), но сообщается: повтор этой стадии тогда посчитается заново.
fn stage_store<T: serde::Serialize>(
    cache: &mut StageCache,
    work_dir: &std::path::Path,
    stage: &str,
    key: &str,
    file: &str,
    value: &T,
    progress: &Progress,
) {
    let p = work_dir.join(file);
    let res = serde_json::to_vec(value)
        .map_err(|e| t!("analyze-serialize", what = file.to_string(), error = e.to_string()))
        .and_then(|body| dub_core::atomic::write(&p, &body))
        .and_then(|()| {
            cache.write_stage(work_dir, stage, key, &[&p]);
            cache.save(work_dir)
        });
    if let Err(e) = res {
        emit(progress, stage, &t!("analyze-checkpoint-not-saved", stage = stage.to_string(), error = e.to_string()));
    }
}

/// Отпечаток файла модели для ключа стадии: имя + размер (0 — файла нет).
fn file_tag(p: &std::path::Path) -> String {
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    format!("{}:{size}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

fn diar_cache_key(input: &str, detector: &std::path::Path, voice_model: &std::path::Path, count: usize) -> Result<String, String> {
    let continuous = count > 1 && dub_asr::continuous_diarization_enabled()?;
    diar_cache_key_with_mode(input, detector, voice_model, count, continuous)
}

fn diar_cache_key_with_mode(input: &str, detector: &std::path::Path, voice_model: &std::path::Path, count: usize, continuous: bool) -> Result<String, String> {
    if count == 0 {
        return Ok(cache::hash_stage(&[DIAR_VER, input, &file_tag(detector)]));
    }
    let version = if count > 1 && continuous { "known-voices-continuous-v1" } else { "known-voices-v3" };
    Ok(cache::hash_stage(&[DIAR_VER, version, input, &file_tag(detector), &count.to_string(),
        &if count > 1 { cache::hash_model(voice_model)? } else { "none".into() }]))
}

fn diarization_fingerprint(diar: &dub_asr::DiarTurns, key: &str, count: usize) -> Result<String, String> {
    let output = serde_json::to_string(&DiarOut::of(diar)).map_err(|e| t!("analyze-serialize", what = "diarization", error = e.to_string()))?;
    Ok(if count == 0 { cache::hash_stage(&[&output]) } else { cache::hash_stage(&[key, &output]) })
}

#[cfg(test)]
mod continuous_diarization_cache_tests {
    use super::*;

    #[test]
    fn switching_mode_invalidates_diarization_and_asr_but_not_single_speaker_or_auto() {
        let dir = tempfile::tempdir().unwrap();
        let detector = dir.path().join("detector.onnx");
        let voice = dir.path().join("voice.onnx");
        std::fs::write(&voice, b"voice-model").unwrap();
        let normal = diar_cache_key_with_mode("audio", &detector, &voice, 8, false).unwrap();
        let continuous = diar_cache_key_with_mode("audio", &detector, &voice, 8, true).unwrap();
        assert_ne!(normal, continuous);
        let diar = dub_asr::DiarTurns { turns: Vec::new(), n_speakers: 8, ref_windows: Default::default() };
        assert_ne!(diarization_fingerprint(&diar, &normal, 8).unwrap(), diarization_fingerprint(&diar, &continuous, 8).unwrap());
        for count in [0, 1] {
            assert_eq!(diar_cache_key_with_mode("audio", &detector, &voice, count, false).unwrap(), diar_cache_key_with_mode("audio", &detector, &voice, count, true).unwrap());
        }
    }
}

fn known_speaker_turns(
    wav: &std::path::Path,
    detector: &std::path::Path,
    voice_model: &std::path::Path,
    count: usize,
    progress: &Progress,
) -> Result<dub_asr::DiarTurns, String> {
    let msg = if dub_asr::continuous_diarization_enabled()? {
        t!("analyze-diarize-continuous", speakers = count)
    } else {
        t!("analyze-diarize-fragments", speakers = count)
    };
    emit(progress, "diarize", &msg);
    let mut embedder = dub_faces::VoiceEmbedder::load(voice_model)
        .map_err(|e| t!("analyze-wespeaker-needed", error = e.localize()))?;
    dub_asr::turns_with_speaker_count(wav, detector, count, &mut |samples| {
        crate::jobs::check_cancelled()?;
        embedder.embed_samples(samples).map_err(|e| e.localize())
    }).map_err(|e| e.localize())
}

/// Кто переводит (stage "llm") или смотрит кадры (stage "vision") — для ключа стадии перевода: своя Gemma
/// своими файлами, локальный сервер адресом и моделью, OpenRouter моделью.
fn provider_tag(paths: &AnalyzePaths, stage: &str) -> String {
    let mr = &paths.models_root;
    match crate::models::llm_backend(mr, stage) {
        crate::models::LlmBackend::Local if stage == "vision" => {
            format!("local:{}+{}", file_tag(&paths.mt_model), file_tag(&paths.mmproj))
        }
        crate::models::LlmBackend::Local => format!("local:{}", file_tag(&paths.mt_model)),
        crate::models::LlmBackend::Server => {
            format!("server:{}:{}", crate::models::server_url(mr), crate::models::server_model(mr, stage))
        }
        crate::models::LlmBackend::OpenRouter => format!("openrouter:{}", crate::models::openrouter_model(mr, stage)),
    }
}

fn mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// План оконной обработки длинных дорожек. Короткий файл = 1 окно = текущее поведение (паритет).
struct WindowPlan {
    /// Число окон (≥1). 1 → монолитный путь.
    n_windows: usize,
    /// Границы окон [start,end) в секундах (для будущего оконного dub-asr API). При n=1 — весь файл.
    windows: Vec<(f64, f64)>,
}

impl WindowPlan {
    /// Построить план по длительности. Длиннее порога → нарезка на ~равные окна ≤win_target_sec.
    /// ВНИМАНИЕ: пока dub-asr не имеет оконного API (turns_windowed/transcribe_windowed), фактическая
    /// обработка идёт как window=1 (см. TODO-гейт в run) — план строится и эмитится для видимости и как
    /// каркас под #79, но НЕ меняет вызовы стадий, чтобы не сломать паритет. Короткий файл → n=1 всегда.
    fn build(duration: f64) -> Self {
        let dur = duration.max(0.0);
        if dur <= chunk_min_sec() || dur <= 0.0 {
            return WindowPlan { n_windows: 1, windows: vec![(0.0, dur)] };
        }
        let target = win_target_sec().max(60.0);
        let n = (dur / target).ceil().max(1.0) as usize;
        let step = dur / n as f64;
        let mut windows = Vec::with_capacity(n);
        for i in 0..n {
            let s = step * i as f64;
            let e = if i + 1 == n { dur } else { step * (i + 1) as f64 };
            windows.push((s, e));
        }
        WindowPlan { n_windows: n, windows }
    }
}

/// Выровнять импортированные реплики по распознанной речи (subalign). None — выравнивание не
/// применяется (субтитры на языке перевода, речь не распознана или текст не совпал с речью); причина
/// уходит в журнал, тайминги файла остаются.
fn align_cues(
    args: &AnalyzeArgs,
    paths: &AnalyzePaths,
    asr_wav: &std::path::Path,
    cues: &[crate::subimport::Cue],
    progress: &Progress,
) -> Result<Option<crate::subalign::Alignment>, String> {
    if args.import_translated {
        emit(progress, "asr", &t!("analyze-align-skipped"));
        return Ok(None);
    }
    emit(progress, "asr", &t!("analyze-align-recognizing"));
    std::env::set_var("DUB_ASR_BACKEND", crate::models::stage_backend(&paths.models_root, "asr_backend"));
    let mut asr = crate::models::build_engine(&paths.asr);
    let words: Vec<crate::subalign::Heard> = asr
        .transcribe(asr_wav, &args.src_lang)
        .map_err(|e| t!("analyze-align-recognition-failed", error = e.localize()))?
        .into_iter()
        .flat_map(|s| s.words)
        .map(|w| (w.word, w.start, w.end))
        .collect();
    let refs: Vec<(f64, f64, &str)> = cues.iter().map(|c| (c.start, c.end, c.text.as_str())).collect();
    let Some(a) = crate::subalign::align(&refs, &words) else {
        emit(progress, "asr", &t!("analyze-align-no-speech"));
        return Ok(None);
    };
    let pct = (a.share * 100.0).round();
    if a.share < crate::subalign::MIN_ALIGNED_SHARE {
        emit(progress, "asr", &t!("analyze-align-mismatch", share = pct));
        return Ok(None);
    }
    emit(progress, "asr", &t!("analyze-align-done", share = pct, offset = format!("{:+.2}", a.offset)));
    Ok(Some(a))
}

/// «a», «b», … — до 5 фраз для журнала.
fn quote_list(items: &[String]) -> String {
    let mut parts: Vec<String> = items.iter().take(5).map(|t| format!("«{}»", t.chars().take(60).collect::<String>())).collect();
    if items.len() > 5 {
        parts.push(t!("analyze-more", count = items.len() - 5));
    }
    parts.join(", ")
}

/// Слить короткие огрызки ОДНОГО спикера в одну фразу (#115). Whisper дробит предложение на «If» +
/// «they find you.» — каждый огрызок озвучивается отдельно и звучит рвано. Клеим сосед в предыдущий,
/// если: тот же спикер, зазор < 0.35с, хотя бы один из двух короткий (<1.6с) или из одного слова,
/// суммарно не длиннее капа реплики (SEG_MAX_DUR) и <200 симв. Так «одна фраза, разбитая таймингом»
/// снова становится одной; две полные разные фразы НЕ склеиваются.
fn merge_short_turns(segs: &mut Vec<Segment>) {
    if segs.len() < 2 {
        return;
    }
    const GAP: f64 = 0.35;
    // Whisper иногда даёт лёгкое ПЕРЕКРЫТИЕ соседних слов (w[i+1].start < w[i].end) -> отрицательный зазор
    // на границе огрызков ОДНОЙ фразы. Допускаем небольшой overlap, иначе «If|they find you» с −0.02с не
    // склеится и озвучится рвано (ровно то, что merge и должен убирать).
    const OVERLAP: f64 = 0.2;
    const SHORT: f64 = 1.6;
    const MAX_DUR: f64 = dub_asr::SEG_MAX_DUR;
    const MAX_CH: usize = 200;
    let src = std::mem::take(segs);
    let mut out: Vec<Segment> = Vec::with_capacity(src.len());
    for s in src {
        if let Some(last) = out.last_mut() {
            // Помеченные фильтром галлюцинаций реплики не склеиваем: флаг и hidden остаются на своём
            // интервале, пользователь видит и возвращает ровно её.
            let flagged = |x: &Segment| x.extra.contains_key("asr_flag");
            if flagged(last) || flagged(&s) {
                out.push(s);
                continue;
            }
            let same_spk = last.speaker == s.speaker;
            let gap = s.start - last.end;
            let one_word = |x: &Segment| x.src_text.split_whitespace().count() <= 1;
            let short = (last.end - last.start) < SHORT || (s.end - s.start) < SHORT || one_word(last) || one_word(&s);
            let dur_ok = (s.end - last.start) <= MAX_DUR;
            let ch_ok = last.src_text.chars().count() + s.src_text.chars().count() < MAX_CH;
            if same_spk && gap > -OVERLAP && gap < GAP && short && dur_ok && ch_ok {
                let lt = last.src_text.trim_end();
                let rt = s.src_text.trim_start();
                let sep = if lt.is_empty() || rt.is_empty() { "" } else { " " };
                last.src_text = format!("{lt}{sep}{rt}");
                last.end = last.end.max(s.end); // overlap: не укорачивать вложенным сегментом
                // слить пословный тайминг (karaoke) если есть у обоих.
                if let Some(Value::Array(wr)) = s.extra.get("words") {
                    match last.extra.get_mut("words") {
                        Some(Value::Array(wl)) => wl.extend(wr.clone()),
                        _ => {
                            last.extra.insert("words".into(), Value::Array(wr.clone()));
                        }
                    }
                }
                continue;
            }
        }
        out.push(s);
    }
    for (i, s) in out.iter_mut().enumerate() {
        s.id = format!("s{i}");
    }
    *segs = out;
}

/// Запустить analyze: extract -> diarize -> transcribe -> Project. Возвращает готовый Project.
/// Диаризация/транскрипция тяжёлые (ONNX CPU) — вызывать в блокирующем контексте (job worker).
///
/// Поверх стадий лежит content-addressable per-stage кэш (cache.json): каждая тяжёлая стадия сразу пишет
/// durable-выход (vocals16.wav, diar.json, transcript.json, translated.json, ocr.json, casting.json) и
/// регистрирует его под ключом своих параметров. Повторный analyze на том же проекте (продолжение после
/// сбоя или отмены) берёт совпавшие стадии из кэша и шлёт resumed:true.
pub fn run(args: &AnalyzeArgs, paths: &AnalyzePaths, progress: &Progress) -> Result<Project, String> {
    // 0) кэш стадий (#80). Отсутствие/битый cache.json → пустой кэш (честный пересчёт).
    let mut cache = StageCache::load(&paths.work_dir);
    // Бенчмаркинг (галка в настройках, ВЫКЛ по умолчанию): семплер GPU/CPU/VRAM -> bench.json + ⏱ в журнал.
    let mut bench = crate::bench::Bench::start(&paths.work_dir, "analyze", crate::models::bench_enabled(&paths.models_root));
    bench.stage("probe");

    // 1) probe (метаданные видео).
    let meta = media::probe(&paths.input)?;
    emit(
        progress,
        "probe",
        &format!(
            "input {} dur={:.1}s {}x{}",
            paths.input.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
            meta.duration,
            meta.width,
            meta.height
        ),
    );

    // 1b) оконный план (#79). Короткий файл / import_subs → 1 окно (текущее поведение).
    let plan = if paths.import_subs.is_some() {
        WindowPlan { n_windows: 1, windows: vec![(0.0, meta.duration.max(0.0))] }
    } else {
        WindowPlan::build(meta.duration)
    };
    if plan.n_windows > 1 {
        // TODO(#79): фактический оконный ASR/диаризация подключаются, когда dub-asr отдаст
        // turns_windowed/transcribe_windowed. Пока — window=1-путь: план построен и виден в прогрессе,
        // но стадии идут монолитно (паритет). Не ломаем короткие/длинные режимы.
        let first = plan.windows.first().map(|w| w.1 - w.0).unwrap_or(0.0);
        emit(
            progress,
            "window_plan",
            &t!(
                "analyze-window-plan",
                duration = format!("{:.0}", meta.duration),
                windows = plan.n_windows,
                first = format!("{first:.0}")
            ),
        );
    }

    // 2) extract audio -> wav 16k mono. Кэш-гейт: если источник не менялся (префикс-хэш) и vocals16
    //    на месте непуст — переиспользуем без повторного ffmpeg (дорого на многочасовом входе).
    bench.stage("extract");
    let vocals16 = paths.work_dir.join("vocals16.wav");
    let src_hash = cache::hash_file_prefix(&paths.input);
    let extract_key = cache::hash_stage(&[EXTRACT_VER, &src_hash]);
    if cache.stage_cached(&paths.work_dir, "extract", &extract_key) {
        // resumed:true — SSE-маркер «возобновлено из чекпоинта» (#80); фронт без поля его игнорирует.
        progress(json!({
            "stage": "extract_audio",
            "msg": t!("analyze-audio-cached"),
            "resumed": true
        }));
    } else {
        emit(progress, "extract_audio", &t!("analyze-extracting-audio"));
        media::extract_wav_16k_mono(&paths.input, &vocals16)?;
        cache.write_stage(&paths.work_dir, "extract", &extract_key, &[&vocals16]);
        let _ = cache.save(&paths.work_dir);
    }

    // 3) диаризация. merge_gap=0.8, min_speaker_dur=2.5 — дефолты diarize.turns питона (asr.py-контракт).
    //    Если модель sortformer недоступна ИЛИ вернула <2 спикеров -> single-speaker (штатная ветка).
    //    ГЕЙТ. Два случая: (1) dub/auto — ПАРИТЕТ с питоном (pipeline.py:169 `cfg.dub and _per_spk`; голос по
    //    умолчанию clone -> _per_spk=true, поэтому диаризуем). (2) transcribe — НАМЕРЕННОЕ РАСШИРЕНИЕ порта:
    //    в питоне transcribe ставит cfg.dub=False и НЕ диаризует, но у нас это отдельный режим «транскрипт+
    //    диаризация» (спикеры показываются в UI), поэтому диаризуем. nodub (только субтитры) — НЕ диаризуем
    //    (как питон whole-clip; иначе diarize-first порезал бы субтитры по спикерам иначе, чем оригинал).
    // 2c) СЕПАРАЦИЯ ДО диаризации/ASR (best practices: чистый вокал вместо сырого микса — диаризация
    // не путается на музыке, ASR точнее; приказ 2026-07-17). stems-кэш ОБЩИЙ с рендером (wd/stems) —
    // рендер переиспользует, двойной сепарации нет. Fail-safe: сбой/нет движка -> сырой vocals16.
    let want_diar = wants_diarization(args);
    crate::jobs::check_cancelled()?;
    bench.stage("separate");
    if media::drop_stale_separation(&paths.work_dir)? {
        emit(progress, "separate", &t!("analyze-stems-stale"));
    }
    let asr_wav: std::path::PathBuf = if want_diar
        && paths.bsroformer_cli.is_file()
        && paths.bsroformer_model.is_file()
    {
        let clean16 = paths.work_dir.join("vocals16_clean.wav");
        let stems = paths.work_dir.join("stems");
        let cached_voc = stems.join("vocals.wav");
        let sep_result: Result<std::path::PathBuf, String> = (|| {
            if !cached_voc.is_file() {
                let audio_hq = paths.work_dir.join("audio_hq.wav");
                if !audio_hq.is_file() {
                    emit(progress, "separate", &t!("atomic-extract-separation-audio"));
                    media::extract_audio(&paths.input, &audio_hq, 44100, 2)?;
                }
                emit(progress, "separate", &t!("analyze-separating", model = "BSRoformer"));
                dub_sep::separate(&audio_hq, &stems, &paths.bsroformer_cli, &paths.bsroformer_model)
                    .map_err(|e| e.localize())?;
                media::mark_separation(&stems)?;
            } else {
                emit(progress, "separate", &t!("analyze-separation-cached"));
            }
            media::to_16k_mono(&cached_voc, &clean16)?;
            Ok(clean16.clone())
        })();
        match sep_result {
            Ok(p) => p,
            Err(e) => {
                emit(progress, "separate", &t!("analyze-separation-failed", error = e.to_string()));
                vocals16.clone()
            }
        }
    } else {
        if want_diar {
            emit(progress, "separate", &t!("analyze-separator-missing", model = "BSRoformer"));
        }
        vocals16.clone()
    };
    crate::jobs::check_cancelled()?;
    bench.stage("diarize");
    // Backend диаризации (onnx CUDA-EP / CPU) — exec_config читает DUB_ASR_BACKEND при создании сессии.
    std::env::set_var("DUB_ASR_BACKEND", crate::models::stage_backend(&paths.models_root, "diar_backend"));
    emit(progress, "diarize", &t!("analyze-diarizing", model = "Nemotron 3 Diarization"));
    // Вход диаризации и ASR — asr_wav (чистый вокал или сырое аудио): ключи стадий от его отпечатка.
    let asr_in_hash = cache::hash_file_prefix(&asr_wav);
    let voice_model = dub_faces::wespeaker_path(&paths.models_root);
    let diar_key = diar_cache_key(&asr_in_hash, &paths.sortformer_onnx, &voice_model, args.speaker_count)?;
    let diar = if args.speaker_count == 1 {
        let d = dub_asr::DiarTurns { turns: Vec::new(), n_speakers: 1, ref_windows: Default::default() };
        stage_store(&mut cache, &paths.work_dir, "diarize", &diar_key, DIAR_FILE, &DiarOut::of(&d), progress);
        Some(d)
    } else if want_diar && paths.sortformer_onnx.is_file() {
        if let Some(d) = stage_load::<DiarOut>(&cache, &paths.work_dir, "diarize", &diar_key, DIAR_FILE, progress) {
            crate::jobs::emit_resumed(progress, "diarize", &t!("analyze-diarization-cached"));
            Some(d.into_turns())
        } else {
            let result = if args.speaker_count > 0 {
                known_speaker_turns(&asr_wav, &paths.sortformer_onnx, &voice_model, args.speaker_count, progress)
            } else {
                dub_asr::turns(&asr_wav, &paths.sortformer_onnx, 0.8, 2.5).map_err(|e| e.localize())
            };
            match result {
                Ok(d) => {
                    stage_store(&mut cache, &paths.work_dir, "diarize", &diar_key, DIAR_FILE, &DiarOut::of(&d), progress);
                    Some(d)
                }
                Err(e) => {
                    if args.speaker_count > 0 {
                        return Err(t!("analyze-diarization-count-failed", error = e.to_string()));
                    }
                    emit(
                        progress,
                        "diarize",
                        &t!("analyze-diarization-failed", error = e.to_string()),
                    );
                    None
                }
            }
        }
    } else {
        if args.speaker_count > 1 {
            return Err(t!("analyze-diarization-model-missing-count", model = "Nemotron 3 Diarization"));
        }
        emit(
            progress,
            "diarize",
            &if !want_diar { t!("analyze-subs-no-diarization") } else { t!("analyze-diarization-model-missing") },
        );
        None
    };
    if args.speaker_count > 0 {
        let found = diar.as_ref().map_or(1, |d| d.n_speakers);
        emit(progress, "diarize", &if found < args.speaker_count {
            t!("analyze-fewer-speakers", expected = args.speaker_count, found = found)
        } else {
            t!("analyze-speakers-matched", speakers = found)
        });
    }
    // 4) сегменты: из импортированных субтитров (точный текст+тайминг, ASR пропущен) ЛИБО через ASR.
    //    Импорт: спикеров всё равно раздаём — по максимальному перекрытию реплики с диаризацией.
    //    Транскрипт (после слияния огрызков) — durable-выход стадии: ключ от входа ASR, движка, языка,
    //    источника реплик и диаризации (спикеры раздаются по ней).
    crate::jobs::check_cancelled()?;
    let diar_fp = match &diar {
        Some(d) => diarization_fingerprint(d, &diar_key, args.speaker_count)?,
        None => "none".to_string(),
    };
    let asr_source = match &paths.import_subs {
        Some(p) => format!(
            "import:{}:{}:{}",
            cache::hash_file_prefix(p),
            if args.import_translated { "tgt" } else { "src" },
            if args.align_subs { format!("aligned:{}", paths.asr.describe()) } else { "as-is".to_string() }
        ),
        None if crate::models::openrouter_asr_on(&paths.models_root) => {
            format!("cloud:{}", crate::models::openrouter_model(&paths.models_root, "asr"))
        }
        None => paths.asr.describe(),
    };
    let asr_key = cache::hash_stage(&[ASR_VER, &asr_in_hash, &asr_source, &args.src_lang, &diar_fp]);
    let cached_transcript =
        stage_load::<TranscriptOut>(&cache, &paths.work_dir, "asr", &asr_key, TRANSCRIPT_FILE, progress);
    let resumed_asr = cached_transcript.is_some();
    let (mut segments, n_spk): (Vec<Segment>, usize) = if let Some(t) = cached_transcript {
        crate::jobs::emit_resumed(progress, "asr", &t!("analyze-transcript-cached", segments = t.segments.len()));
        (t.segments, t.n_spk)
    } else if let Some(subs_path) = &paths.import_subs {
        let content = std::fs::read_to_string(subs_path)
            .map_err(|e| t!("analyze-read-subs", path = subs_path.display().to_string(), error = e.to_string()))?;
        let ext = subs_path.extension().and_then(|s| s.to_str()).unwrap_or("srt");
        let cues = crate::subimport::parse(&content, ext);
        if cues.is_empty() {
            return Err(t!("analyze-subs-empty", path = subs_path.display().to_string()));
        }
        let aligned = if args.align_subs {
            bench.stage("subalign");
            align_cues(args, paths, &asr_wav, &cues, progress)?
        } else {
            None
        };
        let turns: &[dub_asr::Turn] = diar.as_ref().map(|d| d.turns.as_slice()).unwrap_or(&[]);
        let segs: Vec<Segment> = cues
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let mut extra = serde_json::Map::new();
                let (start, end) = match aligned.as_ref().map(|a| &a.cues[i]) {
                    Some(p) => {
                        let how = match p.how {
                            crate::subalign::How::Aligned => "asr_aligned",
                            crate::subalign::How::Shifted => "asr_shifted",
                        };
                        extra.insert("timing".into(), Value::String(how.into()));
                        extra.insert("cue_start".into(), json!(c.start));
                        extra.insert("cue_end".into(), json!(c.end));
                        (p.start, p.end)
                    }
                    None => (c.start, c.end),
                };
                // Сабы УЖЕ на языке перевода -> текст в tgt (озвучка читает tgt), src пустой (оригинал не
                // транскрибировали). Иначе (сабы на языке оригинала) -> текст в src, перевод заполнит tgt.
                let (src_text, tgt_text) = if args.import_translated {
                    (String::new(), c.text)
                } else {
                    (c.text, String::new())
                };
                Segment {
                    id: format!("s{i}"),
                    start,
                    end,
                    speaker: Some(speaker_for(start, end, turns)),
                    src_text,
                    tgt_text,
                    voice: None,
                    dirty: false,
                    ckpt: None,
                    extra,
                }
            })
            .collect();
        let n = diar.as_ref().map(|d| d.n_speakers).unwrap_or(1).max(1);
        emit(progress, "asr", &t!("analyze-subs-imported", lines = segs.len(), speakers = n));
        (segs, n)
    } else if crate::models::openrouter_asr_on(&paths.models_root) {
        // Облачная транскрипция (OpenRouter STT): сегменты из облака, спикеров раздаём диаризацией (как
        // локальный ASR). Тяжёлые Parakeet/Whisper не нужны — облачный пресет для слабых ПК.
        bench.stage("asr");
        emit(progress, "asr", &t!("analyze-cloud-asr"));
        let raw = crate::cloud_asr::transcribe(&paths.models_root, &asr_wav, &args.src_lang)
            .map_err(|e| t!("analyze-cloud-stt-failed", error = e))?;
        let turns: &[dub_asr::Turn] = diar.as_ref().map(|d| d.turns.as_slice()).unwrap_or(&[]);
        let nsp = diar.as_ref().map(|d| d.n_speakers).unwrap_or(1).max(1);
        let segs: Vec<Segment> = raw
            .into_iter()
            .enumerate()
            .map(|(i, (st, en, tx))| Segment {
                id: format!("s{i}"),
                start: st,
                end: en,
                speaker: Some(if turns.is_empty() { "0".to_string() } else { speaker_for(st, en, turns) }),
                src_text: tx,
                tgt_text: String::new(),
                voice: None,
                dirty: false,
                ckpt: None,
                extra: Default::default(),
            })
            .collect();
        emit(progress, "asr", &t!("analyze-cloud-done", lines = segs.len(), speakers = nsp));
        (segs, nsp)
    } else {
        // Движок выбран настройкой (Parakeet/Whisper) — analyze не знает деталей (build_engine).
        bench.stage("asr");
        let parakeet = matches!(paths.asr, crate::models::AsrChoice::Parakeet(_));
        if parakeet && args.src_lang != "auto" && !crate::models::parakeet_knows(&args.src_lang) {
            return Err(t!("analyze-asr-language", lang = args.src_lang.clone()));
        }
        // Backend локального ASR (Parakeet onnx CUDA-EP/CPU, Whisper cuda/cpu) — до создания движка/сессии.
        std::env::set_var("DUB_ASR_BACKEND", crate::models::stage_backend(&paths.models_root, "asr_backend"));
        let mut asr = crate::models::build_engine(&paths.asr);
        let turns: &[dub_asr::Turn] = diar.as_ref().map(|d| d.turns.as_slice()).unwrap_or(&[]);
        let nsp = diar.as_ref().map(|d| d.n_speakers).unwrap_or(1).max(1);
        emit(
            progress,
            "asr",
            &t!("asr-windowed", speakers = nsp),
        );
        let ts = asr
            .transcribe(&asr_wav, &args.src_lang)
            .map_err(|e| e.localize())?;
        // Спикер — по словам: реплика, на которой сменился спикер диаризации, режется на слове смены.
        let segs: Vec<Segment> = dub_asr::split_at_speaker_turns(ts, turns)
            .into_iter()
            .enumerate()
            .map(|(i, (s, spk))| {
                let words: Vec<Value> = s
                    .words
                    .iter()
                    .map(|w| json!({ "word": w.word, "start": w.start, "end": w.end }))
                    .collect();
                let mut extra = serde_json::Map::new();
                extra.insert("words".into(), Value::Array(words));
                Segment {
                    id: format!("s{i}"),
                    start: s.start,
                    end: s.end,
                    speaker: Some(spk.to_string()),
                    src_text: s.text,
                    tgt_text: String::new(),
                    voice: None,
                    dirty: false,
                    ckpt: None,
                    extra,
                }
            })
            .collect();
        if parakeet && args.src_lang == "auto" {
            let clean = (asr_wav != vocals16).then_some(asr_wav.as_path());
            let evidence = crate::asr_filter::VoiceEvidence::build(clean, turns, &vocals16)
                .map_err(|e| t!("analyze-hallucination-filter", error = e.to_string()))?;
            let words: usize = segs
                .iter()
                .map(|s| s.extra.get("words").and_then(Value::as_array).map_or(0, Vec::len))
                .sum();
            if let Some(speech) = evidence.speech_seconds() {
                if unrecognised_language(speech, words) {
                    return Err(t!("analyze-asr-no-words", speech = speech.round() as i64, words = words as i64));
                }
            }
        }
        (segs, nsp)
    };

    // Галлюцинации ASR (титры субтитровщиков, «Продолжение следует», звуки в скобках): текстовое правило
    // находит кандидатов, звук на интервале (отделённый вокал, реплики диаризации или сырой микс) решает
    // «скрыть» или «только пометить». Импорт субтитров — текст пользователя, его не трогаем.
    if paths.import_subs.is_none() {
        let rules = match (&paths.asr, crate::models::openrouter_asr_on(&paths.models_root)) {
            (crate::models::AsrChoice::Whisper { .. }, false) => dub_asr::HallucinationRules::Whisper,
            _ => dub_asr::HallucinationRules::CaseAware,
        };
        let clean = (asr_wav != vocals16).then_some(asr_wav.as_path());
        let turns: &[dub_asr::Turn] = diar.as_ref().map(|d| d.turns.as_slice()).unwrap_or(&[]);
        let evidence = crate::asr_filter::VoiceEvidence::build(clean, turns, &vocals16)
            .map_err(|e| t!("analyze-hallucination-filter", error = e.to_string()))?;
        let report = crate::asr_filter::apply(&mut segments, rules, &evidence);
        if !report.hidden.is_empty() {
            emit(progress, "asr", &t!(
                "analyze-hidden-hallucinations",
                count = report.hidden.len(),
                lines = quote_list(&report.hidden)
            ));
        }
        if !report.hidden_by_text.is_empty() {
            emit(progress, "asr", &t!(
                "analyze-hidden-by-text",
                count = report.hidden_by_text.len(),
                lines = quote_list(&report.hidden_by_text)
            ));
        }
        if !report.voiced.is_empty() {
            emit(progress, "asr", &t!(
                "analyze-voiced-suspects",
                count = report.voiced.len(),
                lines = quote_list(&report.voiced)
            ));
        }
    }

    // Слияние коротких огрызков (#115): whisper режет «If they find you» на «If» + «they find you.» —
    // каждый огрызок TTS-ится отдельно и звучит рвано. Клеим near-continuous короткие реплики ОДНОГО
    // спикера в одну фразу (не для import_subs — там реплики уже цельные из сабов).
    if !resumed_asr {
        if paths.import_subs.is_none() {
            let before = segments.len();
            merge_short_turns(&mut segments);
            if segments.len() != before {
                emit(progress, "asr", &t!("analyze-merged-fragments", before = before, after = segments.len()));
            }
        }
        let out = TranscriptOut { segments: segments.clone(), n_spk };
        stage_store(&mut cache, &paths.work_dir, "asr", &asr_key, TRANSCRIPT_FILE, &out, progress);
    }

    // Голосовая переразметка спикеров (#115): при кастинге разворачиваем ≤4 Sortformer-спикеров в реальных
    // персонажей по голосу и ПЕРЕРАЗМЕЧАЕМ сегменты (метки «v{k}») -> дубляж по N голосам + верный счётчик
    // реплик у персонажей (0 реплик больше не бывает). Не для import_subs. Гейт по n_spk СНЯТ: Sortformer
    // часто схлопывает многоголосый клип в 1 спикера (шум/моно) — именно этот случай фича и чинит;
    // внутренний guard recluster (k<=1 || k<=orig) не трогает истинно-односпикерные.
    if args.casting && paths.import_subs.is_none() {
        let k = crate::casting::recluster_segments(paths, &mut segments, progress);
        if k > 0 {
            emit(progress, "asr", &t!("analyze-characters-by-voice", count = k));
        }
    }

    emit(
        progress,
        "asr",
        &t!("analyze-segments-speakers", segments = segments.len(), speakers = n_spk),
    );

    // Глоссарий проекта (из прежнего project.json) и профиля сериала: ошибки распознавания терминов (asr_fix)
    // исправляются до перевода — импортированные субтитры (текст пользователя) не трогаем.
    let prev_proj: Option<Project> = match std::fs::read_to_string(paths.work_dir.join("project.json")) {
        Ok(text) => Some(Project::from_json(&text).map_err(|e| {
            t!("analyze-project-unparsable", error = e.to_string())
        })?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(t!("common-read", path = "project.json", error = e.to_string())),
    };
    let glossary = crate::glossary_api::for_analyze(&paths.repo_root, prev_proj.as_ref(), &args.casting_ref, progress)?;
    if paths.import_subs.is_none() && !glossary.is_empty() {
        let mut fixed = 0usize;
        for s in &mut segments {
            let (text, n) = dub_core::glossary::apply_asr_fix(&s.src_text, &glossary);
            if n > 0 {
                s.src_text = text;
                fixed += n;
            }
        }
        if fixed > 0 {
            emit(progress, "asr", &t!("analyze-glossary-fixed", count = fixed));
        }
    }
    let glossary_fp = crate::glossary_api::fingerprint(&glossary, &args.tgt_lang);

    // Отпечаток транскрипта (вход перевода): хэш всех src-текстов+спикеров по порядку. Правка src
    // инвалидирует translate; правка tgt (ручная в редакторе) — нет (её ключ в render, не здесь).
    let transcript_fp = {
        let mut h = blake3::Hasher::new();
        for s in &segments {
            h.update(s.src_text.as_bytes());
            h.update(b"\x1f");
            h.update(s.speaker.as_deref().unwrap_or("").as_bytes());
            h.update(b"\x1e");
        }
        h.finalize().to_hex().to_string()
    };

    // 5) собрать Project по контракту dub-core (mode-дефолты).
    let (mut mode, subs_mode) = resolve_modes(args);
    // auto -> nodub гейт (порт pipeline.py:124-129): режим auto дублирует ТОЛЬКО при реальной речи;
    // музыкальный/иноязычный клип (пустой транскрипт ИЛИ галлюцинация-повтор) -> nodub (оставить
    // оригинальную дорожку, локализовать лишь экранный текст). Пустые сегменты -> тоже nodub.
    let audible: Vec<Segment> = segments.iter().filter(|s| !crate::asr_filter::is_hidden(s)).cloned().collect();
    if args.mode == "auto" && (audible.is_empty() || !has_speech(&audible, meta.duration)) {
        mode = "nodub".to_string();
        emit(
            progress,
            "asr",
            &if audible.is_empty() {
                t!("analyze-no-speech-nodub")
            } else {
                t!("analyze-auto-nodub")
            },
        );
    }
    let mut proj = Project::default();
    // Стиль перевода (#112): ОСНОВНОЙ путь — параметр analyze (со стартового экрана; patch ДО analyze
    // невозможен — project.json ещё не создан). Фолбэк — перенос из существующего project.json, чтобы
    // стиль, выставленный в редакторе, пережил ре-анализ (там project.json уже есть).
    if !args.translate_style.is_empty() {
        proj.audio.translate_style = args.translate_style.clone();
    } else if let Ok(text) = std::fs::read_to_string(paths.work_dir.join("project.json")) {
        if let Ok(prev) = Project::from_json(&text) {
            proj.audio.translate_style = prev.audio.translate_style;
        }
    }
    // #115 ИНЖЕКЦИЯ ОПИСАНИЙ ПЕРСОНАЖЕЙ: при casting_ref подмешиваем speech_note профиля в translate_style
    // ДО перевода — Gemma переводит реплики с учётом манеры речи/характера каждого персонажа. Профиль
    // грузим по slug из app-библиотеки. Дополняет (не затирает) явный стиль перевода. Гейтим по args.casting
    // тоже — иначе casting_ref без включённого кастинга протёк бы описания в промпт без применения профиля.
    if args.casting && !args.casting_ref.trim().is_empty() {
        if let Some(prof) = crate::casting_library::load_profile_casting(&paths.repo_root, args.casting_ref.trim()) {
            let notes = crate::casting::speech_notes_to_style(&prof);
            if !notes.is_empty() {
                // Идемпотентно (единый маркер-блок): ре-анализ не копит дубли — см. merge_speech_notes_style.
                proj.audio.translate_style =
                    crate::casting::merge_speech_notes_style(&proj.audio.translate_style, &notes);
                emit(progress, "translate", &t!(
                    "analyze-casting-style",
                    chars = proj.audio.translate_style.len()));
            }
        }
    }
    proj.meta = Meta {
        video: paths.input.to_string_lossy().into_owned(),
        duration: meta.duration,
        width: meta.width,
        height: meta.height,
        fps: meta.fps,
        src_codec: meta.src_codec,
        extra: Default::default(),
    };
    proj.mode = mode;
    proj.tgt_lang = args.tgt_lang.clone();
    // Язык оригинала (для метки 2-й дорожки при keep_original_track). "auto" не детектится типизированно —
    // храним как есть; render маппит непустой не-auto код через iso639, иначе "und". Инвариант extra=allow.
    proj.meta.extra.insert("src_lang".into(), Value::String(args.src_lang.clone()));
    proj.meta.extra.insert("speaker_count".into(), json!(args.speaker_count));
    proj.subs.mode = subs_mode;
    proj.subs.burn = args.burn; // композируемость: вжигать субтитры/титры или нет
    proj.glossary = glossary;
    proj.casting_ref = match args.casting_ref.trim() {
        "" => prev_proj.as_ref().map(|p| p.casting_ref.clone()).unwrap_or_default(),
        slug => slug.to_string(),
    };
    let before = segments.len();
    proj.segments = segments
        .into_iter()
        .filter(|s| has_speech_text(if args.import_translated { &s.tgt_text } else { &s.src_text }))
        .collect();
    if proj.segments.len() < before {
        emit(progress, "asr", &t!("analyze-empty-removed", count = before - proj.segments.len()));
    }
    proj.work_dir = Some(paths.work_dir.to_string_lossy().into_owned());
    if !args.rewrite.is_empty() {
        proj.audio.rewrite = Some(args.rewrite.clone());
    }

    // 6) стадии translate + vision (раунд 3). Порт pipeline._build_dub translate-ветки: если есть
    //    что переводить, поднимаем llama-server (сайдкар Gemma+mmproj), гоним единый ctx-проход
    //    (vision layout/scene + перевод всего транскрипта), заполняем tgt_text/titles/sub_style.
    //    Пайплайн последовательный: TTS в этот момент не загружен (как tts.release() в питоне) —
    //    Gemma получает всю VRAM. Перевод, который нужен режиму, но не выполнен, — ошибка анализа:
    //    ролик не должен озвучиться на исходном языке под видом дубляжа.
    // vocals16 уже объявлен выше (стадия ASR) и не перемещался — переиспользуем.
    crate::jobs::check_cancelled()?;
    bench.stage("translate");
    let llm_tag = format!("{}|{}", provider_tag(paths, "llm"), provider_tag(paths, "vision"));
    let translate_key = cache::hash_stage(&[
        TRANSLATE_VER,
        &transcript_fp,
        &proj.tgt_lang,
        &proj.mode,
        &proj.subs.mode,
        if proj.subs.burn { "burn1" } else { "burn0" },
        &args.rewrite,
        if args.import_translated { "imp1" } else { "imp0" },
        &args.src_lang,
        &proj.audio.translate_style,
        if args.casting && args.content_type == "auto" { "ct-auto" } else { "" },
        &llm_tag,
        &glossary_fp,
    ]);
    if let Some(t) =
        stage_load::<TranslatedOut>(&cache, &paths.work_dir, "translate", &translate_key, TRANSLATED_FILE, progress)
    {
        t.apply(&mut proj);
        crate::jobs::emit_resumed(progress, "translate", &t!("analyze-translation-cached"));
    } else {
        crate::translate::stage(args, paths, &mut proj, &asr_wav, meta.height, meta.duration, progress)?;
        let out = TranslatedOut::of(&proj);
        stage_store(&mut cache, &paths.work_dir, "translate", &translate_key, TRANSLATED_FILE, &out, progress);
    }
    proj.glossary_fp = glossary_fp;

    // 7) OCR-стадия (раунд 4): детекция вшитого текста -> блюр-боксы субтитр-полосы + уточнение sub_y.
    //    Порт pipeline.run ocr_detect + compose.analyze_layout. Fail-safe: сбой OCR не валит analyze
    //    (боксы блюра — не блокер; их можно добавить руками в редакторе). Дорогая стадия (минуты на 4K):
    //    пропускаем по галочке detect_text (в режиме без субтитров вшитый текст не трогаем — экономит время).
    crate::jobs::check_cancelled()?;
    bench.stage("ocr");
    if args.detect_text && meta.width > 0 && meta.height > 0 {
        // Кэшируется дорогая детекция (кадры + OCR), композит по ней пересчитывается всегда: он зависит
        // от перевода и раскладки, а не только от видео.
        let ocr_models = dub_ocr::OcrPaths::under(&paths.models_root);
        let ocr_key = cache::hash_stage(&[
            OCR_VER,
            &src_hash,
            &paths.caption_fps.to_string(),
            &file_tag(&ocr_models.det),
            &file_tag(&ocr_models.cls),
            &file_tag(&ocr_models.rec),
        ]);
        let detection = match stage_load::<OcrOut>(&cache, &paths.work_dir, "ocr", &ocr_key, OCR_FILE, progress) {
            Some(o) => {
                crate::jobs::emit_resumed(progress, "ocr_detect", &t!("analyze-ocr-cached"));
                Some(o.into_parts())
            }
            None => match crate::ocr::detect(paths, progress) {
                Ok((regions, raw)) => {
                    let out = OcrOut::of(&regions, &raw);
                    stage_store(&mut cache, &paths.work_dir, "ocr", &ocr_key, OCR_FILE, &out, progress);
                    Some((regions, raw))
                }
                Err(e) => {
                    emit(progress, "ocr_detect", &t!("ocr-no-blur", error = e));
                    None
                }
            },
        };
        if let Some((regions, raw)) = detection {
            crate::ocr::compose_captions(
                args, paths, &mut proj, &regions, &raw, meta.width, meta.height, meta.duration, progress,
            );
        }
    } else if meta.width == 0 || meta.height == 0 {
        emit(progress, "ocr_detect", &t!("analyze-audio-no-ocr"));
    } else {
        emit(progress, "ocr_detect", &t!("analyze-ocr-off"));
    }

    // 7b) КАСТИНГ ПЕРСОНАЖЕЙ (#115). ГЕЙТ: только при галочке casting И наличии видео. При выкл — НИ
    //     ОДНОГО кадра не сканируется (ноль оверхеда, как OCR). Ставится ПОСЛЕ ASR/диаризации (нужны
    //     спикеры+тайминги) и перевода. Fail-safe: сбой не валит analyze (кастинг — не блокер).
    proj.casting_enabled = args.casting;
    crate::jobs::check_cancelled()?;
    bench.stage("casting");
    if args.casting && meta.width > 0 && meta.height > 0 {
        // content_type="auto" -> берём тип, определённый Gemma в translate-стадии (proj.audio.content_type).
        // Если translate пропущен ранним return (same-lang/transcribe/нет LLM) — тип пуст: классифицируем
        // АВТОНОМНО здесь (иначе анимация молча пойдёт через "real"-детектор). Только тогда, ноль оверхеда
        // если тип уже определён. Не удалось — честный дефолт "real".
        let eff_ct = if args.content_type == "auto" {
            let mut d = proj.audio.content_type.clone();
            if d.is_empty() {
                if let Some(ct) = crate::translate::classify_content_type_standalone(paths, meta.duration, progress) {
                    emit(progress, "casting", &t!("analyze-content-type", kind = ct.clone()));
                    d = ct;
                }
            }
            if d.is_empty() { "real".to_string() } else { d }
        } else {
            args.content_type.clone()
        };
        // casting.json — durable-выход стадии. Регистрируется, только если стадия его реально переписала
        // (сбой стадии оставляет прежний файл, и выдавать его за результат этих параметров нельзя).
        let segs_fp = {
            let mut h = blake3::Hasher::new();
            for s in &proj.segments {
                h.update(format!("{}|{}|{:.2}|{:.2}\x1e", s.id, s.speaker.as_deref().unwrap_or(""), s.start, s.end).as_bytes());
            }
            h.finalize().to_hex().to_string()
        };
        let cast_key = cache::hash_stage(&[
            CAST_VER,
            &src_hash,
            &segs_fp,
            &eff_ct,
            args.casting_ref.trim(),
            &crate::casting::models_fingerprint(&paths.models_root, eff_ct.eq_ignore_ascii_case("anime")),
        ]);
        let cast_file = paths.work_dir.join(CASTING_FILE);
        if cache.stage_cached(&paths.work_dir, "casting", &cast_key) {
            crate::jobs::emit_resumed(progress, "cast_detect", &t!("analyze-casting-cached"));
        } else {
            let before = mtime(&cast_file);
            crate::casting::stage(paths, &proj, &args.casting_ref, &eff_ct, progress);
            if cast_file.is_file() && mtime(&cast_file) != before {
                cache.write_stage(&paths.work_dir, "casting", &cast_key, &[&cast_file]);
                if let Err(e) = cache.save(&paths.work_dir) {
                    emit(progress, "cast_speaker", &t!("analyze-casting-checkpoint", error = e));
                }
            }
        }

        // #115 АВТО-ПРИМЕНЕНИЕ ПЕРЕНЕСЁННЫХ ГОЛОСОВ: при casting_ref кросс-матч проставил персонажам
        // dub_voice из профиля (в casting.json). Строим позиционный voice-CSV по спикерам ЭТОЙ серии и
        // кладём в proj.audio.voice -> РЕНДЕР сразу озвучит правильными голосами, без ручного «Применить».
        if !args.casting_ref.trim().is_empty() {
            if let Some(casting) = dub_faces::load_casting(&paths.work_dir.join("casting.json")) {
                let mut vmap = crate::casting::casting_voice_map(&casting);
                // Перенесённый из профиля голос может отсутствовать в voices/ ЭТОЙ установки (профиль собран
                // на другой машине). Без проверки рендер молча свалится на клон. Отсекаем несуществующие ->
                // клон + явное предупреждение (а не тихая подмена).
                let voices_dir = std::env::var("DUBENGINE_VOICES")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| paths.repo_root.join("voices"));
                let mut missing: Vec<String> = Vec::new();
                for v in vmap.values_mut() {
                    if let Some(nm) = v.clone() {
                        let exists = ["wav", "mp3"].iter().any(|e| voices_dir.join(format!("{nm}.{e}")).is_file());
                        if !exists {
                            missing.push(nm);
                            *v = None;
                        }
                    }
                }
                if !missing.is_empty() {
                    missing.sort();
                    missing.dedup();
                    emit(progress, "casting", &t!(
                        "analyze-profile-voices-missing", voices = missing.join(", ")));
                }
                if vmap.values().any(|v| v.is_some()) {
                    let mut spk_ids: Vec<String> = proj
                        .segments
                        .iter()
                        .map(|s| s.speaker.clone().unwrap_or_else(|| "0".to_string()))
                        .collect();
                    spk_ids.sort();
                    spk_ids.dedup();
                    let old_name = proj.audio.voice.name.clone();
                    let old_is_voice = proj.audio.voice.mode == "voice";
                    let csv = crate::casting::merge_voice_csv(&spk_ids, &vmap, old_name.as_deref(), old_is_voice);
                    proj.audio.voice.mode = "voice".to_string();
                    proj.audio.voice.name = Some(csv);
                    // Считаем по ОТФИЛЬТРОВАННОМУ vmap (не по casting.characters) — иначе персонажи с
                    // отсутствующим в voices/ голосом (сброшены в клон выше) попадают в счётчик.
                    let n = vmap.values().filter(|v| v.is_some()).count();
                    emit(progress, "casting", &t!("analyze-profile-voices-applied", count = n));
                }
            }
        }
    } else if args.casting {
        emit(progress, "casting", &t!("analyze-audio-no-casting"));
    }

    // Реплики новые: истории дублей прежних реплик к ним не относятся.
    crate::takes::drop_all(&paths.work_dir)?;

    // 8) финализация кэша: зеркалим param-ключи крупных стадий в project.stage_ckpts (resume работает
    //    даже при потере work_dir/cache.json, т.к. project.json автосейвится), и атомарно пишем cache.json.
    for (stage, entry) in &cache.stages {
        proj.stage_ckpts.insert(stage.clone(), Value::String(entry.param_hash.clone()));
    }
    if let Err(e) = cache.save(&paths.work_dir) {
        emit(progress, "cache", &t!("analyze-cache-not-saved", error = e));
    }
    bench.finish(|m| emit(progress, "bench", m));

    Ok(proj)
}

#[cfg(test)]
mod speech_text_tests {
    use super::has_speech_text;

    #[test]
    fn punctuation_only_segment_is_not_speech() {
        assert!(!has_speech_text("."));
        assert!(!has_speech_text(" … — "));
        assert!(has_speech_text("Где твоя форма?"));
        assert!(has_speech_text("42"));
        assert!(has_speech_text("好"));
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dub_analyze_resume_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn quiet() -> impl Fn(Value) + Send + Sync {
        |_: Value| {}
    }

    #[test]
    fn stage_with_matching_key_is_loaded_not_recomputed() {
        let wd = tmp_dir("hit");
        let cb = quiet();
        let mut cache = StageCache::default();
        let d = dub_asr::DiarTurns {
            turns: vec![dub_asr::Turn { start: 0.0, end: 1.5, speaker: 1 }],
            n_speakers: 2,
            ref_windows: [(1, (0.0, 1.5))].into_iter().collect(),
        };
        stage_store(&mut cache, &wd, "diarize", "k1", DIAR_FILE, &DiarOut::of(&d), &cb);
        // Новый прогон читает cache.json с диска, как после рестарта.
        let cache = StageCache::load(&wd);
        let back = stage_load::<DiarOut>(&cache, &wd, "diarize", "k1", DIAR_FILE, &cb).expect("кэш стадии");
        let t = back.into_turns();
        assert_eq!(t.n_speakers, 2);
        assert_eq!(t.turns[0].speaker, 1);
        assert_eq!(t.ref_windows.get(&1), Some(&(0.0, 1.5)));
        // Другие параметры -> стадия считается заново.
        assert!(stage_load::<DiarOut>(&cache, &wd, "diarize", "k2", DIAR_FILE, &cb).is_none());
        // Файл выхода удалён -> тоже пересчёт.
        std::fs::remove_file(wd.join(DIAR_FILE)).unwrap();
        assert!(stage_load::<DiarOut>(&cache, &wd, "diarize", "k1", DIAR_FILE, &cb).is_none());
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn cached_translation_restores_what_the_stage_wrote() {
        let wd = tmp_dir("translate");
        let cb = quiet();
        let mut p = Project {
            segments: vec![
                Segment { id: "s0".into(), src_text: "hi".into(), tgt_text: "привет".into(), ..Default::default() },
                Segment { id: "s1".into(), src_text: "bye".into(), tgt_text: "пока".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        p.raw_ctx.insert("titles".into(), json!([{"text": "A", "tgt": "Б"}]));
        p.captions.sub_y = Some(900);
        p.audio.content_type = "anime".into();
        let mut cache = StageCache::default();
        stage_store(&mut cache, &wd, "translate", "tk", TRANSLATED_FILE, &TranslatedOut::of(&p), &cb);

        let mut fresh = Project {
            segments: p.segments.iter().map(|s| Segment { tgt_text: String::new(), ..s.clone() }).collect(),
            ..Default::default()
        };
        stage_load::<TranslatedOut>(&StageCache::load(&wd), &wd, "translate", "tk", TRANSLATED_FILE, &cb)
            .expect("кэш перевода")
            .apply(&mut fresh);
        assert_eq!(fresh.segments[1].tgt_text, "пока");
        assert_eq!(fresh.raw_ctx["titles"][0]["tgt"], "Б");
        assert_eq!(fresh.captions.sub_y, Some(900));
        assert_eq!(fresh.audio.content_type, "anime");
        let _ = std::fs::remove_dir_all(&wd);
    }

    #[test]
    fn translate_key_follows_the_provider_of_each_stage() {
        let root = tmp_dir("provider");
        let p = std::path::PathBuf::new;
        let paths = AnalyzePaths {
            input: p(),
            work_dir: p(),
            repo_root: p(),
            asr: crate::models::AsrChoice::Parakeet(p()),
            sortformer_onnx: p(),
            llama_bin: p(),
            mt_model: root.join("gemma.gguf"),
            mmproj: root.join("mmproj.gguf"),
            models_root: root.clone(),
            caption_fps: 1,
            import_subs: None,
            bsroformer_cli: p(),
            bsroformer_model: p(),
        };
        let local = provider_tag(&paths, "llm");
        assert!(local.starts_with("local:"));
        assert_ne!(provider_tag(&paths, "vision"), local);

        crate::models::set_selection(&root, "llm_provider", "server").unwrap();
        crate::models::set_selection(&root, "srv_llm", "qwen3").unwrap();
        let server = provider_tag(&paths, "llm");
        assert!(server.starts_with("server:") && server.ends_with(":qwen3"));
        crate::models::set_selection(&root, "srv_url", "http://192.168.1.5:1234/v1").unwrap();
        assert_ne!(provider_tag(&paths, "llm"), server);

        crate::models::set_selection(&root, "vision_provider", "openrouter").unwrap();
        crate::models::set_selection(&root, "or_llm", "google/gemma-4").unwrap();
        assert_eq!(provider_tag(&paths, "vision"), "openrouter:google/gemma-4");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ocr_detection_roundtrips() {
        let regions = vec![dub_ocr::Region { text: "TITLE".into(), x: 1, y: 2, w: 30, h: 10, t0: 0.5, t1: 2.0 }];
        let raw = vec![dub_ocr::RawDet { text: "TITLE".into(), x: 1.0, y: 2.0, w: 30.0, h: 10.0, t: 0.5 }];
        let body = serde_json::to_string(&OcrOut::of(&regions, &raw)).unwrap();
        let (r, d) = serde_json::from_str::<OcrOut>(&body).unwrap().into_parts();
        assert_eq!(r[0].text, "TITLE");
        assert_eq!((r[0].w, r[0].t1), (30, 2.0));
        assert_eq!((d[0].h, d[0].t), (10.0, 0.5));
    }
}

#[cfg(test)]
mod segment_rules_tests {
    use super::*;

    fn seg(id: &str, start: f64, end: f64, spk: &str, text: &str) -> Segment {
        Segment { id: id.into(), start, end, speaker: Some(spk.into()), src_text: text.into(), ..Default::default() }
    }

    fn texts(v: &[Segment]) -> Vec<&str> {
        v.iter().map(|s| s.src_text.as_str()).collect()
    }

    #[test]
    fn a_short_english_piece_joins_the_next_line_of_its_speaker() {
        let mut v = vec![seg("s0", 0.0, 0.3, "0", "If"), seg("s1", 0.5, 1.9, "0", "they find you.")];
        merge_short_turns(&mut v);
        assert_eq!(texts(&v), ["If they find you."]);
        assert_eq!((v[0].start, v[0].end), (0.0, 1.9));
    }

    #[test]
    fn a_russian_one_word_line_joins_even_when_it_is_long() {
        let mut v = vec![seg("s0", 0.0, 1.8, "1", "Ну-у-у"), seg("s1", 2.0, 4.5, "1", "ладно, пойдём.")];
        merge_short_turns(&mut v);
        assert_eq!(texts(&v), ["Ну-у-у ладно, пойдём."]);
    }

    #[test]
    fn a_german_piece_of_another_speaker_stays_apart() {
        let mut v = vec![seg("s0", 0.0, 0.4, "0", "Ja"), seg("s1", 0.5, 2.0, "1", "das geht nicht.")];
        merge_short_turns(&mut v);
        assert_eq!(texts(&v), ["Ja", "das geht nicht."]);
    }

    #[test]
    fn a_join_never_goes_over_the_line_cap_or_across_a_pause() {
        let mut long = vec![seg("s0", 0.0, 0.5, "0", "Hey"), seg("s1", 0.6, 8.4, "0", "a very long line that runs past the cap")];
        merge_short_turns(&mut long);
        assert_eq!(long.len(), 2, "итог длиннее SEG_MAX_DUR");
        let mut pause = vec![seg("s0", 0.0, 0.5, "0", "Hey"), seg("s1", 1.0, 2.0, "0", "you there")];
        merge_short_turns(&mut pause);
        assert_eq!(pause.len(), 2, "пауза 0.5 с — это две фразы");
    }

    fn args(mode: &str, subs: &str) -> AnalyzeArgs {
        AnalyzeArgs {
            speaker_count: 0,
            tgt_lang: "ru".into(),
            mode: mode.into(),
            src_lang: "auto".into(),
            subs: subs.into(),
            rewrite: String::new(),
            translate_style: String::new(),
            burn: true,
            detect_text: false,
            casting: false,
            casting_ref: String::new(),
            content_type: "auto".into(),
            import_translated: false,
            align_subs: false,
        }
    }

    #[test]
    fn speaker_count_invalidates_diarization_but_auto_keeps_the_old_key() {
        let detector = std::path::Path::new("missing_detector.onnx");
        let dir = tempfile::tempdir().unwrap();
        let voice = dir.path().join("voice.onnx");
        std::fs::write(&voice, b"model").unwrap();
        assert_eq!(diar_cache_key("audio", detector, &voice, 0).unwrap(), cache::hash_stage(&[DIAR_VER, "audio", &file_tag(detector)]));
        assert_ne!(diar_cache_key("audio", detector, &voice, 8).unwrap(), diar_cache_key("audio", detector, &voice, 4).unwrap());
        let mut options = args("nodub", "transcribe");
        assert!(!wants_diarization(&options));
        options.speaker_count = 8;
        assert!(wants_diarization(&options));
        options.speaker_count = 1;
        assert!(!wants_diarization(&options));
    }

    #[test]
    fn middle_of_voice_model_invalidates_diarization_and_asr() {
        use std::io::{Seek, SeekFrom, Write};
        let dir = tempfile::tempdir().unwrap();
        let voice = dir.path().join("voice.onnx");
        let detector = dir.path().join("detector.onnx");
        let file = std::fs::File::create(&voice).unwrap();
        file.set_len(16 * 1024 * 1024 + 1).unwrap();
        drop(file);
        let first = diar_cache_key("audio", &detector, &voice, 8).unwrap();
        let prefix = cache::hash_file_prefix(&voice);
        let diar = dub_asr::DiarTurns { turns: Vec::new(), n_speakers: 0, ref_windows: Default::default() };
        let first_asr = diarization_fingerprint(&diar, &first, 8).unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).open(&voice).unwrap();
        file.seek(SeekFrom::Start(8 * 1024 * 1024)).unwrap();
        file.write_all(&[1]).unwrap();
        drop(file);
        assert_eq!(prefix, cache::hash_file_prefix(&voice));
        let second = diar_cache_key("audio", &detector, &voice, 8).unwrap();
        assert_ne!(first, second);
        assert_ne!(first_asr, diarization_fingerprint(&diar, &second, 8).unwrap());
        assert!(diar_cache_key("audio", &detector, &dir.path().join("missing.onnx"), 8).is_err());
    }

    #[test]
    fn voice_model_change_invalidates_diarization_and_asr_even_with_identical_turns() {
        let dir = tempfile::tempdir().unwrap();
        let detector = dir.path().join("detector.onnx");
        let voice = dir.path().join("voice.onnx");
        std::fs::write(&voice, b"model-a").unwrap();
        let first = diar_cache_key("audio", &detector, &voice, 8).unwrap();
        let diar = dub_asr::DiarTurns { turns: vec![dub_asr::Turn { start: 0.0, end: 1.0, speaker: 0 }], n_speakers: 1, ref_windows: Default::default() };
        let first_asr = diarization_fingerprint(&diar, &first, 8).unwrap();
        std::fs::write(&voice, b"model-b").unwrap();
        let second = diar_cache_key("audio", &detector, &voice, 8).unwrap();
        assert_ne!(first, second);
        assert_ne!(first_asr, diarization_fingerprint(&diar, &second, 8).unwrap());
        let fewer = diar_cache_key("audio", &detector, &voice, 4).unwrap();
        assert_ne!(diarization_fingerprint(&diar, &second, 8).unwrap(), diarization_fingerprint(&diar, &fewer, 4).unwrap());
    }

    #[test]
    fn bilingual_subtitles_are_kept_for_a_dub_and_the_original_is_kept_for_a_transcript() {
        assert_eq!(resolve_modes(&args("dub", "bilingual")), ("dub".into(), "bilingual".into()));
        assert_eq!(resolve_modes(&args("voiceover", "bilingual")).1, "bilingual");
        assert_eq!(resolve_modes(&args("nodub", "bilingual")).1, "bilingual");
        assert_eq!(resolve_modes(&args("dub", "transcribe")).1, "transcribe");
        assert_eq!(resolve_modes(&args("transcribe", "bilingual")).1, "transcribe");
        assert_eq!(resolve_modes(&args("auto", "auto")).1, "translate");
    }
}

#[cfg(test)]
mod language_check_tests {
    use super::unrecognised_language;

    #[test]
    fn minutes_of_speech_with_a_stray_word_is_a_language_parakeet_does_not_know() {
        assert!(unrecognised_language(92.0, 1));
    }

    #[test]
    fn ordinary_speech_and_short_clips_pass() {
        assert!(!unrecognised_language(92.0, 210));
        assert!(!unrecognised_language(10.0, 0));
        assert!(!unrecognised_language(30.0, 9));
    }
}
