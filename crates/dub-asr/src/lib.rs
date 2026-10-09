//! dub-asr — ASR со словными таймстемпами + диаризация поверх parakeet-rs.
//!
//! Движок: Parakeet-TDT-0.6B-v3 (или его дообученные варианты той же архитектуры — Parakeet Ultra) +
//! Nemotron 3 Diarization (Streaming Sortformer v3, до 8 спикеров) для диаризации, оба через ONNX Runtime
//! (провайдер CPU по умолчанию). parakeet-rs требует ровно 16 кГц моно — входной WAV приводится к
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
mod whisper;
mod window;
pub use hallucination::{hallucination_kind, is_hallucination, HallucinationKind, HallucinationRules};
pub use known_speakers::SpeakerMatchError;
pub use reconcile::{speaker_for_overlap, DiarIndex};
pub use speaker_global::{
    cluster_embeddings, cosine, map_local_to_global, Embedding, LocalSpeaker, NullEmbedder,
    SpeakerEmbedder,
};
pub use whisper::WhisperAsr;
pub use window::{
    detect_active_spans, merge_windows, plan_windows, speech_envelope, Window, WindowConfig,
};

use parakeet_rs::sortformer::{DiarizationConfig, Sortformer, StreamingProfile};
use parakeet_rs::{ExecutionConfig, ParakeetTDT, TimestampMode, Transcriber};
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

pub use segment::{segment_words, split_at_speaker_turns, Segment, Word, SEG_MAX_GAP, SEG_MAX_DUR};

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

