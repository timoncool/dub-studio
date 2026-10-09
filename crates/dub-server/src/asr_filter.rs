//! Галлюцинации ASR в транскрипте дубляжа. Текстовое правило (dub_asr::hallucination_kind) находит
//! кандидатов, а «скрыть или только пометить» решает голос на интервале реплики: «Thanks for watching»
//! в конце ролика часто сказано на самом деле, а «Watch out!» — обычная реплика. Реплику не удаляем —
//! `extra.hidden` исключает её из озвучки и субтитров, пользователь возвращает её в редакторе;
//! `extra.asr_flag`, `extra.asr_reason` и `extra.asr_evidence` объясняют решение.

use dub_asr::{hallucination_kind, HallucinationRules};
use dub_core::Segment;
use serde_json::Value;
use std::path::Path;

/// Доля интервала реплики, где звучит голос, начиная с которой кандидат считается сказанным.
const VOICE_MIN_COVER: f64 = 0.3;

/// Чем подтверждается голос на интервале реплики.
pub enum VoiceEvidence {
    /// Речевые спаны огибающей ОТДЕЛЁННОГО вокала (BSRoformer): музыки в нём нет, энергия = голос.
    Vocals(Vec<(f64, f64)>),
    /// Реплики диаризации Sortformer (нейросетевая речевая активность).
    Turns(Vec<(f64, f64)>),
    /// Спаны огибающей сырого микса (режим субтитров без сепарации): тишина надёжна, а энергия может
    /// быть и музыкой.
    Mix(Vec<(f64, f64)>),
}

fn active_spans(wav: &Path) -> Result<Vec<(f64, f64)>, String> {
    let (samples, sr) = crate::wavio::read_mono_f32(wav)?;
    let cfg = dub_asr::WindowConfig::default();
    let (env, frame_sec) = dub_asr::speech_envelope(&samples, sr, cfg.frame_sec);
    Ok(dub_asr::detect_active_spans(&env, frame_sec, &cfg))
}

impl VoiceEvidence {
    /// Собрать свидетельство для analyze: отделённый вокал 16 кГц, если сепарация сработала; иначе
    /// реплики диаризации (пусто при одном спикере или без диаризации); иначе огибающая сырого `mix`.
    pub fn build(clean_vocals: Option<&Path>, turns: &[dub_asr::Turn], mix: &Path) -> Result<Self, String> {
        if let Some(p) = clean_vocals {
            return Ok(VoiceEvidence::Vocals(active_spans(p)?));
        }
        if !turns.is_empty() {
            return Ok(VoiceEvidence::Turns(turns.iter().map(|t| (t.start, t.end)).collect()));
        }
        Ok(VoiceEvidence::Mix(active_spans(mix)?))
    }

    fn name(&self) -> &'static str {
        match self {
            VoiceEvidence::Vocals(_) => "vocals",
            VoiceEvidence::Turns(_) => "diarization",
            VoiceEvidence::Mix(_) => "mix",
        }
    }

    fn spans(&self) -> &[(f64, f64)] {
        match self {
            VoiceEvidence::Vocals(s) | VoiceEvidence::Turns(s) | VoiceEvidence::Mix(s) => s,
        }
    }

    /// Seconds of voice, when the evidence is voice: separated vocals or diarization. The raw mix cannot
    /// tell a voice from music, so it gives none.
    pub fn speech_seconds(&self) -> Option<f64> {
        match self {
            VoiceEvidence::Vocals(s) | VoiceEvidence::Turns(s) => Some(s.iter().map(|&(a, b)| (b - a).max(0.0)).sum()),
            VoiceEvidence::Mix(_) => None,
        }
    }

    /// Звучит ли что-то на [start, end] (для Mix — голос или музыка).
    fn voiced(&self, start: f64, end: f64) -> bool {
        let dur = (end - start).max(1e-3);
        let covered: f64 = self.spans().iter().map(|&(a, b)| (b.min(end) - a.max(start)).max(0.0)).sum();
        covered / dur >= VOICE_MIN_COVER
    }

    /// Что делать с кандидатом. В тишине — скрыть. По вокалу и диаризации звук на интервале — голос,
    /// кандидат остаётся. По сырому миксу энергия не отличает голос от музыки: сильный признак (титр,
    /// звук в скобках, текст без букв и цифр) скрывается по тексту, слабый (фраза из списка, крик, одни
    /// цифры, слово «субтитры» без автора) остаётся.
    fn decide(&self, kind: dub_asr::HallucinationKind, start: f64, end: f64) -> Decision {
        if !self.voiced(start, end) {
            return Decision::HideSilent;
        }
        match self {
            VoiceEvidence::Mix(_) if kind.is_strong() => Decision::HideByText,
            _ => Decision::Keep,
        }
    }
}

