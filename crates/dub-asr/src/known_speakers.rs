use std::collections::{BTreeMap, BTreeSet};

use crate::speaker_global::{cosine, Embedding};
use crate::{DiarTurns, Turn};

const NEW_VOICE_SCORE: f32 = 0.5;
const SAME_VOICE_SCORE: f32 = 0.95;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DiarWindow {
    pub start: f64,
    pub end: f64,
    pub keep_start: f64,
    pub keep_end: f64,
}

pub(crate) fn diar_windows(total: f64, width: f64, overlap: f64) -> Vec<DiarWindow> {
    let count = ((total / width).ceil() as usize).max(1);
    (0..count)
        .map(|i| DiarWindow {
            start: if i == 0 {
                0.0
            } else {
                i as f64 * width - overlap
            },
            end: ((i + 1) as f64 * width).min(total),
            keep_start: if i == 0 {
                0.0
            } else {
                i as f64 * width - overlap / 2.0
            },
            keep_end: if i + 1 == count {
                total
            } else {
                (i + 1) as f64 * width - overlap / 2.0
            },
        })
        .collect()
}

pub(crate) fn finish_turns(raw: Vec<Turn>) -> DiarTurns {
    let mut by_speaker: BTreeMap<i32, Vec<Turn>> = BTreeMap::new();
    for turn in raw {
        by_speaker.entry(turn.speaker).or_default().push(turn);
    }
    let n_speakers = by_speaker.len();
    let mut turns = Vec::new();
    let mut ref_windows = std::collections::HashMap::new();
    for (global, (_, mut local)) in by_speaker.into_iter().enumerate() {
        local.sort_by(|a, b| {
            a.start
                .total_cmp(&b.start)
                .then_with(|| a.end.total_cmp(&b.end))
        });
        let mut merged: Vec<Turn> = Vec::new();
        for mut turn in local {
            turn.speaker = global as i32;
            if let Some(previous) = merged.last_mut().filter(|p| p.end >= turn.start) {
                previous.end = previous.end.max(turn.end);
            } else {
                merged.push(turn);
            }
        }
        if let Some(longest) = merged
            .iter()
            .max_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
        {
            ref_windows.insert(global as i32, (longest.start, longest.end));
        }
        turns.extend(merged);
    }
    turns.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then_with(|| a.speaker.cmp(&b.speaker))
    });
    DiarTurns {
        turns,
        n_speakers,
        ref_windows,
    }
}

pub(crate) struct VoiceTracker {
    voices: Vec<Option<Embedding>>,
}

impl VoiceTracker {
    pub(crate) fn new(count: usize) -> Self {
        Self {
            voices: vec![None; count],
        }
    }

    pub(crate) fn process_window(
        &mut self,
        local: &[Turn],
        audio: &[f32],
        sr: u32,
        window: DiarWindow,
        embed: &mut impl FnMut(&[f32]) -> Result<Embedding, String>,
    ) -> Result<Vec<Turn>, String> {
        let mut owned: Vec<Turn> = local
            .iter()
            .filter_map(|t| {
                let start = t.start.max(window.keep_start);
                let end = t.end.min(window.keep_end);
                (end > start).then_some(Turn {
                    start,
                    end,
                    speaker: t.speaker,
                })
            })
            .collect();
        self.remap_with_references(&mut owned, local, audio, sr, window.start, embed)?;
        Ok(owned)
    }

    #[cfg(test)]
    fn remap(
        &mut self,
        turns: &mut [Turn],
        audio: &[f32],
        sr: u32,
        offset: f64,
        embed: &mut impl FnMut(&[f32]) -> Result<Embedding, String>,
    ) -> Result<(), String> {
        let references = turns.to_vec();
        self.remap_with_references(turns, &references, audio, sr, offset, embed)
    }

