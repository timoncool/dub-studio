//! Таблицы импорта DLL движков: всё, что грузят llama.cpp, Higgs, onnxruntime (и его CUDA-провайдер),
//! BSRoformer, ffmpeg и CUDA-библиотеки Whisper, должно либо лежать в каталогах поиска приложения, либо
//! давать Windows/драйвер, либо ставиться «Первым запуском» (файлы компонентов манифеста) или идти в комплекте
//! (Bundled). Так ловится класс ошибки «onnxruntime_providers_cuda.dll требует cufft64_12.dll, а его никто не
//! ставит» — тестом, а не пользователем без CUDA Toolkit.
//!
//! Юнит-тест по манифесту идёт всегда; проверка настоящих архивов (скачать закреплённые zip/whl штатной
//! закачкой в пустой корень и прочитать их импорты) — `scripts/check-dll-imports.ps1`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::setup::{self, Delivery};

/// Имена DLL из таблицы импорта PE (PE32 и PE32+), включая отложенную загрузку (delay-load): CUDA-провайдер
/// onnxruntime берёт cuDNN и cuFFT именно так, и без неё проверка их не видит.
pub fn imported_libraries(binary: &Path) -> Result<Vec<String>, String> {
    let data = std::fs::read(binary).map_err(|e| format!("{}: {e}", binary.display()))?;
    let at = |offset: usize| -> Result<u32, String> {
        let b = data.get(offset..offset + 4).ok_or("truncated PE header")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let short = |offset: usize| -> Result<u16, String> {
        let b = data.get(offset..offset + 2).ok_or("truncated PE header")?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    };
    let pe = at(0x3c)? as usize;
    if data.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(format!("{} is not a PE file", binary.display()));
    }
    let sections = short(pe + 6)? as usize;
    let optional_size = short(pe + 20)? as usize;
    let optional = pe + 24;
    // 0x20b — PE32+, его каталоги данных на 16 байт дальше.
    let directories = optional + if short(optional)? == 0x20b { 112 } else { 96 };
    let headers: Vec<(u32, u32, u32)> = (0..sections)
        .map(|i| {
            let base = optional + optional_size + i * 40;
            Ok((at(base + 12)?, at(base + 16)?, at(base + 20)?))
        })
        .collect::<Result<_, String>>()?;
    let offset_of = |rva: u32| -> Option<usize> {
        headers
            .iter()
            .find(|(va, size, _)| rva >= *va && rva < va + size)
            .map(|(va, _, raw)| (raw + (rva - va)) as usize)
    };
    let name_at = |rva: u32| -> Result<String, String> {
        let start = offset_of(rva).ok_or("import name outside the file")?;
        let end = data[start..].iter().position(|b| *b == 0).ok_or("import name without a terminator")? + start;
        Ok(String::from_utf8_lossy(&data[start..end]).into_owned())
    };
    let mut names = Vec::new();
    // (каталог данных, размер дескриптора, смещение RVA имени в дескрипторе): импорт (1) и отложенный импорт (13).
    for (dir_index, descriptor, name_field) in [(1usize, 20usize, 12usize), (13, 32, 4)] {
        let table_rva = at(directories + dir_index * 8)?;
        if table_rva == 0 {
            continue;
        }
        let mut entry = offset_of(table_rva).ok_or("import table outside the file")?;
        loop {
            let d = data.get(entry..entry + descriptor).ok_or("truncated import table")?;
            if d.iter().all(|b| *b == 0) {
                break;
            }
            let f = &d[name_field..name_field + 4];
            names.push(name_at(u32::from_le_bytes([f[0], f[1], f[2], f[3]]))?);
            entry += descriptor;
        }
    }
    Ok(names)
}

