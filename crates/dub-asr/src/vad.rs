//! Silero VAD v6.2 on transcribe.cpp ([`crate::tcpp`]) for every studio (Dub Studio and Speech Studio take this one
//! module): where a recording holds speech. A long recording is cut at its pauses into pieces for the recogniser
//! ([`windows`]), not by a timer in the middle of a word.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const SR: usize = 16_000;
pub const FRAME: usize = 512;
/// A piece for the recogniser is no longer than this many seconds.
pub const MAX_WINDOW_SECS: f64 = 90.0;

/// The marking settings (names and defaults of Silero's `get_speech_timestamps`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct VadConfig {
    pub threshold: f64,
    /// Speech ends when the probability falls below `threshold - hysteresis`.
    pub hysteresis: f64,
    pub min_speech_ms: u32,
    pub min_silence_ms: u32,
    pub pad_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        VadConfig { threshold: 0.5, hysteresis: 0.15, min_speech_ms: 250, min_silence_ms: 100, pad_ms: 30 }
    }
}

/// The components of speech marking missing on disk, by setup ids: the Silero VAD model and the transcribe.cpp build
/// `build` of the recognition device.
pub fn missing(models_root: &Path, build: crate::tcpp::Build) -> Vec<String> {
    let mut ids = Vec::new();
    if !crate::tcpp::helper_path(models_root, &crate::tcpp::VAD).is_file() {
        ids.push("silero-vad".to_string());
    }
    if !crate::tcpp::engine_dir(models_root, build).join(crate::tcpp::LIBRARY).is_file() {
        ids.push(build.dir().to_string());
    }
    ids
}

/// The probability of speech in every 32 ms frame (16 kHz mono), on the transcribe.cpp build `build`.
pub fn probabilities(models_root: &Path, build: crate::tcpp::Build, samples: &[f32]) -> Result<Vec<f32>, String> {
    let model = crate::tcpp::helper_path(models_root, &crate::tcpp::VAD);
    crate::tcpp::vad_probabilities(&crate::tcpp::engine_dir(models_root, build), &model, samples)
}

/// Spans of speech in seconds by the frames' probabilities (Silero's `get_speech_timestamps` without a length limit:
/// [`windows`] cuts the long ones).
pub fn speech(probs: &[f32], total_samples: usize, cfg: &VadConfig) -> Vec<(f64, f64)> {
    let ms = |v: u32| v as usize * SR / 1000;
    let (min_speech, min_silence, pad) = (ms(cfg.min_speech_ms), ms(cfg.min_silence_ms), ms(cfg.pad_ms));
    let off = (cfg.threshold - cfg.hysteresis).max(0.01);
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut temp_end = 0usize;
    for (i, &p) in probs.iter().enumerate() {
        let (at, p) = (i * FRAME, p as f64);
        if p >= cfg.threshold {
            temp_end = 0;
            if start.is_none() {
                start = Some(at);
            }
            continue;
        }
        let Some(s) = start else { continue };
        if p < off {
            if temp_end == 0 {
                temp_end = at;
            }
            if at - temp_end >= min_silence {
                if temp_end - s > min_speech {
                    spans.push((s, temp_end));
                }
                start = None;
                temp_end = 0;
            }
        }
    }
    if let Some(s) = start {
        if total_samples.saturating_sub(s) > min_speech {
            spans.push((s, total_samples));
        }
    }
    for i in 0..spans.len() {
        if i == 0 {
            spans[0].0 = spans[0].0.saturating_sub(pad);
        }
        if i + 1 < spans.len() {
            let gap = spans[i + 1].0 - spans[i].1;
            let grow = if gap < 2 * pad { gap / 2 } else { pad };
            spans[i].1 += grow;
            spans[i + 1].0 -= grow;
        } else {
            spans[i].1 = (spans[i].1 + pad).min(total_samples);
        }
    }
    spans.into_iter().map(|(a, b)| (a as f64 / SR as f64, b as f64 / SR as f64)).collect()
}

