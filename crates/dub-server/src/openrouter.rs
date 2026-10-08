//! OpenRouter на стороне сервера: каталог моделей с кэшем (память -> диск -> сеть), профиль выбранной модели
//! для чата, затраты по ключу. Сам HTTP — `dub_llm::openrouter` по маршруту прокси.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use dub_llm::openrouter::{CapabilityCatalog, CatalogModel, OpenRouter};
use serde::{Deserialize, Serialize};

/// Каталог и время, когда он пришёл из сети (секунды Unix).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CachedCatalog {
    pub refreshed_at: u64,
    pub models: Vec<CatalogModel>,
}

impl CachedCatalog {
    pub fn catalog(&self) -> CapabilityCatalog {
        CapabilityCatalog { models: self.models.clone() }
    }
}

fn memory() -> &'static Mutex<Option<CachedCatalog>> {
    static CATALOG: Mutex<Option<CachedCatalog>> = Mutex::new(None);
    &CATALOG
}

fn cache_path(models_root: &Path) -> PathBuf {
    models_root.join("openrouter-catalog.json")
}

fn load_cached(models_root: &Path) -> Option<CachedCatalog> {
    let path = cache_path(models_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::error!("the OpenRouter catalogue cache {} is unreadable: {e}", path.display());
            return None;
        }
    };
    match serde_json::from_str::<CachedCatalog>(&text) {
        Ok(cached) => Some(cached),
        Err(e) => {
            tracing::error!("the OpenRouter catalogue cache {} is broken ({e}); it will be downloaded again", path.display());
            None
        }
    }
}

fn save_cached(models_root: &Path, cached: &CachedCatalog) -> Result<(), String> {
    std::fs::create_dir_all(models_root).map_err(|e| format!("{}: {e}", models_root.display()))?;
    let path = cache_path(models_root);
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec(cached).map_err(|e| t!("openrouter-catalog-failed", error = e.to_string()))?;
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Скачать каталог заново (ключ не нужен), сохранить на диск и в память.
pub fn refresh(models_root: &Path) -> Result<CachedCatalog, String> {
    let client = OpenRouter::new(None).map_err(|e| format!("{e:#}"))?;
    let catalog = client.catalog().map_err(|e| format!("{e:#}"))?;
    let cached = CachedCatalog { refreshed_at: now(), models: catalog.models };
    save_cached(models_root, &cached)?;
    *memory().lock().expect("catalog cache") = Some(cached.clone());
    Ok(cached)
}

/// Каталог: из памяти, иначе с диска, иначе из сети.
pub fn catalog(models_root: &Path) -> Result<CachedCatalog, String> {
    if let Some(cached) = memory().lock().expect("catalog cache").clone() {
        return Ok(cached);
    }
    if let Some(cached) = load_cached(models_root) {
        *memory().lock().expect("catalog cache") = Some(cached.clone());
        return Ok(cached);
    }
    refresh(models_root)
}

/// Запись каталога о модели `id`. Модели нет в сохранённом каталоге — каталог перечитывается из сети один раз
/// (модель могла появиться позже). None — модель OpenRouter неизвестна или каталог недоступен (причина в лог).
pub fn describe(models_root: &Path, id: &str) -> Option<CatalogModel> {
    let known = match catalog(models_root) {
        Ok(cached) => cached.models.into_iter().find(|model| model.id == id),
        Err(e) => {
            tracing::error!("the OpenRouter catalogue is unavailable ({e}); model {id} goes without a capability check");
            return None;
        }
    };
    if known.is_some() {
        return known;
    }
    match refresh(models_root) {
        Ok(cached) => cached.models.into_iter().find(|model| model.id == id),
        Err(e) => {
            tracing::error!("the OpenRouter catalogue did not refresh ({e}); model {id} goes without a capability check");
            None
        }
    }
}

/// Потрачено ключом OpenRouter, USD (`usage` из `GET /key`). None — облако ни на одной стадии не выбрано,
/// ключа нет или OpenRouter не ответил (причина в лог).
pub fn total_usage_usd(models_root: &Path) -> Option<f64> {
    if !crate::models::openrouter_any_on(models_root) {
        return None;
    }
    let key = crate::models::openrouter_key()?;
    let usage = OpenRouter::new(Some(key)).and_then(|client| client.key_usage_usd());
    match usage {
        Ok(usage) => Some(usage),
        Err(e) => {
            tracing::warn!("OpenRouter spending was not read: {e:#}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_catalog_is_read_back_whole() {
        let root = std::env::temp_dir().join(format!("dub-or-cache-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        let listing = r#"{"data":[{"id":"a/text","name":"Text","context_length":8192,"pricing":{"prompt":"0.000001","completion":"0.000002"},
            "architecture":{"input_modalities":["text"],"output_modalities":["text"]}}]}"#;
        let models = CapabilityCatalog::parse(listing).unwrap().models;
        save_cached(&root, &CachedCatalog { refreshed_at: 42, models: models.clone() }).unwrap();
        let back = load_cached(&root).unwrap();
        assert_eq!(back.refreshed_at, 42);
        assert_eq!(back.models, models);
        assert!(!cache_path(&root).with_extension("json.tmp").exists());
        std::fs::write(cache_path(&root), "{broken").unwrap();
        assert!(load_cached(&root).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
