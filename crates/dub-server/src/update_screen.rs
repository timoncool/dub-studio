//! The update screen (CONTRACT, «The update screen»): what this release changed in what the studio downloads. The
//! plan compares the disk with the release's pins and its retired list: what is updated (old → new, size), what is
//! new, what is no longer needed and is removed (space freed), each with its reason from the release's table of
//! changes below. One button downloads it all and removes the former files only after the downloads that take their
//! place; the screen comes once per release whose plan is not empty, and later from the settings.

use std::path::Path;

use serde::Serialize;

use crate::setup::{self, Component, Delivery, Extract, Requirement};

/// The release whose plan the window has shown (a version per line, the last one counts), beside the window's language.
pub const SEEN_FILE: &str = "update-seen.txt";

/// One change of this release: a component (by id) or a leftover of a former release (by the name `setup::leftovers`
/// gives it), the reason the screen shows, and for a component the files of a former release it takes the place of.
struct Change {
    what: &'static str,
    reason: fn() -> String,
    replaces: &'static [&'static str],
}

/// The table of changes of this release against 4.4.0. A release that changes what the studio downloads writes its
/// own rows here.
const CHANGES: &[Change] = &[
    Change { what: "higgs-engine", reason: || t!("update-why-audiocpp-server"), replaces: &["models/higgs-engine/audiocpp_engine.dll", "models/higgs-engine/libaudiocpp_engine.so"] },
    Change { what: "transcribe-engine", reason: || t!("update-why-transcribe-avx2"), replaces: &[] },
    Change { what: "transcribe-engine-cpu", reason: || t!("update-why-transcribe-avx2"), replaces: &[] },
    Change { what: "crisperwhisper-turbo", reason: || t!("update-why-crisperwhisper"), replaces: &[] },
    Change { what: "langid", reason: || t!("update-why-langid"), replaces: &[] },
    Change { what: "cuda12-runtime", reason: || t!("update-why-cuda12"), replaces: &[] },
    Change { what: "llama", reason: || t!("update-why-llama"), replaces: &[] },
    Change { what: "bsroformer-engine", reason: || t!("update-why-bsroformer-studio"), replaces: &[] },
    Change { what: "bsroformer-engine-cuda12", reason: || t!("update-why-bsroformer-studio"), replaces: &[] },
    Change { what: "ffmpeg", reason: || t!("update-why-ffmpeg"), replaces: &[] },
    Change { what: "ytdlp", reason: || t!("update-why-ytdlp"), replaces: &[] },
    Change { what: "whisper", reason: || t!("update-why-faster-whisper-gone"), replaces: &[] },
    Change { what: "tools/whisper", reason: || t!("update-why-faster-whisper-gone"), replaces: &[] },
    Change { what: "tdt", reason: || t!("update-why-parakeet-onnx-gone"), replaces: &[] },
    Change { what: "tdt-fp32", reason: || t!("update-why-parakeet-onnx-gone"), replaces: &[] },
    Change { what: "tdt-ultra", reason: || t!("update-why-parakeet-onnx-gone"), replaces: &[] },
    Change { what: "tdt-ultra-int8", reason: || t!("update-why-parakeet-onnx-gone"), replaces: &[] },
    Change { what: "models/higgs-engine/audiocpp_engine.dll", reason: || t!("update-why-higgs-library-gone"), replaces: &[] },
    Change { what: "models/higgs-engine/libaudiocpp_engine.so", reason: || t!("update-why-higgs-library-gone"), replaces: &[] },
    Change { what: "models/runtime/onnxruntime-win-x64-1.24.2", reason: || t!("update-why-ort-gone"), replaces: &[] },
    Change { what: "models/runtime/onnxruntime-win-x64-gpu-1.24.2", reason: || t!("update-why-ort-gone"), replaces: &[] },
];

fn change(what: &str) -> Option<&'static Change> {
    CHANGES.iter().find(|c| c.what == what)
}

/// A component to download: updated (`from` names the release on disk when the disk tells it) or new.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub name: String,
    pub from: Option<String>,
    pub to: String,
    /// Bytes still to download.
    pub download: u64,
    pub reason: String,
}