/// Единый интерфейс ASR-движка: транскрипция whole-clip и per-speaker (реплики). Реализуют Parakeet
/// (`Asr`) и Whisper (`WhisperAsr`) — analyze выбирает движок по настройке (models/active.json), не зная
/// деталей. Методы берут `&Path` (объект-трейт: без дженериков).
pub trait AsrEngine {
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError>;
    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], lang: &str) -> Result<Vec<SpeakerSegment>, AsrError>;
    /// Пакетная транскрипция МНОГИХ коротких файлов → полный текст каждого; Err — файл не распознан (с
    /// причиной). Дефолт — цикл transcribe (Parakeet in-process и так быстр); Whisper переопределяет ОДНИМ
    /// сабпроцессом на весь список (старт процесса дорогой, 333 файла по-одному — минуты впустую).
    /// Используется QC-верификацией синтеза в рендере (сверка сказанного с ожидаемым переводом).
    fn transcribe_many(&mut self, files: &[std::path::PathBuf], lang: &str) -> Vec<Result<String, AsrError>> {
        files
            .iter()
            .map(|f| self.transcribe(f, lang).map(|segs| segs.into_iter().map(|s| s.text).collect::<Vec<_>>().join(" ")))
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

/// ASR-движок: держит загруженную TDT-модель тёплой между вызовами.
pub struct Asr {
    tdt_dir: PathBuf,
    model: Option<ParakeetTDT>,
}

impl Asr {
    /// Каталог с TDT-моделью (encoder-model.onnx + .data, decoder_joint-model.onnx, vocab.txt).
    pub fn new(tdt_dir: impl AsRef<Path>) -> Self {
        Self {
            tdt_dir: tdt_dir.as_ref().to_path_buf(),
            model: None,
        }
    }

    fn model(&mut self) -> Result<&mut ParakeetTDT, AsrError> {
        if self.model.is_none() {
            ensure_ort_dylib(); // до первого касания ort — иначе дедлок на чужой system32 DLL
            let m = ParakeetTDT::from_pretrained(&self.tdt_dir, Some(exec_config()))
                .map_err(|e| AsrError::Parakeet(e.to_string()))?;
            self.model = Some(m);
        }
        Ok(self.model.as_mut().unwrap())
    }

    /// Транскрипция всего клипа со словными таймстемпами -> сегменты по паузам/пунктуации/макс-длине.
    /// Порт asr.transcribe: словный поток -> _segment. `_lang` зарезервирован (TDT сам определяет язык).
    pub fn transcribe(&mut self, wav: impl AsRef<Path>, _lang: &str) -> Result<Vec<Segment>, AsrError> {
        let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
        let words = self.transcribe_words(&audio, sr)?;
        Ok(segment_words(&words, SEG_MAX_GAP, SEG_MAX_DUR))
    }

    /// ОКОННАЯ транскрипция длинного клипа (задача #79): нарезать вокал на окна min-cut'ом
    /// (`plan_windows`), транскрибировать КАЖДОЕ окно отдельно (RAM O(окна), не O(фильма)), прибавить
    /// `window_offset` ко всем словным таймкодам, склеить с де-дупом слов на overlap соседних окон,
    /// затем обычная `segment_words`. Для короткого файла (`total ≤ chunk_size`) `plan_windows` вернёт
    /// ОДНО окно [0,total) -> путь совпадает с `transcribe` (питон-паритет, короткий режим не ломаем).
    ///
    /// `cfg=None` -> `WindowConfig::default()`. Публичный словный поток (после offset+dedup) можно
    /// получить через [`Asr::transcribe_words_windowed`] — эта функция лишь добавляет сегментацию.
    pub fn transcribe_windowed(
        &mut self,
        wav: impl AsRef<Path>,
        _lang: &str,
        cfg: Option<window::WindowConfig>,
    ) -> Result<Vec<Segment>, AsrError> {
        let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
        let words = self.transcribe_words_windowed(&audio, sr, cfg)?;
        Ok(segment_words(&words, SEG_MAX_GAP, SEG_MAX_DUR))
    }

    /// Словный поток по окнам (offset + де-дуп на overlap), без сегментации. Открытая точка для
    /// оркестратора, которому нужны сырые слова (например для reconcile или сборки собственных сегментов).
    pub fn transcribe_words_windowed(
        &mut self,
        audio: &[f32],
        sr: u32,
        cfg: Option<window::WindowConfig>,
    ) -> Result<Vec<Word>, AsrError> {
        let cfg = cfg.unwrap_or_default();
        let total = audio.len() as f64 / sr as f64;
        let windows = window::plan_windows(audio, sr, total, &cfg);
        // Один window -> прямой прогон (идентично transcribe_words: без нарезки/дедупа).
        if windows.len() <= 1 {
            return self.transcribe_words(audio, sr);
        }
        // ПАРАЛЛЕЛЬ ЧЕРЕЗ N НЕЗАВИСИМЫХ СЕССИЙ: `ort::Session::run` берёт `&mut self` — параллель на
        // ОДНОЙ сессии невозможна, но N отдельных инстансов ParakeetTDT (~0.7-1.5ГБ каждый при 24ГБ
        // VRAM) работают одновременно. DUB_STUDIO_ASR_WORKERS (дефолт 3; 1 = старый последовательный).
        // Любая ошибка параллельного пути -> тихий фолбэк на последовательный (ниже).
        let workers: usize = std::env::var("DUB_STUDIO_ASR_WORKERS")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(3)
            .clamp(1, windows.len());
        if workers > 1 {
            if let Ok(all) = self.windowed_parallel(audio, sr, &windows, workers) {
                return Ok(all);
            }
        }
        let mut all: Vec<Word> = Vec::new();
        for w in &windows {
            let a = ((w.start * sr as f64) as usize).min(audio.len());
            let b = ((w.end * sr as f64) as usize).min(audio.len());
            if b <= a {
                continue;
            }
            let clip = &audio[a..b];
            let mut ws = self.transcribe_words(clip, sr)?;
            let off = w.offset();
            for word in &mut ws {
                word.start += off;
                word.end += off;
            }
            append_dedup_words(&mut all, ws, w.start);
        }
        Ok(all)
    }

    /// Параллельный оконный прогон: пул из `workers` потоков, КАЖДЫЙ со своей сессией
    /// `ParakeetTDT::from_pretrained` (эксклюзивность `&mut self` обходится числом сессий, не очередью).
    /// Очередь окон общая; результаты собираются по индексу окна и склеиваются В ПОРЯДКЕ окон тем же
    /// `append_dedup_words`, что и последовательный путь (та же обработка overlap-швов).
    fn windowed_parallel(
        &mut self,
        audio: &[f32],
        sr: u32,
        windows: &[window::Window],
        workers: usize,
    ) -> Result<Vec<Word>, AsrError> {
        ensure_ort_dylib();
        // Задания: (индекс окна, offset, w.start, клип-копия O(окна)).
        let jobs: Vec<(usize, f64, f64, Vec<f32>)> = windows
            .iter()
            .enumerate()
            .filter_map(|(i, w)| {
                let a = ((w.start * sr as f64) as usize).min(audio.len());
                let b = ((w.end * sr as f64) as usize).min(audio.len());
                if b <= a {
                    None
                } else {
                    Some((i, w.offset(), w.start, audio[a..b].to_vec()))
                }
            })
            .collect();
        let n_slots = windows.len();
        let queue = std::sync::Arc::new(std::sync::Mutex::new(jobs));
        let results: std::sync::Arc<std::sync::Mutex<Vec<Option<(f64, Vec<Word>)>>>> =
            std::sync::Arc::new(std::sync::Mutex::new((0..n_slots).map(|_| None).collect()));
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let mut handles = Vec::new();
        for _ in 0..workers {
            let (q, res, fail) = (queue.clone(), results.clone(), failed.clone());
            let dir = self.tdt_dir.clone();
            let sr_c = sr;
            handles.push(std::thread::spawn(move || {
                let mut model = match ParakeetTDT::from_pretrained(&dir, Some(exec_config())) {
                    Ok(m) => m,
                    Err(_) => {
                        // не хватило VRAM/сессия не встала -> помечаем, воркер выходит
                        fail.store(true, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                };
                loop {
                    let job = { q.lock().unwrap().pop() };
                    let Some((i, off, w_start, clip)) = job else { break };
                    let clip_secs = clip.len() as f64 / sr_c as f64;
                    match model.transcribe_samples(clip, sr_c, 1, Some(TimestampMode::Words)) {
                        Ok(r) => {
                            let ws = words_from_tokens(r.tokens, off, clip_secs);
                            res.lock().unwrap()[i] = Some((w_start, ws));
                        }
                        Err(_) => {
                            fail.store(true, std::sync::atomic::Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        if failed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AsrError::Parakeet("the parallel window run failed (falling back to sequential)".into()));
        }
        let slots = std::sync::Arc::try_unwrap(results)
            .map_err(|_| AsrError::Parakeet("results arc".into()))?
            .into_inner()
            .map_err(|_| AsrError::Parakeet("results lock".into()))?;
        let mut all: Vec<Word> = Vec::new();
        for slot in slots.into_iter().flatten() {
            let (w_start, ws) = slot;
            append_dedup_words(&mut all, ws, w_start);
        }
        Ok(all)
    }

    /// Прогнать модель на семплах 16k/mono и получить словные таймстемпы (TimestampMode::Words).
    fn transcribe_words(&mut self, audio: &[f32], sr: u32) -> Result<Vec<Word>, AsrError> {
        let model = self.model()?;
        let res = model
            .transcribe_samples(audio.to_vec(), sr, 1, Some(TimestampMode::Words))
            .map_err(|e| AsrError::Parakeet(e.to_string()))?;
        Ok(words_from_tokens(res.tokens, 0.0, audio.len() as f64 / sr as f64))
    }

    /// DIARIZE-FIRST: транскрибировать КАЖДУЮ реплику отдельно (один спикер на сегмент). Порт
    /// asr.transcribe_turns: клип на turn, транскрипция, паузная разбивка внутри turn.
    pub fn transcribe_turns(
        &mut self,
        wav: impl AsRef<Path>,
        turns: &[Turn],
    ) -> Result<Vec<SpeakerSegment>, AsrError> {
        eprintln!("[asr] Parakeet transcribe_turns: {} turns, wav={}", turns.len(), wav.as_ref().display());
        let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
        let min_len = (0.2 * sr as f64) as usize;
        let mut out = Vec::new();
        for t in turns {
            let a = (t.start * sr as f64) as usize;
            let b = ((t.end * sr as f64) as usize).min(audio.len());
            if b <= a || (b - a) < min_len {
                continue; // слишком коротко для транскрипции
            }
            let clip = &audio[a..b];
            let words = self.transcribe_words(clip, sr)?;
            // Паузная разбивка ВНУТРИ реплики, чтобы длинный монолог не стал одним гигантским сегментом.
            for s in segment_words(&words, SEG_MAX_GAP, SEG_MAX_DUR) {
                out.push(SpeakerSegment {
                    start: t.start + s.start,
                    end: t.start + s.end,
                    text: s.text,
                    speaker: t.speaker,
                });
            }
        }
        Ok(out)
    }
}

impl AsrEngine for Asr {
    // The model's attention holds a few minutes of audio, so a recording is heard in windows cut at
    // pauses; a short one is a single window.
    fn transcribe(&mut self, wav: &Path, lang: &str) -> Result<Vec<Segment>, AsrError> {
        Asr::transcribe_windowed(self, wav, lang, None)
    }
    // Parakeet-TDT сам определяет язык (мультиязычная модель) — lang игнорируем, как и в whole-clip.
    fn transcribe_turns(&mut self, wav: &Path, turns: &[Turn], _lang: &str) -> Result<Vec<SpeakerSegment>, AsrError> {
        Asr::transcribe_turns(self, wav, turns)
    }
}

/// Диаризация: Nemotron 3 Diarization (Streaming Sortformer v3, до [`MAX_SPEAKERS`] спикеров) -> реплики
/// [(start,end,speaker)] в секундах, speaker перенумерован 0..k-1. diar_onnx — путь к nemotron3_diar_v3.onnx.
///
/// Файл целиком известен заранее, поэтому профиль стриминга — `offline()` (самый длинный контекст модели,
/// буфер 30.4 с), постобработка — дефолт parakeet-rs, воспроизводящий `diarize()` NeMo для этой модели.
pub fn diarize(
    wav: impl AsRef<Path>,
    diar_onnx: impl AsRef<Path>,
) -> Result<Vec<Turn>, AsrError> {
    ensure_ort_dylib(); // до первого касания ort — иначе дедлок на чужой system32 DLL
    let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
    let mut sf = Sortformer::with_config(diar_onnx.as_ref(), Some(exec_config()), DiarizationConfig::default())
        .map_err(|e| AsrError::Parakeet(e.to_string()))?;
    sf.set_profile(StreamingProfile::offline())
        .map_err(|e| AsrError::Parakeet(e.to_string()))?;
    // ДЛИННЫЙ файл — оконная диаризация: окна DIAR_WIN с перекрытием, спикеры соседних окон
    // сшиваются по пересечению реплик в оверлапе (страховка от роста памяти/деградации на часах).
    let total = audio.len() as f64 / sr as f64;
    let raw: Vec<Turn> = if total > DIAR_WINDOW_GATE_SECS {
        diarize_windowed(&mut sf, &audio, sr)?
    } else {
        let segs = sf
            .diarize(audio, sr, 1)
            .map_err(|e| AsrError::Parakeet(e.to_string()))?;
        segments_to_turns(&segs, 0.0)
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
    sf: &mut Sortformer,
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
        let segs = sf
            .diarize(audio[a0..a1].to_vec(), sr, 1)
            .map_err(|e| AsrError::Parakeet(e.to_string()))?;
        let local: Vec<Turn> = segments_to_turns(&segs, w0);
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
    diar_onnx: impl AsRef<Path>,
    merge_gap: f64,
    min_speaker_dur: f64,
) -> Result<DiarTurns, AsrError> {
    let raw = diarize(wav, diar_onnx)?;
    Ok(merge_turns(&raw, merge_gap, min_speaker_dur))
}

pub fn turns_with_speaker_count(
    wav: impl AsRef<Path>,
    diar_onnx: impl AsRef<Path>,
    count: usize,
    embed: &mut impl FnMut(&[f32]) -> Result<Vec<f32>, String>,
) -> Result<DiarTurns, AsrError> {
    if !(1..=MAX_SPEAKERS).contains(&count) {
        return Err(AsrError::SpeakerCount { max: MAX_SPEAKERS });
    }
    if count == 1 {
        return Ok(DiarTurns { turns: Vec::new(), n_speakers: 1, ref_windows: Default::default() });
    }
    ensure_ort_dylib();
    let (audio, sr) = load_wav_16k_mono(wav.as_ref())?;
    let mut sf = Sortformer::with_config(diar_onnx.as_ref(), Some(exec_config()), DiarizationConfig::default())
        .map_err(|e| AsrError::Parakeet(e.to_string()))?;
    sf.set_profile(StreamingProfile::offline()).map_err(|e| AsrError::Parakeet(e.to_string()))?;
    let total = audio.len() as f64 / sr as f64;
    let window = known_diarization_window(total, continuous_diarization_enabled().map_err(AsrError::Parakeet)?);
    let mut tracker = known_speakers::VoiceTracker::new(count);
    let mut out = Vec::new();
    for range in known_speakers::diar_windows(total, window, DIAR_OVERLAP_SECS) {
        let a0 = (range.start * sr as f64) as usize;
        let a1 = ((range.end * sr as f64) as usize).min(audio.len());
        let segs = sf.diarize(audio[a0..a1].to_vec(), sr, 1).map_err(|e| AsrError::Parakeet(e.to_string()))?;
        let local = segments_to_turns(&segs, range.start);
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

/// Кадр энкодера TDT (8× сабсэмплинг по 10 мс) — временное разрешение токена.
const TDT_FRAME_SECS: f64 = 0.08;

/// Токены режима `TimestampMode::Words` (уже слова, секунды от начала клипа) -> [`Word`] со сдвигом
/// `offset`. Конец слова = кадр эмиссии последнего токена + его TDT-длительность; длительность 0
/// (следующий токен в том же кадре) даёт end == start, и такому слову отдаётся кадр эмиссии, не дальше
/// конца клипа `clip_secs`.
fn words_from_tokens(tokens: Vec<parakeet_rs::TimedToken>, offset: f64, clip_secs: f64) -> Vec<Word> {
    let mut out: Vec<Word> = tokens
        .into_iter()
        .filter_map(|t| {
            let word = t.text.trim();
            if word.is_empty() {
                return None;
            }
            let start = t.start as f64;
            let mut end = (t.end as f64).max(start);
            if end <= start {
                end = (start + TDT_FRAME_SECS).min(clip_secs).max(start);
            }
            Some(Word { word: word.to_string(), start: start + offset, end: end + offset })
        })
        .collect();
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    attach_punct(out)
}

fn is_closing_punct(w: &str) -> bool {
    w.chars().all(|c| matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '…' | ')' | ']' | '}' | '%'))
}

fn is_opening_punct(w: &str) -> bool {
    w.chars().all(|c| matches!(c, '(' | '[' | '{' | '¿' | '¡'))
}

/// Хвост слова, записанного без пробела после точки или запятой («e» + «.g.», «3» + «.5», «т» + «.е.»):
/// parakeet-rs начинает на знаке новое слово и дописывает к нему следующий токен без границы слова.
fn is_glued_tail(w: &str) -> bool {
    let mut chars = w.chars();
    matches!(chars.next(), Some('.' | ',')) && chars.next().is_some_and(char::is_alphanumeric)
}

/// parakeet-rs в режиме Words отдаёт знак препинания отдельным словом: закрывающий приклеивается к
/// предыдущему слову, открывающий — к следующему; тайминги слова не меняются. Хвост слова после точки
/// или запятой без пробела ([`is_glued_tail`]) возвращается в своё слово, конец слова — конец хвоста.
fn attach_punct(words: Vec<Word>) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut opening = String::new();
    for mut w in words {
        if is_closing_punct(&w.word) {
            if let Some(prev) = out.last_mut() {
                prev.word.push_str(&w.word);
                continue;
            }
        }
        if is_glued_tail(&w.word) && opening.is_empty() {
            if let Some(prev) = out.last_mut() {
                prev.word.push_str(&w.word);
                prev.end = prev.end.max(w.end);
                continue;
            }
        }
        if is_opening_punct(&w.word) {
            opening.push_str(&w.word);
            continue;
        }
        if !opening.is_empty() {
            w.word.insert_str(0, &std::mem::take(&mut opening));
        }
        out.push(w);
    }
    if let (false, Some(prev)) = (opening.is_empty(), out.last_mut()) {
        prev.word.push_str(&opening);
    }
    out
}

/// Нормализовать слово для сравнения на шве: lowercase + снять КОНЕЧНУЮ пунктуацию (.,!?…;:).
/// Это чинит баг дедупа: ASR на границе окна отдаёт то же слово с разной пунктуацией ("fox" vs
/// "fox,") — по сырому lowercased-тексту это «разные» слова, дубль просачивался. Ведущую пунктуацию
/// (кавычки/тире) тоже снимаем, чтобы «„world" == world». Всё остальное (внутри-словные дефисы,
/// апострофы) сохраняем — они различают настоящие слова.
fn norm_word(w: &str) -> String {
    w.trim()
        .trim_matches(|c: char| {
            matches!(c, '.' | ',' | '!' | '?' | '…' | ';' | ':' | '"' | '\'' | '«' | '»' | '„' | '“' | '”' | '‘' | '’' | '(' | ')' | '[' | ']' | '-' | '—' | '–')
        })
        .to_lowercase()
}

/// Дописать слова нового окна к общему потоку с де-дупом ре-транскрипции шва (overlap соседних окон).
///
/// Окна режутся с захлёстом (`WindowConfig::overlap`), поэтому у соседних окон есть общая зона
/// `[window_start, prev_last_end]`, где ОБА окна распознают одни и те же слова. Дубль возникает ТОЛЬКО
/// здесь. Слово нового окна отбрасываем, если оно (а) попадает в зону захлёста по времени И (б) по
/// нормализованному тексту (см. [`norm_word`] — без конечной пунктуации) совпадает с уже добавленным
/// словом в близком времени. Настоящий быстрый повтор («да да») сюда не попадает: его второй экземпляр
/// либо за пределами зоны захлёста, либо это ТОТ ЖЕ поток одного окна (дедуп межоконный, incoming — из
/// нового окна). Порядок сохраняется. Первое окно (all пусто) дописывается как есть.
pub(crate) fn append_dedup_words(all: &mut Vec<Word>, incoming: Vec<Word>, window_start: f64) {
    if all.is_empty() {
        all.extend(incoming);
        return;
    }
    // Допуск по времени: на шве два окна дают чуть разные тайминги одного слова. 0.6с покрывает
    // overlap≈0.5с плюс дрожание ASR. Порог узкий намеренно — иначе съест настоящий быстрый повтор.
    const DUP_TIME_TOL: f64 = 0.6;
    // Верхняя граница зоны захлёста = конец уже покрытого предыдущим окном времени.
    let last_end = all.last().map(|w| w.end).unwrap_or(f64::NEG_INFINITY);
    for w in incoming {
        // Дубль возможен ТОЛЬКО в зоне захлёста: слово началось до конца покрытого времени (+допуск)
        // И не раньше начала общей зоны. Всё, что позже покрытого времени, — новый контент, добавляем.
        let in_overlap_zone = w.start >= window_start - DUP_TIME_TOL && w.start <= last_end + DUP_TIME_TOL;
        if in_overlap_zone && is_dup_of_tail(all, &w, DUP_TIME_TOL) {
            continue;
        }
        all.push(w);
    }
}

/// Является ли слово `w` ре-транскрипцией уже добавленного слова на шве: нормализованный текст (без
/// конечной пунктуации, lowercase) совпадает И временные интервалы РЕАЛЬНО пересекаются (одно и то же
/// физическое аудио распознано дважды). Смотрим только на хвост `all` — захлёст локален по времени.
///
/// КЛЮЧЕВОЕ отличие от настоящего быстрого повтора: ре-транскрипция шва занимает ТУ ЖЕ временную
/// область, что уже покрытое слово (интервалы пересекаются). Настоящий повтор («да да») — это ДВА
/// последовательных, НЕ пересекающихся во времени интервала. Поэтому дубль = пересечение интервалов
/// (>0) при совпадении нормализованного текста; проверку по близости start оставляем как дешёвый
/// пред-фильтр, но решает именно пересечение.
fn is_dup_of_tail(all: &[Word], w: &Word, tol: f64) -> bool {
    let wn = norm_word(&w.word);
    if wn.is_empty() {
        return false;
    }
    // до 6 последних слов достаточно для типичного overlap (0.5с ≈ 2-3 слова)
    for prev in all.iter().rev().take(6) {
        if norm_word(&prev.word) != wn {
            continue;
        }
        // Пересечение интервалов [prev.start,prev.end] и [w.start,w.end]. >0 -> то же физическое
        // аудио (ре-транскрипция шва). Настоящий повтор «да да» не пересекается (start_2 > end_1).
        let overlap = prev.end.min(w.end) - prev.start.max(w.start);
        if overlap > 0.0 {
            return true;
        }
        // Вырожденный случай нулевой длительности (start==end): fallback на близость start в tol.
        if (prev.end - prev.start).abs() < 1e-6 && (prev.start - w.start).abs() <= tol {
            return true;
        }
    }
    false
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
mod dedup_tests {
    use super::*;

    fn w(word: &str, start: f64, end: f64) -> Word {
        Word { word: word.into(), start, end }
    }

    #[test]
    fn first_window_appended_verbatim() {
        let mut all: Vec<Word> = Vec::new();
        append_dedup_words(&mut all, vec![w("a", 0.0, 0.3), w("b", 0.3, 0.6)], 0.0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].word, "a");
    }

    #[test]
    fn overlap_duplicate_dropped() {
        // окно 1 дало "...world" на 9.8-10.0; окно 2 (window_start=9.5, overlap) повторяет "world"
        // на ~9.85 -> дубль отбрасываем, а новое "next" на 10.2 добавляем.
        let mut all: Vec<Word> = vec![w("hello", 9.4, 9.7), w("world", 9.8, 10.0)];
        let incoming = vec![w("world", 9.85, 10.05), w("next", 10.2, 10.5)];
        append_dedup_words(&mut all, incoming, 9.5);
        let texts: Vec<&str> = all.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(texts, vec!["hello", "world", "next"], "дубль 'world' должен уйти: {texts:?}");
    }

    #[test]
    fn non_overlap_word_kept_even_if_same_text_far_away() {
        // одинаковый текст, но далеко по времени (>tol) — НЕ дубль, оставляем оба
        let mut all: Vec<Word> = vec![w("yes", 1.0, 1.2)];
        append_dedup_words(&mut all, vec![w("yes", 50.0, 50.2)], 49.5);
        assert_eq!(all.len(), 2, "далёкий повтор — не дубль overlap");
    }

    #[test]
    fn offset_applied_monotonic_after_dedup() {
        // проверяем, что после дедупа порядок по времени сохранён (монотонность стыка)
        let mut all: Vec<Word> = vec![w("one", 88.0, 88.4), w("two", 88.5, 89.0)];
        // новое окно с offset уже применён вызывающим; overlap повторяет "two"
        append_dedup_words(&mut all, vec![w("two", 88.55, 89.05), w("three", 89.2, 89.6)], 88.2);
        for pair in all.windows(2) {
            assert!(pair[1].start >= pair[0].start - 1e-9, "не монотонно: {all:?}");
        }
        assert_eq!(all.last().unwrap().word, "three");
    }

    #[test]
    fn seam_duplicate_with_different_punctuation_dropped() {
        // ГЛАВНЫЙ баг: окно 1 закончилось "fox," (с запятой), окно 2 на шве ре-транскрибировало
        // то же слово как "fox" (без) -> по сырому тексту это «разные» слова и дубль просачивался.
        // norm_word снимает конечную пунктуацию -> "fox,"=="fox" -> дубль уходит, сегмент не рвётся.
        let mut all: Vec<Word> = vec![w("the", 9.4, 9.6), w("fox,", 9.7, 10.0)];
        let incoming = vec![w("fox", 9.75, 10.05), w("jumps.", 10.2, 10.6)];
        append_dedup_words(&mut all, incoming, 9.5);
        let texts: Vec<&str> = all.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(texts, vec!["the", "fox,", "jumps."], "дубль 'fox' (с/без пунктуации): {texts:?}");
    }

    #[test]
    fn seam_duplicate_punct_on_new_side_dropped() {
        // Симметрично: окно 1 дало "world" (без), окно 2 на шве — "world." (с точкой). Тоже дубль.
        let mut all: Vec<Word> = vec![w("hello", 5.2, 5.5), w("world", 5.6, 5.9)];
        let incoming = vec![w("world.", 5.62, 5.92), w("bye", 6.1, 6.3)];
        append_dedup_words(&mut all, incoming, 5.4);
        let texts: Vec<&str> = all.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(texts, vec!["hello", "world", "bye"], "дубль 'world.'/'world': {texts:?}");
    }

    #[test]
    fn genuine_fast_repeat_kept_across_windows() {
        // Настоящий быстрый повтор "да да" (0.15с врозь) НЕ должен схлопнуться. Второе "да" из нового
        // окна начинается ПОСЛЕ конца покрытого времени (last_end=10.0) -> не в зоне захлёста -> живёт.
        let mut all: Vec<Word> = vec![w("да", 9.8, 10.0)];
        let incoming = vec![w("да", 10.05, 10.25), w("точно", 10.4, 10.7)];
        // window_start ниже, но второе "да" за last_end+tol не попадает в зону -> сохраняем
        append_dedup_words(&mut all, incoming, 9.6);
        let texts: Vec<&str> = all.iter().map(|x| x.word.as_str()).collect();
        assert_eq!(texts, vec!["да", "да", "точно"], "быстрый повтор не должен схлопнуться: {texts:?}");
    }

    #[test]
    fn norm_strips_terminal_and_wrapping_punctuation() {
        assert_eq!(norm_word("Fox,"), "fox");
        assert_eq!(norm_word("«world»"), "world");
        assert_eq!(norm_word("End…"), "end");
        // внутрисловные апострофы/дефисы НЕ трогаем (различают настоящие слова)
        assert_eq!(norm_word("don't"), "don't");
        assert_eq!(norm_word("well-known"), "well-known");
    }
}

#[cfg(test)]
mod diar_word_tests {
    use super::*;

    fn tok(text: &str, start: f32, end: f32) -> parakeet_rs::TimedToken {
        parakeet_rs::TimedToken { text: text.into(), start, end }
    }

    #[test]
    fn zero_duration_word_gets_its_frame_not_clip_end() {
        let ws = words_from_tokens(
            vec![tok(" Well", 6.0, 6.16), tok(" so", 6.24, 6.24), tok(" ", 6.3, 6.3), tok(" Next", 7.0, 7.3)],
            0.0,
            22.0,
        );
        let got: Vec<(&str, f64, f64)> = ws.iter().map(|w| (w.word.as_str(), w.start, w.end)).collect();
        assert_eq!(got.len(), 3, "пустой токен отброшен: {got:?}");
        assert!((ws[1].start - 6.24).abs() < 1e-5);
        assert!((ws[1].end - (6.24 + TDT_FRAME_SECS)).abs() < 1e-5, "нулевая длительность -> один кадр: {got:?}");
    }

    #[test]
    fn zero_duration_word_clamped_to_clip_end_and_offset_applied() {
        let ws = words_from_tokens(vec![tok("end.", 9.98, 9.98)], 100.0, 10.0);
        assert!((ws[0].start - 109.98).abs() < 1e-4);
        assert!((ws[0].end - 110.0).abs() < 1e-4, "кадр не выходит за конец клипа: {:?}", ws[0]);
    }

    #[test]
    fn punctuation_joins_neighbour_words() {
        let ws = words_from_tokens(
            vec![
                tok(" Where's", 0.5, 0.9),
                tok(" uniform", 1.0, 1.6),
                tok("?", 1.68, 1.76),
                tok(" ¿", 2.0, 2.0),
                tok(" Qué", 2.1, 2.4),
                tok("?", 2.5, 2.5),
                tok(" Bien", 3.0, 3.3),
                tok(".", 3.4, 3.4),
            ],
            0.0,
            10.0,
        );
        let got: Vec<&str> = ws.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(got, ["Where's", "uniform?", "¿Qué?", "Bien."]);
        assert!((ws[1].end - 1.6).abs() < 1e-5, "тайминг слова не тянется к знаку: {:?}", ws[1]);
        assert!((ws[2].start - 2.1).abs() < 1e-5, "открывающий знак не сдвигает начало слова: {:?}", ws[2]);
        let seg = segment::segment_words(&ws, segment::SEG_MAX_GAP, segment::SEG_MAX_DUR);
        assert_eq!(seg[0].text, "Where's uniform?");
    }

    #[test]
    fn dotted_abbreviations_and_decimals_stay_one_word() {
        // Токены так, как их группирует parakeet-rs: знак начинает слово, токен без «▁» дописывается к нему.
        let ws = words_from_tokens(
            vec![
                tok(" e", 0.0, 0.1),
                tok(".g", 0.1, 0.2),
                tok(".", 0.2, 0.2),
                tok(" apples", 0.3, 0.7),
                tok(" 3", 1.0, 1.1),
                tok(".5", 1.1, 1.3),
                tok(" т", 1.6, 1.7),
                tok(".е", 1.7, 1.8),
                tok(".", 1.8, 1.8),
                tok(" 1", 2.0, 2.1),
                tok(",5", 2.1, 2.3),
                tok(" Next", 2.6, 2.9),
                tok(".", 2.9, 2.9),
            ],
            0.0,
            10.0,
        );
        let got: Vec<&str> = ws.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(got, ["e.g.", "apples", "3.5", "т.е.", "1,5", "Next."]);
        assert!((ws[2].end - 1.3).abs() < 1e-5, "конец слова — конец хвоста: {:?}", ws[2]);
        let seg = segment::segment_words(&ws, segment::SEG_MAX_GAP, 100.0);
        assert_eq!(seg.len(), 1, "{seg:?}");
    }

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
