//! Облачный TTS через OpenRouter (`/audio/speech`, формат pcm) напрямую из Rust. PCM 24 кГц моно оборачивается
//! в WAV без ffmpeg; другая частота, стерео или сжатый ответ (mp3) приводятся ffmpeg к 24 кГц моно один раз:
//! таймлайн дубляжа кладёт seg-файлы по частоте первого (TTS = 24 кГц) без ресемпла.
//! Результат кладётся ПРЯМО в seg-файл дубляжа, fit_to_slot читает его ОДИН раз. Модель/голос — только из
//! настроек юзера (or_tts_model/or_tts_voice), без хардкода.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicU64;

use dub_llm::openrouter::{OpenRouter, SpeechAudio};

pub struct Job {
    pub out: PathBuf,
    pub key: String,
    pub text: String,
    pub voice: String,
    pub style: String,
}

pub fn style(s: &dub_core::Segment) -> &str {
    s.extra
        .get("tts_style")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// Уникализатор temp-файлов раскодировки (для потокобезопасности synth_batch).
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "windows")]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(target_os = "windows"))]
const FFMPEG: &str = "ffmpeg";

/// Синтез одной реплики -> WAV-байты (готовы к записи в seg-файл). `voice` пусто -> дефолт из настроек.
pub fn synth_audio(models_root: &Path, text: &str, voice: &str,
    style: &str,
) -> Result<Vec<u8>, String> {
    if crate::models::tts_provider(models_root) == "google" {
        return Err("Google TTS must use the journalled project synthesis stage; no untracked standard request is allowed".into());
    }
    let key = crate::models::openrouter_key()
        .ok_or("облачный TTS включён, но ключ OpenRouter не задан")?;
    let model = crate::models::openrouter_model(models_root, "tts");
    if model.is_empty() {
        return Err("TTS-модель не выбрана в настройках (Облачные модели · OpenRouter)".into());
    }
    let v = if voice.trim().is_empty() {
        crate::models::openrouter_tts_voice(models_root)
    } else {
        voice.trim().to_string()
    };
    if v.is_empty() {
        return Err("голос TTS не задан в настройках (у каждой модели свои голоса)".into());
    }
    let client = OpenRouter::new(Some(key)).map_err(|e| format!("облачный TTS: {e:#}"))?;
    let bytes = match client.speech_with_style(&model, text, &v, style).map_err(|e| format!("облачный TTS: {e:#}"))? {
        SpeechAudio::Wav(wav) if is_seg_format(&wav) => wav,
        SpeechAudio::Wav(wav) => to_seg_wav("audio/wav", &wav)?,
        SpeechAudio::Encoded { mime, bytes } => to_seg_wav(&mime, &bytes)?,
    };
    if bytes.len() < 200 {
        return Err(format!("облачный TTS: слишком короткое аудио ({} байт)", bytes.len()));
    }
    Ok(bytes)
}

pub fn seg_audio(mime: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if is_seg_format(&bytes) {
        return Ok(bytes);
    }
    if mime.starts_with("audio/L16") || mime.starts_with("audio/pcm") {
        let rate = mime
            .split(';')
            .find_map(|p| p.trim().strip_prefix("rate="))
            .and_then(|n| n.parse().ok())
            .ok_or("Google PCM has no sample rate")?;
        let wav = dub_llm::openrouter::pcm16_to_wav(&bytes, rate, 1).map_err(|e| e.to_string())?;
        return if rate == SEG_RATE {
            Ok(wav)
        } else {
            to_seg_wav("audio/wav", &wav)
        };
    }
    to_seg_wav(mime, &bytes)
}

/// Частота seg-файлов дубляжа.
const SEG_RATE: u32 = 24_000;

/// WAV уже в формате seg-файлов (24 кГц моно).
fn is_seg_format(wav: &[u8]) -> bool {
    hound::WavReader::new(std::io::Cursor::new(wav))
        .map(|reader| reader.spec().sample_rate == SEG_RATE && reader.spec().channels == 1)
        .unwrap_or(false)
}

