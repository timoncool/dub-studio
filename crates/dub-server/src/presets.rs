//! Пресеты настроек под железо: автоопределение GPU/VRAM (NVML) -> рекомендованный пресет квантов, но
//! юзер применяет любой сам. Пресет = бандл selection-ключей (tts/mt/asr_engine + провайдеры перевода/vision + or_tts_on). «Облако»
//! выносит тяжёлые модели (LLM/vision/TTS) в OpenRouter — разгрузка GPU и скорость. Сепарация,
//! диаризация и локальный ASR остаются на GPU, поэтому NVIDIA всё равно нужна.
//!
//! Кванты: Higgs TTS q8_0>q6_k>q4_k_m, Gemma q8_0>q6_k>q5_0>q4_0. Больше VRAM -> выше квант.
//! backend cuda/cpu тут НЕ трогаем — он авто-детектится по наличию CUDA-либ.

use serde::Serialize;
use std::path::Path;

/// Один пресет: id (стабильный ключ), человекочитаемое имя, и набор selection-ключей к применению.
pub struct Preset {
    pub id: &'static str,
    pub title: fn() -> String,
    pub subtitle: fn() -> String,
    /// Минимум VRAM в ГБ для авто-рекомендации этого пресета (0 = облако/любой).
    pub min_vram_gb: f64,
    /// Ключи active.json, которые ставит пресет. «custom» -> пусто (ничего не трогаем).
    pub keys: &'static [(&'static str, &'static str)],
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "rtx5090",
        title: || t!("preset-rtx5090-title"),
        subtitle: || t!("preset-top-subtitle"),
        min_vram_gb: 30.0,
        keys: &[("tts", "q8_0"), ("mt", "q8_0"), ("asr_engine", "parakeet"), ("local_backend", "gpu"), ("llm_provider", "local"), ("vision_provider", "local"), ("or_tts_on", "0")],
    },
    Preset {
        id: "rtx4090",
        title: || t!("preset-rtx4090-title"),
        subtitle: || t!("preset-top-subtitle"),
        min_vram_gb: 22.0,
        keys: &[("tts", "q8_0"), ("mt", "q8_0"), ("asr_engine", "parakeet"), ("local_backend", "gpu"), ("llm_provider", "local"), ("vision_provider", "local"), ("or_tts_on", "0")],
    },
    Preset {
        id: "gpu16",
        title: || t!("preset-gpu16-title"),
        subtitle: || t!("preset-gpu16-subtitle"),
        min_vram_gb: 15.0,
        keys: &[("tts", "q8_0"), ("mt", "q6_k"), ("asr_engine", "parakeet"), ("local_backend", "gpu"), ("llm_provider", "local"), ("vision_provider", "local"), ("or_tts_on", "0")],
    },
    Preset {
        id: "gpu12",
        title: || t!("preset-gpu12-title"),
        subtitle: || t!("preset-gpu12-subtitle"),
        min_vram_gb: 11.0,
        keys: &[("tts", "q6_k"), ("mt", "q5_0"), ("asr_engine", "parakeet"), ("local_backend", "gpu"), ("llm_provider", "local"), ("vision_provider", "local"), ("or_tts_on", "0")],
    },
    Preset {
        id: "gpu8",
        title: || t!("preset-gpu8-title"),
        subtitle: || t!("preset-gpu8-subtitle"),
        min_vram_gb: 7.0,
        keys: &[("tts", "q4_k_m"), ("mt", "q4_0"), ("asr_engine", "parakeet"), ("local_backend", "gpu"), ("llm_provider", "local"), ("vision_provider", "local"), ("or_tts_on", "0")],
    },
    Preset {
        id: "weak-nvidia-cloud",
        title: || t!("preset-weak-nvidia-cloud-title"),
        subtitle: || t!("preset-weak-nvidia-cloud-subtitle"),
        min_vram_gb: 0.0,
        keys: &[("local_backend", "gpu"), ("llm_provider", "openrouter"), ("vision_provider", "openrouter"), ("or_tts_on", "1")],
    },
    Preset {
        id: "cloud",
        title: || t!("preset-cloud-title"),
        subtitle: || t!("preset-cloud-subtitle"),
        min_vram_gb: 0.0,
        keys: &[("local_backend", "cpu"), ("llm_provider", "openrouter"), ("vision_provider", "openrouter"), ("or_tts_on", "1")],
    },
    Preset {
        id: "custom",
        title: || t!("preset-custom-title"),
        subtitle: || t!("preset-custom-subtitle"),
        min_vram_gb: -1.0,
        keys: &[],
    },
];

/// Найти пресет по id.
pub fn by_id(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Снимок железа + рекомендованный пресет (для UI).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareRecommendation {
    pub gpu_name: String,
    pub total_vram_gb: f64,
    pub total_ram_gb: f64,
    pub has_gpu: bool,
    pub recommended: &'static str,
    pub reason: String,
}

/// Рекомендовать пресет по железу: явное совпадение имени карты (5090/4090) приоритетнее, иначе по VRAM;
/// нет NVIDIA GPU или <7 ГБ -> облако (локально не потянет комфортно).
pub fn recommend() -> HardwareRecommendation {
    let hw = crate::hw::snapshot();
    let vram_gb = hw.total_vram as f64 / 1e9;
    let ram_gb = hw.total_ram as f64 / 1e9;
    let has_gpu = hw.total_vram > 0;
    let name_lc = hw.gpu_name.to_lowercase();

    let (recommended, reason) = if !has_gpu {
        ("cloud", t!("preset-reason-no-gpu"))
    } else if name_lc.contains("5090") {
        ("rtx5090", t!("preset-reason-top-card", gpu = hw.gpu_name.clone()))
    } else if name_lc.contains("4090") {
        ("rtx4090", t!("preset-reason-top-card", gpu = hw.gpu_name.clone()))
    } else {
        // По объёму VRAM: берём первый пресет, чей порог не выше фактического (PRESETS отсортированы по убыванию).
        let pick = PRESETS
            .iter()
            .filter(|p| p.min_vram_gb >= 0.0 && p.id != "cloud")
            .find(|p| vram_gb >= p.min_vram_gb);
        match pick {
            Some(p) => (p.id, t!("preset-reason-by-vram", gpu = hw.gpu_name.clone(), vram = format!("{vram_gb:.0}"), preset = (p.title)())),
            None => ("cloud", t!("preset-reason-low-vram", gpu = hw.gpu_name.clone(), vram = format!("{vram_gb:.0}"))),
        }
    };

    HardwareRecommendation { gpu_name: hw.gpu_name, total_vram_gb: vram_gb, total_ram_gb: ram_gb, has_gpu, recommended, reason }
}

/// Применить пресет: записать все его ключи в active.json. «custom» -> ничего (юзер сам). Возвращает
/// список применённых пар для показа/лога.
pub fn apply(models_root: &Path, id: &str) -> Result<Vec<(String, String)>, String> {
    let preset = by_id(id).ok_or_else(|| t!("preset-unknown", id = id.to_string()))?;
    let mut applied = Vec::new();
    for (k, v) in preset.keys {
        crate::models::set_selection(models_root, k, v).map_err(|e| t!("common-write", what = k.to_string(), error = e.to_string()))?;
        applied.push((k.to_string(), v.to_string()));
    }
    Ok(applied)
}
