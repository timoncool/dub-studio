//! Где фраза дубляжа реально звучит: после cursor-укладки, подгонки темпа, QC-пересинтеза и tempo-fit
//! всей дорожки фраза часто лежит не там, где реплика оригинала. `dub_timing.json` в каталоге проекта
//! хранит на каждую озвученную реплику начало и длительность в финальной дорожке и (для пословных
//! пресетов субтитров) услышанные в самом дубле слова. build_ass берёт отсюда тайминги событий и
//! подсветку слов в режимах dub/voiceover; устаревшая запись (текст или тайминг реплики правились после
//! сборки дубляжа) не используется.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "dub_timing.json";
const WORDS_CACHE: &str = "dub_words.json";

/// Одна озвученная реплика в финальной дорожке.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SegTiming {
    /// Показанный tgt-текст фразы на момент сборки (свежесть записи), не текст для синтеза.
    pub text: String,
    /// Тайминг реплики в проекте на момент сборки (свежесть записи).
    pub seg_start: f64,
    pub seg_end: f64,
    /// Начало фразы в финальной дорожке, сек.
    pub at: f64,
    /// Длительность фразы в финальной дорожке, сек.
    pub dur: f64,
    /// Услышанные в дубле слова (текст, начало, конец; секунды финальной дорожки). Пусто — не
    /// распознавались (пресет без пословной подсветки) или распознавание не удалось.
    #[serde(default)]
    pub words: Vec<(String, f64, f64)>,
    /// Как фраза уложилась в слот: отчёт «не влезло» и выборка для калибровки темпа голоса.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<FitRecord>,
}

/// Укладка озвученной фразы: сырой клип TTS до подгонки темпа против её слота.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitRecord {
    /// Длительность сырого клипа TTS, сек.
    pub raw: f64,
    /// Слот, в который фраза укладывалась (dub_core::fit::Slot::target), сек.
    pub slot: f64,
    /// Во сколько раз клип длиннее слота.
    pub needed: f64,
    /// Штатный предел ускорения слота.
    pub cap: f64,
    /// Предел, с которым рендер подогнал клип (с эскалацией).
    pub eff_cap: f64,
    pub speaker: String,
    /// Голос: clone, имя голоса из библиотеки или облачный голос.
    pub voice: String,
    /// Язык озвучки.
    pub lang: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DubTiming {
    /// id сегмента -> тайминг.
    pub segments: BTreeMap<String, SegTiming>,
}

