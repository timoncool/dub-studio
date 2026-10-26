//! dub-asr — ASR со словными таймстемпами + диаризация поверх parakeet-rs.
//!
//! Движок: Parakeet-TDT-0.6B-v3 (или его дообученные варианты той же архитектуры — Parakeet Ultra) +
//! Nemotron 3 Diarization (Streaming Sortformer v3, до 8 спикеров) для диаризации — через ONNX Runtime
//! (провайдер CPU по умолчанию) или сервер audio.cpp ([`Diarizer`]). parakeet-rs требует ровно 16 кГц моно — входной WAV приводится к
//! 16k/mono здесь (даунмикс + ресемплинг с ограничением полосы).
//!
//! Сегментация словного потока (_segment), transcribe / diarize / transcribe_turns — порт
//! dubengine/asr.py и dubengine/diarize.py: паузы >0.6с, конец предложения .!?…, макс 8.0с.

mod hallucination;
mod known_speakers;
mod reconcile;
mod resample;
mod segment;
mod speaker_global;
pub mod acpp;
pub mod tcpp;
mod window;
pub use hallucination::{hallucination_kind, is_hallucination, loop_kind, repeated_lines, HallucinationKind, HallucinationRules, LoopRules};
pub use known_speakers::SpeakerMatchError;
pub use reconcile::{speaker_for_overlap, DiarIndex};
pub use speaker_global::{
    cluster_embeddings, cosine, map_local_to_global, Embedding, LocalSpeaker, NullEmbedder,
    SpeakerEmbedder,
};
pub use window::{
    detect_active_spans, merge_windows, plan_windows, speech_envelope, Window, WindowConfig,
};

use parakeet_rs::sortformer::{DiarizationConfig, Sortformer, StreamingProfile};
use parakeet_rs::ExecutionConfig;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Максимум одновременно различаемых спикеров модели диаризации (Nemotron 3 Diarization).
pub const MAX_SPEAKERS: usize = parakeet_rs::sortformer::NUM_SPEAKERS;

/// Каталог модели диаризации внутри корня моделей и имя её файла.
pub const DIAR_MODEL_DIR: &str = "nemotron-diar";
pub const DIAR_MODEL_FILE: &str = "nemotron3_diar_v3.onnx";

/// Конфиг исполнения ONNX. КРИТИЧНО: понижаем уровень оптимизации графа до Level1 — на int8-кванте
/// Parakeet дефолтный Level3 виснет при создании CPU-сессии на минуты (оптимайзер спинит на
/// DynamicQuantizeLinear/MatMulInteger). Переопределяется через DUB_ASR_OPT_LEVEL (0..3).
///
/// Backend: env DUB_ASR_BACKEND=gpu регистрирует CUDA execution provider с `error_on_failure` — при
/// недоступности CUDA (нет GPU-сборки onnxruntime / провайдера / cuDNN) создание сессии падает ОШИБКОЙ,
/// а НЕ тихо откатывается на CPU. Вызывающий (сервер) ловит и показывает юзеру уведомление с выбором
/// (переключить на CPU / доустановить компонент). Иначе (cpu/пусто) — CPU-провайдер по умолчанию.
fn exec_config() -> ExecutionConfig {
    use ort::session::builder::GraphOptimizationLevel;
    let level = std::env::var("DUB_ASR_OPT_LEVEL")
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(1);
    let gpu = std::env::var("DUB_ASR_BACKEND").map(|v| v == "gpu").unwrap_or(false);
    ExecutionConfig::new().with_custom_configure(move |b| {
        let lvl = match level {
            0 => GraphOptimizationLevel::Disable,
            2 => GraphOptimizationLevel::Level2,
            3 => GraphOptimizationLevel::Level3,
            _ => GraphOptimizationLevel::Level1,
        };
        let mut b = b.with_optimization_level(lvl)?;
        tracing::info!("onnxruntime diarization session: {}", if gpu && cfg!(feature = "cuda") { "CUDA" } else { "CPU" });
        #[cfg(feature = "cuda")]
        {
            if gpu {
                // Только CUDA + error_on_failure: недоступность прилетает Err(ort), не тихий CPU-фоллбек.
                b = b.with_execution_providers([ort::ep::CUDA::default().build().error_on_failure()])?;
            }
        }
        #[cfg(not(feature = "cuda"))]
        let _ = gpu; // без фичи cuda GPU-режим ловит пре-флайт сервера (сборка без CUDA-провайдера)
        Ok(b)
    })
}