/// A leftover of a former release that the button removes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gone {
    pub path: String,
    /// Bytes its removal frees (a linked-in folder frees none).
    pub size: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub version: String,
    pub updated: Vec<Row>,
    pub added: Vec<Row>,
    pub retired: Vec<Gone>,
    pub download: u64,
    pub freed: u64,
    /// The window shows the screen at start: the plan is not empty, the studio was set up by a former release and
    /// this release's plan has not been shown yet.
    pub show: bool,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.added.is_empty() && self.retired.is_empty()
    }

    /// The components the button downloads.
    pub fn ids(&self) -> Vec<String> {
        self.updated.iter().chain(&self.added).map(|r| r.id.clone()).collect()
    }
}

/// The release a pinned URL names: the GitHub release tag, the Hugging Face revision, or else the file name.
fn release_of(url: &str) -> String {
    let parts: Vec<&str> = url.split('/').collect();
    let after = |word: &str| parts.iter().position(|p| *p == word).and_then(|i| parts.get(i + 1)).copied();
    if let (Some(_), Some(tag)) = (parts.iter().position(|p| *p == "releases"), after("download")) {
        return tag.to_string();
    }
    if let Some(revision) = after("resolve") {
        return revision.chars().take(8).collect();
    }
    parts.last().copied().unwrap_or_default().to_string()
}

/// A former release of the component on disk, and its name when the disk tells it: a file it replaces, an install
/// record of another build, or a file or marker of the component that is not the pinned one.
fn former(repo_root: &Path, c: &Component) -> Option<Option<String>> {
    let replaced = change(c.id).into_iter().flat_map(|x| x.replaces.iter()).find(|rel| repo_root.join(rel).is_file());
    if let Some(rel) = replaced {
        return Some(rel.rsplit('/').next().map(str::to_string));
    }
    for f in c.files {
        if let Some(record) = setup::read_record(repo_root, f) {
            if record.sha256 != f.sha256 {
                return Some((record.url != f.url).then(|| release_of(&record.url)));
            }
        }
    }
    let direct = c.files.iter().any(|f| f.extract == Extract::None && repo_root.join(f.dest_rel).is_file() && !setup::file_installed(repo_root, f));
    let unrecorded = c.files.iter().any(|f| f.extract != Extract::None && setup::read_record(repo_root, f).is_none())
        && c.markers.iter().any(|m| repo_root.join(m.rel).is_file());
    (direct || unrecorded).then_some(None)
}

/// The plan of this release for the studio in `repo_root`.
pub fn plan(repo_root: &Path, models_root: &Path, version: &str) -> Plan {
    let status = setup::setup_status(repo_root);
    let manifest = setup::manifest();
    let (mut updated, mut added) = (Vec::new(), Vec::new());
    for st in status.components.iter().filter(|s| s.delivery == Delivery::Download && !s.installed) {
        let Some(c) = manifest.iter().find(|c| c.id == st.id) else { continue };
        let row = |from: Option<String>, reason: String| Row {
            id: st.id.clone(),
            name: st.name.clone(),
            from,
            to: c.files.first().map(|f| release_of(f.url)).unwrap_or_default(),
            download: st.size.saturating_sub(st.bytes_on_disk),
            reason,
        };
        match former(repo_root, c) {
            Some(from) => updated.push(row(from, change(c.id).map_or_else(|| t!("update-why-newer"), |x| (x.reason)()))),
            None if st.requirement == Requirement::Required => added.push(row(None, change(c.id).map_or_else(|| st.purpose.clone(), |x| (x.reason)()))),
            None => {}
        }
    }
    let retired: Vec<Gone> = setup::leftovers(repo_root, models_root)
        .into_iter()
        .map(|(what, paths)| Gone {
            // a retired model folder is named by the models folder, the rest by the program folder
            path: if what.contains('/') { what.to_string() } else { format!("models/{what}") },
            size: paths.iter().map(|p| setup::tree_size(p)).sum(),
            reason: change(what).map_or_else(|| t!("update-why-unused"), |x| (x.reason)()),
        })
        .collect();
    let download = updated.iter().chain(&added).map(|r| r.download).sum();
    let freed = retired.iter().map(|g| g.size).sum();
    let set_up_before = !updated.is_empty() || !retired.is_empty() || repo_root.join("models").join(".setup-download.json").is_file();
    let mut plan = Plan { version: version.to_string(), updated, added, retired, download, freed, show: false };
    plan.show = set_up_before && !plan.is_empty() && !seen(repo_root, version);
    plan
}

