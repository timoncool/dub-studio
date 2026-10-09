//! Голосовой эмбеддер персонажа: WeSpeaker ResNet34-LM (ONNX) поверх чистого вокала.
//!
//! Пайплайн: WAV 16к моно (реф-клип спикера) -> Kaldi-совместимый log-mel fbank [1,T,80] ->
//! WeSpeaker ResNet34-LM (ort) -> 256-d эмбеддинг -> L2. Косинус этого вектора между эпизодами даёт
//! «тот же спикер» (порог ~0.5, env DUB_FACES_VOICE_COS для cross-video 0.45-0.6).
//!
//! ФИЧИ ГРАФА (проверено, RESEARCH #115): вход — НЕ сырой waveform, а fbank [1,T,80] float32:
//!   • 80 mel-bins, окно 25мс, шаг 10мс, 16кГц моно;
//!   • Kaldi-совместимый log-mel: preemphasis 0.97, Povey-окно, power-спектр, mel-фильтры (Slaney/HTK
//!     как в Kaldi — треугольники на mel-шкале), log(max(e, EPS));
//!   • dither = 0 (детерминированно);
//!   • per-utterance mean-subtraction (CMN): вычесть среднее по кадрам на КАЖДЫЙ бин; деление на
//!     дисперсию НЕ применяем.
//! Выход графа L2-нормирован, но защитно нормируем ещё раз (x/‖x‖).
//!
//! fbank считаем СВОИМ log-mel (не knf-rs): knf-rs тянет knf-rs-sys → bindgen+cmake+libclang, что ломает
//! запертый Windows/CUDA билд (проверено: build падает без LIBCLANG_PATH). Своя реализация без C-зависимостей
//! повторяет Kaldi FbankComputer с параметрами WeSpeaker.

use crate::FacesError;
use crate::ort_engine::OnnxModel;
use std::path::Path;

/// Параметры fbank WeSpeaker (Kaldi-дефолты + 80 бинов). Менять НЕ нужно — заперты под граф модели.
const SAMPLE_RATE: u32 = 16_000;
const NUM_MEL_BINS: usize = 80;
const FRAME_LENGTH_MS: f32 = 25.0;
const FRAME_SHIFT_MS: f32 = 10.0;
const PREEMPH: f32 = 0.97; // коэффициент preemphasis (Kaldi default)
const EPS: f32 = 1.192_092_9e-7; // std::numeric_limits<float>::epsilon() — floor логарифма (Kaldi)
const LOW_FREQ: f32 = 20.0; // Kaldi mel low cutoff
const HIGH_FREQ: f32 = 0.0; // 0 => Nyquist (sr/2), как Kaldi при high-freq<=0

/// Размерность выходного эмбеддинга WeSpeaker ResNet34-LM.
pub const VOICE_DIM: usize = 256;