pub use segment::{join_text, segment_words, split_at_speaker_turns, Segment, Word, SEG_MAX_GAP, SEG_MAX_DUR};

/// Целевая частота parakeet-rs.
pub const TARGET_SR: u32 = 16_000;

/// Гарантировать, что ort (load-dynamic) грузит ПРАВИЛЬНУЮ onnxruntime.dll (1.28.2, под которую собран
/// ort rc.13 с api-28). Без явного ORT_DYLIB_PATH ort ищет DLL по системному PATH и цепляет
/// C:\Windows\System32\onnxruntime.dll (1.17, поставляется с Windows) — рассинхрон OrtApi даёт ДЕДЛОК
/// при создании сессии (процесс висит с 0% CPU, ни модель, ни диск не грузятся). Поэтому если
/// ORT_DYLIB_PATH не задан пользователем, выставляем его на встроенную 1.28.2-DLL до первого касания ort.
///
/// Поиск (первый существующий): env DUB_ASR_ORT_DYLIB -> <models_root>/runtime/onnxruntime-win-x64-1.28.2/
/// lib/onnxruntime.dll -> та же DLL рядом с бинарём (портативная раскладка). models_root: env
/// DUBENGINE_MODELS_ROOT, иначе <exe_dir>/models или <exe_dir>/../../models (dev-раскладка target/…).
fn ensure_ort_dylib() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Пользователь задал явно -> уважаем, ничего не трогаем.
        if std::env::var_os("ORT_DYLIB_PATH").is_some() {
            return;
        }
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os("DUB_ASR_ORT_DYLIB") {
            cands.push(PathBuf::from(p));
        }
        // корни для поиска models/
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(m) = std::env::var_os("DUBENGINE_MODELS_ROOT") {
            roots.push(PathBuf::from(m));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.join("models"));
                // dev-раскладка: target/release/examples/asr.exe -> ../../../models
                if let Some(p2) = dir.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) {
                    roots.push(p2.join("models"));
                }
                if let Some(p1) = dir.parent().and_then(|d| d.parent()) {
                    roots.push(p1.join("models"));
                }
                // DLL рядом с бинарём (портативная упаковка)
                cands.push(dir.join(dub_core::runtime::ORT_LIBRARY));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("models"));
        }
        for r in &roots {
            // GPU-сборка (cuda13) ПРИОРИТЕТНЕЕ: она суперсет — умеет и CPU-провайдер, и CUDA-EP. Если
            // скачана, грузим её, чтобы переключение backend gpu<->cpu работало БЕЗ рестарта (dll
            // фиксируется в процессе при первом касании ort; выбор провайдера — уже в exec_config).
            cands.extend(dub_core::runtime::ort_candidates(r));
        }
        for c in cands {
            if c.is_file() {
                std::env::set_var("ORT_DYLIB_PATH", &c);
                return;
            }
        }
        // Ничего не нашли — оставляем как есть; ort даст явную ошибку загрузки (лучше дедлока не станет,
        // но хотя бы не молча). Диагностику берёт на себя вызывающий по AsrError.
    });
}

#[derive(Error, Debug)]
pub enum AsrError {
    #[error("parakeet: {0}")]
    Parakeet(String),
    #[error("cannot read wav {0}: {1}")]
    WavRead(String, String),
    #[error("io: {0}")]
    Io(String),
    #[error("resampling: {0}")]
    Resample(String),
    #[error("the number of speakers must be from 1 to {max}")]
    SpeakerCount { max: usize },
    #[error("speakers: {0}")]
    Speakers(#[from] SpeakerMatchError),
}

impl AsrError {
    /// Стабильный код ошибки (аргументы — поля варианта).
    pub fn code(&self) -> &'static str {
        match self {
            AsrError::Parakeet(_) => "asr_engine",
            AsrError::WavRead(..) => "asr_wav_read",
            AsrError::Io(_) => "asr_io",
            AsrError::Resample(_) => "asr_resample",
            AsrError::SpeakerCount { .. } => "asr_speaker_count",
            AsrError::Speakers(e) => e.code(),
        }
    }
}

