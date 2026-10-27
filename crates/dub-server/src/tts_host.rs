//! The voicing model for the whole process, one on the card, as Speech Studio keeps it (`engines/tts_host.rs`): a render
//! or a phrase voiced again takes the model the last one loaded when its settings are the same, another model or other
//! settings unload it first. It leaves when it has not been used for the minutes of `tts_idle_unload_min`, when a card
//! under 12 GB needs the room for recognition or separation, before the studio's own Gemma starts, and when its engine
//! stalled or stopped answering.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::render::TtsEngine;

static HOST: Mutex<Option<(String, Arc<TtsEngine>)>> = Mutex::new(None);
/// When the voicing model was last taken: the idle watcher unloads it after the set minutes.
static LAST_USE: Mutex<Option<Instant>> = Mutex::new(None);
/// A card this small does not hold the voicing model next to recognition or separation.
const SHARED_VRAM: u64 = 12 * 1024 * 1024 * 1024;

fn host() -> std::sync::MutexGuard<'static, Option<(String, Arc<TtsEngine>)>> {
    HOST.lock().unwrap_or_else(|p| p.into_inner())
}

fn touch() {
    *LAST_USE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
}

/// The voicing model of `key` (the model and every setting it was loaded with): the loaded one when the key is the same,
/// else `load`'s, after the other one is gone.
pub fn take(key: String, load: impl FnOnce() -> Result<TtsEngine, String>) -> Result<Arc<TtsEngine>, String> {
    let mut slot = host();
    if let Some((k, e)) = slot.as_ref() {
        if *k == key {
            touch();
            e.begin();
            return Ok(e.clone());
        }
    }
    *slot = None;
    let e = Arc::new(load()?);
    *slot = Some((key, e.clone()));
    touch();
    Ok(e)
}

/// Room for another heavy model (recognition, separation): on a card under 12 GB the voicing model goes. True when it
/// went, so the caller drops its own hold too.
pub fn make_room() -> bool {
    let small = crate::hw::total_vram() < SHARED_VRAM;
    if small {
        unload();
    }
    small
}

/// Unloads the voicing model once it has not been used for `idle`; one a render holds now stays. True when it went.
pub fn unload_if_idle(idle: Duration) -> bool {
    let last = *LAST_USE.lock().unwrap_or_else(|p| p.into_inner());
    if !last.is_some_and(|at| at.elapsed() >= idle) {
        return false;
    }
    let mut slot = host();
    match slot.as_ref() {
        None => return false,
        Some((_, e)) if Arc::strong_count(e) > 1 => return false,
        Some(_) => {}
    }
    *slot = None;
    true
}

/// Watches the voicing model and unloads it after the idle minutes of the setting `tts_idle_unload_min` (0 never).
pub fn watch_idle(models_root: PathBuf) {
    std::thread::Builder::new()
        .name("tts-idle".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            let minutes = crate::models::load_selection(&models_root)
                .get("tts_idle_unload_min")
                .and_then(|v| v.as_str())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(15);
            if minutes > 0 && unload_if_idle(Duration::from_secs(minutes * 60)) {
                tracing::info!("the voicing model was unloaded after {minutes} min idle");
            }
        })
        .expect("tts idle watcher thread");
}

/// Lets the voicing model go: it leaves the card once the render holding it is done with it.
pub fn unload() {
    host().take();
}

#[cfg(test)]
mod tests {
    #[test]
    fn idle_minutes_take_the_listed_values() {
        for v in ["0", "5", "15", "30", "60"] {
            assert!(crate::models::is_selection_value("tts_idle_unload_min", v), "{v}");
        }
        assert!(!crate::models::is_selection_value("tts_idle_unload_min", "7"));
        assert!(crate::models::is_selection_key("tts_idle_unload_min"));
    }
}