/// Pieces for the recogniser: consecutive spans of speech, a piece no longer than `max`. A piece that would overflow
/// ends on a pause of its second half: the one where the diarization turns `turns` change speaker (none given — no
/// such pause), else the longest; a span of speech longer than `max` is cut at the quietest frame of its second half.
pub fn windows(spans: &[(f64, f64)], probs: &[f32], turns: &[crate::Turn], max: f64) -> Vec<(f64, f64)> {
    let frame = FRAME as f64 / SR as f64;
    let mut pieces: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in spans {
        let mut a = a;
        while b - a > max {
            let lo = ((a + max / 2.0) / frame) as usize;
            let hi = (((a + max) / frame) as usize).min(probs.len());
            let cut = (lo..hi).min_by(|&x, &y| probs[x].total_cmp(&probs[y])).map(|k| k as f64 * frame).unwrap_or(a + max);
            pieces.push((a, cut));
            a = cut;
        }
        pieces.push((a, b));
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        let start = pieces[i].0;
        let mut j = i;
        while j + 1 < pieces.len() && pieces[j + 1].1 - start <= max {
            j += 1;
        }
        if j + 1 < pieces.len() && j > i {
            let half = start + max / 2.0;
            let gap = |k: usize| pieces[k + 1].0 - pieces[k].1;
            let changes = |k: usize| speaker_at(turns, pieces[k].1, true) != speaker_at(turns, pieces[k + 1].0, false);
            // on a tie max_by takes the last one, the natural end of piece `j`
            j = (i..=j)
                .filter(|&k| k == j || pieces[k].1 >= half)
                .max_by(|&x, &y| (changes(x), gap(x)).partial_cmp(&(changes(y), gap(y))).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(j);
        }
        out.push((start, pieces[j].1));
        i = j + 1;
    }
    out
}

/// The speaker at the edge `t` of a pause: the turn holding `t`, else the nearest one before `t` (`before`) or after it.
fn speaker_at(turns: &[crate::Turn], t: f64, before: bool) -> Option<i32> {
    if let Some(turn) = turns.iter().find(|turn| turn.start <= t && t <= turn.end) {
        return Some(turn.speaker);
    }
    let side = turns.iter().filter(|turn| if before { turn.end < t } else { turn.start > t });
    let nearest = if before {
        side.max_by(|a, b| a.end.total_cmp(&b.end))
    } else {
        side.min_by(|a, b| a.start.total_cmp(&b.start))
    };
    nearest.map(|turn| turn.speaker)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(pattern: &[(f32, usize)]) -> Vec<f32> {
        pattern.iter().flat_map(|&(p, n)| std::iter::repeat_n(p, n)).collect()
    }

    #[test]
    fn speech_follows_the_thresholds_and_drops_short_blips() {
        // 1 s silence, 2 s speech, a dip to 0.4 (above the off threshold 0.35) for 0.32 s, 1 s speech, 1 s silence,
        // a 64 ms blip, silence
        let probs = frames(&[(0.0, 31), (0.9, 62), (0.4, 10), (0.9, 31), (0.0, 31), (0.9, 2), (0.0, 31)]);
        let total = probs.len() * FRAME;
        let spans = speech(&probs, total, &VadConfig::default());
        assert_eq!(spans.len(), 1, "{spans:?}");
        let (a, b) = spans[0];
        assert!((a - (31.0 * 0.032 - 0.03)).abs() < 1e-6, "{a}");
        assert!((b - ((31.0 + 62.0 + 10.0 + 31.0) * 0.032 + 0.03)).abs() < 1e-6, "{b}");
    }

    /// The model on disk without the engine build names the build, not the model: Live asked to download Silero VAD,
    /// which was there, and went on refusing.
    #[test]
    fn a_missing_engine_build_is_named_and_not_the_model() {
        let root = std::env::temp_dir().join(format!("vad-missing-{}", std::process::id()));
        let build = crate::tcpp::Build::Cpu;
        assert_eq!(missing(&root, build), vec!["silero-vad".to_string(), build.dir().to_string()]);
        let vad = crate::tcpp::helper_path(&root, &crate::tcpp::VAD);
        std::fs::create_dir_all(vad.parent().unwrap()).unwrap();
        std::fs::write(&vad, b"gguf").unwrap();
        assert_eq!(missing(&root, build), vec![build.dir().to_string()]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn windows_end_on_the_longest_pause_and_cut_a_long_span_where_it_is_quietest() {
        let spans = [(0.0, 20.0), (20.2, 50.0), (53.0, 70.0), (70.1, 100.0)];
        let w = windows(&spans, &[], &[], 90.0);
        assert_eq!(w, vec![(0.0, 50.0), (53.0, 100.0)], "the 3 s pause wins over the last short one");
        let mut probs = vec![0.9f32; (200.0 / 0.032) as usize];
        probs[(70.0 / 0.032) as usize] = 0.1;
        let w = windows(&[(0.0, 120.0)], &probs, &[], 90.0);
        assert_eq!(w.len(), 2);
        assert!((w[0].1 - 70.0).abs() < 0.04 && w[1] == (w[0].1, 120.0), "{w:?}");
    }

    #[test]
    fn a_window_ends_where_the_speaker_changes_rather_than_on_a_longer_pause_of_one_speaker() {
        let turn = |start: f64, end: f64, speaker: i32| crate::Turn { start, end, speaker };
        // the 3 s pause at 50 s is inside speaker 0's talk; speaker 1 takes over after the 0.4 s pause at 70 s
        let spans = [(0.0, 20.0), (20.2, 50.0), (53.0, 70.0), (70.4, 100.0)];
        let turns = [turn(0.0, 20.0, 0), turn(20.2, 50.0, 0), turn(53.0, 70.0, 0), turn(70.4, 100.0, 1)];
        assert_eq!(windows(&spans, &[], &turns, 90.0), vec![(0.0, 70.0), (70.4, 100.0)]);
        let one = turns.map(|t| turn(t.start, t.end, 0));
        assert_eq!(windows(&spans, &[], &one, 90.0), vec![(0.0, 50.0), (53.0, 100.0)], "one speaker: the longest pause");
    }

    #[test]
    fn the_speaker_at_a_pause_edge_is_the_turn_holding_it_or_the_nearest_on_its_side() {
        let turns = [crate::Turn { start: 0.0, end: 5.0, speaker: 2 }, crate::Turn { start: 8.0, end: 9.0, speaker: 1 }];
        assert_eq!(speaker_at(&turns, 4.0, true), Some(2));
        assert_eq!(speaker_at(&turns, 6.0, true), Some(2));
        assert_eq!(speaker_at(&turns, 6.0, false), Some(1));
        assert_eq!(speaker_at(&turns, 10.0, false), None);
        assert_eq!(speaker_at(&[], 1.0, true), None);
    }
}
