//! Подгонка фразы дубляжа под слот: слот и кап ускорения (одна формула у рендера, у прогноза в
//! project_get и у сокращения перевода), оценка длительности перевода по темпу речи и его калибровка по
//! фактическим клипам TTS, бюджет длины строки для промпта перевода.
//!
//! Длина текста везде в `text_units`: символы строки с одиночными пробелами между словами — так же
//! строку видит LLM, когда ей задан лимит символов.

use serde::{Deserialize, Serialize};

/// Длина текста в единицах темпа: символы слов плюс по одному пробелу между словами.
pub fn text_units(text: &str) -> usize {
    let mut n = 0usize;
    for (i, w) in text.split_whitespace().enumerate() {
        if i > 0 {
            n += 1;
        }
        n += w.chars().count();
    }
    n
}

/// Темп речи языка без калибровки, если язык не в таблице.
pub const DEFAULT_CPS: f64 = 13.0;

/// Темп речи дубляжа, единиц `text_units` в секунду, для языка без калибровки: база прогноза, пока у пары
/// (язык, спикер) меньше `CALIB_MIN_SAMPLES` озвученных фраз.
pub fn table_cps(lang: &str) -> f64 {
    let lc = lang.trim().to_ascii_lowercase();
    let code = lc.split(['-', '_']).next().unwrap_or_default();
    match code {
        "en" => 15.0,
        "es" => 15.5,
        "fr" | "it" | "pt" | "ca" | "gl" => 15.0,
        "de" | "nl" | "sv" | "no" | "nn" | "da" | "el" | "af" | "lb" => 14.0,
        "ru" | "uk" | "be" | "pl" | "cs" | "sk" | "bg" | "sr" | "hr" | "bs" | "sl" | "mk" | "fi" | "et" | "lv" | "lt" | "hu" | "ro" => 13.0,
        "tr" | "az" | "kk" | "uz" | "tk" | "tt" | "ba" | "ar" | "he" => 12.0,
        "fa" | "ur" | "ps" | "sd" => 13.0,
        "hi" | "bn" => 17.0,
        "mr" | "gu" | "pa" | "ne" | "as" => 16.0,
        "ta" | "te" | "kn" | "ml" | "si" => 14.0,
        "vi" => 16.0,
        "id" | "ms" | "tl" | "jw" | "su" => 14.0,
        "th" | "lo" | "km" | "my" => 10.0,
        "zh" | "yue" => 5.5,
        "ja" => 7.0,
        "ko" => 7.5,
        _ => DEFAULT_CPS,
    }
}

/// Калибровка засчитывается с этого числа годных клипов: одна странная фраза (выдох, заикание клона)
/// сдвинула бы оценку сильнее ошибки таблицы.
pub const CALIB_MIN_SAMPLES: usize = 3;
/// Короче — в клипе больше тишины и разгона TTS, чем речи.
pub const CALIB_MIN_SECS: f64 = 0.4;
pub const CALIB_MIN_UNITS: usize = 4;

/// Темп речи конкретного голоса по фактическим клипам.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calib {
    pub cps: f64,
    pub samples: usize,
}

/// Медиана темпа сырых клипов TTS (до подгонки темпа): пары (text_units озвученного текста, секунды клипа).
/// None — годных клипов меньше `CALIB_MIN_SAMPLES`.
pub fn calibrate(samples: impl IntoIterator<Item = (usize, f64)>) -> Option<Calib> {
    let mut rates: Vec<f64> = samples
        .into_iter()
        .filter(|&(units, secs)| units >= CALIB_MIN_UNITS && secs >= CALIB_MIN_SECS && secs.is_finite())
        .map(|(units, secs)| units as f64 / secs)
        .collect();
    if rates.len() < CALIB_MIN_SAMPLES {
        return None;
    }
    rates.sort_by(|a, b| a.total_cmp(b));
    let n = rates.len();
    let median = if n % 2 == 1 { rates[n / 2] } else { (rates[n / 2 - 1] + rates[n / 2]) / 2.0 };
    (median > 0.0).then_some(Calib { cps: median, samples: n })
}

