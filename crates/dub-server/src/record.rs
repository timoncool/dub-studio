//! Запись голоса с микрофона (нативный cpal + hound → mono 16-bit WAV). Порт Higgs recorder.rs, но
//! уровень отдаём через атомик (GET /record/level опрашивает), не Tauri-события. WAV идёт в референс
//! клонирования / голос в галерею.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

struct RecState {
    stop_tx: Sender<()>,
    path: std::path::PathBuf,
}

fn recorder() -> &'static Mutex<Option<RecState>> {
    static R: OnceLock<Mutex<Option<RecState>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

fn peak_cell() -> &'static AtomicU32 {
    static P: OnceLock<AtomicU32> = OnceLock::new();
    P.get_or_init(|| AtomicU32::new(0))
}

/// Текущий пик 0..1 (для метра уровня).
pub fn level() -> f32 {
    f32::from_bits(peak_cell().load(Ordering::Relaxed))
}

/// Имя микрофона для UI и выбора: FriendlyName устройства (WASAPI).
fn device_name(d: &cpal::Device) -> Option<String> {
    use cpal::traits::DeviceTrait;
    d.description().ok().map(|desc| desc.name().to_string())
}

/// Список микрофонов (первым — системный по умолчанию).
pub fn input_devices() -> Vec<String> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let def = host.default_input_device().and_then(|d| device_name(&d));
    let mut names = Vec::new();
    if let Some(n) = &def {
        names.push(n.clone());
    }
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            if let Some(n) = device_name(&d) {
                if Some(&n) != def.as_ref() {
                    names.push(n);
                }
            }
        }
    }
    names
}

/// Начать запись в WAV по пути `path`. `device` — имя микрофона (None = системный).
pub fn start(path: &std::path::Path, device: Option<String>) -> Result<(), String> {
    let mut guard = recorder().lock().map_err(|_| "recorder poisoned".to_string())?;
    if guard.is_some() {
        return Err(t!("record-busy"));
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let path_buf = path.to_path_buf();
    let thread_path = path_buf.clone();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let wanted = device;
        let device = match &wanted {
            Some(name) => host.input_devices().ok().and_then(|mut it| it.find(|d| device_name(d).is_some_and(|n| &n == name))),
            None => host.default_input_device(),
        };
        let Some(device) = device else {
            let missing = match &wanted {
                Some(name) => t!("record-mic-not-found", name = name.clone()),
                None => t!("record-no-mic"),
            };
            let _ = ready_tx.send(Err(missing));
            return;
        };
        let supported = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => { let _ = ready_tx.send(Err(t!("record-mic-config", error = e.to_string()))); return; }
        };
        let fmt = supported.sample_format();
        let ch = (supported.channels() as usize).max(1);
        let sr = supported.sample_rate();
        let config: cpal::StreamConfig = supported.into();
        let spec = hound::WavSpec { channels: 1, sample_rate: sr, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let writer = match hound::WavWriter::create(&thread_path, spec) {
            Ok(w) => Arc::new(Mutex::new(Some(w))),
            Err(e) => { let _ = ready_tx.send(Err(t!("record-create-wav", error = e.to_string()))); return; }
        };
        let emit_every = (sr / 20).max(1) as usize;

        macro_rules! cb {
            ($ty:ty, $to_f:expr) => {{
                let w = writer.clone();
                let mut peak = 0f32;
                let mut frames = 0usize;
                move |data: &[$ty], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut g) = w.lock() {
                        if let Some(wr) = g.as_mut() {
                            for frame in data.chunks(ch) {
                                let mono = frame.iter().map(|&s| $to_f(s)).sum::<f32>() / ch as f32;
                                peak = peak.max(mono.abs());
                                let _ = wr.write_sample((mono.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                            }
                        }
                    }
                    frames += data.len() / ch;
                    if frames >= emit_every {
                        peak_cell().store(peak.min(1.0).to_bits(), Ordering::Relaxed);
                        peak = 0.0;
                        frames = 0;
                    }
                }
            }};
        }
        let err_fn = |e| eprintln!("microphone stream error: {e}");
        let built = match fmt {
            cpal::SampleFormat::F32 => device.build_input_stream(config, cb!(f32, |s: f32| s), err_fn, None),
            cpal::SampleFormat::I16 => device.build_input_stream(config, cb!(i16, |s: i16| s as f32 / i16::MAX as f32), err_fn, None),
            cpal::SampleFormat::U16 => device.build_input_stream(config, cb!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0), err_fn, None),
            other => { let _ = ready_tx.send(Err(t!("record-format-unsupported", format = format!("{other:?}")))); return; }
        };
        let stream = match built {
            Ok(s) => s,
            Err(e) => { let _ = ready_tx.send(Err(t!("record-mic-open", error = e.to_string()))); return; }
        };
        if let Err(e) = stream.play() {
            let _ = ready_tx.send(Err(t!("record-mic-start", error = e.to_string())));
            return;
        }
        let _ = ready_tx.send(Ok(()));
        let _ = stop_rx.recv();
        drop(stream);
        if let Some(w) = writer.lock().ok().and_then(|mut g| g.take()) {
            let _ = w.finalize();
        }
    });

    match ready_rx.recv().map_err(|e| e.to_string())? {
        Ok(()) => { *guard = Some(RecState { stop_tx, path: path_buf }); Ok(()) }
        Err(e) => Err(e),
    }
}

