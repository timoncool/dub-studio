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
const VOICES_DATASET: &str = "Slait/russia_voices";
const VOICES_REVISION: &str = "215d799f529bfe8815063064246b5c09e4b380d6";
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

/// Голоса из списка файлов датасета: mp3 с SHA-256 и размером, txt рядом — с размером.
fn voices_of(items: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let txt: std::collections::HashMap<&str, u64> = items
        .iter()
        .filter_map(|it| Some((it.get("path")?.as_str()?.strip_suffix(".txt")?, it.get("size")?.as_u64()?)))
        .collect();
    let mut voices: Vec<serde_json::Value> = items
        .iter()
        .filter_map(|it| {
            let path = it.get("path")?.as_str()?;
            let name = path.strip_suffix(".mp3")?;
            let lfs = it.get("lfs")?;
            let sha256 = lfs.get("oid")?.as_str()?;
            let size = lfs.get("size")?.as_u64()?;
            let gender = if name.contains("Female") { "female" } else if name.contains("Male") { "male" } else { "?" };
            Some(serde_json::json!({
                "name": name,
                "gender": gender,
                "url": voice_file_url(path),
                "size": size,
                "sha256": sha256,
                "txt_size": txt.get(name).copied(),
            }))
        })
        .collect();
    voices.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    voices
}

fn catalog_cache() -> &'static std::sync::Mutex<Option<serde_json::Value>> {
    static CACHE: OnceLock<std::sync::Mutex<Option<serde_json::Value>>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Каталог доп-голосов из HF-датасета закреплённой ревизии: [{name, gender, url, size, sha256, txt_size}]. Ревизия
/// не меняется, поэтому удачный ответ живёт до конца процесса; неудачный не запоминается.
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
    let v = serde_json::json!({ "voices": voices_of(&items) });
    *catalog_cache().lock().unwrap_or_else(|e| e.into_inner()) = Some(v.clone());
    Ok(v)
}

/// Скачать `url` в `dest` через .part: размер и (если задан) SHA-256 сверяются до того, как файл появится под своим
/// именем. `progress(получено)` — по ходу.
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
    let result = (|| -> Result<(), String> {
        let mut resp = client.get(url).send().and_then(|r| r.error_for_status()).map_err(|e| format!("{url}: {e}"))?;
        let mut f = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut buf = [0u8; 1 << 16];
        let mut got = 0u64;
        loop {
            let n = resp.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                break;
            }
            got += n as u64;
            if got > size {
                return Err(t!("voices-too-large", url = url.to_string(), size = size));
            }
            f.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
            progress(got);
        }
        f.flush().map_err(|e| format!("{}: {e}", part.display()))?;
        if got != size {
            return Err(t!("voices-size-mismatch", url = url.to_string(), got = got, size = size));
        }
        if let Some(want) = sha256 {
            let got = crate::setup::sha256_file(&part, &|| false)
                .map_err(|e| format!("{}: {e}", part.display()))?
                .unwrap_or_default();
            if got != want {
                return Err(t!("voices-sha-mismatch", url = url.to_string(), got = got.clone(), want = want.to_string()));
            }
        }
        std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

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
    let size = entry["size"].as_u64().filter(|s| *s <= VOICE_FILE_LIMIT).ok_or_else(|| t!("voices-bad-size", name = name.to_string()))?;
    let sha256 = entry["sha256"].as_str().ok_or_else(|| t!("voices-no-sha", name = name.to_string()))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let client = hf_client()?;
    let base = std::path::Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or(name);
    fetch_checked(&client, &voice_file_url(&format!("{name}.mp3")), &dir.join(format!("{base}.mp3")), size, Some(sha256), &|_| {})?;
    if let Some(txt) = entry["txt_size"].as_u64() {
        if txt > VOICE_FILE_LIMIT {
            return Err(t!("voices-bad-size", name = format!("{name}.txt")));
        }
        fetch_checked(&client, &voice_file_url(&format!("{name}.txt")), &dir.join(format!("{base}.txt")), txt, None, &|_| {})?;
    }
    Ok(())
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
    fn the_catalog_takes_hashes_from_the_tree_and_follows_its_pages() {
        let items: Vec<serde_json::Value> = serde_json::from_str(r#"[
            {"type":"file","oid":"fd3a","size":6038690,"lfs":{"oid":"d4c4af1c2704a671f9352cd300b80bb92eee4d88b7642b84aa073009140cc710","size":6038690},"path":"RU_Female_abramova_oljga.mp3"},
            {"type":"file","oid":"517a","size":7928,"path":"RU_Female_abramova_oljga.txt"},
            {"type":"file","oid":"aa","size":10,"path":"RU_Male_no_lfs.mp3"},
            {"type":"file","oid":"bb","size":5,"lfs":{"oid":"cc","size":5},"path":"RU_Male_x.mp3"}
        ]"#).unwrap();
        let v = voices_of(&items);
        assert_eq!(v.len(), 2, "an mp3 without a SHA-256 is not offered");
        assert_eq!(v[0]["name"], "RU_Female_abramova_oljga");
        assert_eq!((v[0]["size"].as_u64(), v[0]["txt_size"].as_u64(), v[0]["gender"].as_str()), (Some(6038690), Some(7928), Some("female")));
        assert!(v[0]["url"].as_str().unwrap().contains(VOICES_REVISION));
        assert_eq!(v[1]["txt_size"], serde_json::Value::Null);
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("link", "<https://huggingface.co/api/datasets/Slait/russia_voices/tree/x?recursive=1&cursor=abc>; rel=\"next\"".parse().unwrap());
        assert_eq!(next_page(&h).as_deref(), Some("https://huggingface.co/api/datasets/Slait/russia_voices/tree/x?recursive=1&cursor=abc"));
        assert_eq!(next_page(&reqwest::header::HeaderMap::new()), None);
    }

    #[test]
    fn a_download_is_checked_before_it_takes_its_name() {
        use dub_llm::test_http::{serve, Reply};
        use sha2::{Digest, Sha256};
        let body = b"voice bytes".to_vec();
        let sha: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let server = serve(vec![Reply::bytes(200, "audio/mpeg", body.clone()), Reply::bytes(200, "audio/mpeg", body.clone()), Reply::bytes(200, "audio/mpeg", body.clone())]);
        let client = reqwest::blocking::Client::builder().no_proxy().build().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("v.mp3");
        let n = body.len() as u64;
        fetch_checked(&client, &format!("{}/v", server.base()), &dest, n, Some(&sha), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let other = dir.path().join("w.mp3");
        assert!(fetch_checked(&client, &format!("{}/v", server.base()), &other, n, Some(&"0".repeat(64)), &|_| {}).unwrap_err().contains("SHA-256"));
        assert!(fetch_checked(&client, &format!("{}/v", server.base()), &other, n + 1, None, &|_| {}).unwrap_err().contains("закреплено"));
        assert!(!other.exists(), "a file that failed its check never appears");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no .part is left");
    }
}