/// Короче слот не бывает: у реплики без паузы до соседа всё равно есть место на выдох.
pub const MIN_SLOT: f64 = 0.3;
/// Отставание дубля от реплики оригинала, после которого кап ускорения поднимается, чтобы догнать синхрон.
pub const DRIFT_ESCALATE: f64 = 0.6;
/// Потолок ускорения, когда контроль длительности выключен.
pub const CAP_UNCHECKED: f64 = 10.0;
/// Слоты короче этого получают кап не ниже `SHORT_SLOT_CAP`: на коротком слоте тот же сдвиг в секундах —
/// большой множитель.
pub const SHORT_SLOT: f64 = 1.5;
pub const SHORT_SLOT_CAP: f64 = 1.30;
/// Потолки эскалации: при «динамическом темпе речи» и при догоне дрейфа.
pub const CAP_SPEECH_RATE: f64 = 4.0;
pub const CAP_DRIFT: f64 = 2.0;

/// Настройки подгонки, от которых зависят слот и кап.
#[derive(Clone, Copy, Debug)]
pub struct FitRules {
    /// «Динамический темп речи»: фраза укладывается в точные границы своей реплики.
    pub speech_rate_on: bool,
    /// Контроль длительности: выключен — ускорение не ограничено.
    pub qc_duration: bool,
    /// Штатный предел ускорения (EngineOpts::max_stretch).
    pub max_stretch: f64,
    /// «Начинать в тишине»: не влезающая фраза начинается раньше, в тишину перед репликой, вместо ускорения.
    pub lead_into_silence: bool,
}

/// Самый большой сдвиг начала реплики назад, в тишину перед ней, с.
pub const LEAD_MAX: f64 = 1.0;

/// На сколько секунд начать реплику раньше, чтобы фраза длиной `clip` уложилась в `slot` без ускорения: недостающее
/// время, но не больше тишины перед репликой `free` и [`LEAD_MAX`]; 0 без «Начинать в тишине».
pub fn lead(clip: f64, slot: &Slot, free: f64, rules: &FitRules) -> f64 {
    if !rules.lead_into_silence {
        return 0.0;
    }
    (clip - slot.target).min(free).min(LEAD_MAX).max(0.0)
}

/// Слот фразы: `room` — от начала укладки до начала следующей реплики, `target` — во что фраза
/// укладывается, `cap` — штатный предел ускорения для этого слота (без эскалации).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub room: f64,
    pub target: f64,
    pub cap: f64,
}

/// Слот реплики [seg_start, seg_end], укладываемой с момента `at` (не раньше начала реплики), до начала
/// следующей реплики `next_start`.
pub fn slot(seg_start: f64, seg_end: f64, at: f64, next_start: f64, rules: &FitRules) -> Slot {
    let room = (next_start - at).max(MIN_SLOT);
    let target = if rules.speech_rate_on { (seg_end - seg_start).max(MIN_SLOT) } else { room };
    let cap = if !rules.qc_duration {
        CAP_UNCHECKED
    } else if target < SHORT_SLOT {
        rules.max_stretch.max(SHORT_SLOT_CAP)
    } else {
        rules.max_stretch
    };
    Slot { room, target, cap }
}

/// Во сколько раз клип длиннее слота.
pub fn needed(raw_secs: f64, target: f64) -> f64 {
    if target > 0.05 {
        raw_secs / target
    } else {
        1.0
    }
}

/// Кап, с которым рендер реально подгоняет клип: при «динамическом темпе речи» — сколько нужно (до
/// `CAP_SPEECH_RATE`), при отставании дубля больше `DRIFT_ESCALATE` — сколько нужно для догона (до
/// `CAP_DRIFT`), иначе штатный.
pub fn eff_cap(cap: f64, needed: f64, drift: f64, rules: &FitRules) -> f64 {
    if rules.speech_rate_on {
        cap.max(needed).min(CAP_SPEECH_RATE)
    } else if drift > DRIFT_ESCALATE {
        cap.max(needed).min(CAP_DRIFT)
    } else {
        cap
    }
}

/// Потолок `eff_cap` при отставании `drift`: сильнее рендер не ускорит клип никогда.
pub fn cap_ceiling(cap: f64, drift: f64, rules: &FitRules) -> f64 {
    eff_cap(cap, f64::INFINITY, drift, rules)
}

