//! Субтитры отдельными дорожками mkv: по режиму субтитров — перевод, оригинал или обе дорожки, каждая
//! со своей языковой меткой. Дорожка перевода в дубляже идёт по таймингам дубля (как вжигаемые
//! субтитры), дорожка оригинала — по таймингам реплик оригинала (как вторая звуковая дорожка).

use std::path::Path;
use std::process::Command;

use dub_core::Project;

use crate::dub_timing::DubTiming;
use crate::project_files::{srt, Row};

#[cfg(windows)]
const FFMPEG: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG: &str = "ffmpeg";

/// Дорожка субтитров: язык (ISO 639-2), название и текст SRT.
#[derive(Debug)]
pub struct Track {
    pub lang: &'static str,
    pub title: String,
    pub srt: String,
}

/// Реплики, у которых есть субтитр: скрытые и оставленные оригиналом (keep_original) не субтитруются.
fn subtitled(proj: &Project) -> impl Iterator<Item = &dub_core::Segment> {
    proj.segments.iter().filter(|s| {
        !s.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
            && !s.extra.get("keep_original").and_then(|v| v.as_bool()).unwrap_or(false)
    })
}

/// Дорожки субтитров проекта: translate — перевод, transcribe — оригинал, bilingual — перевод и
/// оригинал двумя дорожками; none — ни одной. `timing` — где фразы дубля звучат (dub/voiceover).
pub fn tracks(proj: &Project, timing: Option<&DubTiming>, tgt_title: &str, src_title: &str) -> Vec<Track> {
    let is_dub = proj.mode == "dub" || proj.mode == "voiceover";
    let overrides: std::collections::HashMap<&str, &str> = proj
        .captions
        .overrides
        .iter()
        .filter_map(|o| o.text.as_deref().map(|t| (o.seg_id.as_str(), t)))
        .collect();
    let row = |s: &dub_core::Segment, (start, end): (f64, f64), text: String| Row {
        id: s.id.clone(),
        start,
        end,
        speaker: s.speaker.clone().unwrap_or_else(|| "0".into()),
        text,
        original: None,
        words: None,
    };
    let translation: Vec<Row> = subtitled(proj)
        .filter_map(|s| {
            let text = overrides.get(s.id.as_str()).map_or(s.tgt_text.as_str(), |t| t).trim();
            if text.is_empty() {
                return None;
            }
            let at = match timing.and_then(|t| t.fresh(s)) {
                Some(st) => (st.at, st.at + st.dur),
                None => (s.start, s.end),
            };
            Some(row(s, at, text.to_string()))
        })
        .collect();
    let original: Vec<Row> = subtitled(proj)
        .filter(|s| !s.src_text.trim().is_empty())
        .map(|s| row(s, (s.start, s.end), s.src_text.trim().to_string()))
        .collect();
    let src_code = proj.meta.extra.get("src_lang").and_then(|v| v.as_str()).unwrap_or("");
    let tgt = || Track { lang: crate::media::iso639_1_to_2(&proj.tgt_lang), title: tgt_title.to_string(), srt: srt(&translation) };
    let src = || Track { lang: crate::media::iso639_1_to_2(src_code), title: src_title.to_string(), srt: srt(&original) };
    let mut out = match proj.subs.mode.as_str() {
        "translate" => vec![tgt()],
        "transcribe" if is_dub => vec![src()],
        "transcribe" => vec![Track { lang: crate::media::iso639_1_to_2(src_code), ..tgt() }],
        "bilingual" => vec![tgt(), src()],
        _ => Vec::new(),
    };
    out.retain(|t| !t.srt.is_empty());
    out
}

