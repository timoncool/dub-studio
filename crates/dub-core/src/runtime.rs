//! Где лежит onnxruntime 1.31.1, которую скачивает «Первый запуск»: каталоги под `models/runtime` — корни
//! архивов релиза, GPU-сборка (суперсет CPU и CUDA) первой — и файл библиотеки в их `lib/`. С фичей `ort` —
//! ещё выбор этой библиотеки для ort (load-dynamic) и единственное окружение ONNX Runtime процесса.

#[cfg(windows)]
pub const ORT_DIRS: &[&str] = &["onnxruntime-win-x64-gpu_cuda13-1.31.1", "onnxruntime-win-x64-1.31.1"];
#[cfg(not(any(windows, target_os = "macos")))]
pub const ORT_DIRS: &[&str] = &["onnxruntime-linux-x64-gpu_cuda13-1.31.1", "onnxruntime-linux-x64-1.31.1"];
// macOS has no CUDA build, and ONNX Runtime has an Apple silicon archive only.
#[cfg(target_os = "macos")]
pub const ORT_DIRS: &[&str] = &["onnxruntime-osx-arm64-1.31.1"];

#[cfg(windows)]
pub const ORT_LIBRARY: &str = "onnxruntime.dll";
#[cfg(not(any(windows, target_os = "macos")))]
pub const ORT_LIBRARY: &str = "libonnxruntime.so.1.31.1";
#[cfg(target_os = "macos")]
pub const ORT_LIBRARY: &str = "libonnxruntime.1.31.1.dylib";

/// Кандидаты библиотеки под `<models>/runtime` в порядке предпочтения.
pub fn ort_candidates(models_root: &std::path::Path) -> Vec<std::path::PathBuf> {
    ORT_DIRS.iter().map(|d| models_root.join("runtime").join(d).join("lib").join(ORT_LIBRARY)).collect()
}

/// Points ort (load-dynamic) at the studio's ONNX Runtime before ort first touches one. Without `ORT_DYLIB_PATH` ort
/// takes the first library on the system path, on Windows `System32\onnxruntime.dll` of another version, and a
/// session over another OrtApi hangs while it is made. A path the user set stays; else the first that exists of:
/// `DUB_ASR_ORT_DYLIB`, the library beside the program, the runtime folders under the models root
/// (`DUBENGINE_MODELS_ROOT`, `<exe>/models`, the dev layout two and three folders up, `<cwd>/models`).
#[cfg(feature = "ort")]
pub fn ensure_ort_dylib() {
    use std::path::PathBuf;
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("ORT_DYLIB_PATH").is_some() {
            return;
        }
        let mut cands: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os("DUB_ASR_ORT_DYLIB") {
            cands.push(PathBuf::from(p));
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(m) = std::env::var_os("DUBENGINE_MODELS_ROOT") {
            roots.push(PathBuf::from(m));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.join("models"));
                if let Some(p2) = dir.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) {
                    roots.push(p2.join("models"));
                }
                if let Some(p1) = dir.parent().and_then(|d| d.parent()) {
                    roots.push(p1.join("models"));
                }
                cands.push(dir.join(ORT_LIBRARY));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("models"));
        }
        for r in &roots {
            cands.extend(ort_candidates(r));
        }
        if let Some(found) = cands.into_iter().find(|c| c.is_file()) {
            std::env::set_var("ORT_DYLIB_PATH", found);
        }
    });
}

/// The process's ONNX Runtime environment, with Microsoft's usage telemetry off, built over the library
/// `ensure_ort_dylib` picks: ort allows one per process and refuses a second even after the first is dropped, so every
/// session of every crate is made in this one. A library that is not on disk yet (before its download) is an error
/// naming the path, and the next call tries again; ort itself would panic on it.
#[cfg(feature = "ort")]
pub fn ort_env() -> Result<ort::environment::Environment, String> {
    static ENV: std::sync::Mutex<Option<ort::environment::Environment>> = std::sync::Mutex::new(None);
    let mut env = ENV.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(built) = env.as_ref() {
        return Ok(built.clone());
    }
    ensure_ort_dylib();
    match std::env::var_os("ORT_DYLIB_PATH").map(std::path::PathBuf::from) {
        Some(path) if path.is_file() => {}
        Some(path) => return Err(format!("ONNX Runtime {} is not downloaded yet", path.display())),
        None => return Err(format!("ONNX Runtime ({ORT_LIBRARY}) is not downloaded yet")),
    }
    let built = ort::init().with_telemetry(false).build().map_err(|e| format!("ONNX Runtime: {e}"))?;
    *env = Some(built.clone());
    Ok(built)
}