enum Decision {
    /// На интервале нет звука.
    HideSilent,
    /// Звук есть, но голос от музыки не отделён, а признак в тексте сильный.
    HideByText,
    Keep,
}

/// Итог фильтра для журнала analyze.
#[derive(Debug, Default)]
pub struct Report {
    /// Скрыты: на интервале реплики голоса нет.
    pub hidden: Vec<String>,
    /// Скрыты по сильному признаку в тексте: звук на интервале есть, но без отделённого вокала и
    /// диаризации неизвестно, голос это или музыка.
    pub hidden_by_text: Vec<String>,
    /// Похожи на галлюцинацию, но на интервале звучит голос — оставлены, только помечены.
    pub voiced: Vec<String>,
}

/// Пометить реплики-галлюцинации: `extra.asr_flag = "hallucination"`, `extra.asr_reason` (wordless |
/// numeric | bracketed | credit | subtitle_word | shouted | known_phrase), `extra.asr_evidence` (vocals |
/// diarization | mix); скрытым — ещё `extra.hidden = true`.
pub fn apply(segs: &mut [Segment], rules: HallucinationRules, evidence: &VoiceEvidence) -> Report {
    let mut report = Report::default();
    for s in segs.iter_mut() {
        let Some(kind) = hallucination_kind(&s.src_text, rules) else { continue };
        s.extra.insert("asr_flag".into(), Value::String("hallucination".into()));
        s.extra.insert("asr_reason".into(), Value::String(kind.as_str().into()));
        s.extra.insert("asr_evidence".into(), Value::String(evidence.name().into()));
        let text = s.src_text.trim().to_string();
        let decision = evidence.decide(kind, s.start, s.end);
        if !matches!(decision, Decision::Keep) {
            s.extra.insert("hidden".into(), Value::Bool(true));
        }
        match decision {
            Decision::HideSilent => report.hidden.push(text),
            Decision::HideByText => report.hidden_by_text.push(text),
            Decision::Keep => report.voiced.push(text),
        }
    }
    report
}