/// Фраза не влезла: клипу нужно ускорение сильнее `eff_cap`, с которым рендер его подгоняет.
pub fn over(needed: f64, eff_cap: f64) -> bool {
    needed > eff_cap + 1e-9
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Звучит в своём темпе: оценка не длиннее слота.
    Fits,
    /// Рендер уложит её ускорением в пределах своего капа (`cap_ceiling` без отставания).
    Tight,
    /// Не уложится и с ним.
    Impossible,
}

/// Прогноз до рендера: отставание дубля неизвестно, кап — как у реплики без отставания.
pub fn verdict(est: f64, slot: &Slot, rules: &FitRules) -> Verdict {
    if est <= slot.target + 1e-9 {
        Verdict::Fits
    } else if !over(needed(est, slot.target), cap_ceiling(slot.cap, 0.0, rules)) {
        Verdict::Tight
    } else {
        Verdict::Impossible
    }
}

/// Оценка звучания текста при темпе `cps`, секунд.
pub fn estimate(text: &str, cps: f64) -> f64 {
    if cps > 0.0 {
        text_units(text) as f64 / cps
    } else {
        0.0
    }
}

/// Лимит длины сокращённой строки: влезть в слот с запасом 5 %.
pub fn shorten_limit(target: f64, cps: f64) -> usize {
    (target * cps * 0.95).floor().max(1.0) as usize
}

/// Пол бюджета промпта перевода: «(≤3)» на междометиях искажает перевод.
pub const MIN_BUDGET: usize = 12;