    fn remap_with_references(
        &mut self,
        turns: &mut [Turn],
        references: &[Turn],
        audio: &[f32],
        sr: u32,
        offset: f64,
        embed: &mut impl FnMut(&[f32]) -> Result<Embedding, String>,
    ) -> Result<(), String> {
        let speakers: BTreeSet<i32> = turns.iter().map(|t| t.speaker).collect();
        let mut groups = Vec::new();
        for speaker in speakers {
            let clips = reference_clips(references, speaker);
            let mut vectors = Vec::new();
            for &(clip_start, clip_end) in
                clips.iter().filter(|clip| clip.1 - clip.0 >= 0.3).take(3)
            {
                let start = ((clip_start - offset).max(0.0) * sr as f64) as usize;
                let end = ((clip_end - offset).max(0.0) * sr as f64) as usize;
                let end = end
                    .min(audio.len())
                    .min(start.saturating_add(6 * sr as usize));
                if start >= end {
                    continue;
                }
                let vector = embed(&audio[start..end])
                    .map_err(|e| format!("голос спикера {speaker}: {e}"))?;
                validate_embedding(&vector)?;
                vectors.push(vector);
            }
            if vectors.is_empty() {
                return Err(format!("спикер {speaker}: нет речевого фрагмента длиной хотя бы 0,3 секунды для сопоставления голоса"));
            }
            groups.push((vec![speaker], mean(&vectors)?));
        }
        loop {
            let mut best = None;
            for i in 0..groups.len() {
                for j in i + 1..groups.len() {
                    let overlap = references
                        .iter()
                        .filter(|t| groups[i].0.contains(&t.speaker))
                        .any(|a| {
                            references
                                .iter()
                                .filter(|t| groups[j].0.contains(&t.speaker))
                                .any(|b| a.end.min(b.end) - a.start.max(b.start) > 0.25)
                        });
                    let score = cosine(&groups[i].1, &groups[j].1);
                    if !overlap
                        && score >= SAME_VOICE_SCORE
                        && best.is_none_or(|(_, _, prev)| score > prev)
                    {
                        best = Some((i, j, score));
                    }
                }
            }
            let Some((i, j, _)) = best else {
                break;
            };
            let removed = groups.remove(j);
            groups[i].0.extend(removed.0);
            groups[i].1 = mean(&[groups[i].1.clone(), removed.1])?;
        }
        if groups.len() > self.voices.len() {
            return Err(format!("указано {} спикеров, но в фрагменте найдено {} различных голосов: безопасно объединить их по голосу не удалось", self.voices.len(), groups.len()));
        }
        let vectors: Vec<_> = groups.iter().map(|g| g.1.clone()).collect();
        let assignment = assign(&vectors, &self.voices)?;
        let mut mapping = BTreeMap::new();
        for ((labels, vector), global) in groups.into_iter().zip(assignment) {
            for label in labels {
                mapping.insert(label, global as i32);
            }
            self.voices[global] = Some(match self.voices[global].take() {
                Some(previous) => mean(&[previous, vector])?,
                None => vector,
            });
        }
        for turn in turns {
            turn.speaker = mapping[&turn.speaker];
        }
        Ok(())
    }
}

fn validate_embedding(vector: &[f32]) -> Result<(), String> {
    let norm_squared = vector.iter().map(|v| v * v).sum::<f32>();
    if vector.is_empty()
        || vector.iter().any(|v| !v.is_finite())
        || !norm_squared.is_finite()
        || norm_squared <= 1e-8
    {
        return Err("модель вернула пустые или некорректные голосовые признаки".into());
    }
    Ok(())
}

// Одновременная речь других участников не должна попадать в образец голоса.
fn reference_clips(turns: &[Turn], speaker: i32) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
    for turn in turns.iter().filter(|t| t.speaker == speaker) {
        let mut parts = vec![(turn.start, turn.end)];
        for other in turns
            .iter()
            .filter(|t| t.speaker != speaker && t.start < turn.end && t.end > turn.start)
        {
            parts = parts
                .into_iter()
                .flat_map(|(start, end)| {
                    let mut remaining = Vec::new();
                    if other.end <= start || other.start >= end {
                        remaining.push((start, end));
                    } else {
                        if other.start > start {
                            remaining.push((start, other.start));
                        }
                        if other.end < end {
                            remaining.push((other.end, end));
                        }
                    }
                    remaining
                })
                .collect();
        }
        result.extend(parts);
    }
    result.sort_by(|a, b| (b.1 - b.0).total_cmp(&(a.1 - a.0)));
    result
}

