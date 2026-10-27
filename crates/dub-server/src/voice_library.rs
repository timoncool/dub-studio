//! Голоса библиотеки: файл `voices/<имя>.<wav|mp3|…>` с транскриптом рефа `<имя>.txt` рядом. Исходник голоса до
//! очистки хранится в `voices/.originals/`, чтобы очистку можно было отменить. Те же функции, что у Speech Studio
//! (`voices/library.rs`), без карточек персон.

use std::path::{Path, PathBuf};

pub const AUDIO_EXTS: [&str; 5] = ["wav", "mp3", "flac", "m4a", "ogg"];
const ORIGINALS: &str = ".originals";
const WORK: &str = ".work";

/// Временный WAV для звука, который пишет ffmpeg: формат он берёт из расширения, поэтому `.wav` в конце, а папка
/// служебная, чтобы недописанный файл не попал в список голосов.
fn staging_wav(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let work = dir.join(WORK);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    Ok(work.join(format!("{name}-{}.wav", uuid::Uuid::new_v4().simple())))
}

fn with_ext(dir: &Path, name: &str, exts: &[&str]) -> Option<(PathBuf, String)> {
    exts.iter().map(|e| (dir.join(format!("{name}.{e}")), e.to_string())).find(|(p, _)| p.is_file())
}

pub fn audio_path(dir: &Path, name: &str) -> Option<PathBuf> {
    with_ext(dir, name, &AUDIO_EXTS).map(|(p, _)| p)
}

pub fn original_path(dir: &Path, name: &str) -> Option<PathBuf> {
    with_ext(&dir.join(ORIGINALS), name, &AUDIO_EXTS).map(|(p, _)| p)
}

/// Файлы голоса и их суффиксы после имени (`.wav`, `.txt`, `.MP3.txt` …), в папке и в `.originals`.
fn sidecars(dir: &Path, name: &str) -> Vec<(PathBuf, String)> {
    let mut suffixes: Vec<String> = AUDIO_EXTS.iter().chain(["txt", "lab"].iter()).map(|e| format!(".{e}")).collect();
    suffixes.extend(AUDIO_EXTS.iter().flat_map(|e| [format!(".{e}.txt"), format!(".{}.txt", e.to_ascii_uppercase())]));
    let mut all = Vec::new();
    for folder in [dir.to_path_buf(), dir.join(ORIGINALS)] {
        for suffix in &suffixes {
            let p = folder.join(format!("{name}{suffix}"));
            if p.is_file() {
                all.push((p, suffix.clone()));
            }
        }
    }
    all
}

pub fn rename(dir: &Path, from: &str, to: &str) -> Result<(), String> {
    if to.is_empty() {
        return Err("the new name is empty".into());
    }
    if from == to {
        return Ok(());
    }
    audio_path(dir, from).ok_or_else(|| format!("no voice named {from}"))?;
    if audio_path(dir, to).is_some() {
        return Err(format!("a voice named {to} already exists"));
    }
    for (p, suffix) in sidecars(dir, from) {
        let target = p.with_file_name(format!("{to}{suffix}"));
        std::fs::rename(&p, &target).map_err(|e| format!("{} -> {}: {e}", p.display(), target.display()))?;
    }
    Ok(())
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    audio_path(dir, name).ok_or_else(|| format!("no voice named {name}"))?;
    for (p, _) in sidecars(dir, name) {
        std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

/// Заменить звук голоса очищенным, сохранив исходник в `.originals` (только первый: повторная очистка не
/// затирает настоящий исходник уже очищенным звуком).
pub fn replace_with_cleaned(dir: &Path, name: &str, cleaned: &Path) -> Result<(), String> {
    let audio = audio_path(dir, name).ok_or_else(|| format!("no voice named {name}"))?;
    let originals = dir.join(ORIGINALS);
    std::fs::create_dir_all(&originals).map_err(|e| format!("{}: {e}", originals.display()))?;
    if original_path(dir, name).is_none() {
        let keep = originals.join(audio.file_name().expect("a file name"));
        std::fs::copy(&audio, &keep).map_err(|e| format!("{}: {e}", keep.display()))?;
    }
    let target = dir.join(format!("{name}.wav"));
    let part = staging_wav(dir, name)?;
    if let Err(e) = crate::media::loudnorm(cleaned, &part, -20.0, -2.0, 11.0) {
        if let Err(rm) = std::fs::remove_file(&part) {
            tracing::warn!("{}: {rm}", part.display());
        }
        return Err(e);
    }
    if audio != target {
        std::fs::remove_file(&audio).map_err(|e| format!("{}: {e}", audio.display()))?;
    }
    std::fs::rename(&part, &target).map_err(|e| format!("{}: {e}", target.display()))
}

/// Вернуть исходник вместо очищенного звука.
pub fn restore_original(dir: &Path, name: &str) -> Result<(), String> {
    let orig = original_path(dir, name).ok_or_else(|| format!("{name} has no original kept"))?;
    if let Some(cur) = audio_path(dir, name) {
        std::fs::remove_file(&cur).map_err(|e| format!("{}: {e}", cur.display()))?;
    }
    let target = dir.join(orig.file_name().expect("a file name"));
    std::fs::rename(&orig, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    let kept = dir.join(ORIGINALS).join(format!("{name}.txt"));
    if kept.is_file() {
        let back = dir.join(format!("{name}.txt"));
        std::fs::rename(&kept, &back).map_err(|e| format!("{}: {e}", back.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(dir: &Path, name: &str, ext: &str) {
        std::fs::write(dir.join(format!("{name}.{ext}")), b"RIFF").unwrap();
    }

    #[test]
    fn rename_moves_every_file_of_the_voice_and_refuses_a_taken_name() {
        let d = tempfile::tempdir().unwrap();
        voice(d.path(), "A", "wav");
        std::fs::write(d.path().join("A.txt"), "text").unwrap();
        std::fs::create_dir(d.path().join(".originals")).unwrap();
        voice(&d.path().join(".originals"), "A", "mp3");
        voice(d.path(), "B", "wav");
        assert!(rename(d.path(), "A", "B").is_err());
        rename(d.path(), "A", "C").unwrap();
        assert!(d.path().join("C.wav").is_file() && d.path().join("C.txt").is_file());
        assert!(original_path(d.path(), "C").is_some());
        assert!(audio_path(d.path(), "A").is_none());
    }

    #[test]
    fn restoring_brings_back_the_sound_kept_before_cleaning() {
        let d = tempfile::tempdir().unwrap();
        voice(d.path(), "A", "wav");
        std::fs::create_dir_all(d.path().join(".originals")).unwrap();
        voice(&d.path().join(".originals"), "A", "mp3");
        restore_original(d.path(), "A").unwrap();
        assert_eq!(audio_path(d.path(), "A"), Some(d.path().join("A.mp3")));
        assert!(original_path(d.path(), "A").is_none());
        assert!(restore_original(d.path(), "A").is_err());
    }

    #[test]
    fn delete_removes_the_voice_with_its_files() {
        let d = tempfile::tempdir().unwrap();
        voice(d.path(), "A", "wav");
        std::fs::write(d.path().join("A.txt"), "text").unwrap();
        std::fs::create_dir_all(d.path().join(".originals")).unwrap();
        voice(&d.path().join(".originals"), "A", "wav");
        delete(d.path(), "A").unwrap();
        assert!(sidecars(d.path(), "A").is_empty());
    }
}
