//! Монитор ресурсов: снимок GPU (NVML) + RAM (sysinfo). Отдаётся по GET /hw/snapshot, фронт опрашивает
//! раз в секунду. NVML/System держим в ленивых статиках — не тащим в AppState.

use std::sync::{Mutex, OnceLock};

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use serde::Serialize;
use sysinfo::{Pid, System};

/// Байт в гигабайте, которым меряют память: карта «24 ГБ» — это 24 × 2³⁰ байт, а не 24 × 10⁹.
pub const GIB: f64 = (1u64 << 30) as f64;

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareSnapshot {
    pub gpu_name: String,
    pub total_vram: u64,
    pub used_vram: u64,
    pub free_vram: u64,
    pub gpu_utilization: f64,
    pub temperature: f64,
    pub power_draw: f64,
    pub power_limit: f64,
    pub process_ram: u64,
    pub total_ram: u64,
    pub used_ram: u64,
    pub message: String,
}

fn nvml() -> &'static Mutex<Option<Nvml>> {
    static N: OnceLock<Mutex<Option<Nvml>>> = OnceLock::new();
    N.get_or_init(|| Mutex::new(None))
}

fn sysinfo() -> &'static Mutex<System> {
    static S: OnceLock<Mutex<System>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(System::new()))
}

pub fn snapshot() -> HardwareSnapshot {
    let mut snap = HardwareSnapshot::default();

    // GPU через NVML (ленивая инициализация на первом опросе).
    let mut ng = nvml().lock().unwrap();
    if ng.is_none() {
        match Nvml::init() {
            Ok(n) => *ng = Some(n),
            Err(e) => snap.message = format!("NVML init: {e}"),
        }
    }
    if let Some(n) = ng.as_ref() {
        match n.device_by_index(0) {
            Ok(d) => {
                snap.gpu_name = d.name().unwrap_or_else(|_| "NVIDIA GPU".into());
                if let Ok(m) = d.memory_info() {
                    snap.total_vram = m.total;
                    snap.used_vram = m.used;
                    snap.free_vram = m.free;
                }
                if let Ok(u) = d.utilization_rates() {
                    snap.gpu_utilization = u.gpu as f64;
                }
                if let Ok(t) = d.temperature(TemperatureSensor::Gpu) {
                    snap.temperature = t as f64;
                }
                if let Ok(p) = d.power_usage() {
                    snap.power_draw = p as f64 / 1000.0;
                }
                if let Ok(l) = d.power_management_limit() {
                    snap.power_limit = l as f64 / 1000.0;
                }
            }
            Err(e) => snap.message = format!("NVML device: {e}"),
        }
    } else if snap.message.is_empty() {
        snap.message = t!("hw-no-nvidia");
    }
    drop(ng);

    // RAM системы + процесса через sysinfo.
    let mut sg = sysinfo().lock().unwrap();
    sg.refresh_memory();
    snap.total_ram = sg.total_memory();
    snap.used_ram = sg.used_memory();
    let pid = Pid::from(std::process::id() as usize);
    sg.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    if let Some(p) = sg.process(pid) {
        snap.process_ram = p.memory();
    }

    snap
}

/// Объём из `var` в ГБ, если он задан: проверка поведения студии на другой машине.
fn simulated(var: &str) -> Option<u64> {
    std::env::var(var).ok().and_then(|v| v.parse::<u64>().ok()).map(|gb| gb << 30)
}

/// Оперативная память машины, байт (sysinfo); `DUB_STUDIO_SIMULATE_RAM_GB` подменяет её.
pub fn total_ram() -> u64 {
    if let Some(bytes) = simulated("DUB_STUDIO_SIMULATE_RAM_GB") {
        return bytes;
    }
    let mut sg = sysinfo().lock().unwrap();
    sg.refresh_memory();
    sg.total_memory()
}