/// Библиотеки, которые есть в любой Windows 10/11 или приходят с драйвером видеокарты. Всё прочее приложение
/// должно привезти само.
pub fn is_provided_by_the_system(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("api-ms-win-")
        || name.starts_with("ext-ms-win-")
        // Библиотека самого драйвера NVIDIA: по правилам NVIDIA не распространяется, её ставит драйвер.
        || name == "nvcuda.dll"
        || [
            "kernel32.dll", "kernelbase.dll", "ntdll.dll", "user32.dll", "gdi32.dll", "advapi32.dll", "shell32.dll",
            "shlwapi.dll", "ole32.dll", "oleaut32.dll", "combase.dll", "ws2_32.dll", "crypt32.dll", "bcrypt.dll",
            "ncrypt.dll", "secur32.dll", "rpcrt4.dll", "setupapi.dll", "cfgmgr32.dll", "version.dll", "dbghelp.dll",
            "powrprof.dll", "psapi.dll", "userenv.dll", "winmm.dll", "msvcrt.dll", "comdlg32.dll", "comctl32.dll",
            "imm32.dll", "iphlpapi.dll", "winhttp.dll", "wininet.dll", "dxgi.dll", "d3d11.dll", "d3d12.dll",
            "dxcore.dll", "mfplat.dll", "mf.dll", "mfreadwrite.dll", "avrt.dll", "ksuser.dll", "vfw32.dll",
            "avicap32.dll", "strmiids.dll", "dwmapi.dll", "uxtheme.dll", "winspool.drv", "normaliz.dll", "wldap32.dll",
            "d2d1.dll", "dwrite.dll", "usp10.dll",
        ]
        .contains(&name.as_str())
}

/// Каталоги, где процессы Dub ищут DLL: PATH приложения (lib.rs::augment_path_for_tools) — ffmpeg, llama и
/// каталог движка Higgs с CUDA-библиотеками. Каталог самого бинаря добавляется к ним при проверке.
fn search_dirs(root: &Path) -> Vec<PathBuf> {
    vec![root.join("tools").join("ffmpeg"), root.join("tools").join("llama"), root.join("models").join("higgs-engine")]
}

/// Что грузят процессы Dub: (компонент, файл). ggml-cuda.dll llama.cpp и BSRoformer, подбиблиотеки cuDNN
/// грузятся LoadLibrary'ем, поэтому проверяются отдельными точками входа.
pub const ENTRY_POINTS: &[(&str, &str)] = &[
    ("llama", "tools/llama/llama-server.exe"),
    ("llama", "tools/llama/ggml-cuda.dll"),
    ("higgs-engine", "models/higgs-engine/audiocpp_engine.dll"),
    ("onnxruntime", "models/runtime/onnxruntime-win-x64-1.28.2/lib/onnxruntime.dll"),
    ("onnxruntime-gpu", "models/runtime/onnxruntime-win-x64-gpu_cuda13-1.28.2/lib/onnxruntime_providers_cuda.dll"),
    ("cudnn", "models/higgs-engine/cudnn64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_graph64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_ops64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_cnn64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_adv64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_heuristic64_9.dll"),
    ("cudnn", "models/higgs-engine/cudnn_engines_precompiled64_9.dll"),
    ("bsroformer-engine", "tools/bsroformer/bs_roformer-cli.exe"),
    ("bsroformer-engine", "tools/bsroformer/ggml-cuda.dll"),
    ("bsroformer-engine-cpu", "tools/bsroformer-cpu/bs_roformer-cli.exe"),
    ("ffmpeg", "tools/ffmpeg/ffmpeg.exe"),
    ("whisper-cuda", "tools/whisper/cublas64_11.dll"),
    ("whisper-cuda", "tools/whisper/cudnn_ops_infer64_8.dll"),
    ("whisper-cuda", "tools/whisper/cudnn_cnn_infer64_8.dll"),
];

/// Итог проверки одной точки входа.
#[derive(Debug, Default)]
pub struct Resolution {
    /// Не найдено ни в каталогах поиска, ни в системе.
    pub missing: Vec<String>,
    /// Найдено в каталогах поиска: (имя, путь).
    pub found: Vec<(String, PathBuf)>,
}

/// Обход импортов от точки входа (exe → ggml.dll → ggml-cuda.dll → cuBLAS…): каждая библиотека ищется в
/// каталоге точки входа, затем в каталогах поиска.
pub fn resolve(entry: &Path, dirs: &[PathBuf]) -> Result<Resolution, String> {
    let mut search: Vec<PathBuf> = entry.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    search.extend(dirs.iter().cloned());
    let mut seen: Vec<String> = Vec::new();
    let mut out = Resolution::default();
    let mut queue: VecDeque<PathBuf> = VecDeque::from([entry.to_path_buf()]);
    while let Some(bin) = queue.pop_front() {
        for import in imported_libraries(&bin)? {
            let lowered = import.to_ascii_lowercase();
            if is_provided_by_the_system(&lowered) || seen.contains(&lowered) {
                continue;
            }
            seen.push(lowered);
            match search.iter().map(|d| d.join(&import)).find(|p| p.is_file()) {
                Some(p) => {
                    out.found.push((import, p.clone()));
                    queue.push_back(p);
                }
                None => out.missing.push(import),
            }
        }
    }
    Ok(out)
}

