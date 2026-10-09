//! Ресемплинг с ограничением полосы (перенос audio_post::resample::mono из студий): всё выше новой
//! частоты Найквиста отфильтровывается, а не заворачивается обратно в полосу шумом — шипящие 8–12 кГц
//! синтеза 24 кГц не должны попадать в 4–8 кГц входа распознавателя 16 кГц.

use audioadapter::Adapter;
use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

use crate::AsrError;

/// Один канал, с ограничением полосы.
pub fn mono(samples: &[f32], from: u32, to: u32) -> Result<Vec<f32>, AsrError> {
    if from == to || samples.len() < 2 {
        return Ok(samples.to_vec());
    }
    let input = vec![samples.iter().map(|&s| s as f64).collect::<Vec<_>>()];
    let adapter = SequentialSliceOfVecs::new(&input, 1, samples.len())
        .map_err(|e| AsrError::Resample(format!("buffer wrap {from}->{to} Hz: {e}")))?;
    let mut resampler = Fft::<f64>::new(from as usize, to as usize, 1024, 1, FixedSync::Both)
        .map_err(|e| AsrError::Resample(format!("resampler {from}->{to} Hz: {e}")))?;
    let output = resampler
        .process_all(&adapter, samples.len(), None)
        .map_err(|e| AsrError::Resample(format!("resampling {from}->{to} Hz: {e}")))?;
    (0..output.frames())
        .map(|frame| {
            output
                .read_sample(0, frame)
                .map(|v| v as f32)
                .ok_or_else(|| AsrError::Resample(format!("frame {frame} is outside the resampler buffer")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sr: u32, secs: f64) -> Vec<f32> {
        (0..(sr as f64 * secs) as usize)
            .map(|i| (0.5 * (2.0 * std::f64::consts::PI * freq * i as f64 / sr as f64).sin()) as f32)
            .collect()
    }

    /// Амплитуда тона `freq` в сигнале (проекция на синус и косинус середины клипа, без краёв).
    fn tone_level(x: &[f32], sr: u32, freq: f64) -> f64 {
        let a = x.len() / 4;
        let b = x.len() * 3 / 4;
        let (mut s, mut c) = (0.0f64, 0.0f64);
        for (i, &v) in x[a..b].iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * freq * (a + i) as f64 / sr as f64;
            s += v as f64 * ph.sin();
            c += v as f64 * ph.cos();
        }
        2.0 * (s * s + c * c).sqrt() / (b - a) as f64
    }

    fn linear(input: &[f32], src_sr: u32, dst_sr: u32) -> Vec<f32> {
        let ratio = dst_sr as f64 / src_sr as f64;
        let out_len = ((input.len() as f64) * ratio).round() as usize;
        (0..out_len)
            .map(|i| {
                let pos = i as f64 / ratio;
                let idx = pos.floor() as usize;
                let frac = (pos - idx as f64) as f32;
                let a = input.get(idx).copied().unwrap_or(0.0);
                let b = input.get(idx + 1).copied().unwrap_or(a);
                a + (b - a) * frac
            })
            .collect()
    }

    #[test]
    fn a_tone_above_the_new_nyquist_is_filtered_not_folded() {
        let x = sine(11_000.0, 24_000, 1.0);
        let lin = linear(&x, 24_000, 16_000);
        let band = mono(&x, 24_000, 16_000).unwrap();
        // 11 кГц на 16 кГц заворачивается в 16−11 = 5 кГц.
        let alias_lin = tone_level(&lin, 16_000, 5_000.0);
        let alias_band = tone_level(&band, 16_000, 5_000.0);
        assert!(alias_lin > 0.05, "линейная интерполяция должна давать ложный тон 5 кГц: {alias_lin}");
        assert!(alias_band < 0.005, "алиасинг не подавлен: {alias_band}");
    }

    #[test]
    fn a_tone_inside_the_band_keeps_its_level_and_length() {
        let x = sine(1_000.0, 24_000, 1.0);
        let y = mono(&x, 24_000, 16_000).unwrap();
        assert!((y.len() as i64 - 16_000).abs() <= 1, "длина {}", y.len());
        let lvl = tone_level(&y, 16_000, 1_000.0);
        assert!((lvl - 0.5).abs() < 0.01, "уровень 1 кГц {lvl}");
    }

    #[test]
    fn same_rate_passes_through() {
        let x = vec![0.1f32, -0.2, 0.3];
        assert_eq!(mono(&x, 16_000, 16_000).unwrap(), x);
    }
}