/// Порог косинуса «тот же спикер» для голоса. ~0.5 по RESEARCH; cross-video калибруемо 0.45-0.6.
/// Env DUB_FACES_VOICE_COS.
pub fn voice_cos_threshold() -> f32 {
    std::env::var("DUB_FACES_VOICE_COS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0.5)
}

/// Резолв пути к WeSpeaker ONNX (по образцу FacesModels::resolve для SCRFD/LVFace): <root>/faces/wespeaker/…
pub fn wespeaker_path(models_root: &Path) -> std::path::PathBuf {
    models_root
        .join("faces")
        .join("wespeaker")
        .join("voxceleb_resnet34_LM.onnx")
}

/// WeSpeaker эмбеддер: ONNX-сессия [1,T,80] f32 -> [1,256].
pub struct VoiceEmbedder {
    model: OnnxModel,
    mel: MelBanks,
}

impl VoiceEmbedder {
    /// Загрузить ONNX (ту же onnxruntime.dll, что SCRFD/LVFace — ensure_ort_dylib внутри OnnxModel::load).
    pub fn load(onnx: &Path) -> Result<Self, FacesError> {
        Ok(Self {
            model: OnnxModel::load(onnx)?,
            mel: MelBanks::new(),
        })
    }

    /// Эмбеддинг для WAV-клипа (16к моно; чужой sr — ошибка, вызывающий режет ffmpeg'ом в 16к). Пустой/
    /// слишком короткий клип (<1 кадра) -> ошибка. Возвращает 256-d L2-нормированный вектор.
    pub fn embed_wav(&mut self, wav: &Path) -> Result<Vec<f32>, FacesError> {
        let (samples, sr) = read_wav_mono_f32(wav)?;
        if sr != SAMPLE_RATE {
            return Err(FacesError::SampleRate { expected: SAMPLE_RATE, got: sr });
        }
        self.embed_samples(&samples)
    }

    /// Эмбеддинг для готовых сэмплов моно f32 @16к (в [-1,1]). Kaldi считает по int16-масштабу, поэтому
    /// внутри домножаем на 32768.
    pub fn embed_samples(&mut self, samples: &[f32]) -> Result<Vec<f32>, FacesError> {
        let feats = self.mel.fbank(samples); // [T][80], уже с CMN
        let t = feats.len();
        if t == 0 {
            return Err(FacesError::ClipTooShort);
        }
        // Вход графа WeSpeaker — ровно 3-D [1,T,80] (b,t,mel). Строим row-major flat и отдаём run_3d.
        let mut flat = Vec::with_capacity(t * NUM_MEL_BINS);
        for row in &feats {
            flat.extend_from_slice(row);
        }
        let (_shape, out) = self.model.run_3d(&[1, t, NUM_MEL_BINS], flat)?;
        Ok(l2(out))
    }
}

/// L2-нормализация (нулевой -> как есть).
fn l2(mut v: Vec<f32>) -> Vec<f32> {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-8 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

// ─── Kaldi-совместимый log-mel fbank ────────────────────────────────────────────────────────────────

/// Предрасчитанные mel-фильтры + окно + FFT-размер. Строится один раз на эмбеддер.
struct MelBanks {
    frame_len: usize,   // сэмплов в окне (25мс @16к = 400)
    frame_shift: usize, // сэмплов сдвига (10мс @16к = 160)
    fft_size: usize,    // ближайшая степень 2 >= frame_len (512)
    window: Vec<f32>,   // Povey-окно длины frame_len
    // Треугольные mel-фильтры: для каждого из 80 бинов — (первый bin спектра, веса).
    filters: Vec<(usize, Vec<f32>)>,
}

impl MelBanks {
    fn new() -> Self {
        let frame_len = ((FRAME_LENGTH_MS / 1000.0) * SAMPLE_RATE as f32).round() as usize; // 400
        let frame_shift = ((FRAME_SHIFT_MS / 1000.0) * SAMPLE_RATE as f32).round() as usize; // 160
        let fft_size = next_pow2(frame_len); // 512
        let window = povey_window(frame_len);
        let filters = mel_filters(fft_size, SAMPLE_RATE, NUM_MEL_BINS);
        MelBanks { frame_len, frame_shift, fft_size, window, filters }
    }

    /// Полный fbank: сэмплы [-1,1] -> [T][80] log-mel с per-utterance CMN.
    fn fbank(&self, samples_norm: &[f32]) -> Vec<Vec<f32>> {
        // Kaldi работает в int16-масштабе; входные сэмплы у нас в [-1,1] (hound f32/int уже нормировал).
        let samples: Vec<f32> = samples_norm.iter().map(|&x| x * 32768.0).collect();
        if samples.len() < self.frame_len {
            return Vec::new();
        }
        let num_frames = 1 + (samples.len() - self.frame_len) / self.frame_shift;
        let bins = self.filters.len();
        let mut feats: Vec<Vec<f32>> = Vec::with_capacity(num_frames);
        let half = self.fft_size / 2;
        let mut re = vec![0.0f32; self.fft_size];
        let mut im = vec![0.0f32; self.fft_size];
        let mut power = vec![0.0f32; half + 1];
        for f in 0..num_frames {
            let start = f * self.frame_shift;
            let frame = &samples[start..start + self.frame_len];
            // 1) DC-remove (Kaldi remove_dc_offset=true): вычесть среднее окна.
            let mean: f32 = frame.iter().sum::<f32>() / self.frame_len as f32;
            let mut buf: Vec<f32> = frame.iter().map(|&x| x - mean).collect();
            // 2) preemphasis y[i]=x[i]-0.97*x[i-1] (i=0 использует x[0] как x[-1], как Kaldi).
            for i in (1..self.frame_len).rev() {
                buf[i] -= PREEMPH * buf[i - 1];
            }
            buf[0] -= PREEMPH * buf[0];
            // 3) окно Povey.
            for i in 0..self.frame_len {
                buf[i] *= self.window[i];
            }
            // 4) FFT (zero-pad до fft_size).
            for i in 0..self.fft_size {
                re[i] = if i < self.frame_len { buf[i] } else { 0.0 };
                im[i] = 0.0;
            }
            fft_inplace(&mut re, &mut im);
            // 5) power-спектр |X|² на [0..half].
            for i in 0..=half {
                power[i] = re[i] * re[i] + im[i] * im[i];
            }
            // 6) mel-энергии + log(max(e, EPS)).
            let mut row = vec![0.0f32; bins];
            for (b, (offset, weights)) in self.filters.iter().enumerate() {
                let mut e = 0.0f32;
                for (k, &w) in weights.iter().enumerate() {
                    e += w * power[offset + k];
                }
                row[b] = e.max(EPS).ln();
            }
            feats.push(row);
        }
        // 7) CMN: per-utterance mean-subtraction по КАЖДОМУ бину (дисперсию НЕ трогаем — RESEARCH).
        if !feats.is_empty() {
            let n = feats.len() as f32;
            let mut means = vec![0.0f32; bins];
            for row in &feats {
                for b in 0..bins {
                    means[b] += row[b];
                }
            }
            for m in &mut means {
                *m /= n;
            }
            for row in &mut feats {
                for b in 0..bins {
                    row[b] -= means[b];
                }
            }
        }
        feats
    }
}

/// Окно Povey (Kaldi default): (0.5 - 0.5*cos(2π i/(N-1)))^0.85.
fn povey_window(n: usize) -> Vec<f32> {
    let mut w = vec![0.0f32; n];
    let denom = (n - 1).max(1) as f32;
    for (i, wi) in w.iter_mut().enumerate() {
        let a = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / denom).cos();
        *wi = a.powf(0.85);
    }
    w
}

fn next_pow2(mut n: usize) -> usize {
    let mut p = 1;
    while p < n {
        p <<= 1;
        n = n.max(1);
    }
    p.max(1)
}

/// Гц -> mel (Kaldi/Slaney формула, как в kaldi mel-computations: 1127*ln(1+f/700)).
fn hz_to_mel(f: f32) -> f32 {
    1127.0 * (1.0 + f / 700.0).ln()
}
#[cfg_attr(not(test), allow(dead_code))] // паритет-обратка mel->hz: используется в тестах mel-шкалы
fn mel_to_hz(m: f32) -> f32 {
    700.0 * ((m / 1127.0).exp() - 1.0)
}

/// Треугольные mel-фильтры на power-спектре [0..fft/2]. Возвращает для каждого бина (offset, веса) —
/// плотная форма без нулевых хвостов (быстрее свёртка). Повторяет Kaldi ComputeMelBanks.
fn mel_filters(fft_size: usize, sr: u32, num_bins: usize) -> Vec<(usize, Vec<f32>)> {
    let nyquist = sr as f32 / 2.0;
    let high = if HIGH_FREQ <= 0.0 { nyquist + HIGH_FREQ } else { HIGH_FREQ };
    let mel_low = hz_to_mel(LOW_FREQ);
    let mel_high = hz_to_mel(high);
    let num_fft_bins = fft_size / 2 + 1;
    let fft_bin_width = sr as f32 / fft_size as f32;
    // num_bins+2 точек по mel-шкале -> центры каждого треугольника.
    let mel_delta = (mel_high - mel_low) / (num_bins + 1) as f32;
    let mut out: Vec<(usize, Vec<f32>)> = Vec::with_capacity(num_bins);
    for b in 0..num_bins {
        let left = mel_low + b as f32 * mel_delta;
        let center = mel_low + (b + 1) as f32 * mel_delta;
        let right = mel_low + (b + 2) as f32 * mel_delta;
        // Плотная форма: [first_bin, weights[…]] — только участок спектра под треугольником, без нулевых
        // хвостов. Идём по спектру, собираем непрерывный диапазон ненулевых весов.
        let mut first: Option<usize> = None;
        let mut weights: Vec<f32> = Vec::new();
        for k in 0..num_fft_bins {
            let mel = hz_to_mel(fft_bin_width * k as f32);
            if mel <= left || mel >= right {
                if first.is_some() {
                    break; // треугольник пройден — правее только нули
                }
                continue;
            }
            let w = if mel <= center {
                (mel - left) / (center - left)
            } else {
                (right - mel) / (right - center)
            };
            if first.is_none() {
                first = Some(k);
            }
            weights.push(w);
        }
        out.push((first.unwrap_or(0), weights));
    }
    out
}

// ─── Радикс-2 FFT (итеративный, in-place) ───────────────────────────────────────────────────────────
// Свой FFT (без rustfft): fft_size=512 фикс, вызывается на кадр; O(N log N) достаточно. Битреверс + бабочки.

fn fft_inplace(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    // bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // butterflies
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wl_re, wl_im) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut w_re, mut w_im) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = i + k;
                let b = i + k + len / 2;
                let t_re = re[b] * w_re - im[b] * w_im;
                let t_im = re[b] * w_im + im[b] * w_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
                let nw_re = w_re * wl_re - w_im * wl_im;
                let nw_im = w_re * wl_im + w_im * wl_re;
                w_re = nw_re;
                w_im = nw_im;
            }
            i += len;
        }
        len <<= 1;
    }
}

