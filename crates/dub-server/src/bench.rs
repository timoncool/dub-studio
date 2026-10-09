//! Пер-стадийный бенчмаркинг пайплайна. Фоновый семплер снимает GPU-util/VRAM (через hw::snapshot, NVML) +
//! CPU% (sysinfo) каждые ~1с, помечая каждый сэмпл ТЕКУЩЕЙ стадией. По финишу агрегирует по стадиям:
//! длительность, средн./пик GPU, средн. CPU, пик VRAM/RAM, и кто «узкое место» (GPU vs CPU) — пишет в
//! workspace/<pid>/bench.json и эмитит строки в лог. Цель: гонять один файл с разными настройками и ВИДЕТЬ,
//! сколько времени и каких ресурсов ушло на каждый этап (а не гадать).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::json;

struct Sample {
    stage: String,
    gpu: f64,     // % GPU utilization
    cpu: f64,     // % общий CPU
    vram_mb: f64, // used VRAM, МБ
    ram_mb: f64,  // RAM процесса, МБ
}

pub struct Bench {
    enabled: bool, // галка настроек (active.json "bench"); ВЫКЛ по умолчанию -> все методы no-op
    label: String, // "analyze" | "render"
    dir: PathBuf,
    t0: Instant,
    cur: Arc<Mutex<String>>,
    marks: Vec<(String, f64)>, // (стадия, elapsed_сек на старте)
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<Sample>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Bench {
    /// Запустить бенч + фоновый семплер ресурсов. `enabled=false` (дефолт настроек) — пустышка:
    /// ни фонового потока, ни bench.json, ни ⏱-строк в журнале.
    pub fn start(dir: &Path, label: &str, enabled: bool) -> Self {
        let cur = Arc::new(Mutex::new(String::from("init")));
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        if !enabled {
            return Bench {
                enabled: false,
                label: label.into(),
                dir: dir.to_path_buf(),
                t0: Instant::now(),
                cur,
                marks: Vec::new(),
                stop,
                samples,
                handle: None,
            };
        }
        let (c2, s2, sm2) = (cur.clone(), stop.clone(), samples.clone());
        let handle = std::thread::spawn(move || {
            let mut sys = sysinfo::System::new();
            while !s2.load(Ordering::Relaxed) {
                // sysinfo требует интервал между двумя refresh для дельты CPU
                sys.refresh_cpu_all();
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if s2.load(Ordering::Relaxed) {
                    break;
                }
                sys.refresh_cpu_all();
                let cpu = sys.global_cpu_usage() as f64;
                let hw = crate::hw::snapshot();
                let stage = c2.lock().unwrap().clone();
                sm2.lock().unwrap().push(Sample {
                    stage,
                    gpu: hw.gpu_utilization,
                    cpu,
                    vram_mb: hw.used_vram as f64 / 1_048_576.0,
                    ram_mb: hw.process_ram as f64 / 1_048_576.0,
                });
            }
        });
        Bench {
            enabled: true,
            label: label.into(),
            dir: dir.to_path_buf(),
            t0: Instant::now(),
            cur,
            marks: Vec::new(),
            stop,
            samples,
            handle: Some(handle),
        }
    }

    /// Отметить начало новой стадии (закрывает предыдущую по времени).
    #[allow(dead_code)]
    pub fn stage(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        let el = self.t0.elapsed().as_secs_f64();
        self.marks.push((name.to_string(), el));
        *self.cur.lock().unwrap() = name.to_string();
    }

    /// Остановить семплер, агрегировать по стадиям, дописать bench.json, эмитить строки через `emit_line`.
    pub fn finish(mut self, emit_line: impl Fn(&str)) {
        if !self.enabled {
            return;
        }
        let total = self.t0.elapsed().as_secs_f64();
        self.marks.push(("__end__".into(), total));
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let samples = self.samples.lock().unwrap();
        let mut out = serde_json::Map::new();
        for w in self.marks.windows(2) {
            let name = w[0].0.clone();
            if name == "__end__" {
                continue;
            }
            let dur = w[1].1 - w[0].1;
            let ss: Vec<&Sample> = samples.iter().filter(|s| s.stage == name).collect();
            let n = ss.len().max(1) as f64;
            let gpu_avg = ss.iter().map(|s| s.gpu).sum::<f64>() / n;
            let gpu_max = ss.iter().map(|s| s.gpu).fold(0.0, f64::max);
            let cpu_avg = ss.iter().map(|s| s.cpu).sum::<f64>() / n;
            let cpu_max = ss.iter().map(|s| s.cpu).fold(0.0, f64::max);
            let vram_max = ss.iter().map(|s| s.vram_mb).fold(0.0, f64::max);
            let ram_max = ss.iter().map(|s| s.ram_mb).fold(0.0, f64::max);
            let bound = if gpu_avg >= cpu_avg { "GPU" } else { "CPU" };
            out.insert(
                name.clone(),
                json!({
                    "sec": (dur * 10.0).round() / 10.0,
                    "gpu_avg": gpu_avg.round(),
                    "gpu_max": gpu_max.round(),
                    "cpu_avg": cpu_avg.round(),
                    "cpu_max": cpu_max.round(),
                    "vram_mb_max": vram_max.round(),
                    "ram_mb_max": ram_max.round(),
                    "bound": bound,
                }),
            );
            emit_line(&t!(
                "bench-stage",
                name = name.clone(),
                seconds = format!("{dur:.1}"),
                gpu_avg = format!("{gpu_avg:.0}"),
                gpu_max = format!("{gpu_max:.0}"),
                cpu_avg = format!("{cpu_avg:.0}"),
                vram = format!("{vram_max:.0}"),
                bound = bound
            ));
        }
        emit_line(&t!("bench-total", label = self.label.clone(), seconds = format!("{total:.1}")));
        drop(samples);
        // дописать/смёржить bench.json (analyze и render пишут в один файл)
        let path = self.dir.join("bench.json");
        let mut root: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}));
        root[&self.label] = json!({
            "total_sec": (total * 10.0).round() / 10.0,
            "stages": serde_json::Value::Object(out),
        });
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&root).unwrap_or_default(),
        );
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        // страховка: если finish() не вызвали (ранний return по ошибке `?`) — гасим семплер-поток.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