/// Остановить запись → путь к готовому WAV.
pub fn stop() -> Result<std::path::PathBuf, String> {
    let mut guard = recorder().lock().map_err(|_| "recorder poisoned".to_string())?;
    let st = guard.take().ok_or_else(|| t!("record-not-recording"))?;
    let _ = st.stop_tx.send(());
    std::thread::sleep(std::time::Duration::from_millis(200));
    peak_cell().store(0, Ordering::Relaxed);
    Ok(st.path)
}

// Пак голосов VibeVoice (HF) закреплён коммитом: размер и SHA-256 (lfs.oid) — этой ревизии.
const VOICE_PACK_URL: &str = "https://huggingface.co/datasets/nerualdreming/VibeVoice/resolve/5c884b3cbdc932c39a5167951c4072120e1a0310/voice-pack.zip";
const VOICE_PACK_SIZE: u64 = 104_685_334;
const VOICE_PACK_SHA256: &str = "682e73c189dbe9bc66fc35bc8fb29438641155957a917a7cc914f2f176a65264";

// Датасет доп-голосов (mp3+txt пары) закреплён коммитом: список и файлы одной ревизии. У mp3 (LFS) есть SHA-256,
// у txt (обычные файлы git, по SHA-256 Hugging Face их не описывает) — точный размер.
const VOICES_DATASET: &str = "nerualdreming/russia_voices";
const VOICES_REVISION: &str = "0269d8b31e7f522a3c4ff4cb2e096720ad764c44";
/// Больше этого голос датасета не бывает (самый крупный ~10 МБ).
const VOICE_FILE_LIMIT: u64 = 64 * 1024 * 1024;

fn hf_client() -> Result<reqwest::blocking::Client, String> {
    dub_llm::net::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http: {e}"))
}

fn voice_file_url(path: &str) -> String {
    format!("https://huggingface.co/datasets/{VOICES_DATASET}/resolve/{VOICES_REVISION}/{path}")
}

/// Следующая страница списка файлов HF: заголовок `Link: <…>; rel="next"`.
fn next_page(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get("link")?.to_str().ok()?;
    link.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| url.trim().trim_start_matches('<').trim_end_matches('>').to_string())
    })
}

/// The dataset's files by path: size and, for a file in LFS, its SHA-256.
fn files_of(items: &[serde_json::Value]) -> std::collections::HashMap<&str, (u64, Option<&str>)> {
    let mut files = std::collections::HashMap::new();
    for it in items {
        let Some(path) = it.get("path").and_then(|p| p.as_str()) else { continue };
        if let Some(lfs) = it.get("lfs") {
            if let (Some(sha), Some(size)) = (lfs.get("oid").and_then(|o| o.as_str()), lfs.get("size").and_then(|s| s.as_u64())) {
                files.insert(path, (size, Some(sha)));
                continue;
            }
        }
        if let Some(size) = it.get("size").and_then(|s| s.as_u64()) {
            files.insert(path, (size, None));
        }
    }
    files
}

/// Голоса каталога: строки metadata.jsonl, обогащённые списком файлов датасета — размер и SHA-256 аудио (mp3 или wav)
/// и размер txt рядом. Строка без своего аудиофайла в датасете в каталог не попадает.
fn voices_of(items: &[serde_json::Value], meta: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let files = files_of(items);
    let mut voices: Vec<serde_json::Value> = meta
        .iter()
        .filter_map(|row| {
            let file = row.get("file")?.as_str()?;
            if file.contains('/') || file.contains('\\') || file.contains("..") {
                return None;
            }
            let (size, sha256) = files.get(file).copied()?;
            let name = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
            let txt_size = files.get(format!("{name}.txt").as_str()).map(|(s, _)| *s);
            let field = |k: &str| row.get(k).cloned().unwrap_or(serde_json::Value::Null);
            Some(serde_json::json!({
                "name": name,
                "file": file,
                "displayName": row.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or(name),
                "gender": field("gender"),
                "age_group": field("age_group"),
                "language": field("language"),
                "description": field("description"),
                "duration_s": field("duration_s"),
                "birth_year": field("birth_year"),
                "url": voice_file_url(file),
                "size": size,
                "sha256": sha256,
                "txt_size": txt_size,
            }))
        })
        .collect();
    voices.sort_by(|a, b| a["displayName"].as_str().cmp(&b["displayName"].as_str()));
    voices
}