/// Имена файлов (в нижнем регистре), которые ставит или приносит манифест в каталоги поиска: прямые файлы,
/// маркеры (ключевые файлы архивов) и файлы Bundled-компонентов.
fn manifest_basenames(root: &Path) -> Vec<(String, PathBuf, &'static str)> {
    let mut out = Vec::new();
    for c in setup::manifest() {
        let rels = c
            .files
            .iter()
            .filter(|f| f.extract == setup::Extract::None)
            .map(|f| f.dest_rel)
            .chain(c.markers.iter().map(|m| m.rel));
        for rel in rels {
            let p = root.join(rel);
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                out.push((name.to_ascii_lowercase(), p.parent().map(Path::to_path_buf).unwrap_or_default(), c.id));
            }
        }
    }
    out
}

/// Недостающее на корне, собранном «Первым запуском»: чего нет в каталогах поиска и что не приносит в них
/// Bundled-компонент (VC++ runtime кладёт установщик, а не закачка).
/// seen получает каждую библиотеку, найденную в каталоге поиска (для разбора, откуда что берётся).
pub fn check_root(root: &Path, seen: &mut dyn FnMut(&str, &(String, PathBuf))) -> Result<Vec<String>, String> {
    let dirs = search_dirs(root);
    let manifest = setup::manifest();
    let bundled: Vec<(String, PathBuf)> = manifest_basenames(root)
        .into_iter()
        .filter(|(_, _, id)| manifest.iter().any(|c| c.id == *id && c.delivery == Delivery::Bundled))
        .map(|(n, d, _)| (n, d))
        .collect();
    let mut problems = Vec::new();
    for (component, rel) in ENTRY_POINTS {
        let entry = root.join(rel);
        if !entry.is_file() {
            problems.push(format!("{component}: no {rel} after installation"));
            continue;
        }
        let r = resolve(&entry, &dirs)?;
        for hit in &r.found {
            seen(rel, hit);
        }
        for name in r.missing {
            let lowered = name.to_ascii_lowercase();
            let shipped = bundled
                .iter()
                .any(|(n, d)| *n == lowered && (entry.parent() == Some(d.as_path()) || dirs.contains(d)));
            if !shipped {
                problems.push(format!("{rel} imports {name}: it is neither next to it, nor in Windows, nor in the manifest"));
            }
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Библиотеки, которые точка входа берёт из ЧУЖОГО компонента: (кто грузит, DLL, кто её ставит, грузится ли
    /// она LoadLibrary'ем во время работы). Таблицу импорта сняли с настоящих архивов
    /// (scripts/check-dll-imports.ps1, dumpbin /DEPENDENTS); cuDNN и cuFFT CUDA-провайдер onnxruntime, а cuBLAS и
    /// cuDNN 8 — CTranslate2 внутри whisper-faster.exe грузят сами, в таблице импорта их нет.
    const CROSS_COMPONENT: &[(&str, &str, &str, bool)] = &[
        ("llama", "cublas64_13.dll", "cuda-runtime", false),
        ("llama", "cublasLt64_13.dll", "cuda-runtime", false),
        ("higgs-engine", "cublas64_13.dll", "cuda-runtime", false),
        ("bsroformer-engine", "cublas64_13.dll", "cuda-runtime", false),
        ("bsroformer-engine", "cublasLt64_13.dll", "cuda-runtime", false),
        ("onnxruntime-gpu", "cublas64_13.dll", "cuda-runtime", false),
        ("onnxruntime-gpu", "cublasLt64_13.dll", "cuda-runtime", false),
        ("onnxruntime-gpu", "cufft64_12.dll", "cuda-runtime", true),
        ("onnxruntime-gpu", "cudnn64_9.dll", "cudnn", true),
    ];

    /// Юнит-тест по манифесту: каждая точка входа — скачиваемый компонент в своём каталоге, а каждая DLL, которую
    /// точка входа берёт из чужого компонента, этим компонентом и ставится (маркер) туда, где ищет загрузчик.
    #[test]
    fn cross_component_imports_are_installed_where_the_loader_looks() {
        let root = Path::new("R");
        let manifest = setup::manifest();
        for (component, rel) in ENTRY_POINTS {
            let c = manifest.iter().find(|c| c.id == *component).unwrap_or_else(|| panic!("нет компонента {component}"));
            assert_eq!(c.delivery, Delivery::Download, "{component}");
            assert!(
                c.files.iter().any(|f| Path::new(rel).starts_with(Path::new(f.dest_rel).parent().unwrap())),
                "{rel} не в каталоге компонента {component}"
            );
        }
        let dirs = search_dirs(root);
        let known = manifest_basenames(root);
        for (importer, dll, provider, _) in CROSS_COMPONENT {
            assert!(ENTRY_POINTS.iter().any(|(c, _)| c == importer), "{importer} не проверяется");
            let lowered = dll.to_ascii_lowercase();
            let hit = known.iter().find(|(n, _, id)| *n == lowered && id == provider);
            let Some((_, dir, _)) = hit else { panic!("{dll} ({importer}) не значится в маркерах {provider}") };
            assert!(dirs.contains(dir), "{dll} от {provider} ставится в {}, где загрузчик не ищет", dir.display());
        }
    }

    /// Таблица импорта — основа проверки: если она не читается, проверка молча проходит на чём угодно.
    #[test]
    #[cfg(windows)]
    fn imports_are_read_out_of_a_real_binary() {
        let system = Path::new("C:/Windows/System32/notepad.exe");
        if !system.is_file() {
            return;
        }
        let imports: Vec<String> =
            imported_libraries(system).expect("у notepad есть таблица импорта").iter().map(|n| n.to_ascii_lowercase()).collect();
        assert!(imports.iter().any(|n| n == "user32.dll"), "обычный импорт: {imports:?}");
        assert!(imports.iter().any(|n| n == "comdlg32.dll"), "отложенный импорт: {imports:?}");
        assert!(imports.iter().all(|n| n.ends_with(".dll") || n.ends_with(".drv")), "{imports:?}");
    }

    /// Проверка настоящих архивов: штатная закачка закреплённых zip/whl в пустой корень DUB_DLL_STAGING и обход
    /// импортов. Тяжёлая (~3 ГБ), запускается скриптом scripts/check-dll-imports.ps1.
    #[test]
    #[ignore]
    fn downloaded_runtime_can_be_loaded() {
        let root = PathBuf::from(std::env::var("DUB_DLL_STAGING").expect("DUB_DLL_STAGING = пустой корень для закачки"));
        let mut ids: Vec<String> = ENTRY_POINTS.iter().map(|(c, _)| c.to_string()).collect();
        ids.extend(CROSS_COMPONENT.iter().map(|(_, _, p, _)| p.to_string()));
        ids.sort();
        ids.dedup();
        let progress = |ev: serde_json::Value| {
            if let Some(phase) = ev.get("phase").and_then(|v| v.as_str()) {
                if phase != "download" {
                    eprintln!("[{phase}] {}", ev.get("file").and_then(|v| v.as_str()).unwrap_or(""));
                }
            }
        };
        setup::download_components(&root, &ids, &|| false, &progress).unwrap_or_else(|e| panic!("закачка: {} ({})", e.detail, e.code));
        let problems = check_root(&root, &mut |entry, (name, path)| {
            if Path::new(entry).parent() != path.strip_prefix(&root).ok().and_then(Path::parent) {
                eprintln!("{entry} -> {name} ({})", path.strip_prefix(&root).unwrap_or(path).display());
            }
        })
        .expect("чтение таблиц импорта");
        let mut problems = problems;
        for (importer, dll, provider, runtime) in CROSS_COMPONENT {
            if *runtime && !search_dirs(&root).iter().any(|d| d.join(dll).is_file()) {
                problems.push(format!("{importer} грузит {dll}, а {provider} его не положил в каталог поиска"));
            }
        }
        assert!(problems.is_empty(), "не загрузится: {problems:#?}");
    }
}
