//! Голоса библиотеки: файл `voices/<имя>.<wav|mp3|…>` с транскриптом рефа `<имя>.txt` рядом. Исходник голоса до
//! очистки хранится в `voices/.originals/`, чтобы очистку можно было отменить. Те же функции, что у Speech Studio
//! (`voices/library.rs`); карточка голоса — `<имя>.json` в её форме.

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

/// What is known of a voice beyond its sound, kept beside it as `<имя>.json` in Speech Studio's form, so a library
/// moved between the programs keeps it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct VoiceCard {
    pub gender: String,
    pub age: String,
    pub lang: String,
    pub tags: Vec<String>,
    pub source: String,
    pub consent: String,
    pub favorite: bool,
    pub kind: String,
    pub transcript_auto: bool,
    pub cleaned: bool,
    /// The voice this one is a variant of (a performance of the same character: angry, whispering); empty — its own.
    pub variant_of: String,
    /// The variant's name, shown where a block picks it.
    pub variant: String,
    /// What the voice may be used for: an SPDX id such as CC-BY-4.0, or the terms in words.
    pub license: String,
    /// A long reference gives each new take its next good piece instead of always the best one.
    pub rotate: bool,
    /// The name its source gives the voice (the cloud catalogue's «Алана Хасанова»), shown instead of the file's.
    pub display: String,
    /// Who the voice is, as its source describes it (the cloud catalogue's line).
    pub description: String,
    /// The speaker's year of birth and the recording's length in seconds, as its source gives them.
    pub birth_year: Option<u32>,
    pub duration_s: Option<f64>,
    /// The reference's loudness, LUFS, measured when its sound last changed (None: not measured yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lufs: Option<f64>,
}

impl Default for VoiceCard {
    fn default() -> Self {
        VoiceCard {
            gender: String::new(),
            age: String::new(),
            lang: String::new(),
            tags: Vec::new(),
            source: String::new(),
            consent: String::new(),
            favorite: false,
            kind: "clone".into(),
            transcript_auto: false,
            cleaned: false,
            variant_of: String::new(),
            variant: String::new(),
            license: String::new(),
            rotate: false,
            display: String::new(),
            description: String::new(),
            birth_year: None,
            duration_s: None,
            lufs: None,
        }
    }
}

const LANG_WORDS: [(&str, &str); 11] = [
    ("arabic", "ar"), ("chinese", "zh"), ("english", "en"), ("french", "fr"), ("german", "de"), ("italian", "it"),
    ("japanese", "ja"), ("korean", "ko"), ("portuguese", "pt"), ("russian", "ru"), ("spanish", "es"),
];

fn gender_word(w: &str) -> Option<&'static str> {
    match w.to_ascii_lowercase().trim_end_matches(|c: char| c.is_ascii_digit()) {
        "female" | "famale" => Some("female"),
        "male" => Some("male"),
        _ => None,
    }
}

/// Карточка по имени из войспака: `RU_Female_…` (регистр любой, бывает и `Famale`) и `English_Female2` — язык и пол.
pub fn guess_card(name: &str) -> VoiceCard {
    let mut card = VoiceCard::default();
    let parts: Vec<&str> = name.splitn(3, '_').collect();
    if parts.len() == 3 && parts[0].len() == 2 && parts[0].chars().all(|c| c.is_ascii_alphabetic()) {
        if let Some(g) = gender_word(parts[1]) {
            card.lang = parts[0].to_ascii_lowercase();
            card.gender = g.into();
            card.source = "pack".into();
        }
    } else if parts.len() == 2 {
        let lang = LANG_WORDS.iter().find(|(w, _)| parts[0].eq_ignore_ascii_case(w)).map(|(_, c)| *c);
        if let (Some(lang), Some(g)) = (lang, gender_word(parts[1])) {
            card.lang = lang.into();
            card.gender = g.into();
            card.source = "pack".into();
        }
    }
    card
}