impl DubTiming {
    /// Прочитать из каталога проекта. Нет файла — None (дубляж ещё не собирался).
    pub fn load(dir: &Path) -> Result<Option<Self>, String> {
        let p = dir.join(FILE);
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).map(Some).map_err(|e| format!("{}: {e}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", p.display())),
        }
    }

    fn save(&self, dir: &Path) -> Result<(), String> {
        write_json_atomic(&dir.join(FILE), self)
    }

    /// Тайминг реплики, если он записан для её ТЕКУЩЕГО текста и тайминга.
    pub fn fresh(&self, s: &dub_core::Segment) -> Option<&SegTiming> {
        self.segments.get(&s.id).filter(|t| {
            t.text == s.tgt_text.trim() && (t.seg_start - s.start).abs() < 1e-3 && (t.seg_end - s.end).abs() < 1e-3
        })
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, v: &T) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(v).map_err(|e| format!("{}: {e}", path.display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{} -> {}: {e}", tmp.display(), path.display()))
}

/// Кэш распознанных слов дубля: id сегмента -> (хэш уложенного файла, слова от начала файла).
#[derive(Default, Serialize, Deserialize)]
struct WordsCache {
    entries: BTreeMap<String, CachedWords>,
}

#[derive(Serialize, Deserialize)]
struct CachedWords {
    key: String,
    words: Vec<(String, f64, f64)>,
}

fn file_key(p: &Path) -> Result<String, String> {
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Озвученная фраза, как она легла: сегмент проекта с показанным текстом, уложенный файл, спан в дорожке
/// до tempo-fit всей дорожки.
pub struct Laid<'a> {
    pub seg: &'a dub_core::Segment,
    pub file: &'a Path,
    pub span: (f64, f64),
    pub fit: Option<FitRecord>,
}

/// Записать dub_timing.json. `track_sf` — во сколько раз сжата вся дорожка tempo-fit'ом (1.0 — нет).
/// `asr` — движок распознавания слов дубля; None — слова не нужны (пресет без пословной подсветки).
/// Сбой распознавания отдельных фраз не рвёт рендер: эти фразы остаются без слов, причина уходит в
/// журнал через `warn`.
pub fn record(
    dir: &Path,
    laid: &[Laid],
    track_sf: f64,
    asr: Option<(&crate::models::AsrChoice, &str)>,
    warn: &dyn Fn(String),
) -> Result<DubTiming, String> {
    let sf = if track_sf > 0.0 { track_sf } else { 1.0 };
    let mut words_rel: BTreeMap<String, Vec<(String, f64, f64)>> = BTreeMap::new();
    if let Some((choice, lang)) = asr {
        let cache_path = dir.join(WORDS_CACHE);
        let mut cache: WordsCache = match std::fs::read_to_string(&cache_path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| format!("{}: {e}", cache_path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => WordsCache::default(),
            Err(e) => return Err(format!("{}: {e}", cache_path.display())),
        };
        let mut todo: Vec<(String, String, PathBuf)> = Vec::new();
        for l in laid {
            let key = file_key(l.file)?;
            match cache.entries.get(&l.seg.id) {
                Some(c) if c.key == key => {
                    words_rel.insert(l.seg.id.clone(), c.words.clone());
                }
                _ => todo.push((l.seg.id.clone(), key, l.file.to_path_buf())),
            }
        }
        if !todo.is_empty() {
            let files: Vec<PathBuf> = todo.iter().map(|t| t.2.clone()).collect();
            let mut engine = crate::models::build_engine(choice);
            let heard = engine.transcribe_many_words(&files, lang);
            let mut failed: Vec<String> = Vec::new();
            for ((id, key, _), r) in todo.into_iter().zip(heard) {
                match r {
                    Ok(ws) => {
                        let ws: Vec<(String, f64, f64)> = ws.into_iter().map(|w| (w.word, w.start, w.end)).collect();
                        cache.entries.insert(id.clone(), CachedWords { key, words: ws.clone() });
                        words_rel.insert(id, ws);
                    }
                    Err(e) => failed.push(format!("{id}: {e}")),
                }
            }
            if !failed.is_empty() {
                warn(t!(
                    "timing-words-not-recognized",
                    count = failed.len(),
                    examples = failed.iter().take(3).cloned().collect::<Vec<_>>().join("; ")
                ));
            }
            write_json_atomic(&cache_path, &cache)?;
        }
    }
    let mut out = DubTiming::default();
    for l in laid {
        let (a, b) = l.span;
        let words = words_rel
            .get(&l.seg.id)
            .map(|ws| ws.iter().map(|(w, s, e)| (w.clone(), (a + s) / sf, (a + e) / sf)).collect())
            .unwrap_or_default();
        out.segments.insert(
            l.seg.id.clone(),
            SegTiming {
                text: l.seg.tgt_text.trim().to_string(),
                seg_start: l.seg.start,
                seg_end: l.seg.end,
                at: a / sf,
                dur: (b - a).max(0.0) / sf,
                words,
                fit: l.fit.clone(),
            },
        );
    }
    out.save(dir)?;
    Ok(out)
}

/// Удалить dub_timing.json (укладка не сопоставилась с сегментами — старые тайминги были бы ложью).
pub fn clear(dir: &Path) -> Result<(), String> {
    let p = dir.join(FILE);
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dub_timing_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn record_scales_by_the_track_tempo_fit_and_round_trips() {
        let d = tmpdir("rt");
        let f = d.join("seg_000_fit.wav");
        std::fs::write(&f, b"x").unwrap();
        let seg = dub_core::Segment { id: "s0".into(), start: 1.0, end: 2.0, tgt_text: " Привет ".into(), ..Default::default() };
        let laid = [Laid { seg: &seg, file: &f, span: (2.2, 3.4), fit: None }];
        let t = record(&d, &laid, 1.2, None, &|_| {}).unwrap();
        let st = &t.segments["s0"];
        assert!((st.at - 2.2 / 1.2).abs() < 1e-9 && (st.dur - 1.0).abs() < 1e-9, "{st:?}");
        let back = DubTiming::load(&d).unwrap().unwrap();
        assert!(back.fresh(&seg).is_some());
        let edited = dub_core::Segment { tgt_text: "Пока".into(), ..seg.clone() };
        assert!(back.fresh(&edited).is_none(), "правка текста делает запись устаревшей");
        let moved = dub_core::Segment { start: 1.5, ..seg.clone() };
        assert!(back.fresh(&moved).is_none(), "перенос реплики делает запись устаревшей");
        clear(&d).unwrap();
        assert!(DubTiming::load(&d).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
