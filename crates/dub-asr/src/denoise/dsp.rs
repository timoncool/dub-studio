//! DPDFNet spectral processing: centered reflect padding, Vorbis-window STFT, attenuation and inverse STFT.

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;

pub const SR: u32 = 48_000;
/// The attenuation reference follows the model's four-frame output delay.
const NOISY_FRAME_OFFSET: usize = 4;

fn vorbis(n: usize) -> Vec<f32> {
    let h = n as f64 / 2.0;
    (0..n)
        .map(|i| {
            let s = (0.5 * std::f64::consts::PI * (i as f64 + 0.5) / h).sin();
            (0.5 * std::f64::consts::PI * s * s).sin() as f32
        })
        .collect()
}

/// `infer` replaces one interleaved real/imaginary spectrum in place.
pub(crate) fn process(
    input: &[f32],
    win: usize,
    limit_db: Option<f32>,
    progress: &dyn Fn(f32),
    mut infer: impl FnMut(&mut [f32]) -> Result<(), String>,
) -> Result<Vec<f32>, String> {
    if win < 2 || win % 2 != 0 {
        return Err(format!("denoise: invalid FFT window {win}"));
    }
    let hop = win / 2;
    let bins = win / 2 + 1;
    let window = vorbis(win);
    let mut x: Vec<f32> = input.to_vec();
    x.resize(input.len() + win, 0.0);
    let pad = win / 2;
    let n = x.len();
    let mut padded = Vec::with_capacity(n + 2 * pad);
    padded.extend((1..=pad).rev().map(|i| x[i.min(n - 1)]));
    padded.extend_from_slice(&x);
    padded.extend((0..pad).map(|i| x[(n - 2).saturating_sub(i)]));
    let frames = 1 + (padded.len() - win) / hop;
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(win);
    let inv = planner.plan_fft_inverse(win);
    let mut buf = fwd.make_input_vec();
    let mut spec = fwd.make_output_vec();
    // the attenuation limit blends in the noisy frame NOISY_FRAME_OFFSET frames back: only those are kept
    let mut recent: std::collections::VecDeque<Vec<Complex32>> = std::collections::VecDeque::with_capacity(NOISY_FRAME_OFFSET + 1);
    let alpha = limit_db.map(|db| 10f32.powf(-db / 20.0));
    let total = padded.len();
    let mut out = vec![0f32; total];
    let mut norm = vec![0f32; total];
    let mut time = inv.make_output_vec();
    let mut frame_ri = vec![0f32; bins * 2];
    let mut cleaned = inv.make_input_vec();
    for t in 0..frames {
        for (k, b) in buf.iter_mut().enumerate() {
            *b = padded[t * hop + k] * window[k];
        }
        fwd.process(&mut buf, &mut spec).map_err(|e| format!("stft: {e}"))?;
        for (k, c) in spec.iter().enumerate() {
            frame_ri[2 * k] = c.re;
            frame_ri[2 * k + 1] = c.im;
        }
        recent.push_back(spec.clone());
        if recent.len() > NOISY_FRAME_OFFSET + 1 {
            recent.pop_front();
        }
        infer(&mut frame_ri)?;
        for (k, c) in cleaned.iter_mut().enumerate() {
            *c = Complex32::new(frame_ri[2 * k], frame_ri[2 * k + 1]);
        }
        if let Some(alpha) = alpha {
            let reference = (recent.len() == NOISY_FRAME_OFFSET + 1).then(|| &recent[0]);
            for (k, c) in cleaned.iter_mut().enumerate() {
                let r = reference.map_or(Complex32::new(0.0, 0.0), |r| r[k]);
                *c = r * alpha + *c * (1.0 - alpha);
            }
        }
        cleaned[0].im = 0.0;
        cleaned[bins - 1].im = 0.0;
        inv.process(&mut cleaned, &mut time).map_err(|e| format!("istft: {e}"))?;
        for k in 0..win {
            out[t * hop + k] += time[k] / win as f32 * window[k];
            norm[t * hop + k] += window[k] * window[k];
        }
        if t % 500 == 0 {
            progress(t as f32 / frames as f32);
        }
    }
    let wave: Vec<f32> = (pad..total - pad)
        .map(|i| if norm[i] > f32::MIN_POSITIVE { out[i] / norm[i] } else { out[i] })
        .collect();
    let wave = &wave[..((frames - 1) * hop).min(wave.len())];
    let mut result: Vec<f32> = wave.get(2 * win..).unwrap_or(&[]).to_vec();
    result.resize(input.len(), 0.0);
    progress(1.0);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[test]
    fn silent_and_short_signals_keep_their_length_and_finish_progress() {
        for len in [0, 1, 3, 47, 64] {
            let progress = RefCell::new(Vec::new());
            let output = process(&vec![0.0; len], 8, None, &|value| progress.borrow_mut().push(value), |frame| {
                assert_eq!(frame.len(), 10);
                assert!(frame.iter().all(|value| value.is_finite()));
                frame.fill(0.0);
                Ok(())
            })
            .unwrap();
            assert_eq!(output, vec![0.0; len]);
            let progress = progress.into_inner();
            assert_eq!(progress.last(), Some(&1.0));
            assert!(progress.iter().all(|value| (0.0..=1.0).contains(value)));
            assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
        }
    }

    #[test]
    fn an_inference_failure_stops_without_retry_or_success_progress() {
        let calls = Cell::new(0);
        let progress = RefCell::new(Vec::new());
        let error = process(&[0.5; 64], 8, None, &|value| progress.borrow_mut().push(value), |_| {
            calls.set(calls.get() + 1);
            Err("injected inference failure".into())
        })
        .unwrap_err();
        assert_eq!(error, "injected inference failure");
        assert_eq!(calls.get(), 1);
        assert!(!progress.borrow().contains(&1.0));
    }

    #[test]
    fn invalid_windows_fail_before_the_frame_processor_runs() {
        for win in [0, 1, 3] {
            let called = Cell::new(false);
            assert!(process(&[0.0; 8], win, None, &|_| {}, |_| {
                called.set(true);
                Ok(())
            })
            .is_err());
            assert!(!called.get());
        }
    }

    #[test]
    fn attenuation_limits_blend_the_aligned_noisy_reference() {
        let input = (0..64)
            .map(|i| (i as f32 * 0.27).sin() * 0.5 + (i as f32 * 0.13).cos() * 0.2)
            .collect::<Vec<_>>();
        let run = |limit| {
            process(&input, 8, limit, &|_| {}, |frame| {
                frame.fill(0.0);
                Ok(())
            })
            .unwrap()
        };
        let suppressed = run(None);
        let reference = run(Some(0.0));
        let limited = run(Some(20.0));
        assert!(suppressed.iter().all(|value| *value == 0.0));
        for (actual, expected) in reference.iter().zip(&input).take(16) {
            assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
        }
        for (actual, expected) in limited.iter().zip(reference) {
            assert!((actual - expected * 0.1).abs() < 1e-5);
        }
        assert_eq!(limited.len(), input.len());
    }
}
