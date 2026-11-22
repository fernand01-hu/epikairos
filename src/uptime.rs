//! 30-day uptime strip for the /health page, read from UptimeRobot.
//!
//! The API key comes from `UPTIMEROBOT_KEY`, or the file `.uptimerobot_key` in
//! `SECRET_DIR` (preferably a monitor-specific, read-only key). Without a key the
//! strip is simply not shown. Answers are cached for a minute (the free API allows
//! 10 requests a minute), and any error hides the strip.

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DAYS: usize = 30;
const CACHE_FOR: Duration = Duration::from_secs(60);
const KEY_FILE: &str = ".uptimerobot_key";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UptimeDay {
    /// End of the 24 h window, Paris time, "22/11"
    pub label: String,
    /// Percentage up, None before the monitor existed
    pub ratio: Option<f64>,
    /// "up" | "warn" | "down" | "none"
    pub class: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Uptime {
    pub total: Option<f64>,
    pub days: Vec<UptimeDay>,
}

static CACHE: Mutex<Option<(Instant, Option<Uptime>)>> = Mutex::new(None);

fn api_key() -> Option<String> {
    std::env::var("UPTIMEROBOT_KEY")
        .ok()
        .or_else(|| std::fs::read_to_string(crate::state::secret_path(KEY_FILE)).ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

pub fn bar_class(ratio: Option<f64>) -> &'static str {
    match ratio {
        None => "none",
        Some(r) if r >= 99.9 => "up",
        Some(r) if r >= 99.5 => "warn",
        Some(_) => "down",
    }
}

/// Start of each daily window (rolling 24 h windows ending at `now`), oldest first.
pub fn window_starts(now: i64) -> Vec<i64> {
    (0..DAYS as i64).map(|i| now - (DAYS as i64 - i) * 86_400).collect()
}

/// Builds the strip from UptimeRobot's getMonitors answer.
pub fn parse(json: &serde_json::Value, starts: &[i64]) -> Option<Uptime> {
    if json.get("stat")?.as_str()? != "ok" {
        return None;
    }
    let m = json.get("monitors")?.as_array()?.first()?;
    let created = m.get("create_datetime").and_then(|v| v.as_i64()).unwrap_or(0);
    let ranges: Vec<Option<f64>> = m
        .get("custom_uptime_ranges")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .split('-')
        .map(|s| s.parse::<f64>().ok())
        .collect();
    let total = m
        .get("custom_uptime_ratio")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    let days = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            // Days that ended before the monitor existed have no data
            let ratio = if start + 86_400 <= created { None } else { ranges.get(i).copied().flatten() };
            let end = chrono::DateTime::from_timestamp(start + 86_400, 0)
                .map(|d| d.with_timezone(&chrono_tz::Europe::Paris).format("%d/%m").to_string())
                .unwrap_or_default();
            UptimeDay { label: end, ratio, class: bar_class(ratio) }
        })
        .collect::<Vec<_>>();
    if days.iter().all(|d| d.ratio.is_none()) {
        return None;
    }
    Some(Uptime { total, days })
}

async fn fetch(key: &str) -> Option<Uptime> {
    let now = chrono::Utc::now().timestamp();
    let starts = window_starts(now);
    let ranges = starts.iter().map(|s| format!("{}_{}", s, s + 86_400)).collect::<Vec<_>>().join("-");
    let client = reqwest::Client::builder().timeout(Duration::from_secs(4)).build().ok()?;
    let res = client
        .post("https://api.uptimerobot.com/v2/getMonitors")
        .form(&[
            ("api_key", key),
            ("format", "json"),
            ("custom_uptime_ratios", "30"),
            ("custom_uptime_ranges", ranges.as_str()),
        ])
        .send()
        .await
        .ok()?;
    let json: serde_json::Value = res.json().await.ok()?;
    parse(&json, &starts)
}

/// The strip, or None without a key or when UptimeRobot does not answer.
pub async fn status() -> Option<Uptime> {
    let key = api_key()?;
    if let Some((at, cached)) = CACHE.lock().ok()?.as_ref() {
        if at.elapsed() < CACHE_FOR {
            return cached.clone();
        }
    }
    let fresh = fetch(&key).await;
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((Instant::now(), fresh.clone()));
    }
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_follow_uptimerobot() {
        assert_eq!(bar_class(Some(100.0)), "up");
        assert_eq!(bar_class(Some(99.9)), "up");
        assert_eq!(bar_class(Some(99.7)), "warn");
        assert_eq!(bar_class(Some(99.295)), "down");
        assert_eq!(bar_class(None), "none");
    }

    #[test]
    fn days_before_the_monitor_have_no_data() {
        let now = 1_795_400_000;
        let starts = window_starts(now);
        let mut ranges = vec!["100.000"; DAYS];
        ranges[DAYS - 1] = "99.295";
        let json = serde_json::json!({
            "stat": "ok",
            "monitors": [{
                "create_datetime": now - 3_000,
                "custom_uptime_ratio": "99.977",
                "custom_uptime_ranges": ranges.join("-"),
            }]
        });
        let up = parse(&json, &starts).unwrap();
        assert_eq!(up.days.len(), DAYS);
        assert_eq!(up.total, Some(99.977));
        assert!(up.days[..DAYS - 1].iter().all(|d| d.class == "none"));
        assert_eq!(up.days[DAYS - 1].class, "down");
        assert_eq!(up.days[DAYS - 1].ratio, Some(99.295));
    }

    #[test]
    fn errors_hide_the_strip() {
        let starts = window_starts(1_795_400_000);
        assert!(parse(&serde_json::json!({ "stat": "fail" }), &starts).is_none());
        assert!(parse(&serde_json::json!({ "stat": "ok", "monitors": [] }), &starts).is_none());
    }
}