/// Добавить дорожки субтитров в готовый mkv (видео и звук копируются как есть). `on_screen` — субтитры
/// уже вжжены в картинку: тогда дорожки не включаются по умолчанию, иначе первая — по умолчанию.
pub fn add_to_mkv(mkv: &Path, tracks: &[Track], work_dir: &Path, on_screen: bool) -> Result<(), String> {
    if tracks.is_empty() {
        return Ok(());
    }
    let mut files = Vec::with_capacity(tracks.len());
    for (i, t) in tracks.iter().enumerate() {
        let p = work_dir.join(format!("subtrack_{i}.srt"));
        std::fs::write(&p, &t.srt).map_err(|e| format!("{}: {e}", p.display()))?;
        files.push(p);
    }
    let tmp = mkv.with_extension("subs.mkv");
    let mut cmd = Command::new(FFMPEG);
    cmd.arg("-y").arg("-i").arg(mkv);
    for f in &files {
        cmd.arg("-i").arg(f);
    }
    cmd.args(["-map", "0"]);
    for i in 0..files.len() {
        cmd.arg("-map").arg(format!("{}:0", i + 1));
    }
    cmd.args(["-c", "copy", "-c:s", "srt"]);
    for (i, t) in tracks.iter().enumerate() {
        cmd.arg(format!("-metadata:s:s:{i}")).arg(format!("language={}", t.lang));
        cmd.arg(format!("-metadata:s:s:{i}")).arg(format!("title={}", t.title));
        let default = !on_screen && i == 0;
        cmd.arg(format!("-disposition:s:{i}")).arg(if default { "default" } else { "0" });
    }
    cmd.arg(&tmp);
    let out = dub_core::proc::output(&mut cmd).map_err(|e| t!("common-ffmpeg-start", error = e.to_string()))?;
    for f in &files {
        let _ = std::fs::remove_file(f);
    }
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(1500).collect::<String>().chars().rev().collect();
        return Err(t!("common-ffmpeg-exit", code = format!("{:?}", out.status.code()), tail = tail));
    }
    std::fs::rename(&tmp, mkv).map_err(|e| format!("{} -> {}: {e}", tmp.display(), mkv.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(mode: &str, subs: &str) -> Project {
        serde_json::from_value(json!({
            "mode": mode,
            "tgt_lang": "ru",
            "meta": { "src_lang": "en" },
            "subs": { "mode": subs },
            "segments": [
                { "id": "s0", "start": 1.0, "end": 2.0, "src_text": "Hello", "tgt_text": "Привет" },
                { "id": "s1", "start": 3.0, "end": 4.0, "src_text": "Bye", "tgt_text": "Пока", "hidden": true },
            ]
        }))
        .unwrap()
    }

    #[test]
    fn bilingual_is_two_tracks_with_their_languages() {
        let t = tracks(&project("dub", "bilingual"), None, "Русский", "English");
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].lang, t[0].title.as_str()), ("rus", "Русский"));
        assert_eq!((t[1].lang, t[1].title.as_str()), ("eng", "English"));
        assert_eq!(t[0].srt, "1\n00:00:01,000 --> 00:00:02,000\nПривет\n");
        assert_eq!(t[1].srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n");
    }

    #[test]
    fn one_track_follows_the_subtitle_language_and_none_gives_none() {
        let dub_original = tracks(&project("dub", "transcribe"), None, "Русский", "English");
        assert_eq!(dub_original.len(), 1);
        assert_eq!(dub_original[0].lang, "eng");
        assert!(dub_original[0].srt.contains("Hello"));
        let translation = tracks(&project("voiceover", "translate"), None, "Русский", "English");
        assert_eq!(translation.len(), 1);
        assert_eq!(translation[0].lang, "rus");
        assert!(tracks(&project("dub", "none"), None, "Русский", "English").is_empty());
    }

    #[test]
    fn the_translation_track_follows_where_the_dub_is_heard() {
        let proj = project("dub", "translate");
        let mut timing = DubTiming::default();
        timing.segments.insert(
            "s0".into(),
            crate::dub_timing::SegTiming { text: "Привет".into(), seg_start: 1.0, seg_end: 2.0, at: 1.25, dur: 1.5, words: vec![], fit: None },
        );
        let t = tracks(&proj, Some(&timing), "Русский", "English");
        assert_eq!(t[0].srt, "1\n00:00:01,250 --> 00:00:02,750\nПривет\n");
    }

    #[test]
    #[ignore = "нужны ffmpeg и ffprobe на PATH"]
    fn tracks_land_in_the_mkv_with_their_languages() {
        let dir = tempfile::tempdir().unwrap();
        let mkv = dir.path().join("output.mkv");
        let made = Command::new(FFMPEG)
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=160x90:rate=10:duration=3"])
            .args(["-f", "lavfi", "-i", "sine=duration=3", "-c:v", "libx264", "-c:a", "aac"])
            .arg(&mkv)
            .status()
            .unwrap();
        assert!(made.success());
        let t = tracks(&project("dub", "bilingual"), None, "Russian", "English");
        add_to_mkv(&mkv, &t, dir.path(), true).unwrap();
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "s", "-show_entries", "stream=codec_name:stream_tags=language,title:stream_disposition=default", "-of", "csv=p=0"])
            .arg(&mkv)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&probe.stdout).replace('\r', "");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines, ["subrip,0,rus,Russian", "subrip,0,eng,English"], "{text}");
        assert!(!dir.path().join("subtrack_0.srt").exists(), "временные srt убраны");
    }
}
