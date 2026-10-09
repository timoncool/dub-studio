//! dub-core — типы Project (serde) и EngineOpts, зеркало Pydantic-контракта dub-engine.

pub mod atomic;
pub mod fit;
pub mod glossary;
mod opts;
pub mod proc;
pub mod runtime;
mod project;

pub use glossary::{GlossaryEntry, GlossarySource};
pub use opts::EngineOpts;
pub use project::{
    Audio, Bilingual, Brand, BlurBox, CaptionOverride, Captions, Meta, Preset, Project, Render,
    SecondaryStyle, Segment, SubStyle, Subs, Title, Voice, ORDER_ORIGINAL_TOP, ORDER_TRANSLATION_TOP,
    SECONDARY_SIZE_PCT,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_roundtrips() {
        let p = Project::default();
        let j = p.to_json().unwrap();
        let p2 = Project::from_json(&j).unwrap();
        assert_eq!(p2.mode, "dub");
        assert_eq!(p2.tgt_lang, "en");
        assert_eq!(p2.render.burn_cq, 24);
        assert_eq!(p2.audio.voice.mode, "clone");
    }

    #[test]
    fn unknown_fields_passthrough() {
        // extra="allow": неизвестные поля должны пережить round-trip.
        let src = r#"{"mode":"dub","tgt_lang":"ru","future_field":42,
            "segments":[{"id":"s0","start":0.0,"end":1.5,"src_text":"hi","gui_only":true}],
            "captions":{"blur_boxes":[{"x":1,"y":2,"w":3,"h":4}]}}"#;
        let p = Project::from_json(src).unwrap();
        assert_eq!(p.tgt_lang, "ru");
        assert!(p.extra.contains_key("future_field"));
        assert_eq!(p.segments[0].extra.get("gui_only").unwrap(), &serde_json::json!(true));
        let out = p.to_json().unwrap();
        assert!(out.contains("future_field"));
        assert!(out.contains("gui_only"));
    }

    #[test]
    fn a_project_without_bilingual_settings_reads_with_the_defaults() {
        let p = Project::from_json(r#"{"subs":{"mode":"translate","burn":false}}"#).unwrap();
        assert_eq!(p.subs.mode, "translate");
        assert!(!p.subs.burn);
        assert_eq!(p.subs.bilingual, Bilingual::default());
        assert_eq!(p.subs.bilingual.order, ORDER_TRANSLATION_TOP);
        assert_eq!(p.subs.bilingual.secondary.size_pct, SECONDARY_SIZE_PCT);
        assert_eq!(p.subs.bilingual.secondary.color, None);
    }

    #[test]
    fn bilingual_settings_roundtrip() {
        let src = r##"{"subs":{"mode":"bilingual","bilingual":{"order":"original_top",
            "secondary":{"size_pct":60,"color":"#FFD400","opacity":80}}}}"##;
        let p = Project::from_json(src).unwrap();
        assert_eq!(p.subs.mode, "bilingual");
        assert_eq!(p.subs.bilingual.order, ORDER_ORIGINAL_TOP);
        assert_eq!(p.subs.bilingual.secondary.size_pct, 60);
        assert_eq!(p.subs.bilingual.secondary.color.as_deref(), Some("#FFD400"));
        assert_eq!(p.subs.bilingual.secondary.opacity, Some(80));
        let back = Project::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back.subs.bilingual, p.subs.bilingual);
        assert!(!back.subs.extra.contains_key("bilingual"), "bilingual — типизированное поле, не extra");
    }
}
