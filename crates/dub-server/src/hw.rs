//! Монитор ресурсов: снимок GPU (NVML) + RAM (sysinfo). Отдаётся по GET /hw/snapshot, фронт опрашивает
//! раз в секунду. NVML/System держим в ленивых статиках — не тащим в AppState.

use std::sync::{Mutex, OnceLock};

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use serde::Serialize;
use sysinfo::{Pid, System};

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

// ── Видеокарта против требований CUDA 13 ─────────────────────────────────────
//
// Весь GPU-стек (llama.cpp, Higgs, onnxruntime CUDA-EP, BSRoformer) собран под CUDA 13: ему нужен драйвер,
// поддерживающий CUDA 13 (ветка 580+, «minor version compatibility» NVIDIA), и карта не старше Turing
// (compute capability 7.5: CUDA 13 убрала Maxwell, Pascal и Volta). Решение берётся у самого драйвера CUDA
// (nvcuda.dll: cuDriverGetVersion и атрибуты устройства 0 — той карты, что возьмут движки); NVML — только
// имя карты и версия драйвера для показа.

/// Первый драйвер ветки CUDA 13 (Windows и Linux).
pub const CUDA13_DRIVER: u32 = 580;
/// Самая старая архитектура: CUDA 13 собирает от Turing; Linux-сборка движка Higgs есть только под sm 86/89/120
/// (RTX 30 и новее).
#[cfg(windows)]
pub const CUDA13_OLDEST: (u32, u32) = (7, 5);
#[cfg(not(windows))]
pub const CUDA13_OLDEST: (u32, u32) = (8, 6);
/// cuDriverGetVersion драйвера, поддерживающего CUDA 13.0 (1000 * major + 10 * minor).
const CUDA13_DRIVER_API: u32 = 13_000;

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
            r.nvidia = true;
            r.cuda_driver = Some(format!("{}.{}", version / 1000, (version % 1000) / 10));
            r.compute = Some(format!("{}.{}", compute.0, compute.1));
            match cuda13_verdict(compute, version) {
                Ok(()) => r.cuda13_ok = true,
                Err(why) => r.reason = Some(why),
            }
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
    fn the_report_is_consistent_on_this_machine() {
        let r = gpu_report();
        assert_eq!(r.cuda13_ok, r.reason.is_none(), "{r:?}");
        if !r.nvidia {
            assert_eq!(r.reason, Some("no_nvidia"));
        }
    }
}
