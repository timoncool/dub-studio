//! Где лежит onnxruntime 1.28.2, которую скачивает «Первый запуск»: каталоги под `models/runtime` — корни
//! архивов релиза, GPU-сборка (суперсет CPU и CUDA) первой — и файл библиотеки в их `lib/`.

#[cfg(windows)]
pub const ORT_DIRS: &[&str] = &["onnxruntime-win-x64-gpu_cuda13-1.28.2", "onnxruntime-win-x64-1.28.2"];
#[cfg(not(any(windows, target_os = "macos")))]
pub const ORT_DIRS: &[&str] = &["onnxruntime-linux-x64-gpu_cuda13-1.28.2", "onnxruntime-linux-x64-1.28.2"];
// macOS has no CUDA build, and 1.28.2 has an Apple silicon archive only.
#[cfg(target_os = "macos")]
pub const ORT_DIRS: &[&str] = &["onnxruntime-osx-arm64-1.28.2"];

#[cfg(windows)]
pub const ORT_LIBRARY: &str = "onnxruntime.dll";
#[cfg(not(any(windows, target_os = "macos")))]
pub const ORT_LIBRARY: &str = "libonnxruntime.so.1.28.2";
#[cfg(target_os = "macos")]
pub const ORT_LIBRARY: &str = "libonnxruntime.1.28.2.dylib";

/// Кандидаты библиотеки под `<models>/runtime` в порядке предпочтения.
pub fn ort_candidates(models_root: &std::path::Path) -> Vec<std::path::PathBuf> {
    ORT_DIRS.iter().map(|d| models_root.join("runtime").join(d).join("lib").join(ORT_LIBRARY)).collect()
}