/// Видеопамять карты 0, байт (NVML); 0 — карты нет или NVML не ответил. `DUB_STUDIO_SIMULATE_VRAM_GB` подменяет её.
pub fn total_vram() -> u64 {
    if let Some(bytes) = simulated("DUB_STUDIO_SIMULATE_VRAM_GB") {
        return bytes;
    }
    let mut ng = nvml().lock().unwrap();
    if ng.is_none() {
        *ng = Nvml::init().ok();
    }
    ng.as_ref()
        .and_then(|n| n.device_by_index(0).ok())
        .and_then(|d| d.memory_info().ok())
        .map_or(0, |m| m.total)
}

// ── Видеокарта против требований CUDA 13 ─────────────────────────────────────
//
// Весь GPU-стек (llama.cpp, Higgs, onnxruntime CUDA-EP, BSRoformer) собран под CUDA 13: ему нужен драйвер,
// поддерживающий CUDA 13 (ветка 580+, «minor version compatibility» NVIDIA), и карта не старше Turing
// (compute capability 7.5: CUDA 13 убрала Maxwell, Pascal и Volta). Решение берётся у самого драйвера CUDA
// (nvcuda.dll: cuDriverGetVersion и атрибуты устройства 0 — той карты, что возьмут движки); NVML — только
// имя карты и версия драйвера для показа.

/// Первый драйвер ветки CUDA 13 (Windows и Linux).
pub const CUDA13_DRIVER: u32 = 580;
/// Самая старая архитектура: CUDA 13 собирает от Turing, движок Higgs собран под неё на обеих платформах.
pub const CUDA13_OLDEST: (u32, u32) = (7, 5);
/// cuDriverGetVersion драйвера, поддерживающего CUDA 13.0 (1000 * major + 10 * minor).
const CUDA13_DRIVER_API: u32 = 13_000;
/// Second builds on CUDA 12.9 (SASS 5.2-12.0a, no PTX) for the cards and drivers CUDA 13 left behind: from Maxwell
/// (compute capability 5.2) on a driver of the CUDA 12 branch (525+); minor version compatibility runs 12.9 there.
pub const CUDA12_DRIVER: u32 = 525;
pub const CUDA12_OLDEST: (u32, u32) = (5, 2);
const CUDA12_DRIVER_API: u32 = 12_000;

/// Which CUDA build of an engine runs on this card: CUDA 13 where it can, else CUDA 12.9.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CudaBuild {
    Cuda13,
    Cuda12,
}