/// The dataset's `metadata.jsonl` of the pinned revision: one row per voice.
fn fetch_metadata(client: &reqwest::blocking::Client) -> Result<Vec<serde_json::Value>, String> {
    let url = voice_file_url("metadata.jsonl");
    let text = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{url}: {e}"))?
        .text()
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(metadata_rows(&text))
}

fn metadata_rows(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("voices catalog: a metadata line is not JSON, skipped: {e}");
                None
            }
        })
        .collect()
}

/// The catalogue's filters (all optional, the MCP tool's and the window's).
#[derive(Debug, Default)]
pub struct CatalogFilter {
    pub gender: Option<String>,
    pub age_group: Option<String>,
    pub language: Option<String>,
    pub query: Option<String>,
    pub min_duration: Option<f64>,
    pub max_duration: Option<f64>,
}

/// The voices of the catalogue that pass `f`: fields compared without case, the query in the names and description.
pub fn filter_catalog(voices: &[serde_json::Value], f: &CatalogFilter) -> Vec<serde_json::Value> {
    let eq = |value: Option<&str>, want: &Option<String>| match want {
        Some(w) if !w.trim().is_empty() => value.is_some_and(|v| v.eq_ignore_ascii_case(w.trim())),
        _ => true,
    };
    let query = f.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
    voices
        .iter()
        .filter(|v| {
            eq(v["gender"].as_str(), &f.gender)
                && eq(v["age_group"].as_str(), &f.age_group)
                && eq(v["language"].as_str(), &f.language)
                && f.min_duration.is_none_or(|lo| v["duration_s"].as_f64().is_some_and(|d| d >= lo))
                && f.max_duration.is_none_or(|hi| v["duration_s"].as_f64().is_some_and(|d| d <= hi))
                && query.as_ref().is_none_or(|q| {
                    [v["displayName"].as_str(), v["name"].as_str(), v["description"].as_str()]
                        .into_iter()
                        .flatten()
                        .any(|s| s.to_lowercase().contains(q))
                })
        })
        .cloned()
        .collect()
}

fn catalog_cache() -> &'static std::sync::Mutex<Option<serde_json::Value>> {
    static CACHE: OnceLock<std::sync::Mutex<Option<serde_json::Value>>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Каталог доп-голосов из HF-датасета закреплённой ревизии: по строке metadata.jsonl на голос
/// ({name, file, displayName, gender, age_group, language, description, duration_s, birth_year, url, size, sha256,
/// txt_size}), размер и SHA-256 аудио — из списка файлов датасета. Ревизия не меняется, поэтому удачный ответ живёт
/// до конца процесса; неудачный не запоминается.
pub fn catalog() -> Result<serde_json::Value, String> {
    if let Some(v) = catalog_cache().lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(v);
    }
    let client = hf_client()?;
    let mut url = Some(format!("https://huggingface.co/api/datasets/{VOICES_DATASET}/tree/{VOICES_REVISION}?recursive=1"));
    let mut items: Vec<serde_json::Value> = Vec::new();
    while let Some(page) = url.take() {
        let resp = client.get(&page).send().and_then(|r| r.error_for_status()).map_err(|e| format!("{page}: {e}"))?;
        url = next_page(resp.headers());
        let chunk: Vec<serde_json::Value> = resp.json().map_err(|e| t!("voices-not-file-list", page = page.clone(), error = e.to_string()))?;
        items.extend(chunk);
        if items.len() > 100_000 {
            return Err(t!("voices-list-endless", page = page.clone()));
        }
    }
    let meta = fetch_metadata(&client)?;
    let v = serde_json::json!({ "voices": voices_of(&items, &meta) });
    *catalog_cache().lock().unwrap_or_else(|e| e.into_inner()) = Some(v.clone());
    Ok(v)
}