/// Мягкий лимит длины перевода реплики длительностью `dur` при темпе `cps`; `dur` ≤ 0 (нет таймингов) —
/// без лимита.
pub fn char_budget(dur: f64, cps: f64) -> Option<usize> {
    if dur > 0.0 {
        Some(((dur * cps).round() as usize).max(MIN_BUDGET))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: FitRules = FitRules { speech_rate_on: false, qc_duration: true, max_stretch: 1.25, lead_into_silence: false };

    #[test]
    fn units_count_characters_with_single_spaces() {
        assert_eq!(text_units("  Привет,   мир!  "), 12);
        assert_eq!(text_units("你好，世界"), 5);
        assert_eq!(text_units(""), 0);
    }

    #[test]
    fn table_knows_regional_codes_and_defaults() {
        assert_eq!(table_cps("pt-BR"), table_cps("pt"));
        assert_eq!(table_cps("ZH"), 5.5);
        assert_eq!(table_cps("xx"), DEFAULT_CPS);
        assert!(table_cps("ja") < table_cps("en") && table_cps("zh") < table_cps("ja"));
    }

    #[test]
    fn calibration_is_the_median_of_usable_clips() {
        assert_eq!(calibrate([(20, 2.0), (30, 2.0)]), None);
        let c = calibrate([(20, 2.0), (30, 2.0), (26, 2.0), (3, 0.1), (40, 0.2)]).unwrap();
        assert_eq!(c.samples, 3);
        assert!((c.cps - 13.0).abs() < 1e-9, "{c:?}");
        let even = calibrate([(10, 1.0), (12, 1.0), (14, 1.0), (16, 1.0)]).unwrap();
        assert!((even.cps - 13.0).abs() < 1e-9);
    }

    #[test]
    fn slot_follows_the_speech_rate_switch() {
        let s = slot(1.0, 2.0, 1.2, 4.0, &RULES);
        assert!((s.room - 2.8).abs() < 1e-9 && (s.target - 2.8).abs() < 1e-9 && s.cap == 1.25);
        let sr = FitRules { speech_rate_on: true, ..RULES };
        let s = slot(1.0, 2.0, 1.0, 4.0, &sr);
        assert!((s.target - 1.0).abs() < 1e-9 && s.cap == SHORT_SLOT_CAP);
        let off = FitRules { qc_duration: false, ..RULES };
        assert_eq!(slot(0.0, 5.0, 0.0, 6.0, &off).cap, CAP_UNCHECKED);
        assert_eq!(slot(3.0, 3.1, 3.0, 3.1, &RULES).target, MIN_SLOT);
    }

    #[test]
    fn eff_cap_escalates_only_for_speech_rate_or_drift() {
        assert_eq!(eff_cap(1.25, 1.8, 0.0, &RULES), 1.25);
        assert_eq!(eff_cap(1.25, 1.8, 0.7, &RULES), 1.8);
        assert_eq!(eff_cap(1.25, 2.6, 0.7, &RULES), CAP_DRIFT);
        let sr = FitRules { speech_rate_on: true, ..RULES };
        assert_eq!(eff_cap(1.25, 3.0, 0.0, &sr), 3.0);
        assert_eq!(eff_cap(1.25, 5.0, 0.0, &sr), CAP_SPEECH_RATE);
        assert_eq!(eff_cap(1.25, 0.9, 0.0, &sr), 1.25);
    }

    #[test]
    fn verdict_bands() {
        let s = Slot { room: 2.0, target: 2.0, cap: 1.25 };
        assert_eq!(verdict(2.0, &s, &RULES), Verdict::Fits);
        assert_eq!(verdict(2.5, &s, &RULES), Verdict::Tight);
        assert_eq!(verdict(2.51, &s, &RULES), Verdict::Impossible);
        assert_eq!(verdict(estimate("a".repeat(26).as_str(), 13.0), &s, &RULES), Verdict::Fits);
        let sr = FitRules { speech_rate_on: true, ..RULES };
        assert_eq!(verdict(2.51, &s, &sr), Verdict::Tight, "the dynamic speech rate speeds a line up to x4");
        assert_eq!(verdict(8.0, &s, &sr), Verdict::Tight);
        assert_eq!(verdict(8.01, &s, &sr), Verdict::Impossible);
    }

    #[test]
    fn over_is_measured_against_the_cap_the_render_applied() {
        let sr = FitRules { speech_rate_on: true, ..RULES };
        assert_eq!(cap_ceiling(1.25, 0.0, &RULES), 1.25);
        assert_eq!(cap_ceiling(1.25, 0.7, &RULES), CAP_DRIFT);
        assert_eq!(cap_ceiling(1.25, 0.0, &sr), CAP_SPEECH_RATE);
        for (needed, drift, rules) in [(1.3, 0.0, &RULES), (1.9, 0.7, &RULES), (2.1, 0.7, &RULES), (3.9, 0.0, &sr), (4.2, 0.0, &sr)] {
            let applied = eff_cap(1.25, needed, drift, rules);
            assert_eq!(over(needed, applied), over(needed, cap_ceiling(1.25, drift, rules)), "{needed} {drift}");
        }
        assert!(over(1.3, eff_cap(1.25, 1.3, 0.0, &RULES)));
        assert!(!over(3.9, eff_cap(1.25, 3.9, 0.0, &sr)), "sped up x3.9, the line fits");
        assert!(over(4.2, eff_cap(1.25, 4.2, 0.0, &sr)));
    }

    #[test]
    fn limits_and_budgets() {
        assert_eq!(shorten_limit(2.0, 13.0), 24);
        assert_eq!(shorten_limit(0.01, 13.0), 1);
        assert_eq!(char_budget(2.0, 14.0), Some(28));
        assert_eq!(char_budget(3.0, 13.0), Some(39));
        assert_eq!(char_budget(0.01, 14.0), Some(MIN_BUDGET));
        assert_eq!(char_budget(0.0, 14.0), None);
        assert_eq!(char_budget(-1.0, 14.0), None);
    }

    #[test]
    fn a_line_that_does_not_fit_starts_earlier_into_the_silence_before_it() {
        let on = FitRules { lead_into_silence: true, ..RULES };
        let sl = slot(10.0, 12.0, 10.0, 12.0, &on);
        assert_eq!(lead(2.5, &sl, 3.0, &on), 0.5);
        assert_eq!(lead(2.5, &sl, 0.2, &on), 0.2);
        assert_eq!(lead(4.0, &sl, 3.0, &on), LEAD_MAX);
        assert_eq!(lead(1.5, &sl, 3.0, &on), 0.0);
        assert_eq!(lead(2.5, &sl, 3.0, &RULES), 0.0);
        let shifted = slot(10.0 - 0.5, 12.0, 10.0 - 0.5, 12.0, &on);
        assert_eq!(shifted.room, 2.5);
    }
}