/// Одна реплика диаризации: [start, end] в секундах, speaker — контиг. id (0..k-1).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: i32,
}

/// Сегмент с привязкой к спикеру (результат transcribe_turns).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpeakerSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub speaker: i32,
}

/// Единый интерфейс ASR-движка: транскрипция whole-clip и per-speaker (реплики). Реализуют Parakeet на transcribe.cpp
/// (`tcpp::TcppEngine`) и CrisperWhisper в audiocpp_server (`acpp::AcppAsr`) — analyze выбирает движок по настройке
/// (models/active.json), не зная деталей. Методы берут `&Path` (объект-трейт: без дженериков).
pub trait AsrEngine {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError>;
    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError>;
    /// Пакетная транскрипция МНОГИХ коротких файлов → полный текст каждого; Err — файл не распознан (с
    /// причиной). Дефолт — цикл transcribe; transcribe.cpp переопределяет пакетным прогоном.
    /// Используется QC-верификацией синтеза в рендере (сверка сказанного с ожидаемым переводом).
    fn transcribe_many(&mut self, files: &[std::path::PathBuf], lang: &str) -> Vec<Result<String, AsrError>> {
        files
            .iter()
            .map(|f| self.transcribe(f, lang).map(|segs| join_text(segs.iter().map(|s| s.text.as_str()))))
            .collect()
    }
    /// Пакетная транскрипция МНОГИХ коротких файлов со словными таймингами (секунды от начала файла).
    /// Ошибка распознавания файла — Err с причиной (вызывающий решает, как показать её пользователю).
    /// Субтитры дубляжа берут отсюда, где в уложенной фразе реально звучит каждое слово.
    fn transcribe_many_words(&mut self, files: &[std::path::PathBuf], lang: &str) -> Vec<Result<Vec<Word>, AsrError>> {
        files
            .iter()
            .map(|f| self.transcribe(f, lang).map(|segs| segs.into_iter().flat_map(|s| s.words).collect()))
            .collect()
    }
}