/// Скачать `url` в `dest` через .part, как Speech Studio: оборвавшееся скачивание продолжается с места обрыва (до
/// FETCH_TRIES попыток, .part остаётся и для следующего нажатия); размер и (если задан) SHA-256 сверяются до того, как
/// файл появится под своим именем. `progress(получено)` — по ходу.
fn fetch_checked(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &std::path::Path,
    size: u64,
    sha256: Option<&str>,
    progress: &dyn Fn(u64),
) -> Result<(), String> {
    use std::io::{Read, Write};
    let part = dest.with_extension(format!("{}.part", dest.extension().and_then(|e| e.to_str()).unwrap_or("")));
    // a dropped connection is taken up again where it stopped, a few times; the part stays for the next try too
    let mut tries = 0;
    loop {
        tries += 1;
        let on_disk = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        // a part longer than the file is not this file's (an earlier version of it): it is downloaded anew
        if on_disk > size {
            std::fs::remove_file(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        }
        let have = if on_disk > size { 0 } else { on_disk };
        // a part already whole (the last try broke off before it took its name) is checked as it is: asked for the
        // rest, a server answers there is none
        if size > 0 && have == size {
            break;
        }
        let got = (|| -> Result<u64, String> {
            let mut req = client.get(url);
            if have > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
            }
            let mut resp = req.send().and_then(|r| r.error_for_status()).map_err(|e| format!("{url}: {e}"))?;
            let resumed = have > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            let mut f = if resumed {
                std::fs::OpenOptions::new().append(true).open(&part).map_err(|e| format!("{}: {e}", part.display()))?
            } else {
                std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?
            };
            let mut got = if resumed { have } else { 0 };
            let mut buf = [0u8; 1 << 16];
            loop {
                let n = resp.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
                if n == 0 {
                    break;
                }
                got += n as u64;
                if got > size {
                    let _ = std::fs::remove_file(&part);
                    return Ok(got);
                }
                f.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
                progress(got);
            }
            f.flush().map_err(|e| format!("{}: {e}", part.display()))?;
            Ok(got)
        })();
        match got {
            Ok(got) if got == size => break,
            Ok(got) if got > size => return Err(t!("voices-too-large", url = url.to_string(), size = size)),
            Ok(got) if tries >= FETCH_TRIES => return Err(t!("voices-size-mismatch", url = url.to_string(), got = got, size = size)),
            Err(e) if tries >= FETCH_TRIES => return Err(t!("voices-fetch-failed", error = e, tries = tries as i64)),
            _ => {
                tracing::warn!("voices: {url} broke off, taken up again where it stopped (try {tries} of {FETCH_TRIES})");
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
    if let Some(want) = sha256 {
        let got = crate::setup::sha256_file(&part, &|| false).map_err(|e| format!("{}: {e}", part.display()))?.unwrap_or_default();
        if got != want {
            let _ = std::fs::remove_file(&part);
            return Err(t!("voices-sha-mismatch", url = url.to_string(), got = got.clone(), want = want.to_string()));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))
}

/// How many times a download that broke off is taken up again before it is given up.
const FETCH_TRIES: u32 = 3;

/// Скачать один голос датасета (mp3 + txt, если он есть) в каталог голосов, сверив с каталогом закреплённой ревизии.
pub fn fetch_voice(dir: &std::path::Path, name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(t!("voices-bad-name"));
    }
    let catalog = catalog()?;
    let entry = catalog["voices"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| v["name"] == name)
        .cloned()
        .ok_or_else(|| t!("voices-not-in-dataset", name = name.to_string(), dataset = VOICES_DATASET))?;
    let file = entry["file"].as_str().ok_or_else(|| t!("voices-not-in-dataset", name = name.to_string(), dataset = VOICES_DATASET))?;
    let size = entry["size"].as_u64().filter(|s| *s <= VOICE_FILE_LIMIT).ok_or_else(|| t!("voices-bad-size", name = name.to_string()))?;
    // mp3 lives in LFS and carries a SHA-256; the few wav do too; a file without one is checked by size alone
    let sha256 = entry["sha256"].as_str();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let client = hf_client()?;
    let base = std::path::Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = std::path::Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    fetch_checked(&client, &voice_file_url(file), &dir.join(format!("{base}.{ext}")), size, sha256, &|_| {})?;
    if let Some(txt) = entry["txt_size"].as_u64() {
        if txt > VOICE_FILE_LIMIT {
            return Err(t!("voices-bad-size", name = format!("{name}.txt")));
        }
        fetch_checked(&client, &voice_file_url(&format!("{name}.txt")), &dir.join(format!("{base}.txt")), txt, None, &|_| {})?;
    }
    // the dataset's metadata is surer than the guess from the file name
    let mut card = crate::voice_library::VoiceCard { source: "russia_voices".into(), ..crate::voice_library::guess_card(base) };
    fill_card(&mut card, &entry, true);
    crate::voice_library::write_card(dir, base, &card)
}

/// A voice's card from its catalogue entry: everything the dataset gives; `over` — over what the card says (a fresh
/// download), else only what the card leaves empty (a voice taken before its card kept the catalogue's data).
fn fill_card(card: &mut crate::voice_library::VoiceCard, e: &serde_json::Value, over: bool) {
    let text = |k: &str| e[k].as_str().map(str::trim).filter(|s| !s.is_empty());
    let set = |field: &mut String, value: Option<String>| {
        if let Some(v) = value.filter(|_| over || field.is_empty()) {
            *field = v;
        }
    };
    set(&mut card.gender, text("gender").map(str::to_string));
    set(&mut card.lang, text("language").map(str::to_ascii_lowercase));
    set(&mut card.age, text("age_group").filter(|a| *a == "child" || *a == "adult").map(str::to_string));
    set(&mut card.display, text("displayName").filter(|d| Some(*d) != e["name"].as_str()).map(str::to_string));
    set(&mut card.description, text("description").map(str::to_string));
    if over || card.birth_year.is_none() {
        card.birth_year = e["birth_year"].as_u64().and_then(|y| u32::try_from(y).ok());
    }
    if over || card.duration_s.is_none() {
        card.duration_s = e["duration_s"].as_f64().filter(|d| *d > 0.0);
    }
}

/// The voices of the library that came from the catalogue before their cards kept its data get it now.
pub fn backfill_cards(voices_dir: &std::path::Path, catalog: &[serde_json::Value]) {
    for name in crate::list_voice_names(voices_dir) {
        let Ok(old) = crate::voice_library::read_card(voices_dir, &name) else { continue };
        if !old.display.is_empty() {
            continue;
        }
        let Some(e) = catalog.iter().find(|e| e["name"] == name.as_str()) else { continue };
        let mut card = old.clone();
        fill_card(&mut card, e, false);
        if card != old {
            if let Err(err) = crate::voice_library::write_card(voices_dir, &name, &card) {
                tracing::warn!("voices: the card of {name} keeps its old data: {err}");
            }
        }
    }
}

/// The files of the pack's archive: name, size and SHA-256 of each.
fn pack_files() -> impl Iterator<Item = (&'static str, u64, &'static str)> {
    include_str!("voice_pack.tsv").lines().filter_map(|l| {
        let mut f = l.split('\t');
        Some((f.next()?, f.next()?.parse().ok()?, f.next()?))
    })
}

/// The whole pack linked from the `voices` folder of another program that has every file of it (only the pack's own
/// files, each by its SHA-256: the rest of that folder is the person's voices and is not touched); the program's id,
/// empty when the pack is here already, or None when no program has all of it (a file already here and whole is kept).
fn take_pack_from_neighbor(dir: &std::path::Path) -> Option<String> {
    let whole = |p: &std::path::Path, size: u64, sha256: &str| {
        p.metadata().is_ok_and(|m| m.len() == size) && matches!(crate::setup::sha256_file(p, &|| false), Ok(Some(h)) if h == sha256)
    };
    let missing: Vec<_> = pack_files().filter(|(name, size, sha256)| !whole(&dir.join(name), *size, sha256)).collect();
    if missing.is_empty() {
        return Some(String::new());
    }
    for (app, data_dir) in crate::hub::neighbors() {
        let theirs = data_dir.join("voices");
        if !missing.iter().all(|(name, size, _)| theirs.join(name).metadata().is_ok_and(|m| m.len() == *size)) {
            continue;
        }
        let taken = missing.iter().all(|(name, size, sha256)| {
            let (from, to) = (theirs.join(name), dir.join(name));
            let placed = crate::setup::adopt_file(&from, &to, true).map_err(|e| e.to_string()).and_then(|()| {
                if whole(&to, *size, sha256) { Ok(()) } else { Err("not the pack's file".to_string()) }
            });
            if let Err(e) = &placed {
                std::fs::remove_file(&to).ok();
                tracing::warn!("{name} from {app} ({}): {e}", theirs.display());
            }
            placed.is_ok()
        });
        if taken {
            tracing::info!("[OK] the voice pack taken from {app} instead of downloading");
            return Some(app);
        }
    }
    None
}

pub fn download_pack(repo_root: &std::path::Path, dir: &std::path::Path, cb: &dyn Fn(serde_json::Value)) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(dir).map_err(|e| t!("common-create-dir", path = dir.display().to_string(), error = e.to_string()))?;
    if let Some(app) = take_pack_from_neighbor(dir) {
        let count = pack_files().count();
        let msg = if app.is_empty() { t!("voices-pack-done", count = count) } else { t!("voices-pack-taken", count = count, app = crate::setup::program_name(&app).to_string()) };
        cb(serde_json::json!({ "stage": "voicepack", "msg": msg, "pct": 100 }));
        return Ok(serde_json::json!({ "extracted": count, "taken_from": (!app.is_empty()).then_some(app) }));
    }
    // The pack unpacks into the program's voices folder (VOICE_PACK.files); a voices folder moved elsewhere by
    // DUBENGINE_VOICES does not get it.
    if dir != repo_root.join("voices") {
        return Err(t!("voices-pack-folder", path = dir.display().to_string()));
    }
    cb(serde_json::json!({ "stage": "voicepack", "msg": t!("voices-pack-downloading"), "pct": 0 }));
    let ctl = crate::jobs::current();
    let cancel = move || ctl.as_ref().is_some_and(|c| c.is_cancelled());
    crate::setup::download_extra(repo_root, &voice_pack(), &cancel, cb).map_err(|e| e.detail)?;
    let count = pack_files().filter(|(name, _, _)| dir.join(name).is_file()).count();
    cb(serde_json::json!({ "stage": "voicepack", "msg": t!("voices-pack-done", count = count), "pct": 100 }));
    Ok(serde_json::json!({ "extracted": count }))
}

/// The voice pack as a download of the setup's loader: one pinned archive unpacked flat into `voices/`.
fn voice_pack() -> crate::setup::Component {
    const FILES: &[crate::setup::FileSpec] = &[crate::setup::FileSpec {
        url: VOICE_PACK_URL,
        dest_rel: "voices/_voice-pack.zip",
        size: VOICE_PACK_SIZE,
        sha256: VOICE_PACK_SHA256,
        extract: crate::setup::Extract::Flat,
    }];
    crate::setup::Component {
        id: "voice-pack",
        name: "VibeVoice voice pack".into(),
        purpose: String::new(),
        requirement: crate::setup::Requirement::Optional,
        delivery: crate::setup::Delivery::Download,
        size: VOICE_PACK_SIZE,
        files: FILES,
        markers: &[],
        external_url: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pack_lists_every_file_of_its_archive_with_size_and_hash() {
        let files: Vec<_> = pack_files().collect();
        assert_eq!(files.len(), 100);
        assert!(files.iter().all(|(name, size, sha)| !name.is_empty() && *size > 0 && sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())));
        let names: std::collections::HashSet<_> = files.iter().map(|f| f.0).collect();
        assert_eq!(names.len(), files.len());
    }

    /// The pack through the setup's loader in an empty program folder (DUB_STUDIO_PACK_ROOT): a download stopped half
    /// way resumes from the chunks it has, and ends with the hundred voices unpacked.
    #[test]
    #[ignore]
    fn the_voice_pack_resumes_after_a_stop() {
        let Ok(root) = std::env::var("DUB_STUDIO_PACK_ROOT") else { return };
        let root = std::path::PathBuf::from(root);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let seen = |ev: serde_json::Value| {
            let done = ev["downloaded"].as_u64().unwrap_or(0);
            if done > VOICE_PACK_SIZE / 2 {
                stop.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        };
        let first = crate::setup::download_extra(&root, &voice_pack(), &|| stop.load(std::sync::atomic::Ordering::SeqCst), &seen);
        assert!(first.is_err(), "the first run is stopped half way");
        let resumed = std::sync::Mutex::new(None::<u64>);
        let seen = |ev: serde_json::Value| {
            if let Some(done) = ev["downloaded"].as_u64() {
                resumed.lock().unwrap().get_or_insert(done);
            }
        };
        crate::setup::download_extra(&root, &voice_pack(), &|| false, &seen).map_err(|e| e.detail).unwrap();
        let start = resumed.lock().unwrap().unwrap_or(0);
        let voices = root.join("voices");
        let unpacked = pack_files().filter(|(name, size, _)| voices.join(name).metadata().is_ok_and(|m| m.len() == *size)).count();
        println!("[OK] resumed at {start} of {VOICE_PACK_SIZE} bytes; {unpacked} voices unpacked");
        assert!(start > VOICE_PACK_SIZE / 4, "the second run starts from the chunks of the first");
        assert_eq!(unpacked, 100);
    }

    #[test]
    fn voice_downloads_are_pinned_to_a_commit() {
        for url in [VOICE_PACK_URL.to_string(), voice_file_url("RU_Female_x.mp3")] {
            assert!(!url.contains("/resolve/main/"), "{url}");
            let rev = url.split("/resolve/").nth(1).and_then(|r| r.split('/').next()).unwrap_or_default();
            assert!(rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()), "{url}");
        }
        assert_eq!(VOICE_PACK_SHA256.len(), 64);
    }

    #[test]
    fn the_catalog_joins_metadata_with_the_file_list_and_follows_its_pages() {
        let items: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {"type":"file","oid":"fd3a","size":6038690,"lfs":{"oid":"d4c4af1c2704a671f9352cd300b80bb92eee4d88b7642b84aa073009140cc710","size":6038690},"path":"RU_Female_abramova_oljga.mp3"},
            {"type":"file","oid":"517a","size":7928,"path":"RU_Female_abramova_oljga.txt"},
            {"type":"file","oid":"aa","size":10,"path":"EN_Male_no_lfs.wav"},
            {"type":"file","oid":"bb","size":5,"lfs":{"oid":"cc","size":5},"path":"RU_Child_x.mp3"}
        ]"#).unwrap();
        let meta = metadata_rows(concat!(
            "{\"file\":\"RU_Female_abramova_oljga.mp3\",\"name\":\"Ольга Абрамова\",\"gender\":\"female\",\"age_group\":\"adult\",\"language\":\"ru\",\"description\":\"диктор\",\"duration_s\":25.8,\"birth_year\":1970}\n",
            "{\"file\":\"EN_Male_no_lfs.wav\",\"name\":\"No Lfs\",\"gender\":\"male\",\"age_group\":\"adult\",\"language\":\"en\",\"duration_s\":9.0}\n",
            "{\"file\":\"RU_Child_x.mp3\",\"name\":\"Kid\",\"gender\":\"female\",\"age_group\":\"child\",\"language\":\"ru\",\"duration_s\":5.0}\n",
            "not json, skipped\n",
            "{\"file\":\"RU_Missing.mp3\",\"name\":\"Gone\",\"gender\":\"male\"}\n",
        ));
        let v = voices_of(&items, &meta);
        let by = |name: &str| v.iter().find(|x| x["name"] == name).unwrap_or_else(|| panic!("{name} not in {v:?}"));
        assert_eq!(v.len(), 3, "a metadata row whose audio is not in the dataset is dropped: {v:?}");
        let a = by("RU_Female_abramova_oljga");
        assert_eq!((a["size"].as_u64(), a["txt_size"].as_u64(), a["gender"].as_str()), (Some(6038690), Some(7928), Some("female")));
        assert_eq!((a["displayName"].as_str(), a["age_group"].as_str(), a["language"].as_str(), a["duration_s"].as_f64(), a["birth_year"].as_i64()), (Some("Ольга Абрамова"), Some("adult"), Some("ru"), Some(25.8), Some(1970)));
        assert!(a["url"].as_str().unwrap().contains(VOICES_REVISION) && a["url"].as_str().unwrap().ends_with(".mp3"));
        // a wav without a SHA-256 is offered, checked by size, and keeps its extension in the url
        let w = by("EN_Male_no_lfs");
        assert_eq!((w["sha256"].clone(), w["txt_size"].clone(), w["file"].as_str()), (serde_json::Value::Null, serde_json::Value::Null, Some("EN_Male_no_lfs.wav")));
        assert_eq!(by("RU_Child_x")["txt_size"], serde_json::Value::Null);
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("link", "<https://huggingface.co/api/datasets/nerualdreming/russia_voices/tree/x?recursive=1&cursor=abc>; rel=\"next\"".parse().unwrap());
        assert_eq!(next_page(&h).as_deref(), Some("https://huggingface.co/api/datasets/nerualdreming/russia_voices/tree/x?recursive=1&cursor=abc"));
        assert_eq!(next_page(&reqwest::header::HeaderMap::new()), None);
    }

    #[test]
    fn the_catalog_filters_by_the_metadata_fields() {
        let meta = metadata_rows(concat!(
            "{\"file\":\"RU_Female_adult.mp3\",\"name\":\"Анна\",\"gender\":\"female\",\"age_group\":\"adult\",\"language\":\"ru\",\"description\":\"спокойный диктор\",\"duration_s\":30.0}\n",
            "{\"file\":\"RU_Female_child.mp3\",\"name\":\"Катя\",\"gender\":\"female\",\"age_group\":\"child\",\"language\":\"ru\",\"description\":\"ребёнок\",\"duration_s\":8.0}\n",
            "{\"file\":\"EN_Male_adult.mp3\",\"name\":\"John\",\"gender\":\"male\",\"age_group\":\"adult\",\"language\":\"en\",\"description\":\"narrator\",\"duration_s\":60.0}\n",
        ));
        let items: Vec<serde_json::Value> = ["RU_Female_adult", "RU_Female_child", "EN_Male_adult"]
            .iter()
            .map(|n| serde_json::json!({"type":"file","size":5,"lfs":{"oid":"aa","size":5},"path":format!("{n}.mp3")}))
            .collect();
        let all = voices_of(&items, &meta);
        let names = |f: &CatalogFilter| filter_catalog(&all, f).iter().map(|v| v["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(names(&CatalogFilter { gender: Some("female".into()), age_group: Some("child".into()), language: Some("ru".into()), ..Default::default() }), ["RU_Female_child"]);
        assert_eq!(names(&CatalogFilter { min_duration: Some(10.0), ..Default::default() }), ["EN_Male_adult", "RU_Female_adult"]);
        assert_eq!(names(&CatalogFilter { query: Some("narrator".into()), ..Default::default() }), ["EN_Male_adult"]);
        assert_eq!(names(&CatalogFilter { query: Some("катя".into()), ..Default::default() }), ["RU_Female_child"]);
        assert_eq!(names(&CatalogFilter::default()).len(), 3);
    }


    #[test]
    fn a_download_takes_the_catalogue_and_a_backfill_keeps_what_the_card_says() {
        let e = serde_json::json!({ "name": "RU_Female_volkova_anna", "displayName": "Анна Волкова", "gender": "female", "age_group": "child",
            "language": "RU", "description": " Юная актриса ", "duration_s": 69.0, "birth_year": 2014 });
        let mut fresh = crate::voice_library::VoiceCard::default();
        fill_card(&mut fresh, &e, true);
        assert_eq!((fresh.display.as_str(), fresh.gender.as_str(), fresh.age.as_str(), fresh.lang.as_str()), ("Анна Волкова", "female", "child", "ru"));
        assert_eq!((fresh.description.as_str(), fresh.birth_year, fresh.duration_s), ("Юная актриса", Some(2014), Some(69.0)));
        let mut own = crate::voice_library::VoiceCard { gender: "male".into(), description: "своё".into(), ..Default::default() };
        fill_card(&mut own, &e, false);
        assert_eq!((own.gender.as_str(), own.description.as_str(), own.display.as_str(), own.age.as_str()), ("male", "своё", "Анна Волкова", "child"));
    }

    #[test]
    fn a_library_voice_from_the_catalogue_gets_its_card_and_a_rename_shows_the_new_name() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("RU_Female_volkova_anna.mp3"), b"x").unwrap();
        let catalog = vec![serde_json::json!({ "name": "RU_Female_volkova_anna", "displayName": "Анна Волкова", "gender": "female" })];
        backfill_cards(d.path(), &catalog);
        assert_eq!(crate::voice_library::read_card(d.path(), "RU_Female_volkova_anna").unwrap().display, "Анна Волкова");
        crate::voice_library::rename(d.path(), "RU_Female_volkova_anna", "Анна").unwrap();
        let card = crate::voice_library::read_card(d.path(), "Анна").unwrap();
        assert_eq!((card.display.as_str(), card.gender.as_str()), ("", "female"), "the card moves with the voice, its name is the one given");
        assert!(!d.path().join("RU_Female_volkova_anna.json").exists());
    }

    #[test]
    fn a_download_is_checked_before_it_takes_its_name() {
        use dub_llm::test_http::{serve, Reply};
        use sha2::{Digest, Sha256};
        let body = b"voice bytes".to_vec();
        let sha: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let reply = || Reply::bytes(200, "audio/mpeg", body.clone());
        let server = serve(vec![reply(), reply(), reply(), reply(), reply()]);
        let client = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("v.mp3");
        let n = body.len() as u64;
        fetch_checked(&client, &format!("{}/v", server.base()), &dest, n, Some(&sha), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let other = dir.path().join("w.mp3");
        assert!(fetch_checked(&client, &format!("{}/v", server.base()), &other, n, Some(&"0".repeat(64)), &|_| {}).unwrap_err().contains("SHA-256"));
        let short = fetch_checked(&client, &format!("{}/v", server.base()), &other, n + 1, None, &|_| {}).unwrap_err();
        assert!(short.contains(&format!("{n}")) && short.contains(&format!("{}", n + 1)), "the size that came and the pinned one: {short}");
        assert!(!other.exists(), "a file that failed its check never appears");
    }

    /// A download that broke off goes on from where it stopped: the part kept is asked for the rest.
    #[test]
    fn a_broken_download_goes_on_from_where_it_stopped() {
        use dub_llm::test_http::{serve, Reply};
        let body = b"voice bytes in two parts".to_vec();
        let server = serve(vec![Reply::bytes(206, "audio/mpeg", body[10..].to_vec())]);
        let client = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("v.mp3");
        std::fs::write(dir.path().join("v.mp3.part"), &body[..10]).unwrap();
        fetch_checked(&client, &format!("{}/v", server.base()), &dest, body.len() as u64, None, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    /// A part already whole on disk takes its name without asking the server anything.
    #[test]
    fn a_whole_part_takes_its_name_without_asking_again() {
        use dub_llm::test_http::serve;
        let body = b"voice bytes, all of them".to_vec();
        let server = serve(vec![]);
        let client = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("v.mp3");
        std::fs::write(dir.path().join("v.mp3.part"), &body).unwrap();
        fetch_checked(&client, &format!("{}/v", server.base()), &dest, body.len() as u64, None, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    /// A part longer than the file is another file's: it is dropped and the file downloaded anew, not given the name.
    #[test]
    fn a_part_longer_than_the_file_is_downloaded_anew() {
        use dub_llm::test_http::{serve, Reply};
        let body = b"the voice as it is now".to_vec();
        let server = serve(vec![Reply::bytes(200, "audio/mpeg", body.clone())]);
        let client = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("v.mp3");
        std::fs::write(dir.path().join("v.mp3.part"), b"an earlier and longer version of this voice").unwrap();
        fetch_checked(&client, &format!("{}/v", server.base()), &dest, body.len() as u64, None, &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }
}