pub fn read_card(dir: &Path, name: &str) -> Result<VoiceCard, String> {
    let p = dir.join(format!("{name}.json"));
    match std::fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(guess_card(name)),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

pub fn write_card(dir: &Path, name: &str, card: &VoiceCard) -> Result<(), String> {
    let text = serde_json::to_string_pretty(card).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.json"));
    let tmp = dir.join(format!(".{name}.json.{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// Файлы голоса и их суффиксы после имени (`.wav`, `.txt`, `.MP3.txt` …), в папке и в `.originals`.
fn sidecars(dir: &Path, name: &str) -> Vec<(PathBuf, String)> {
    let mut suffixes: Vec<String> = AUDIO_EXTS.iter().chain(["txt", "lab", "json"].iter()).map(|e| format!(".{e}")).collect();
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
    // the name given by hand is the one shown from now on, not the one its source gave
    if dir.join(format!("{to}.json")).is_file() {
        let mut card = read_card(dir, to)?;
        if !card.display.is_empty() {
            card.display.clear();
            write_card(dir, to, &card)?;
        }
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

/// Имя голоса, безопасное для файла: буквы, цифры, пробел, `-`, `_`, до 48 знаков.
pub fn sanitize(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_').take(48).collect();
    n.trim().to_string()
}

/// Имя, свободное в папке: `base`, затем `base 2`, `base 3`…
pub fn free_name(dir: &Path, base: &str) -> String {
    let base = if base.is_empty() { "Voice".to_string() } else { base.to_string() };
    if audio_path(dir, &base).is_none() {
        return base;
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| audio_path(dir, n).is_none()).expect("an unused name")
}

/// Новый голос из звукового файла: WAV с громкостью рефа -20 LUFS (пик -2 dBTP), под свободным именем.
pub fn import_audio(dir: &Path, base: &str, src: &Path, card: VoiceCard) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = free_name(dir, base);
    let target = dir.join(format!("{name}.wav"));
    let part = staging_wav(dir, &name)?;
    let made = crate::media::loudnorm(src, &part, -20.0, -2.0, 11.0).and_then(|_| std::fs::rename(&part, &target).map_err(|e| format!("{}: {e}", target.display())));
    if let Err(e) = made {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    write_card(dir, &name, &card)?;
    Ok(name)
}

/// The audio formats the library lists and voicing reads as they are (lib.rs list_voice_names, render.rs pack voices).
pub const LISTED_EXTS: [&str; 2] = ["wav", "mp3"];

pub fn import_zip(dir: &Path, bytes: &[u8]) -> Result<(Vec<String>, Vec<String>), String> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut groups: std::collections::BTreeMap<String, Vec<(String, Vec<u8>)>> = Default::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let raw = f.name().map_err(|e| e.to_string())?.into_owned();
        let entry = Path::new(&raw).file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let Some((stem, ext)) = entry.strip_suffix(".words.json").map(|stem| (stem, "words.json")).or_else(|| entry.rsplit_once('.')) else { continue };
        let name = sanitize(stem);
        let ext = ext.to_ascii_lowercase();
        let known = AUDIO_EXTS.contains(&ext.as_str()) || ["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) || ext == "txt" || ext == "json" || ext == "words.json";
        if name.is_empty() || !known {
            continue;
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        groups.entry(name).or_default().push((ext, data));
    }
    // every card is read before anything is written: a broken one stops the import with the library as it was
    let mut cards: std::collections::BTreeMap<String, VoiceCard> = Default::default();
    for (name, files) in &groups {
        if let Some((_, data)) = files.iter().find(|(e, _)| e == "json") {
            cards.insert(name.clone(), serde_json::from_slice(data).map_err(|e| format!("{name}.json: {e}"))?);
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut added, mut same) = (Vec::new(), Vec::new());
    for (name, files) in groups {
        let Some((audio_ext, audio)) = files.iter().find(|(e, _)| AUDIO_EXTS.contains(&e.as_str())) else { continue };
        if let Some(existing) = audio_path(dir, &name) {
            if std::fs::read(&existing).ok().as_deref() == Some(audio.as_slice()) {
                same.push(name);
                continue;
            }
        }
        let target = free_name(dir, &name);
        if LISTED_EXTS.contains(&audio_ext.as_str()) {
            std::fs::write(dir.join(format!("{target}.{audio_ext}")), audio).map_err(|e| format!("{target}: {e}"))?;
        } else {
            // the library and voicing read .wav and .mp3: another format becomes a .wav at the reference loudness
            let src = staging_wav(dir, &format!("{target}-in"))?.with_extension(audio_ext);
            std::fs::write(&src, audio).map_err(|e| format!("{}: {e}", src.display()))?;
            let part = staging_wav(dir, &target)?;
            let wav = dir.join(format!("{target}.wav"));
            let made = crate::media::loudnorm(&src, &part, -20.0, -2.0, 11.0)
                .and_then(|_| std::fs::rename(&part, &wav).map_err(|e| format!("{}: {e}", wav.display())));
            let _ = std::fs::remove_file(&src);
            if let Err(e) = made {
                let _ = std::fs::remove_file(&part);
                return Err(format!("{name}.{audio_ext}: {e}"));
            }
        }
        for (ext, data) in files.iter().filter(|(e, _)| !AUDIO_EXTS.contains(&e.as_str()) && e != "json") {
            std::fs::write(dir.join(format!("{target}.{ext}")), data).map_err(|e| format!("{target}: {e}"))?;
        }
        if let Some(card) = cards.remove(&name) {
            write_card(dir, &target, &card)?;
        }
        added.push(target);
    }
    Ok((added, same))
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

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            for (name, data) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    /// A voice exported by Speech Studio comes in whole with its card; the same voice again is recognised, a different
    /// one under a taken name gets a number.
    #[test]
    fn an_exported_voice_comes_in_whole_and_a_second_import_is_recognised() {
        let card = serde_json::to_vec(&VoiceCard { favorite: true, gender: "female".into(), ..Default::default() }).unwrap();
        let zip = zip_of(&[("Анна.wav", b"RIFFaudio"), ("Анна.txt", "привет".as_bytes()), ("Анна.json", &card), ("readme.md", b"x")]);
        let b = tempfile::tempdir().unwrap();
        let (added, same) = import_zip(b.path(), &zip).unwrap();
        assert_eq!((added, same.len()), (vec!["Анна".to_string()], 0));
        assert_eq!(std::fs::read_to_string(b.path().join("Анна.txt")).unwrap(), "привет");
        assert!(read_card(b.path(), "Анна").unwrap().favorite);
        assert!(!b.path().join("readme.md").exists(), "what is not a voice's file stays out");
        let (added, same) = import_zip(b.path(), &zip).unwrap();
        assert!(added.is_empty() && same == ["Анна"]);
        std::fs::write(b.path().join("Анна.wav"), b"RIFFother").unwrap();
        let (added, _) = import_zip(b.path(), &zip).unwrap();
        assert_eq!(added, ["Анна 2"], "a different voice under a taken name gets a number");
    }

    #[test]
    fn a_free_name_counts_up_and_a_name_is_made_safe_for_a_file() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(free_name(d.path(), "Голос"), "Голос");
        std::fs::write(d.path().join("Голос.wav"), b"x").unwrap();
        assert_eq!(free_name(d.path(), "Голос"), "Голос 2");
        assert_eq!(free_name(d.path(), ""), "Voice");
        assert_eq!(sanitize("  ../Мой: голос?  "), "Мой голос");
    }
}