// ─── WAV-декод (mono f32 @native sr) ────────────────────────────────────────────────────────────────
// Порт wavio::read_mono_f32 из dub-server (крейт не должен зависеть от сервера — hound локально).

fn read_wav_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), FacesError> {
    use hound::{SampleFormat, WavReader};
    let mut r = WavReader::open(path).map_err(|e| FacesError::Io(format!("open {}: {e}", path.display())))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => r
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| FacesError::Io(format!("read f32: {e}")))?,
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| FacesError::Io(format!("read int: {e}")))?
        }
    };
    let mono: Vec<f32> = if ch <= 1 {
        interleaved
    } else {
        interleaved.chunks(ch).map(|fr| fr.iter().sum::<f32>() / ch as f32).collect()
    };
    Ok((mono, spec.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_pow2_ok() {
        assert_eq!(next_pow2(400), 512);
        assert_eq!(next_pow2(512), 512);
        assert_eq!(next_pow2(1), 1);
    }

    #[test]
    fn mel_scale_monotonic() {
        // mel монотонна и обратима.
        assert!(hz_to_mel(1000.0) > hz_to_mel(500.0));
        let hz = 800.0;
        assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 1e-2);
    }

    #[test]
    fn fft_matches_dft_on_sine() {
        // FFT синусоиды 1 периода на N=8 даёт пик на бине 1.
        let n = 8usize;
        let mut re = vec![0.0f32; n];
        let mut im = vec![0.0f32; n];
        for i in 0..n {
            re[i] = (2.0 * std::f32::consts::PI * i as f32 / n as f32).sin();
        }
        fft_inplace(&mut re, &mut im);
        let mag: Vec<f32> = (0..n).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).collect();
        let peak = (1..n / 2).max_by(|&a, &b| mag[a].partial_cmp(&mag[b]).unwrap()).unwrap();
        assert_eq!(peak, 1, "пик на бине 1, mag={mag:?}");
    }

    #[test]
    fn fbank_shape_and_cmn() {
        // 1с тона 16к -> кадров ~ (16000-400)/160 + 1 = 98; каждый бин после CMN имеет ~нулевое среднее.
        let mel = MelBanks::new();
        let sr = SAMPLE_RATE as f32;
        let samples: Vec<f32> =
            (0..16000).map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin()).collect();
        let feats = mel.fbank(&samples);
        assert_eq!(feats.len(), 98, "число кадров");
        assert!(feats.iter().all(|r| r.len() == NUM_MEL_BINS), "80 бинов на кадр");
        // CMN: среднее по кадрам на каждый бин ~0.
        for b in 0..NUM_MEL_BINS {
            let m: f32 = feats.iter().map(|r| r[b]).sum::<f32>() / feats.len() as f32;
            assert!(m.abs() < 1e-3, "бин {b} среднее {m} не ~0 после CMN");
        }
    }

    #[test]
    fn fbank_empty_on_short() {
        let mel = MelBanks::new();
        assert!(mel.fbank(&[0.0; 100]).is_empty(), "клип короче окна -> пусто");
    }

    #[test]
    fn l2_unit() {
        let v = l2(vec![3.0, 4.0]);
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-6);
    }
}