fn mean(vectors: &[Embedding]) -> Result<Embedding, String> {
    let mut result = vec![0.0; vectors[0].len()];
    for vector in vectors {
        if vector.len() != result.len() {
            return Err("размерность голосовых признаков изменилась".into());
        }
        for (sum, value) in result.iter_mut().zip(vector) {
            *sum += value;
        }
    }
    validate_embedding(&result)?;
    let norm = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    for value in &mut result {
        *value /= norm;
    }
    Ok(result)
}

// Совместное назначение запрещает двум голосам окна независимо выбрать одного участника.
fn assign(local: &[Embedding], voices: &[Option<Embedding>]) -> Result<Vec<usize>, String> {
    if local.len() > voices.len() || voices.len() > crate::MAX_SPEAKERS {
        return Err("число локальных голосов превышает заданное число спикеров".into());
    }
    let mut states: BTreeMap<usize, (f32, Vec<usize>)> = BTreeMap::from([(0, (0.0, Vec::new()))]);
    for vector in local {
        let mut next: BTreeMap<usize, (f32, Vec<usize>)> = BTreeMap::new();
        for (mask, (score, mapping)) in states {
            for (global, voice) in voices.iter().enumerate() {
                if mask & (1 << global) != 0 {
                    continue;
                }
                let similarity = voice
                    .as_ref()
                    .map_or(NEW_VOICE_SCORE, |v| cosine(vector, v));
                if voice.is_some() && similarity < NEW_VOICE_SCORE {
                    continue;
                }
                let candidate = score + similarity;
                let new_mask = mask | (1 << global);
                if next
                    .get(&new_mask)
                    .is_none_or(|(previous, _)| candidate > *previous)
                {
                    let mut chosen = mapping.clone();
                    chosen.push(global);
                    next.insert(new_mask, (candidate, chosen));
                }
            }
        }
        states = next;
    }
    states
        .into_values()
        .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|(_, mapping)| mapping)
        .ok_or_else(|| {
            "не удалось надёжно сопоставить голоса фрагмента с заданным числом участников".into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_at_a_boundary_and_swapped_labels_keep_the_voice() {
        let voices = vec![Some(vec![1.0, 0.0, 0.0]), Some(vec![0.0, 1.0, 0.0]), None];
        assert_eq!(
            assign(&[vec![0.0, 1.0, 0.0], vec![1.0, 0.0, 0.0]], &voices).unwrap(),
            vec![1, 0]
        );
        assert_eq!(assign(&[vec![0.0, 0.0, 1.0]], &voices).unwrap(), vec![2]);
    }

    #[test]
    fn two_local_voices_cannot_both_choose_the_best_global_voice() {
        let voices = vec![Some(vec![1.0, 0.0]), Some(vec![0.8, 0.6])];
        assert_eq!(
            assign(&[vec![1.0, 0.0], vec![0.9, 0.4]], &voices).unwrap(),
            vec![0, 1]
        );
    }

    #[test]
    fn eight_voices_match_across_windows_without_overlap_or_invented_people() {
        let vectors: Vec<_> = (0..8)
            .map(|i| {
                let mut v = vec![0.0; 8];
                v[i] = 1.0;
                v
            })
            .collect();
        let voices: Vec<_> = vectors.iter().cloned().map(Some).collect();
        let mut reversed = vectors.clone();
        reversed.reverse();
        assert_eq!(
            assign(&reversed, &voices).unwrap(),
            (0..8).rev().collect::<Vec<_>>()
        );
        assert_eq!(assign(&vectors[..2], &vec![None; 8]).unwrap(), vec![0, 1]);
    }

    #[test]
    fn remapping_preserves_times_and_reports_embedding_failure() {
        let mut tracker = VoiceTracker::new(2);
        let mut turns = vec![Turn {
            start: 10.0,
            end: 11.0,
            speaker: 7,
        }];
        let audio = vec![0.1; 16000];
        tracker
            .remap(&mut turns, &audio, 16000, 10.0, &mut |_| Ok(vec![1.0, 0.0]))
            .unwrap();
        assert_eq!(
            (turns[0].start, turns[0].end, turns[0].speaker),
            (10.0, 11.0, 0)
        );
        let error = tracker
            .remap(&mut turns, &audio, 16000, 10.0, &mut |_| {
                Err("ошибка ONNX".into())
            })
            .unwrap_err();
        assert!(error.contains("ошибка ONNX"));
    }

    #[test]
    fn unrelated_or_overlapping_voices_are_not_forced_into_one_slot() {
        let mut tracker = VoiceTracker::new(1);
        let mut turns = vec![
            Turn {
                start: 0.0,
                end: 1.0,
                speaker: 0,
            },
            Turn {
                start: 0.0,
                end: 1.0,
                speaker: 1,
            },
        ];
        let audio = vec![0.1; 16000];
        assert!(tracker
            .remap(&mut turns, &audio, 16000, 0.0, &mut |_| Ok(vec![1.0, 0.0]))
            .is_err());
        turns[1].start = 1.0;
        turns[1].end = 2.0;
        let mut i = 0;
        let error = tracker
            .remap(&mut turns, &vec![0.1; 32000], 16000, 0.0, &mut |_| {
                i += 1;
                Ok(if i == 1 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                })
            })
            .unwrap_err();
        assert!(error.contains("безопасно объединить"));
    }

    #[test]
    fn full_windows_keep_eight_identities_after_a_long_silence() {
        let mut tracker = VoiceTracker::new(8);
        let mut audio = Vec::new();
        let mut first = Vec::new();
        for speaker in 0..8 {
            audio.extend(vec![(speaker + 1) as f32; 16000]);
            first.push(Turn {
                start: speaker as f64,
                end: speaker as f64 + 1.0,
                speaker,
            });
        }
        let mut embed = |samples: &[f32]| {
            let mut vector = vec![0.0; 8];
            vector[samples[0] as usize - 1] = 1.0;
            Ok(vector)
        };
        tracker
            .remap(&mut first, &audio, 16000, 0.0, &mut embed)
            .unwrap();
        let mut second = Vec::new();
        audio.clear();
        for local in 0..8 {
            audio.extend(vec![(8 - local) as f32; 16000]);
            second.push(Turn {
                start: 7200.0 + local as f64,
                end: 7201.0 + local as f64,
                speaker: local,
            });
        }
        tracker
            .remap(&mut second, &audio, 16000, 7200.0, &mut embed)
            .unwrap();
        assert_eq!(
            second.iter().map(|t| t.speaker).collect::<Vec<_>>(),
            (0..8).rev().collect::<Vec<_>>()
        );
        assert_eq!(second[0].start, 7200.0);
        assert_eq!(second[7].end, 7208.0);
        assert_eq!(tracker.voices.iter().filter(|v| v.is_some()).count(), 8);
    }

    #[test]
    fn invalid_embeddings_are_rejected() {
        for vector in [vec![], vec![0.0], vec![f32::NAN], vec![f32::INFINITY]] {
            let mut tracker = VoiceTracker::new(2);
            let mut turns = vec![Turn {
                start: 0.0,
                end: 1.0,
                speaker: 0,
            }];
            assert!(tracker
                .remap(&mut turns, &vec![0.1; 16000], 16000, 0.0, &mut |_| Ok(
                    vector.clone()
                ))
                .is_err());
        }
    }

    #[test]
    fn overlapping_speech_is_excluded_from_voice_samples() {
        let turns = vec![
            Turn {
                start: 0.0,
                end: 8.0,
                speaker: 0,
            },
            Turn {
                start: 2.0,
                end: 5.0,
                speaker: 1,
            },
        ];
        assert_eq!(reference_clips(&turns, 0), vec![(5.0, 8.0), (0.0, 2.0)]);
        assert!(reference_clips(&turns, 1).is_empty());
        assert!(assign(&[vec![0.0, 1.0]], &[Some(vec![1.0, 0.0])]).is_err());
    }

    #[test]
    fn a_split_voice_is_merged_by_similarity_instead_of_its_number() {
        let mut tracker = VoiceTracker::new(2);
        let mut turns: Vec<_> = (0..3)
            .map(|speaker| Turn {
                start: speaker as f64,
                end: speaker as f64 + 1.0,
                speaker,
            })
            .collect();
        let mut audio = vec![1.0; 32000];
        audio.extend(vec![2.0; 16000]);
        tracker
            .remap(&mut turns, &audio, 16000, 0.0, &mut |samples| {
                Ok(if samples[0] == 1.0 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                })
            })
            .unwrap();
        assert_eq!(
            turns.iter().map(|t| t.speaker).collect::<Vec<_>>(),
            vec![0, 0, 1]
        );
    }

    fn identity(samples: &[f32]) -> Result<Embedding, String> {
        let mut vector = vec![0.0; 8];
        vector[samples[0] as usize - 1] = 1.0;
        Ok(vector)
    }

    #[test]
    fn a_split_voice_at_the_limit_leaves_room_for_the_eighth_person() {
        let mut tracker = VoiceTracker::new(8);
        let mut audio = Vec::new();
        let mut local = Vec::new();
        for label in 0..8 {
            let voice = if label < 2 { 1 } else { label };
            audio.extend(vec![voice as f32; 100]);
            local.push(Turn {
                start: label as f64,
                end: label as f64 + 1.0,
                speaker: label,
            });
        }
        let first = tracker
            .process_window(
                &local,
                &audio,
                100,
                DiarWindow {
                    start: 0.0,
                    end: 8.0,
                    keep_start: 0.0,
                    keep_end: 8.0,
                },
                &mut identity,
            )
            .unwrap();
        assert_eq!(tracker.voices.iter().filter(|v| v.is_some()).count(), 7);
        assert_eq!(first[0].speaker, first[1].speaker);
        let second = tracker
            .process_window(
                &[Turn {
                    start: 7200.0,
                    end: 7201.0,
                    speaker: 0,
                }],
                &vec![8.0; 100],
                100,
                DiarWindow {
                    start: 7200.0,
                    end: 7201.0,
                    keep_start: 7200.0,
                    keep_end: 7201.0,
                },
                &mut identity,
            )
            .unwrap();
        let result = finish_turns(first.into_iter().chain(second).collect());
        assert_eq!(result.n_speakers, 8);
        assert_eq!(result.turns.last().unwrap().speaker, 7);
    }

    #[test]
    fn similar_but_distinct_local_voices_keep_separate_slots() {
        let mut tracker = VoiceTracker::new(2);
        let mut turns = vec![
            Turn {
                start: 0.0,
                end: 1.0,
                speaker: 0,
            },
            Turn {
                start: 1.0,
                end: 2.0,
                speaker: 1,
            },
        ];
        let mut audio = vec![1.0; 100];
        audio.extend(vec![2.0; 100]);
        tracker
            .remap(&mut turns, &audio, 100, 0.0, &mut |samples| {
                Ok(if samples[0] == 1.0 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.8, 0.6]
                })
            })
            .unwrap();
        assert_ne!(turns[0].speaker, turns[1].speaker);
    }

    #[test]
    fn similar_simultaneous_voices_with_clean_samples_stay_separate() {
        let mut tracker = VoiceTracker::new(2);
        let mut turns = vec![
            Turn { start: 0.0, end: 2.0, speaker: 0 },
            Turn { start: 1.0, end: 3.0, speaker: 1 },
        ];
        let mut audio = vec![1.0; 300];
        audio[200..].fill(2.0);
        tracker.remap(&mut turns, &audio, 100, 0.0, &mut |samples| {
            Ok(if samples[0] == 1.0 { vec![1.0, 0.0] } else { vec![0.99, 0.1] })
        }).unwrap();
        assert_ne!(turns[0].speaker, turns[1].speaker);
        assert_eq!(tracker.voices.iter().filter(|v| v.is_some()).count(), 2);
    }

    #[test]
    fn confirmed_short_voices_survive_finalization() {
        let mut tracker = VoiceTracker::new(2);
        let local = vec![
            Turn {
                start: 0.0,
                end: 10.0,
                speaker: 0,
            },
            Turn {
                start: 20.0,
                end: 21.0,
                speaker: 1,
            },
        ];
        let mut audio = vec![1.0; 2100];
        audio[2000..2100].fill(2.0);
        let mapped = tracker
            .process_window(
                &local,
                &audio,
                100,
                DiarWindow {
                    start: 0.0,
                    end: 21.0,
                    keep_start: 0.0,
                    keep_end: 21.0,
                },
                &mut identity,
            )
            .unwrap();
        let result = finish_turns(mapped);
        assert_eq!(result.n_speakers, 2);
        assert!(result
            .turns
            .iter()
            .any(|t| t.speaker == 1 && t.start == 20.0 && t.end == 21.0));
        let result = finish_turns(
            (0..8)
                .map(|speaker| Turn {
                    start: speaker as f64 * 20.0,
                    end: speaker as f64 * 20.0 + if speaker == 7 { 1.0 } else { 10.0 },
                    speaker,
                })
                .collect(),
        );
        assert_eq!(result.n_speakers, 8);
        assert_eq!(result.turns.last().unwrap().speaker, 7);
    }

    #[test]
    fn window_ownership_partitions_the_audio_without_gaps_or_overlap() {
        for total in [3599.0, 3600.0, 3600.1, 7200.0, 7210.0, 21504.516667] {
            let ranges = diar_windows(total, 3600.0, 120.0);
            assert_eq!(ranges[0].keep_start, 0.0);
            assert_eq!(ranges.last().unwrap().keep_end, total);
            for pair in ranges.windows(2) {
                assert_eq!(pair[0].keep_end, pair[1].keep_start);
            }
            let covered: f64 = ranges.iter().map(|r| r.keep_end - r.keep_start).sum();
            assert!((covered - total).abs() < 1e-6);
        }
    }

    #[test]
    fn overlapping_speakers_at_a_seam_are_written_once() {
        let ranges = diar_windows(7200.0, 3600.0, 120.0);
        let local = vec![
            Turn {
                start: 3500.0,
                end: 3510.0,
                speaker: 0,
            },
            Turn {
                start: 3510.0,
                end: 3520.0,
                speaker: 1,
            },
            Turn {
                start: 3530.0,
                end: 3580.0,
                speaker: 0,
            },
            Turn {
                start: 3540.0,
                end: 3560.0,
                speaker: 1,
            },
        ];
        let mut audio = vec![1.0; 720000];
        audio[351000..352000].fill(2.0);
        let mut tracker = VoiceTracker::new(2);
        let mut out = Vec::new();
        for range in ranges {
            let slice = &audio[(range.start * 100.0) as usize..(range.end * 100.0) as usize];
            out.extend(
                tracker
                    .process_window(&local, slice, 100, range, &mut identity)
                    .unwrap(),
            );
        }
        // Проверяем сырые выходы окон: финальное слияние не должно скрывать повторную запись.
        let raw_a: f64 = out
            .iter()
            .filter(|t| t.speaker == 0 && t.end > 3530.0)
            .map(|t| t.end - t.start)
            .sum();
        let raw_b: f64 = out
            .iter()
            .filter(|t| t.speaker == 1 && t.start >= 3540.0)
            .map(|t| t.end - t.start)
            .sum();
        assert_eq!(raw_a, 50.0);
        assert_eq!(raw_b, 20.0);
        let result = finish_turns(out);
        assert_eq!(
            result
                .turns
                .iter()
                .filter(|t| t.speaker == 0 && t.start == 3530.0 && t.end == 3580.0)
                .count(),
            1
        );
        assert_eq!(
            result
                .turns
                .iter()
                .filter(|t| t.speaker == 1 && t.start == 3540.0 && t.end == 3560.0)
                .count(),
            1
        );
    }

    #[test]
    fn a_discarded_short_head_does_not_request_an_embedding() {
        let mut tracker = VoiceTracker::new(2);
        let local = vec![
            Turn {
                start: 3500.0,
                end: 3500.2,
                speaker: 0,
            },
            Turn {
                start: 3550.0,
                end: 3551.0,
                speaker: 1,
            },
        ];
        let mut requested = 0;
        let out = tracker
            .process_window(
                &local,
                &vec![2.0; 12000],
                100,
                DiarWindow {
                    start: 3480.0,
                    end: 3600.0,
                    keep_start: 3540.0,
                    keep_end: 3600.0,
                },
                &mut |samples| {
                    requested += 1;
                    identity(samples)
                },
            )
            .unwrap();
        assert_eq!(requested, 1);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].start, out[0].end), (3550.0, 3551.0));
    }

    #[test]
    fn a_short_owned_tail_uses_the_full_local_reference() {
        let mut tracker = VoiceTracker::new(2);
        let out = tracker
            .process_window(
                &[Turn {
                    start: 3539.0,
                    end: 3540.1,
                    speaker: 0,
                }],
                &vec![1.0; 12000],
                100,
                DiarWindow {
                    start: 3480.0,
                    end: 3600.0,
                    keep_start: 3540.0,
                    keep_end: 3600.0,
                },
                &mut identity,
            )
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].start, out[0].end), (3540.0, 3540.1));
    }
}