/// Whether the window has shown this release's plan.
fn seen(repo_root: &Path, version: &str) -> bool {
    match std::fs::read_to_string(repo_root.join(SEEN_FILE)) {
        Ok(text) => text.lines().last().is_some_and(|line| line.trim() == version),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            tracing::error!("[ERROR] {SEEN_FILE}: {e}");
            false
        }
    }
}

/// The window has shown this release's plan (the person pressed the button or «Later»).
pub fn mark_seen(repo_root: &Path, version: &str) -> Result<(), String> {
    std::fs::write(repo_root.join(SEEN_FILE), format!("{version}\n")).map_err(|e| format!("{SEEN_FILE}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_is_named_by_its_tag_or_revision() {
        assert_eq!(release_of("https://github.com/ggml-org/llama.cpp/releases/download/b11900/llama-b11900-bin-win-cuda-13.4-x64.zip"), "b11900");
        assert_eq!(release_of("https://huggingface.co/nyralabs/CrisperWhisper2.0_turbo/resolve/de0369c8a68025b7f6e86387b6eb5a3b369787c8/model.safetensors"), "de0369c8");
        assert_eq!(release_of("https://example.invalid/e.dll"), "e.dll");
    }

    #[test]
    fn every_change_names_a_component_or_a_leftover() {
        let ids: Vec<&str> = setup::manifest().iter().map(|c| c.id).collect();
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        for c in CHANGES {
            assert!(!(c.reason)().starts_with("update-why-"), "{}: its reason is not in the catalogue", c.what);
            // components a platform or a card leaves out of the manifest: CUDA 12.9 ones on a card that takes no CUDA
            // 12.9, the CUDA builds on a Mac
            let elsewhere = ["cuda12-runtime", "bsroformer-engine-cuda12", "transcribe-engine", "transcribe-engine-cuda12", "bsroformer-engine"];
            if ids.contains(&c.what) || elsewhere.contains(&c.what) {
                continue;
            }
            let path = if c.what.contains('/') { root.path().join(c.what) } else { models.join(c.what) };
            std::fs::create_dir_all(&path).unwrap();
            if c.what == "tools/whisper" {
                std::fs::write(path.join("whisper-faster.exe"), b"x").unwrap();
            }
            assert!(setup::leftovers(root.path(), &models).iter().any(|(what, _)| *what == c.what), "{} is neither a component nor a leftover", c.what);
        }
    }

    #[test]
    fn a_former_release_shows_once_and_its_leftovers_wait_for_the_button() {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        let fresh = plan(root.path(), &models, "9.9.9");
        assert!(!fresh.show, "a first start is the setup screen's, not the update screen's");
        assert!(fresh.retired.is_empty() && fresh.updated.is_empty());

        let library = root.path().join("models/higgs-engine/audiocpp_engine.dll");
        std::fs::create_dir_all(library.parent().unwrap()).unwrap();
        std::fs::write(&library, vec![0u8; 1000]).unwrap();
        let whisper = models.join("whisper").join("large-v3");
        std::fs::create_dir_all(&whisper).unwrap();
        std::fs::write(whisper.join("model.bin"), vec![0u8; 500]).unwrap();

        let p = plan(root.path(), &models, "9.9.9");
        assert!(p.show);
        let server = p.updated.iter().find(|r| r.id == "higgs-engine").expect("the server takes the place of the library");
        assert_eq!(server.from.as_deref(), Some("audiocpp_engine.dll"));
        assert!(server.to.starts_with("server-"), "{}", server.to);
        let gone: Vec<(&str, u64)> = p.retired.iter().map(|g| (g.path.as_str(), g.size)).collect();
        assert!(gone.contains(&("models/whisper", 500)) && gone.contains(&("models/higgs-engine/audiocpp_engine.dll", 1000)), "{gone:?}");
        assert_eq!(p.freed, 1500);
        assert!(p.ids().contains(&"higgs-engine".to_string()));
        assert!(library.is_file() && whisper.is_dir(), "the plan itself removes nothing");

        mark_seen(root.path(), "9.9.9").unwrap();
        assert!(!plan(root.path(), &models, "9.9.9").show, "shown once per release");
        assert!(plan(root.path(), &models, "10.0.0").show, "the next release shows its own");

        setup::remove_leftovers(root.path(), &models);
        assert!(!library.exists() && !models.join("whisper").exists());
        assert!(plan(root.path(), &models, "10.0.0").retired.is_empty());
    }
}
