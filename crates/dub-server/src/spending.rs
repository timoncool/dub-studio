//! What the cloud calls cost (`cloud/spend.jsonl` of the studio) and the daily limit for agents and programs, the same
//! code as Speech Studio's `app/spending.rs`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Spent {
    pub at: i64,
    pub day: String,
    pub project: String,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub estimate: Option<f64>,
    pub cost: Option<f64>,
    /// `provider` — цена из ответа, `estimate` — оценка до запуска, `unknown` — провайдер цену не назвал.
    pub cost_source: String,
    /// `window:<…>`, `agent`, `api` или `studio`.
    pub origin: String,
}

fn spend_path(repo_root: &Path) -> PathBuf {
    repo_root.join("cloud").join("spend.jsonl")
}

/// Записать вызов облака в журнал расходов.
pub fn record(repo_root: &Path, entry: Spent) {
    use std::io::Write;
    let path = spend_path(repo_root);
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return tracing::error!("[ERROR] {}: {e}", dir.display());
        }
    }
    let line = match serde_json::to_string(&entry) {
        Ok(l) => l,
        Err(e) => return tracing::error!("[ERROR] spend record: {e}"),
    };
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::error!("[ERROR] {}: {e}", path.display());
            }
        }
        Err(e) => tracing::error!("[ERROR] {}: {e}", path.display()),
    }
}

pub fn spent(repo_root: &Path) -> Vec<Spent> {
    std::fs::read_to_string(spend_path(repo_root))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Сколько потрачено: за всё время и сегодня, по провайдерам, моделям и дням; `project` сужает до проекта.
pub fn summary(all: &[Spent], project: Option<&str>) -> Value {
    let rows: Vec<&Spent> = all.iter().filter(|s| project.is_none_or(|p| s.project == p)).collect();
    let usd = |s: &Spent| s.cost.or(s.estimate).unwrap_or(0.0);
    let group = |key: &dyn Fn(&Spent) -> String| {
        let mut m: std::collections::BTreeMap<String, (f64, usize, usize)> = Default::default();
        for s in &rows {
            let e = m.entry(key(s)).or_default();
            e.0 += usd(s);
            e.1 += 1;
            e.2 += usize::from(s.cost.is_none() && s.estimate.is_none());
        }
        m.into_iter()
            .map(|(k, (usd, runs, unknown))| json!({ "key": k, "usd": usd, "runs": runs, "unpriced": unknown }))
            .collect::<Vec<_>>()
    };
    let day = today();
    json!({
        "total": rows.iter().map(|s| usd(s)).fold(0.0, |a, b| a + b),
        "today": rows.iter().filter(|s| s.day == day).map(|s| usd(s)).fold(0.0, |a, b| a + b),
        "unpriced": rows.iter().filter(|s| s.cost.is_none() && s.estimate.is_none()).count(),
        "providers": group(&|s| s.provider.clone()),
        "models": group(&|s| format!("{}:{}", s.provider, s.model)),
        "days": group(&|s| s.day.clone()),
    })
}

/// Дневной лимит агентов и программ (USD, 0 — без лимита) из настроек студии.
pub fn daily_limit(models_root: &Path) -> f64 {
    crate::models::load_selection(models_root)
        .get("cloud_daily_limit_usd")
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64()))
        .unwrap_or(0.0)
}

/// Отказ агенту или программе, если вызов выйдет за дневной лимит; вызов без оценки цены при лимите тоже отказ.
pub fn check_limit(repo_root: &Path, models_root: &Path, estimate: Option<f64>) -> Result<(), String> {
    check_limit_as(&crate::mcp::current_actor(), repo_root, models_root, estimate)
}

/// The same refusal for a caller named outright.
pub fn check_limit_as(origin: &str, repo_root: &Path, models_root: &Path, estimate: Option<f64>) -> Result<(), String> {
    if !matches!(origin, "agent" | "api") {
        return Ok(());
    }
    let limit = daily_limit(models_root);
    if limit <= 0.0 {
        return Ok(());
    }
    let day = today();
    let used = spent(repo_root)
        .iter()
        .filter(|s| s.day == day && matches!(s.origin.as_str(), "agent" | "api"))
        .map(|s| s.cost.or(s.estimate).unwrap_or(0.0))
        .fold(0.0, |a, b| a + b);
    match estimate {
        None => Err(t!("cloud-limit-unpriced", limit = format!("{limit:.2}"))),
        Some(e) if used + e > limit => Err(t!(
            "cloud-limit-reached",
            used = format!("{used:.2}"),
            limit = format!("{limit:.2}")
        )),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(day: &str, provider: &str, cost: Option<f64>, estimate: Option<f64>) -> Spent {
        Spent {
            day: day.into(),
            provider: provider.into(),
            project: "p".into(),
            model: "m".into(),
            cost,
            estimate,
            ..Default::default()
        }
    }

    #[test]
    fn spending_sums_the_real_cost_or_the_estimate_and_counts_the_unpriced() {
        let all = vec![
            row("2026-10-14", "google", Some(0.02), Some(0.03)),
            row("2026-10-14", "google-batch", None, Some(0.01)),
            row("2026-10-13", "google", None, None),
        ];
        let s = summary(&all, None);
        assert!((s["total"].as_f64().unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(s["unpriced"], 1);
        assert_eq!(s["days"].as_array().unwrap().len(), 2);
        assert_eq!(summary(&all, Some("other"))["total"].as_f64().map(f64::is_sign_negative), Some(false), "no negative zero");
    }

    #[test]
    fn the_limit_stops_agents_and_programs_but_not_the_window() {
        let _language = crate::i18n::test_language("en");
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("active.json"), r#"{"cloud_daily_limit_usd":"0.10"}"#).unwrap();
        record(root.path(), Spent { day: today(), origin: "agent".into(), estimate: Some(0.08), ..Default::default() });
        assert!(check_limit_as("window:a", root.path(), &models, None).is_ok());
        assert!(check_limit_as("agent", root.path(), &models, Some(0.01)).is_ok());
        assert_eq!(check_limit_as("api", root.path(), &models, Some(0.05)).unwrap_err(), t!("cloud-limit-reached", used = "0.08", limit = "0.10"));
        assert!(check_limit_as("agent", root.path(), &models, None).is_err(), "a run without a price is refused under a limit");
    }
}
