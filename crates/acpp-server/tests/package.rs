//! The published package of a build (`asset::Build::server` of this platform), as a studio gets it: downloaded into an
//! empty folder, checked by size and SHA-256, unpacked flat into the build's folder, started and answering `/health`.
//! Run by hand with ACPP_PACKAGE_ROOT (an empty folder) and ACPP_PACKAGE_BUILD (cpu, vulkan, cuda13, cuda12 or metal; cpu by
//! default; a CUDA build also needs its libraries in ACPP_PACKAGE_LIBS). With ACPP_PARAKEET (parakeet-tdt-0.6b-v3
//! GGUF) and ACPP_SPEECH (a 16 kHz mono WAV of speech) the started server also recognises the speech.

use acpp_server::asset::{missing_libraries, Asset, Build};
use acpp_server::{ModelEntry, Server, ServerConfig, TranscribeRequest};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn build_from_env() -> Build {
    match std::env::var("ACPP_PACKAGE_BUILD").as_deref() {
        Err(_) | Ok("cpu") => Build::Cpu,
        Ok("vulkan") => Build::Vulkan,
        Ok("cuda13") => Build::Cuda13,
        Ok("cuda12") => Build::Cuda12,
        Ok("metal") => Build::Metal,
        Ok(other) => panic!("ACPP_PACKAGE_BUILD={other}: cpu, vulkan, cuda13, cuda12 or metal"),
    }
}

fn download(asset: &Asset, to: &Path) {
    let mut resp = ureq::get(asset.url).call().unwrap_or_else(|e| panic!("{}: {e}", asset.url));
    let mut body = resp.body_mut().with_config().limit(asset.size + 1).reader();
    let mut file = std::fs::File::create(to).unwrap();
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut len = 0u64;
    loop {
        let n = body.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).unwrap();
        len += n as u64;
    }
    assert_eq!(len, asset.size, "{}: size", asset.file);
    let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, asset.sha256, "{}: SHA-256", asset.file);
}

#[cfg(windows)]
fn unpack(archive: &Path, dir: &Path) {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let name = Path::new(entry.name()).file_name().unwrap().to_owned();
        let mut out = std::fs::File::create(dir.join(name)).unwrap();
        std::io::copy(&mut entry, &mut out).unwrap();
    }
}

#[cfg(unix)]
fn unpack(archive: &Path, dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(archive).unwrap());
    let mut tar = tar::Archive::new(gz);
    for entry in tar.entries().unwrap() {
        let mut entry = entry.unwrap();
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let mode = entry.header().mode().unwrap();
        let name = entry.path().unwrap().file_name().unwrap().to_owned();
        let path = dir.join(name);
        let mut out = std::fs::File::create(&path).unwrap();
        std::io::copy(&mut entry, &mut out).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}

#[test]
#[ignore]
fn the_published_package_downloads_unpacks_and_starts() {
    let root = PathBuf::from(std::env::var("ACPP_PACKAGE_ROOT").expect("ACPP_PACKAGE_ROOT is not set"));
    let build = build_from_env();
    let asset = build.server().expect("no package of the server for this platform");
    let dir = root.join(build.folder());
    std::fs::create_dir_all(&dir).unwrap();
    let archive = root.join(asset.file);
    let t0 = Instant::now();
    download(&asset, &archive);
    let fetched = t0.elapsed().as_secs_f64();
    unpack(&archive, &dir);
    let exe = dir.join(if cfg!(windows) { "audiocpp_server.exe" } else { "audiocpp_server" });
    assert!(exe.is_file(), "{} is not in the package", exe.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "{} is not executable: {mode:o}", exe.display());
    }
    let libs = std::env::var("ACPP_PACKAGE_LIBS").map(PathBuf::from).unwrap_or_else(|_| dir.clone());
    let missing = missing_libraries(build, &[&dir, &libs]);
    assert!(missing.is_empty(), "libraries missing: {missing:?}");
    let parakeet = std::env::var("ACPP_PARAKEET").ok().map(PathBuf::from);
    let config = ServerConfig {
        exe,
        backend: build.backend(),
        device: 0,
        threads: 4,
        models: parakeet.iter().map(|p| ModelEntry::parakeet_tdt("parakeet", p)).collect(),
        data_dir: root.join("data"),
        log: false,
    };
    let t1 = Instant::now();
    let server = Server::start(&config, Duration::from_secs(60), &|_| {}).unwrap();
    let started = t1.elapsed().as_secs_f64();
    assert!(server.alive());
    println!("[OK] {} {} bytes, SHA-256 matched, {fetched:.1} s; /health after {started:.2} s, pid {}", asset.file, asset.size, server.pid());
    if let (Some(_), Ok(speech)) = (parakeet, std::env::var("ACPP_SPEECH")) {
        let t2 = Instant::now();
        let t = server
            .transcribe_details(&TranscribeRequest {
                model: "parakeet".into(),
                wav: PathBuf::from(speech),
                language: None,
                prompt: None,
                options: serde_json::Map::new(),
                within: None,
            })
            .unwrap();
        println!("[OK] parakeet on {:?}: {} words in {:.2} s: {}", build, t.words.len(), t2.elapsed().as_secs_f64(), t.text);
        assert!(!t.words.is_empty());
    }
}