/// Скрыта ли реплика (hidden).
pub fn is_hidden(s: &Segment) -> bool {
    s.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> Segment {
        Segment { id: "s".into(), start, end, src_text: text.into(), ..Default::default() }
    }

    #[test]
    fn a_credit_over_silence_is_hidden() {
        let mut segs = vec![seg(0.0, 2.0, "Мы уходим завтра."), seg(10.0, 12.0, "Субтитры сделал DimaTorzok")];
        let ev = VoiceEvidence::Vocals(vec![(0.0, 2.1)]);
        let r = apply(&mut segs, HallucinationRules::Whisper, &ev);
        assert_eq!(r.hidden, vec!["Субтитры сделал DimaTorzok".to_string()]);
        assert!(!is_hidden(&segs[0]));
        assert!(!segs[0].extra.contains_key("asr_flag"));
        assert!(is_hidden(&segs[1]));
        assert_eq!(segs[1].extra["asr_flag"], "hallucination");
        assert_eq!(segs[1].extra["asr_reason"], "credit");
        assert_eq!(segs[1].extra["asr_evidence"], "vocals");
    }

    #[test]
    fn a_real_thank_you_for_watching_with_voice_stays() {
        let mut segs = vec![seg(30.0, 31.5, "Спасибо за просмотр!")];
        let ev = VoiceEvidence::Vocals(vec![(29.9, 31.6)]);
        let r = apply(&mut segs, HallucinationRules::CaseAware, &ev);
        assert_eq!(r.voiced.len(), 1, "голос есть — реплика остаётся");
        assert!(!is_hidden(&segs[0]));
        assert_eq!(segs[0].extra["asr_flag"], "hallucination");
    }

    #[test]
    fn diarization_turns_count_as_voice() {
        let mut segs = vec![seg(5.0, 6.0, "Thanks for watching!")];
        let ev = VoiceEvidence::Turns(vec![(4.8, 6.2)]);
        apply(&mut segs, HallucinationRules::CaseAware, &ev);
        assert!(!is_hidden(&segs[0]));
        assert_eq!(segs[0].extra["asr_evidence"], "diarization");
    }

    #[test]
    fn over_the_raw_mix_a_common_line_with_sound_stays_but_a_credit_goes() {
        // Режим субтитров без сепарации: на интервалах звучит что-то (голос или музыка).
        let ev = VoiceEvidence::Mix(vec![(0.0, 20.0)]);
        let mut segs = vec![seg(2.0, 3.0, "Watch out!"), seg(8.0, 10.0, "Subtitles by the Amara.org community")];
        let r = apply(&mut segs, HallucinationRules::Whisper, &ev);
        assert!(!is_hidden(&segs[0]), "«Watch out!» со звуком — обычная реплика");
        assert_eq!(segs[0].extra["asr_reason"], "known_phrase");
        assert!(is_hidden(&segs[1]), "титр субтитровщика скрывается и поверх музыки");
        assert!(r.hidden.is_empty(), "звук на интервале есть — это не «голоса нет»");
        assert_eq!(r.hidden_by_text, vec!["Subtitles by the Amara.org community".to_string()]);
        assert_eq!(segs[1].extra["asr_evidence"], "mix");
    }

    #[test]
    fn over_the_raw_mix_numbers_and_a_request_for_subtitles_with_sound_stay() {
        let ev = VoiceEvidence::Mix(vec![(0.0, 20.0)]);
        let mut segs = vec![seg(2.0, 3.0, " 300."), seg(5.0, 6.5, "Включите субтитры"), seg(8.0, 9.0, "Mach die Untertitel an")];
        let r = apply(&mut segs, HallucinationRules::Whisper, &ev);
        assert!(segs.iter().all(|s| !is_hidden(s)), "реплики со звуком остаются в субтитрах");
        assert_eq!(segs[0].extra["asr_reason"], "numeric");
        assert_eq!(segs[1].extra["asr_reason"], "subtitle_word");
        assert!(r.hidden.is_empty() && r.hidden_by_text.is_empty());
        assert_eq!(r.voiced.len(), 3);
    }

    #[test]
    fn over_the_raw_mix_numbers_in_silence_are_hidden() {
        let ev = VoiceEvidence::Mix(vec![(0.0, 5.0)]);
        let mut segs = vec![seg(12.0, 12.8, " 1.")];
        let r = apply(&mut segs, HallucinationRules::Whisper, &ev);
        assert!(is_hidden(&segs[0]));
        assert_eq!(r.hidden, vec!["1.".to_string()]);
    }

    #[test]
    fn over_the_raw_mix_a_phrase_in_silence_is_hidden() {
        let ev = VoiceEvidence::Mix(vec![(0.0, 5.0)]);
        let mut segs = vec![seg(12.0, 13.5, "Thanks for watching!")];
        apply(&mut segs, HallucinationRules::CaseAware, &ev);
        assert!(is_hidden(&segs[0]));
    }

    #[test]
    fn parakeet_shouting_is_speech() {
        let mut segs = vec![seg(1.0, 2.0, "NASA!")];
        let r = apply(&mut segs, HallucinationRules::CaseAware, &VoiceEvidence::Mix(Vec::new()));
        assert!(r.hidden.is_empty() && r.voiced.is_empty());
        assert!(!segs[0].extra.contains_key("asr_flag"));
    }
}
