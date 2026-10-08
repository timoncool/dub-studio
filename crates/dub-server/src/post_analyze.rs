//! Настройки стартового флоу, которые ложатся на проект в конце analyze: громкость оригинала под
//! закадром, блюр-подложка под субтитрами, вторая аудиодорожка и голоса из библиотеки. Приходят
//! параметрами analyze и лежат в job.json вместе с остальными, поэтому «Продолжить» прерванного анализа
//! даёт тот же проект, что и первый запуск.

use std::collections::HashMap;
use std::path::Path;

use dub_core::Project;
use serde_json::{json, Value};

use crate::{patch, voice_slots};

#[derive(Debug, Default, PartialEq)]
pub struct PostAnalyze {
    vo_gain: Option<f64>,
    sub_blur: Option<bool>,
    /// Контейнер выхода со второй (оригинальной) дорожкой: mp4 | mkv.
    keep_original: Option<String>,
    voice_slots: Option<(Vec<String>, Vec<String>)>,
}

impl PostAnalyze {
    /// Параметры query analyze: vo_gain (дБ), sub_blur (0|1), keep_original=1 + container (mp4|mkv),
    /// voice_slots (JSON {male:[имена], female:[имена]}). Кривое значение — ошибка, а не пропуск.
    pub fn from_query(q: &HashMap<String, String>) -> Result<Self, String> {
        let vo_gain = match q.get("vo_gain") {
            None => None,
            Some(v) => Some(
                v.parse::<f64>()
                    .ok()
                    .filter(|g| g.is_finite())
                    .ok_or_else(|| t!("post-analyze-bad-vo-gain", value = format!("{v:?}")))?,
            ),
        };
        let sub_blur = match q.get("sub_blur").map(String::as_str) {
            None => None,
            Some("1") => Some(true),
            Some("0") => Some(false),
            Some(v) => return Err(t!("post-analyze-bad-flag", name = "sub_blur", value = format!("{v:?}"))),
        };
        let keep_original = match q.get("keep_original").map(String::as_str) {
            None | Some("0") => None,
            Some("1") => match q.get("container").map(String::as_str).unwrap_or("mp4") {
                c @ ("mp4" | "mkv") => Some(c.to_string()),
                other => return Err(t!("post-analyze-bad-container", value = format!("{other:?}"))),
            },
            Some(v) => return Err(t!("post-analyze-bad-flag", name = "keep_original", value = format!("{v:?}"))),
        };
        let voice_slots = match q.get("voice_slots") {
            None => None,
            Some(v) => {
                let body: Value = serde_json::from_str(v).map_err(|e| format!("voice_slots: {e}"))?;
                if !body.is_object() {
                    return Err(t!("post-analyze-bad-voice-slots"));
                }
                let s = voice_slots::Slots::from_json(&body);
                Some((s.male, s.female))
            }
        };
        Ok(PostAnalyze { vo_gain, sub_blur, keep_original, voice_slots })
    }

    /// Применить к только что проанализированному проекту `dir`. Возвращает итог для результата
    /// джобы: {voice_slots: {assigned, speakers} | {error: missing_voices, names} | {error: no_vocals}}.
    /// Голоса из библиотеки, которых уже нет, не валят анализ: итог несёт код, окно пишет его в журнал,
    /// а спикеры остаются на клонировании.
    pub fn apply(&self, proj: &mut Project, dir: &Path, available_voices: &[String]) -> Result<Value, String> {
        let mut edits = Vec::new();
        if let Some(g) = self.vo_gain {
            edits.push(json!({ "op": "voiceover_gain", "gain_db": g }));
        }
        if let Some(on) = self.sub_blur {
            edits.push(json!({ "op": "sub_blur", "on": on }));
        }
        if let Some(c) = &self.keep_original {
            edits.push(json!({ "op": "keep_original", "keep": true, "container": c }));
        }
        for e in &edits {
            patch::apply(proj, e).map_err(|(_, msg)| t!("post-analyze-edit-failed", edit = e.to_string(), error = msg))?;
        }
        let mut out = serde_json::Map::new();
        if let Some((male, female)) = &self.voice_slots {
            let slots = voice_slots::Slots { male: male.clone(), female: female.clone() };
            let missing = slots.missing_in(available_voices);
            let summary = if !missing.is_empty() {
                json!({ "error": "missing_voices", "names": missing })
            } else if let Some(vocals) = voice_slots::vocals_for(dir) {
                let assigns = voice_slots::assign(proj, &vocals, dir, &slots);
                let assigned = assigns.iter().filter(|a| a.voice.is_some()).count();
                json!({ "assigned": assigned, "speakers": voice_slots::mapping(&assigns) })
            } else {
                json!({ "error": "no_vocals" })
            };
            out.insert("voice_slots".into(), summary);
        }
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_all_options() {
        let p = PostAnalyze::from_query(&q(&[
            ("vo_gain", "-12.5"),
            ("sub_blur", "0"),
            ("keep_original", "1"),
            ("container", "mkv"),
            ("voice_slots", r#"{"male":[" Bob ",""],"female":["Ann"]}"#),
        ]))
        .unwrap();
        assert_eq!(
            p,
            PostAnalyze {
                vo_gain: Some(-12.5),
                sub_blur: Some(false),
                keep_original: Some("mkv".into()),
                voice_slots: Some((vec!["Bob".into()], vec!["Ann".into()])),
            }
        );
    }

    #[test]
    fn absent_options_change_nothing() {
        assert_eq!(PostAnalyze::from_query(&q(&[("tgt_lang", "ru")])).unwrap(), PostAnalyze::default());
    }

    #[test]
    fn malformed_values_are_errors() {
        assert!(PostAnalyze::from_query(&q(&[("vo_gain", "loud")])).is_err());
        assert!(PostAnalyze::from_query(&q(&[("sub_blur", "yes")])).is_err());
        assert!(PostAnalyze::from_query(&q(&[("keep_original", "1"), ("container", "avi")])).is_err());
        assert!(PostAnalyze::from_query(&q(&[("voice_slots", "[1,2]")])).is_err());
    }

    #[test]
    fn applies_patches_and_reports_missing_voices() {
        let dir = std::env::temp_dir().join(format!("dub-post-analyze-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut proj = Project::default();
        let p = PostAnalyze::from_query(&q(&[
            ("vo_gain", "-9"),
            ("sub_blur", "1"),
            ("keep_original", "1"),
            ("voice_slots", r#"{"male":["Gone"],"female":[]}"#),
        ]))
        .unwrap();
        let out = p.apply(&mut proj, &dir, &["Kept".to_string()]).unwrap();
        assert_eq!(proj.audio.voiceover_gain_db, -9.0);
        assert!(proj.render.blur);
        assert!(proj.audio.keep_original_track);
        assert_eq!(proj.audio.container, "mp4");
        assert_eq!(out["voice_slots"], json!({ "error": "missing_voices", "names": ["Gone"] }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