/// Ответ модели в другом формате -> WAV PCM16 24 кГц моно через ffmpeg (конвейер читает seg как WAV).
fn to_seg_wav(mime: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let uid = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("dub_cloud_tts_{}_{}.bin", std::process::id(), uid));
    let wav_tmp = tmp.with_extension("wav");
    std::fs::write(&tmp, bytes).map_err(|e| format!("облачный TTS ({mime}): {e}"))?;
    let out = dub_core::proc::output(Command::new(FFMPEG).args([
        "-v", "error", "-i", &tmp.to_string_lossy(), "-ar", &SEG_RATE.to_string(), "-ac", "1", "-c:a", "pcm_s16le", "-y", &wav_tmp.to_string_lossy(),
    ]));
    let _ = std::fs::remove_file(&tmp);
    let out = out.map_err(|e| format!("ffmpeg {mime}->wav: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&wav_tmp);
        return Err(format!("ffmpeg {mime}->wav: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let wav = std::fs::read(&wav_tmp).map_err(|e| format!("чтение облачного wav: {e}"));
    let _ = std::fs::remove_file(&wav_tmp);
    wav
}

/// Параллельный пре-синтез: гонит `jobs` (out-путь, текст, голос) в `concurrency` потоков (OpenRouter
/// держит десятки конкурентных запросов). Каждый успешный сегмент пишется в свой out-файл атомарно;
/// провал -> false (основной цикл ретраит/фолбэкнет на оригинал). Возвращает успех по каждой джобе.
/// Потоки привязаны к джобе вызывающего: её отмена прекращает раздачу новых сегментов, а процессы
/// сайдкара попадают в учёт джобы.
pub fn synth_batch(models_root: &Path,
    wd: &Path,
    jobs: Vec<Job>, concurrency: usize,
    progress: &crate::render::Progress,
) -> Result<Vec<bool>, String> {
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    if crate::models::tts_provider(models_root) == "google" {
        return crate::google_tts::synth_jobs(models_root, wd, &jobs, concurrency, progress);
    }
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let n = jobs.len();
    let ok: Vec<AtomicBool> = (0..n).map(|_| AtomicBool::new(false)).collect();
    if n == 0 {
        return Ok(Vec::new());
    }
    let workers = concurrency.max(1).min(n);
    let next = AtomicUsize::new(0);
    let ctl = crate::jobs::current();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (next, ok, jobs, ctl) = (&next, &ok, &jobs, ctl.clone());
            scope.spawn(move || {
                let _entered = ctl.clone().map(crate::jobs::enter);
                loop {
                    if ctl.as_ref().is_some_and(|c| c.is_cancelled()) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= jobs.len() {
                        break;
                    }
                    let j = &jobs[i];
                    match synth_audio(models_root, &j.text, &j.voice, &j.style).and_then(|bytes| dub_core::atomic::write(&j.out, &bytes)) {
                        Ok(()) => ok[i].store(true, Ordering::Relaxed),
                        Err(e) => progress(serde_json::json!({"type":"progress","stage":"tts","msg":format!("OpenRouter {}: {e}",j.key)})),
                    }
                }
            });
        }
    });
    Ok(ok.iter().map(|b| b.load(Ordering::Relaxed)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_24k_mono_goes_to_the_timeline_untouched() {
        let pcm: Vec<u8> = [0i16, 100, -100, 50].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert!(is_seg_format(&dub_llm::openrouter::pcm16_to_wav(&pcm, 24_000, 1).unwrap()));
        assert!(!is_seg_format(&dub_llm::openrouter::pcm16_to_wav(&pcm, 44_100, 1).unwrap()));
        assert!(!is_seg_format(&dub_llm::openrouter::pcm16_to_wav(&pcm, 24_000, 2).unwrap()));
        assert!(!is_seg_format(b"ID3 mp3 bytes"));
    }
}