/// The CUDA build for a card of this compute capability on a driver of this CUDA version; None — neither runs.
pub fn cuda_build(compute: (u32, u32), cuda_driver: u32) -> Option<CudaBuild> {
    if cuda13_verdict(compute, cuda_driver).is_ok() {
        Some(CudaBuild::Cuda13)
    } else if compute >= CUDA12_OLDEST && cuda_driver >= CUDA12_DRIVER_API {
        Some(CudaBuild::Cuda12)
    } else {
        None
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuReport {
    /// Драйвер NVIDIA есть (nvcuda.dll грузится).
    pub nvidia: bool,
    pub name: Option<String>,
    /// Версия драйвера для показа, например "581.29" (NVML).
    pub driver_version: Option<String>,
    /// Какую CUDA поддерживает драйвер, например "13.0".
    pub cuda_driver: Option<String>,
    /// Compute capability карты 0, например "8.9".
    pub compute: Option<String>,
    /// Локальные стадии можно считать на GPU этой сборкой.
    pub cuda13_ok: bool,
    /// The CUDA build the card runs (engines that have both); None — neither.
    pub cuda_build: Option<CudaBuild>,
    /// Почему нельзя: no_nvidia | no_device | cuda_init | driver_old | gpu_old.
    pub reason: Option<&'static str>,
    pub min_driver: u32,
    pub min_compute: String,
}

impl GpuReport {
    /// Строка для колонки «драйвер» в «Первом запуске».
    pub fn summary(&self) -> Option<String> {
        let parts: Vec<String> = [
            self.name.clone(),
            self.driver_version.clone(),
            self.cuda_driver.as_ref().map(|c| format!("CUDA {c}")),
            self.compute.as_ref().map(|c| format!("CC {c}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// Годится ли карта с этой compute capability на драйвере с этой версией CUDA (cuDriverGetVersion) под
/// сборку CUDA 13. Старая карта важнее старого драйвера: обновление драйвера её не спасёт.
pub fn cuda13_verdict(compute: (u32, u32), cuda_driver: u32) -> Result<(), &'static str> {
    if compute < CUDA13_OLDEST {
        Err("gpu_old")
    } else if cuda_driver < CUDA13_DRIVER_API {
        Err("driver_old")
    } else {
        Ok(())
    }
}

/// The display adapter that is not Microsoft's software one (Windows: the adapter with the most memory in the display
/// class of the registry; Linux: the first DRM card on PCI), named for the log, as Speech and YuE2 Studio find it. None
/// without one, and on a Mac, whose one build is its own. Probed once a process.
pub fn display_adapter() -> Option<String> {
    static ADAPTER: OnceLock<Option<String>> = OnceLock::new();
    ADAPTER.get_or_init(probe_adapter).clone()
}

#[cfg(windows)]
fn probe_adapter() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CLASS: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let query = |value: &str| -> Option<String> {
        let output = std::process::Command::new("reg").args(["query", CLASS, "/s", "/v", value]).creation_flags(0x0800_0000).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    best_adapter(&query("DriverDesc")?, &query("HardwareInformation.qwMemorySize")?)
}

/// Joins `reg query /s` listings of the adapter names and memory sizes by their subkey and keeps the adapter with the
/// most memory that is not Microsoft's.
#[cfg(any(windows, test))]
fn best_adapter(names: &str, sizes: &str) -> Option<String> {
    fn values(listing: &str) -> Vec<(String, String)> {
        let mut key = String::new();
        let mut out = Vec::new();
        for line in listing.lines() {
            if line.starts_with("HKEY_") {
                key = line.trim().to_owned();
            } else if let Some((_, value)) = line.trim().split_once("    REG_") {
                if let Some((_, data)) = value.split_once("    ") {
                    out.push((key.clone(), data.trim().to_owned()));
                }
            }
        }
        out
    }
    let names = values(names);
    values(sizes)
        .into_iter()
        .filter_map(|(key, size)| {
            let bytes = u64::from_str_radix(size.trim_start_matches("0x"), 16).ok()?;
            let name = names.iter().find(|(name_key, _)| *name_key == key)?.1.clone();
            (!name.starts_with("Microsoft")).then_some((name, bytes))
        })
        .max_by_key(|(_, bytes)| *bytes)
        .map(|(name, _)| name)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn probe_adapter() -> Option<String> {
    let mut cards: Vec<std::path::PathBuf> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("card") && n.len() > 4 && n[4..].chars().all(|c| c.is_ascii_digit())))
        .collect();
    cards.sort();
    cards.into_iter().find_map(|path| {
        let read = |name: &str| std::fs::read_to_string(path.join("device").join(name)).ok().map(|v| v.trim().to_owned());
        let vendor = read("vendor")?;
        let device = read("device").unwrap_or_default();
        let who = match vendor.trim_start_matches("0x").to_ascii_lowercase().as_str() {
            "8086" => "Intel",
            "1002" | "1022" => "AMD",
            "10de" => "NVIDIA",
            _ => "PCI",
        };
        Some(format!("{who} display adapter ({vendor}:{device})"))
    })
}

#[cfg(target_os = "macos")]
fn probe_adapter() -> Option<String> {
    None
}

/// Отчёт о видеокарте (считается один раз за процесс: драйвер не меняется без перезапуска приложения).
pub fn gpu_report() -> GpuReport {
    static R: OnceLock<GpuReport> = OnceLock::new();
    R.get_or_init(probe_gpu).clone()
}

fn probe_gpu() -> GpuReport {
    let mut r = GpuReport { min_driver: CUDA13_DRIVER, min_compute: format!("{}.{}", CUDA13_OLDEST.0, CUDA13_OLDEST.1), ..Default::default() };
    match cuda_driver_probe() {
        CudaProbe::NoDriver => {
            r.reason = Some("no_nvidia");
            return r;
        }
        CudaProbe::NoDevice => {
            r.nvidia = true;
            r.reason = Some("no_device");
        }
        CudaProbe::Failed(what) => {
            r.nvidia = true;
            tracing::warn!("the CUDA driver does not answer: {what}");
            r.reason = Some("cuda_init");
        }
        CudaProbe::Ok { version, compute } => {
            // `DUB_STUDIO_SIMULATE_CUDA_DRIVER` (the driver's CUDA version, 12080 for 12.8) stands in for the driver's:
            // a card that runs CUDA 13 is then checked on the builds of an older driver
            let version = std::env::var("DUB_STUDIO_SIMULATE_CUDA_DRIVER").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(version);
            r.nvidia = true;
            r.cuda_driver = Some(format!("{}.{}", version / 1000, (version % 1000) / 10));
            r.compute = Some(format!("{}.{}", compute.0, compute.1));
            match cuda13_verdict(compute, version) {
                Ok(()) => r.cuda13_ok = true,
                Err(why) => r.reason = Some(why),
            }
            r.cuda_build = cuda_build(compute, version);
        }
    }
    let mut ng = nvml().lock().unwrap_or_else(|e| e.into_inner());
    if ng.is_none() {
        match Nvml::init() {
            Ok(n) => *ng = Some(n),
            Err(e) => tracing::warn!("NVML init: {e}; the card name and driver version are not shown"),
        }
    }
    if let Some(n) = ng.as_ref() {
        r.driver_version = n.sys_driver_version().ok();
        r.name = n.device_by_index(0).and_then(|d| d.name()).ok();
    }
    r
}

enum CudaProbe {
    NoDriver,
    NoDevice,
    Failed(String),
    Ok { version: u32, compute: (u32, u32) },
}

#[cfg(windows)]
fn cuda_driver_probe() -> CudaProbe {
    use std::os::windows::ffi::OsStrExt;

    let wide: Vec<u16> = std::ffi::OsStr::new("nvcuda.dll").encode_wide().chain(std::iter::once(0)).collect();
    // Модуль не выгружаем: движки процесса всё равно работают через тот же драйвер.
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return CudaProbe::NoDriver;
    }
    driver_api_probe(|name| unsafe { GetProcAddress(module, name.as_ptr()) })
}

#[cfg(windows)]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
    fn GetProcAddress(module: *mut std::ffi::c_void, name: *const u8) -> *mut std::ffi::c_void;
}

/// Linux: тот же API драйвера из libcuda.so.1, которую ставит драйвер NVIDIA.
#[cfg(not(windows))]
fn cuda_driver_probe() -> CudaProbe {
    // Модуль не выгружаем: движки процесса всё равно работают через тот же драйвер.
    let module = unsafe { libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if module.is_null() {
        return CudaProbe::NoDriver;
    }
    driver_api_probe(|name| unsafe { libc::dlsym(module, name.as_ptr().cast()) })
}

/// Версия драйвера, наличие карты и compute capability карты 0 через CUDA driver API; `sym` даёт адрес функции
/// по имени с нулём на конце.
fn driver_api_probe(sym: impl Fn(&[u8]) -> *mut std::ffi::c_void) -> CudaProbe {
    use std::ffi::c_void;
    type CuInit = unsafe extern "system" fn(u32) -> i32;
    type CuDriverGetVersion = unsafe extern "system" fn(*mut i32) -> i32;
    type CuDeviceGetCount = unsafe extern "system" fn(*mut i32) -> i32;
    type CuDeviceGetAttribute = unsafe extern "system" fn(*mut i32, i32, i32) -> i32;
    // CUdevice_attribute из cuda.h.
    const COMPUTE_CAPABILITY_MAJOR: i32 = 75;
    const COMPUTE_CAPABILITY_MINOR: i32 = 76;

    let (init, version, count, attr) = (sym(b"cuInit\0"), sym(b"cuDriverGetVersion\0"), sym(b"cuDeviceGetCount\0"), sym(b"cuDeviceGetAttribute\0"));
    if init.is_null() || version.is_null() || count.is_null() || attr.is_null() {
        return CudaProbe::Failed("the CUDA driver library without the driver API functions".into());
    }
    unsafe {
        let init = std::mem::transmute::<*mut c_void, CuInit>(init);
        let version = std::mem::transmute::<*mut c_void, CuDriverGetVersion>(version);
        let count = std::mem::transmute::<*mut c_void, CuDeviceGetCount>(count);
        let attr = std::mem::transmute::<*mut c_void, CuDeviceGetAttribute>(attr);
        let mut v = 0i32;
        let rc = version(&mut v);
        if rc != 0 {
            return CudaProbe::Failed(format!("cuDriverGetVersion: CUresult {rc}"));
        }
        let rc = init(0);
        if rc == 100 {
            return CudaProbe::NoDevice; // CUDA_ERROR_NO_DEVICE
        }
        if rc != 0 {
            return CudaProbe::Failed(format!("cuInit: CUresult {rc}"));
        }
        let mut n = 0i32;
        let rc = count(&mut n);
        if rc != 0 {
            return CudaProbe::Failed(format!("cuDeviceGetCount: CUresult {rc}"));
        }
        if n < 1 {
            return CudaProbe::NoDevice;
        }
        let (mut major, mut minor) = (0i32, 0i32);
        let rc = attr(&mut major, COMPUTE_CAPABILITY_MAJOR, 0);
        let rc2 = attr(&mut minor, COMPUTE_CAPABILITY_MINOR, 0);
        if rc != 0 || rc2 != 0 {
            return CudaProbe::Failed(format!("cuDeviceGetAttribute: CUresult {rc}/{rc2}"));
        }
        CudaProbe::Ok { version: v.max(0) as u32, compute: (major.max(0) as u32, minor.max(0) as u32) }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_reads_as_its_box_says() {
        let rtx4090 = 24_564u64 << 20;
        assert_eq!(format!("{:.0}", rtx4090 as f64 / GIB), "24");
        let rtx4060 = 8_188u64 << 20;
        assert_eq!(format!("{:.0}", rtx4060 as f64 / GIB), "8");
    }

    #[test]
    fn cuda13_needs_turing_and_a_cuda13_driver() {
        assert_eq!(cuda13_verdict(CUDA13_OLDEST, 13_000), Ok(()), "самая старая карта сборки на драйвере 580");
        assert_eq!(cuda13_verdict((8, 9), 13_040), Ok(()));
        assert_eq!(cuda13_verdict((12, 0), 13_000), Ok(()));
        assert_eq!(cuda13_verdict((8, 6), 12_080), Err("driver_old"), "RTX 30 на драйвере 566");
        assert_eq!(cuda13_verdict((6, 1), 13_000), Err("gpu_old"), "Pascal CUDA 13 не поддерживает");
        assert_eq!(cuda13_verdict((7, 0), 12_000), Err("gpu_old"), "старая карта важнее старого драйвера");
        assert_eq!(cuda13_verdict((5, 2), 13_040), Err("gpu_old"));
    }

    #[test]
    fn cuda12_takes_the_cards_and_drivers_cuda13_left() {
        assert_eq!(cuda_build((8, 9), 13_040), Some(CudaBuild::Cuda13));
        assert_eq!(cuda_build((8, 6), 12_080), Some(CudaBuild::Cuda12), "RTX 30 on driver 566");
        assert_eq!(cuda_build((6, 1), 13_000), Some(CudaBuild::Cuda12), "Pascal");
        assert_eq!(cuda_build((7, 0), 12_020), Some(CudaBuild::Cuda12), "V100");
        assert_eq!(cuda_build(CUDA12_OLDEST, 12_000), Some(CudaBuild::Cuda12), "Maxwell 2 on driver 525");
        assert_eq!(cuda_build((5, 0), 13_000), None, "Maxwell 1");
        assert_eq!(cuda_build((8, 6), 11_080), None, "driver of the CUDA 11 branch");
    }

    #[test]
    fn the_report_is_consistent_on_this_machine() {
        let r = gpu_report();
        assert_eq!(r.cuda13_ok, r.reason.is_none(), "{r:?}");
        if !r.nvidia {
            assert_eq!(r.reason, Some("no_nvidia"));
        }
    }
}