/// Где работает Nemotron 3 Diarization: файл nemotron3_diar_v3.onnx через ONNX Runtime или сервер
/// audio.cpp, в котором этот же файл загружен моделью `model` (`ModelEntry::nemotron_3_diar`).
pub enum Diarizer<'a> {
    Onnx(&'a Path),
    Server { server: std::sync::Arc<acpp_server::Server>, model: &'a str },
}

/// Открытая модель диаризации: реплики одного окна 16 кГц моно со сдвигом `offset` секунд.
enum WindowDiarizer<'a> {
    Onnx(Sortformer),
    Server { server: std::sync::Arc<acpp_server::Server>, model: &'a str },
}

impl<'a> WindowDiarizer<'a> {
    fn open(diarizer: &Diarizer<'a>) -> Result<Self, AsrError> {
        match diarizer {
            Diarizer::Onnx(onnx) => {
                ensure_ort_dylib(); // до первого касания ort — иначе дедлок на чужой system32 DLL
                let mut sf = Sortformer::with_config(onnx, Some(exec_config()), DiarizationConfig::default())
                    .map_err(|e| AsrError::Parakeet(e.to_string()))?;
                sf.set_profile(StreamingProfile::offline()).map_err(|e| AsrError::Parakeet(e.to_string()))?;
                Ok(WindowDiarizer::Onnx(sf))
            }
            Diarizer::Server { server, model } => Ok(WindowDiarizer::Server { server: server.clone(), model }),
        }
    }

    fn turns(&mut self, audio: &[f32], sr: u32, offset: f64) -> Result<Vec<Turn>, AsrError> {
        match self {
            WindowDiarizer::Onnx(sf) => {
                let segs = sf.diarize(audio.to_vec(), sr, 1).map_err(|e| AsrError::Parakeet(e.to_string()))?;
                Ok(segments_to_turns(&segs, offset))
            }
            WindowDiarizer::Server { server, model } => {
                let wav = std::env::temp_dir().join(format!("dub-diar-{}-{offset}.wav", std::process::id()));
                write_wav_pcm16(&wav, audio, sr)?;
                let answer = server.diarize(model, &wav);
                let _ = std::fs::remove_file(&wav);
                let turns = answer.map_err(|e| AsrError::Parakeet(e.to_string()))?;
                Ok(turns
                    .into_iter()
                    .map(|t| Turn { start: offset + t.start, end: offset + t.end, speaker: t.speaker as i32 })
                    .collect())
            }
        }
    }
}

fn write_wav_pcm16(path: &Path, audio: &[f32], sr: u32) -> Result<(), AsrError> {
    let spec = hound::WavSpec { channels: 1, sample_rate: sr, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let io = |e: hound::Error| AsrError::Io(format!("{}: {e}", path.display()));
    let mut w = hound::WavWriter::create(path, spec).map_err(io)?;
    for s in audio {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16).map_err(io)?;
    }
    w.finalize().map_err(io)
}

/// Диаризация: Nemotron 3 Diarization (Streaming Sortformer v3, до [`MAX_SPEAKERS`] спикеров) -> реплики
/// [(start,end,speaker)] в секундах, speaker перенумерован 0..k-1.
///
/// Файл целиком известен заранее, поэтому профиль стриминга — `offline()` (самый длинный контекст модели,
/// буфер 30.4 с), постобработка — дефолт parakeet-rs, воспроизводящий `diarize()` NeMo для этой модели;
/// сервер audio.cpp берёт тот же профиль (`very_high`) и тот же порог.
pub fn diarize(wav: impl AsRef<Path>, diarizer: &Diarizer) -> Result<Vec<Turn>, AsrError> {
    let mut model = WindowDiarizer::open(diarizer)?;
    let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
    // ДЛИННЫЙ файл — оконная диаризация: окна DIAR_WIN с перекрытием, спикеры соседних окон
    // сшиваются по пересечению реплик в оверлапе (страховка от роста памяти/деградации на часах).
    let total = audio.len() as f64 / sr as f64;
    let raw: Vec<Turn> = if total > DIAR_WINDOW_GATE_SECS {
        diarize_windowed(&mut model, &audio, sr)?
    } else {
        model.turns(&audio, sr, 0.0)?
    };
    Ok(renumber_turns(raw))
}

/// Сегменты parakeet-rs (start/end в СЕМПЛАХ при 16 кГц) -> реплики в секундах со сдвигом `offset`.
fn segments_to_turns(segs: &[parakeet_rs::sortformer::SpeakerSegment], offset: f64) -> Vec<Turn> {
    segs.iter()
        .map(|s| Turn {
            start: offset + s.start as f64 / TARGET_SR as f64,
            end: offset + s.end as f64 / TARGET_SR as f64,
            speaker: s.speaker_id as i32,
        })
        .collect()
}

/// Отсортировать реплики по началу и перенумеровать спикеров в плотный ряд 0..k-1 (по возрастанию
/// исходной метки: модель выдаёт id по порядку первого появления, порядок сохраняется).
fn renumber_turns(mut raw: Vec<Turn>) -> Vec<Turn> {
    raw.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    let mut labels: Vec<i32> = raw.iter().map(|t| t.speaker).collect();
    labels.sort_unstable();
    labels.dedup();
    let remap: std::collections::HashMap<i32, i32> =
        labels.iter().enumerate().map(|(i, &l)| (l, i as i32)).collect();
    for t in &mut raw {
        t.speaker = remap[&t.speaker];
    }
    raw
}

/// Гейт оконной диаризации: до 90 мин whole-file ПРОВЕРЕН (L90: 5 мин анализа, RAM 64МБ, спикеры
/// точные), длиннее — окна. Окно крупное и оверлап длинный сознательно: valid-замер L90 с окнами
/// 30мин/оверлап 10с дал 9 спикеров вместо 4 — в коротком оверлапе не все голоса успевают
/// прозвучать, и сшивка плодит новые id; за 120с звучат практически все активные спикеры сцены.
const DIAR_WINDOW_GATE_SECS: f64 = 90.0 * 60.0;
const DIAR_WIN_SECS: f64 = 60.0 * 60.0;
const DIAR_OVERLAP_SECS: f64 = 120.0;

/// Оконная диаризация: модель по окнам DIAR_WIN с перекрытием DIAR_OVERLAP; спикеры окна i+1
/// сшиваются со спикерами окна i по максимальному пересечению реплик в зоне оверлапа (стандартный
/// приём стриминговой диаризации); не сматченные получают новые глобальные id. Реплики из головы
/// окна, уже покрытые предыдущим (до середины оверлапа), отбрасываются — без дублей на шве.
fn diarize_windowed(
    model: &mut WindowDiarizer,
    audio: &[f32],
    sr: u32,
) -> Result<Vec<Turn>, AsrError> {
    let total = audio.len() as f64 / sr as f64;
    let n_win = ((total / DIAR_WIN_SECS).ceil() as usize).max(1);
    let mut out: Vec<Turn> = Vec::new();
    let mut next_gid: i32 = 0;
    // Глобальные реплики предыдущего окна в зоне оверлапа — для матчинга спикеров.
    let mut prev_tail: Vec<Turn> = Vec::new();
    for i in 0..n_win {
        let grid0 = i as f64 * DIAR_WIN_SECS;
        let w0 = if i == 0 { 0.0 } else { grid0 - DIAR_OVERLAP_SECS };
        let w1 = ((i + 1) as f64 * DIAR_WIN_SECS).min(total);
        let a0 = (w0 * sr as f64) as usize;
        let a1 = ((w1 * sr as f64) as usize).min(audio.len());
        let local: Vec<Turn> = model.turns(&audio[a0..a1], sr, w0)?;
        // Мапа локальный спикер -> глобальный id: по максимальному суммарному пересечению с
        // репликами prev_tail в зоне оверлапа [w0, w0+OVERLAP].
        let mut map: std::collections::HashMap<i32, i32> = Default::default();
        if i > 0 {
            let ov_end = w0 + DIAR_OVERLAP_SECS;
            let mut locals: Vec<i32> = local.iter().map(|t| t.speaker).collect();
            locals.sort_unstable();
            locals.dedup();
            for lid in locals {
                let mut best: (f64, Option<i32>) = (0.0, None);
                let mut per_gid: std::collections::HashMap<i32, f64> = Default::default();
                for lt in local.iter().filter(|t| t.speaker == lid && t.start < ov_end) {
                    for pt in prev_tail.iter() {
                        let inter = (lt.end.min(pt.end) - lt.start.max(pt.start)).max(0.0);
                        if inter > 0.0 {
                            *per_gid.entry(pt.speaker).or_insert(0.0) += inter;
                        }
                    }
                }
                for (gid, secs) in per_gid {
                    if secs > best.0 {
                        best = (secs, Some(gid));
                    }
                }
                if let (s, Some(gid)) = best {
                    if s >= 0.5 {
                        map.insert(lid, gid);
                    }
                }
            }
        }
        let cut = if i == 0 { 0.0 } else { w0 + DIAR_OVERLAP_SECS / 2.0 };
        for t in &local {
            let gid = *map.entry(t.speaker).or_insert_with(|| {
                let g = next_gid;
                next_gid += 1;
                g
            });
            // Голову окна до середины оверлапа отдаёт предыдущее окно (без дублей).
            if t.end <= cut {
                continue;
            }
            out.push(Turn { start: t.start.max(cut), end: t.end, speaker: gid });
        }
        // Хвост текущего окна (глобальными id) — вход матчинга следующего.
        let next_ov_start = w1 - DIAR_OVERLAP_SECS;
        prev_tail = out.iter().filter(|t| t.end > next_ov_start).cloned().collect();
        // next_gid не меньше максимального выданного id + 1 (map мог добавить новые).
        next_gid = next_gid.max(out.iter().map(|t| t.speaker).max().unwrap_or(-1) + 1);
    }
    Ok(out)
}

/// Окно референса спикера: [start, end] его самой длинной реплики (для клон-x-вектора).
pub type RefWindow = (f64, f64);

/// Результат turns(): (реплики, число спикеров, ref_windows: speaker -> самая длинная реплика).
pub struct DiarTurns {
    pub turns: Vec<Turn>,
    pub n_speakers: usize,
    pub ref_windows: std::collections::HashMap<i32, RefWindow>,
}

pub fn continuous_diarization_enabled() -> Result<bool, String> {
    match std::env::var("DUB_STUDIO_DIAR_CONTINUOUS") {
        Ok(value) => parse_continuous_diarization(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(error) => Err(format!("DUB_STUDIO_DIAR_CONTINUOUS: {error}")),
    }
}

fn parse_continuous_diarization(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err("DUB_STUDIO_DIAR_CONTINUOUS: expected 0 or 1".into()),
    }
}

fn known_diarization_window(total: f64, continuous: bool) -> f64 {
    if continuous || total <= DIAR_WINDOW_GATE_SECS {
        total.max(1.0)
    } else {
        DIAR_WIN_SECS
    }
}

#[cfg(test)]
mod continuous_diarization_tests {
    use super::*;

    #[test]
    fn continuous_mode_keeps_the_entire_six_hour_recording_in_one_window() {
        let total = 6.0 * 3600.0 - 180.0;
        let windows = known_speakers::diar_windows(total, known_diarization_window(total, true), DIAR_OVERLAP_SECS);
        assert_eq!(windows.len(), 1);
        assert_eq!((windows[0].start, windows[0].end, windows[0].keep_start, windows[0].keep_end), (0.0, total, 0.0, total));
        assert_eq!(known_diarization_window(total, false), 3600.0);
        assert_eq!(known_diarization_window(60.0, false), 60.0);
    }

    #[test]
    fn continuous_mode_rejects_invalid_values_without_enabling_another_path() {
        assert!(!parse_continuous_diarization(None).unwrap());
        assert!(!parse_continuous_diarization(Some("0")).unwrap());
        assert!(parse_continuous_diarization(Some("1")).unwrap());
        for value in ["", "true", "2", " 1"] {
            assert!(parse_continuous_diarization(Some(value)).unwrap_err().contains("expected 0 or 1"));
        }
    }
}

/// DIARIZE-FIRST: порт diarize.turns() — слить подряд идущие реплики одного спикера (merge_gap),
/// и если «настоящих» спикеров (суммарно >= min_speaker_dur) меньше двух, схлопнуть в single-speaker
/// (turns=[], n=1) — это ШТАТНАЯ graceful-деградация питона, не отсебятина. Иначе перенумеровать
/// спикеров 0..k-1 и вернуть ref_windows (самая длинная реплика каждого).
pub fn turns(
    wav: impl AsRef<Path>,
    diarizer: &Diarizer,
    merge_gap: f64,
    min_speaker_dur: f64,
) -> Result<DiarTurns, AsrError> {
    let raw = diarize(wav, diarizer)?;
    Ok(merge_turns(&raw, merge_gap, min_speaker_dur))
}

pub fn turns_with_speaker_count(
    wav: impl AsRef<Path>,
    diarizer: &Diarizer,
    count: usize,
    embed: &mut impl FnMut(&[f32]) -> Result<Vec<f32>, String>,
) -> Result<DiarTurns, AsrError> {
    if !(1..=MAX_SPEAKERS).contains(&count) {
        return Err(AsrError::SpeakerCount { max: MAX_SPEAKERS });
    }
    if count == 1 {
        return Ok(DiarTurns { turns: Vec::new(), n_speakers: 1, ref_windows: Default::default() });
    }
    let mut model = WindowDiarizer::open(diarizer)?;
    let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
    let total = audio.len() as f64 / sr as f64;
    let window = known_diarization_window(total, continuous_diarization_enabled().map_err(AsrError::Parakeet)?);
    let mut tracker = known_speakers::VoiceTracker::new(count);
    let mut out = Vec::new();
    for range in known_speakers::diar_windows(total, window, DIAR_OVERLAP_SECS) {
        let a0 = (range.start * sr as f64) as usize;
        let a1 = ((range.end * sr as f64) as usize).min(audio.len());
        let local = model.turns(&audio[a0..a1], sr, range.start)?;
        out.extend(tracker.process_window(&local, &audio[a0..a1], sr, range, embed)?);
    }
    Ok(known_speakers::finish_turns(out))
}

/// Порог «настоящего» спикера: 10% всей речи ролика, не меньше 1.5 с и не больше `cap`. Ложные спикеры
/// диаризации — обрывки меньше секунды, а живой человек с парой реплик в коротком ролике набирает 2 с
/// и больше; фиксированный `cap` в коротком ролике выбрасывал таких людей.
fn speaker_dur_threshold(total_speech: f64, cap: f64) -> f64 {
    (0.1 * total_speech).clamp(1.5_f64.min(cap), cap)
}

/// Свёртка сырых реплик диаризации (отсортированных по началу) в DiarTurns — логика [`turns`] без модели.
pub fn merge_turns(raw: &[Turn], merge_gap: f64, min_speaker_dur: f64) -> DiarTurns {
    use std::collections::HashMap;
    let single = || DiarTurns { turns: Vec::new(), n_speakers: 1, ref_windows: HashMap::new() };

    if raw.is_empty() {
        return single();
    }

    // Слить подряд идущие реплики одного спикера с зазором <= merge_gap.
    let mut merged: Vec<[f64; 3]> = vec![[raw[0].start, raw[0].end, raw[0].speaker as f64]];
    for t in &raw[1..] {
        let last = merged.last_mut().unwrap();
        if t.speaker as f64 == last[2] && t.start - last[1] <= merge_gap {
            last[1] = last[1].max(t.end);
        } else {
            merged.push([t.start, t.end, t.speaker as f64]);
        }
    }

    // Суммарная длительность на спикера -> «настоящие» спикеры (>= порога).
    let mut dur: HashMap<i32, f64> = HashMap::new();
    for m in &merged {
        *dur.entry(m[2] as i32).or_insert(0.0) += m[1] - m[0];
    }
    let threshold = speaker_dur_threshold(dur.values().sum(), min_speaker_dur);
    let realset: std::collections::HashSet<i32> =
        dur.iter().filter(|(_, &d)| d >= threshold).map(|(&s, _)| s).collect();
    if realset.len() < 2 {
        return single(); // реально один голос -> single-speaker путь
    }

    // Крошечную реплику не-настоящего спикера переназначить ближайшей настоящей (по середине).
    let real_turns: Vec<[f64; 3]> = merged.iter().filter(|m| realset.contains(&(m[2] as i32))).copied().collect();
    for m in &mut merged {
        if !realset.contains(&(m[2] as i32)) {
            let mid = (m[0] + m[1]) / 2.0;
            let nearest = real_turns
                .iter()
                .min_by(|a, b| {
                    let da = (mid - (a[0] + a[1]) / 2.0).abs();
                    let db = (mid - (b[0] + b[1]) / 2.0).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|x| x[2])
                .unwrap_or(m[2]);
            m[2] = nearest;
        }
    }

    // Самая длинная реплика каждого спикера (до перенумерации).
    let mut longest: HashMap<i32, RefWindow> = HashMap::new();
    for m in &merged {
        let sp = m[2] as i32;
        let cur = longest.get(&sp).copied().unwrap_or((0.0, 0.0));
        if (m[1] - m[0]) > (cur.1 - cur.0) {
            longest.insert(sp, (m[0], m[1]));
        }
    }

    // Перенумеровать метки в 0..k-1 по возрастанию.
    let mut labels: Vec<i32> = merged.iter().map(|m| m[2] as i32).collect();
    labels.sort_unstable();
    labels.dedup();
    let remap: HashMap<i32, i32> = labels.iter().enumerate().map(|(i, &l)| (l, i as i32)).collect();

    let out: Vec<Turn> = merged
        .iter()
        .map(|m| Turn { start: m[0], end: m[1], speaker: remap[&(m[2] as i32)] })
        .collect();
    let rw: HashMap<i32, RefWindow> = longest.into_iter().map(|(old, w)| (remap[&old], w)).collect();
    DiarTurns { turns: out, n_speakers: labels.len(), ref_windows: rw }
}

// ─── загрузка/подготовка аудио ──────────────────────────────────────────────

/// Прочитать WAV, свести в моно и ресемплировать в 16 кГц (parakeet-rs требует ровно 16k моно).
pub(crate) fn load_wav_16k_mono(path: &Path) -> Result<(Vec<f32>, u32), AsrError> {
    let disp = path.display().to_string();
    let wav_err = |e: hound::Error| AsrError::WavRead(disp.clone(), e.to_string());
    let mut reader = hound::WavReader::open(path).map_err(wav_err)?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(wav_err)?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(wav_err)?
        }
    };

    // Даунмикс в моно.
    let mono: Vec<f32> = if ch <= 1 {
        interleaved
    } else {
        interleaved
            .chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    let out = resample::mono(&mono, spec.sample_rate, TARGET_SR)?;
    Ok((out, TARGET_SR))
}

#[cfg(test)]
mod diar_word_tests {
    use super::*;

    #[test]
    fn model_supports_eight_speakers() {
        assert_eq!(MAX_SPEAKERS, 8);
    }

    fn turn(start: f64, end: f64, speaker: i32) -> Turn {
        Turn { start, end, speaker }
    }

    #[test]
    fn renumber_keeps_all_eight_speakers_dense_and_sorted() {
        let raw: Vec<Turn> = (0..8).rev().map(|s| turn(s as f64 * 3.0, s as f64 * 3.0 + 2.0, s)).collect();
        let out = renumber_turns(raw);
        let ids: Vec<i32> = out.iter().map(|t| t.speaker).collect();
        assert_eq!(ids, (0..8).collect::<Vec<i32>>());
        assert!(out.windows(2).all(|p| p[0].start <= p[1].start));
    }

    #[test]
    fn renumber_compacts_sparse_ids() {
        let out = renumber_turns(vec![turn(0.0, 1.0, 7), turn(1.0, 2.0, 2), turn(2.0, 3.0, 5)]);
        let ids: Vec<i32> = out.iter().map(|t| t.speaker).collect();
        assert_eq!(ids, vec![2, 0, 1]);
    }

    #[test]
    fn merge_turns_keeps_eight_real_speakers() {
        let raw: Vec<Turn> = (0..16).map(|i| turn(i as f64 * 4.0, i as f64 * 4.0 + 3.0, i % 8)).collect();
        let d = merge_turns(&raw, 0.8, 2.5);
        assert_eq!(d.n_speakers, 8);
        assert_eq!(d.ref_windows.len(), 8);
        let mut ids: Vec<i32> = d.turns.iter().map(|t| t.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids, (0..8).collect::<Vec<i32>>());
    }

    #[test]
    fn merge_turns_short_speaker_goes_to_nearest_real() {
        let raw = vec![turn(0.0, 3.0, 0), turn(3.2, 3.6, 7), turn(4.0, 7.0, 1)];
        let d = merge_turns(&raw, 0.8, 2.5);
        assert_eq!(d.n_speakers, 2);
        assert!(d.turns.iter().all(|t| t.speaker == 0 || t.speaker == 1));
    }

    #[test]
    fn merge_turns_keeps_short_clip_speaker_with_two_lines() {
        // Реальный 22-с ролик: второй человек сказал две фразы (2.2 с) против монолога 10.6 с.
        let raw = vec![
            turn(0.9, 1.7, 0), turn(1.9, 3.2, 1), turn(3.6, 5.2, 1), turn(6.2, 7.7, 0), turn(7.4, 8.9, 1),
            turn(10.2, 11.8, 1), turn(12.2, 13.6, 1), turn(14.9, 15.7, 1), turn(16.7, 17.3, 1), turn(17.8, 19.6, 1),
        ];
        let d = merge_turns(&raw, 0.8, 2.5);
        assert_eq!(d.n_speakers, 2);
    }

    #[test]
    fn merge_turns_drops_sub_second_fragments() {
        // Обрывки 0.2 с и 0.7 с рядом с тремя настоящими голосами — не спикеры.
        let raw = vec![
            turn(0.0, 3.5, 0), turn(3.8, 7.3, 1), turn(7.6, 11.1, 2), turn(11.4, 11.6, 3),
            turn(11.9, 15.4, 0), turn(15.7, 19.2, 1), turn(19.5, 20.2, 4), turn(20.5, 24.0, 2),
        ];
        let d = merge_turns(&raw, 0.8, 2.5);
        assert_eq!(d.n_speakers, 3);
    }

    #[test]
    fn long_clip_keeps_fixed_threshold() {
        assert_eq!(speaker_dur_threshold(600.0, 2.5), 2.5);
        assert_eq!(speaker_dur_threshold(12.8, 2.5), 1.5);
        assert!((speaker_dur_threshold(20.0, 2.5) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn merge_turns_single_real_speaker_collapses() {
        let raw = vec![turn(0.0, 1.0, 0), turn(2.0, 6.0, 1)];
        let d = merge_turns(&raw, 0.8, 2.5);
        assert_eq!(d.n_speakers, 1);
        assert!(d.turns.is_empty());
    }
}
