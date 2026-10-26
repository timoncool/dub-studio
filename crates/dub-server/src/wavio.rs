//! Мини WAV I/O для сборки таймлайна дубляжа (assemble.timeline): чтение WAV в mono f32 + запись
//! mono f32. Higgs отдаёт PCM f32 через audiocpp::encode_wav (PCM16 WAV) — hound читает оба формата,
//! стерео сводим в mono усреднением (как s.mean(axis=1) в питоне).

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::path::Path;

/// Прочитать WAV -> (mono f32 сэмплы, sample_rate).
pub fn read_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut r = WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => r
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read f32: {e}"))?,
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("read int: {e}"))?
        }
    };
    let mono: Vec<f32> = if ch <= 1 {
        interleaved
    } else {
        interleaved
            .chunks(ch)
            .map(|fr| fr.iter().sum::<f32>() / ch as f32)
            .collect()
    };
    Ok((mono, spec.sample_rate))
}

/// Прочитать отрезок [start, end) секунд WAV -> (mono f32 сэмплы, sample_rate).
pub fn read_span_mono_f32(path: &Path, start: f64, end: f64) -> Result<(Vec<f32>, u32), String> {
    let mut r = WavReader::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let total = r.duration();
    let from = ((start.max(0.0) * spec.sample_rate as f64) as u32).min(total);
    let frames = (((end - start).max(0.0) * spec.sample_rate as f64) as u32).min(total - from) as usize;
    r.seek(from).map_err(|e| format!("seek {}: {e}", path.display()))?;
    let interleaved: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => r
            .samples::<f32>()
            .take(frames * ch)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("read f32: {e}"))?,
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .take(frames * ch)
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("read int: {e}"))?
        }
    };
    let mono = if ch <= 1 { interleaved } else { interleaved.chunks(ch).map(|fr| fr.iter().sum::<f32>() / ch as f32).collect() };
    Ok((mono, spec.sample_rate))
}

fn peaks_args(video: &Path) -> Vec<std::ffi::OsString> {
    let mut a: Vec<std::ffi::OsString> = vec!["-v".into(), "quiet".into(), "-i".into(), video.into()];
    a.extend(["-vn", "-af", crate::media::SYNC_AF, "-ac", "1", "-ar", "8000", "-f", "s16le", "-"].map(Into::into));
    a
}

/// Даунсэмпл-пики аудио для WaveformTimeline. Порт app.py._compute_peaks: ffmpeg -> s16le 8kHz mono,
/// N бакетов, в каждом max(|amp|)/max_amp, округление до 3 знаков. CPU, вне GPU-воркера. Сбой ffmpeg
/// или тишина -> пустой список (как питон — не отравляем кэш).
pub fn waveform_peaks(video: &Path, n: usize) -> Vec<f64> {
    let out = dub_core::proc::host_command(dub_core::proc::ffmpeg()).args(peaks_args(video)).output();
    let bytes = match out {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    // s16le -> i16 сэмплы.
    let samples: Vec<f32> = bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32)
        .collect();
    if samples.is_empty() {
        return Vec::new();
    }
    let max_amp = samples.iter().fold(1.0f32, |m, &s| m.max(s.abs()));
    let nb = n.min(samples.len()).max(1);
    // np.array_split: первые (len % nb) бакетов на 1 длиннее.
    let base = samples.len() / nb;
    let rem = samples.len() % nb;
    let mut peaks = Vec::with_capacity(nb);
    let mut i = 0;
    for b in 0..nb {
        let len = base + if b < rem { 1 } else { 0 };
        let slice = &samples[i..i + len];
        i += len;
        let mx = slice.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        peaks.push(((mx / max_amp) as f64 * 1000.0).round() / 1000.0);
    }
    peaks
}

/// Записать mono f32 -> WAV IEEE-float32.
pub fn write_mono_f32(path: &Path, data: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut w = WavWriter::create(path, spec).map_err(|e| format!("create {}: {e}", path.display()))?;
    for &s in data {
        w.write_sample(s).map_err(|e| format!("write: {e}"))?;
    }
    w.finalize().map_err(|e| format!("finalize: {e}"))?;
    Ok(())
}

/// PCM f32 as a PCM16 WAV in memory (a basic PCM header, which every engine of the studio reads as a reference).
pub fn encode_wav(samples: &[f32], sample_rate: i32, channels: i32) -> Vec<u8> {
    let spec = WavSpec { channels: channels.max(1) as u16, sample_rate: sample_rate.max(1) as u32, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut out = std::io::Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    {
        let mut w = WavWriter::new(&mut out, spec).expect("a WAV header into memory");
        for &x in samples {
            w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16).expect("a sample into memory");
        }
        w.finalize().expect("a WAV into memory");
    }
    out.into_inner()
}

#[cfg(test)]
mod tests {
    #[test]
    fn waveform_reads_audio_by_its_timestamps() {
        let args = super::peaks_args(std::path::Path::new("in.mp4"));
        let af = args.iter().position(|a| a == "-af").expect("-af");
        assert!(args[af + 1].to_string_lossy().starts_with(crate::media::SYNC_AF));
    }
}
