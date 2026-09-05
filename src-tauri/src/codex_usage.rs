//! Codex usage — rate limits and plan for the OpenAI Codex CLI.
//!
//! - `get_codex_usage_api`: reads the OAuth token from `~/.codex/auth.json` and
//!   calls the ChatGPT backend usage endpoint the Codex CLI itself polls.
//!
//! Mirrors `claude_usage`'s API path (in-memory TTL cache, 429 backoff, stale
//! fallback) so the frontend treats both agents the same way.
//!
//! - `get_codex_usage_stats`: token history and lifetime stats from
//!   `/wham/profiles/me` — the daily buckets live there, not under any `/usage`
//!   path, which is why the name does not mention usage.
//!
//! Deliberately **not** deserialized: `user_id`, `email`, `account_id` from the
//! usage endpoint, and the whole `profile` object (username, display name,
//! avatar URL) from the stats endpoint. Nothing in TUIC needs them, and a usage
//! payload that carries an email ends up in logs. Serde drops unknown fields, so
//! leaving them out of the structs is the whole guard.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// API types (from the ChatGPT backend usage endpoint)
// ---------------------------------------------------------------------------

/// One rate-limit window (session or weekly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateWindow {
    /// Direct percentage, 0-100.
    pub used_percent: f64,
    /// Window length in seconds (18000 = 5h, 604800 = 7d).
    pub limit_window_seconds: Option<i64>,
    /// Seconds until the window resets — avoids trusting the local clock.
    pub reset_after_seconds: Option<i64>,
    /// Unix epoch seconds of the reset.
    pub reset_at: Option<i64>,
}

/// Primary/secondary window pair plus the reached flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateLimit {
    #[serde(default)]
    pub allowed: bool,
    #[serde(default)]
    pub limit_reached: bool,
    pub primary_window: Option<CodexRateWindow>,
    pub secondary_window: Option<CodexRateWindow>,
}

/// A per-model limit (e.g. "GPT-5.3-Codex-Spark"), alongside the account limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexAdditionalRateLimit {
    pub limit_name: Option<String>,
    pub metered_feature: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
}

/// Credit balance — Codex's equivalent of Claude's extra-usage bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexCredits {
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    /// Decimal string, not a number — the backend sends `"0"`.
    pub balance: Option<String>,
}

/// Availability of a model that the current plan/limit state gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexModelUsage {
    #[serde(default)]
    pub available: bool,
    pub available_at: Option<String>,
    #[serde(default)]
    pub credits_would_enable: bool,
}

/// Response from the Codex usage endpoint, minus the identity fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexUsageApiResponse {
    pub plan_type: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
    #[serde(default)]
    pub additional_rate_limits: Vec<CodexAdditionalRateLimit>,
    pub credits: Option<CodexCredits>,
    #[serde(default)]
    pub model_usage: std::collections::HashMap<String, CodexModelUsage>,
}

/// One day of token consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexDailyBucket {
    /// `YYYY-MM-DD`, already bucketed by the backend.
    pub start_date: String,
    pub tokens: i64,
}

/// Lifetime and rolling stats behind `/wham/profiles/me`.
///
/// The same response carries a `profile` object with username, display name and
/// avatar URL. It is not modelled here on purpose — see the module header.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexStats {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
    pub total_threads: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub fast_mode_usage_percentage: Option<f64>,
    pub total_skills_used: Option<i64>,
    pub unique_skills_used: Option<i64>,
    pub most_used_reasoning_effort: Option<String>,
    pub most_used_reasoning_effort_percentage: Option<f64>,
    #[serde(default)]
    pub daily_usage_buckets: Vec<CodexDailyBucket>,
}

/// Response from the Codex stats endpoint, minus the identity fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexStatsResponse {
    #[serde(default)]
    pub stats: CodexStats,
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// Read the Codex OAuth access token from `~/.codex/auth.json`.
///
/// TUIC never refreshes it: the Codex CLI owns that token and rotates it on its
/// own runs. An expired token surfaces as a 401 the caller reports, exactly like
/// the Claude path — attempting a refresh here could invalidate the user's login.
fn read_codex_token() -> Result<String, String> {
    let home = dirs::home_dir().ok_or_else(|| "No home directory".to_string())?;
    let path = home.join(".codex").join("auth.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("No Codex credentials at {}: {e}", path.display()))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse auth.json: {e}"))?;

    parsed
        .get("tokens")
        .and_then(|t| t.get("access_token"))
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "No Codex OAuth token found".to_string())
}

// ---------------------------------------------------------------------------
// Cache + fetch
// ---------------------------------------------------------------------------

/// TTL cache shared by both endpoints — one generic so the usage ticker and the
/// dashboard cannot drift into two different staleness policies.
struct TtlCache<T>(parking_lot::Mutex<Option<(T, Instant)>>);

impl<T: Clone> TtlCache<T> {
    const fn new() -> Self {
        Self(parking_lot::Mutex::new(None))
    }

    fn fresh(&self) -> Option<T> {
        let guard = self.0.lock();
        let (value, at) = guard.as_ref()?;
        (at.elapsed() < API_CACHE_TTL).then(|| value.clone())
    }

    fn stale(&self) -> Option<T> {
        self.0.lock().as_ref().map(|(value, _)| value.clone())
    }

    fn put(&self, value: &T) {
        *self.0.lock() = Some((value.clone(), Instant::now()));
    }
}

static USAGE_CACHE: TtlCache<CodexUsageApiResponse> = TtlCache::new();
static STATS_CACHE: TtlCache<CodexStatsResponse> = TtlCache::new();

/// Shared across both endpoints: one 429 means the account is throttled, not
/// just one path, so backing off per-URL would keep hammering the other.
static RATE_LIMITED_UNTIL: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);

/// Cache TTL — matches the Claude path so both tickers poll at the same cadence.
const API_CACHE_TTL: Duration = Duration::from_secs(300);
/// Minimum backoff after a 429, so the next poll does not hammer the endpoint.
const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(120);

/// Rate limits — the endpoint the Codex CLI itself polls.
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Token history and lifetime stats. The name says "profile", not "usage": the
/// daily buckets live under `stats` here, and nowhere under a `/usage` path.
const STATS_URL: &str = "https://chatgpt.com/backend-api/wham/profiles/me";

/// Raw HTTP GET returning `T` — no caching, no retry. `(status, message)` on failure.
async fn fetch_json<T: serde::de::DeserializeOwned>(
    url: &str,
    token: &str,
) -> Result<T, (u16, String)> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        // `originator` is what the CLI sends; the endpoint rejects requests without it.
        .header("originator", "codex_cli_rs")
        .header("Authorization", format!("Bearer {token}"))
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| (0, format!("Codex request failed: {e}")))?;

    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        // The body can be an HTML challenge page — truncate so a 20 KB blob
        // never reaches the log or the ticker tooltip.
        let body: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect();
        return Err((status, format!("Codex returned {status}: {body}")));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| (0, format!("Failed to read Codex response: {e}")))?;

    serde_json::from_str(&body).map_err(|e| {
        tracing::error!(source = "codex_usage", "Parse error for {url}: {e}");
        (0, format!("Failed to parse Codex response: {e}"))
    })
}

/// Cached fetch with 429 backoff and stale fallback, shared by both endpoints.
///
/// On error it returns stale cache when there is any, so one bad poll leaves the
/// last known numbers on screen instead of blanking them.
async fn cached_fetch<T: Clone + serde::de::DeserializeOwned>(
    cache: &TtlCache<T>,
    url: &str,
) -> Result<T, String> {
    if let Some(cached) = cache.fresh() {
        return Ok(cached);
    }

    if let Some(until) = *RATE_LIMITED_UNTIL.lock()
        && Instant::now() < until
    {
        if let Some(stale) = cache.stale() {
            return Ok(stale);
        }
        return Err("Rate limited — waiting for backoff to expire".to_string());
    }

    let token = read_codex_token()?;

    match fetch_json::<T>(url, &token).await {
        Ok(data) => {
            *RATE_LIMITED_UNTIL.lock() = None;
            cache.put(&data);
            Ok(data)
        }
        Err((status, message)) => {
            if status == 429 {
                *RATE_LIMITED_UNTIL.lock() = Some(Instant::now() + RATE_LIMIT_BACKOFF);
                tracing::warn!(
                    source = "codex_usage",
                    backoff_secs = RATE_LIMIT_BACKOFF.as_secs(),
                    "Rate limited — backing off"
                );
            }
            if let Some(stale) = cache.stale() {
                tracing::info!(
                    source = "codex_usage",
                    "Returning stale cache after error: {message}"
                );
                return Ok(stale);
            }
            Err(message)
        }
    }
}

/// Fetch Codex rate-limit usage (powers the status bar ticker).
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_codex_usage_api() -> Result<CodexUsageApiResponse, String> {
    cached_fetch(&USAGE_CACHE, USAGE_URL).await
}

/// Fetch Codex token history and lifetime stats (powers the dashboard).
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn get_codex_usage_stats() -> Result<CodexStatsResponse, String> {
    cached_fetch(&STATS_CACHE, STATS_URL).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identity fields the endpoint sends must not survive deserialization —
    /// this is the guard that keeps an email out of the usage payload.
    #[test]
    fn drops_identity_fields() {
        let body = r#"{
            "user_id": "user-abc",
            "account_id": "acct-1",
            "email": "someone@example.com",
            "plan_type": "pro",
            "rate_limit": { "allowed": false, "limit_reached": true,
                "primary_window": { "used_percent": 100, "limit_window_seconds": 604800,
                                    "reset_after_seconds": 143951, "reset_at": 1788747989 },
                "secondary_window": null }
        }"#;
        let parsed: CodexUsageApiResponse = serde_json::from_str(body).unwrap();
        let round_tripped = serde_json::to_string(&parsed).unwrap();
        assert!(!round_tripped.contains("someone@example.com"));
        assert!(!round_tripped.contains("user-abc"));
        assert!(!round_tripped.contains("acct-1"));
        assert_eq!(parsed.plan_type.as_deref(), Some("pro"));
    }

    #[test]
    fn parses_windows_and_per_model_limits() {
        let body = r#"{
            "plan_type": "pro",
            "rate_limit": { "allowed": false, "limit_reached": true,
                "primary_window": { "used_percent": 100, "limit_window_seconds": 604800,
                                    "reset_after_seconds": 143951, "reset_at": 1788747989 },
                "secondary_window": null },
            "additional_rate_limits": [
                { "limit_name": "GPT-5.3-Codex-Spark", "metered_feature": "codex_bengalfox",
                  "rate_limit": { "allowed": true, "limit_reached": false,
                    "primary_window": { "used_percent": 0, "limit_window_seconds": 18000,
                                        "reset_after_seconds": 18000, "reset_at": 1788622039 },
                    "secondary_window": { "used_percent": 4, "limit_window_seconds": 604800,
                                          "reset_after_seconds": 604800, "reset_at": 1789208839 } } }
            ],
            "credits": { "has_credits": false, "unlimited": false, "balance": "0" },
            "model_usage": { "gpt-6-astra": { "available": false,
                                              "available_at": "2026-09-07T02:26:30Z",
                                              "credits_would_enable": true } }
        }"#;
        let parsed: CodexUsageApiResponse = serde_json::from_str(body).unwrap();

        let primary = parsed
            .rate_limit
            .as_ref()
            .and_then(|r| r.primary_window.as_ref())
            .expect("primary window");
        assert!((primary.used_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(primary.limit_window_seconds, Some(604_800));

        assert_eq!(parsed.additional_rate_limits.len(), 1);
        let extra = &parsed.additional_rate_limits[0];
        assert_eq!(extra.limit_name.as_deref(), Some("GPT-5.3-Codex-Spark"));
        let secondary = extra
            .rate_limit
            .as_ref()
            .and_then(|r| r.secondary_window.as_ref())
            .expect("secondary window");
        assert!((secondary.used_percent - 4.0).abs() < f64::EPSILON);

        assert_eq!(
            parsed.credits.as_ref().unwrap().balance.as_deref(),
            Some("0")
        );
        assert!(!parsed.model_usage["gpt-6-astra"].available);
    }

    /// The stats endpoint ships a `profile` object with username, display name
    /// and avatar. It must not survive into anything TUIC can log or serve.
    #[test]
    fn stats_drops_the_profile_object() {
        let body = r#"{
            "profile": { "username": "someone", "display_name": "Some One",
                         "profile_picture_url": "https://cdn.example/a.png" },
            "metadata": { "whatever": 1 },
            "stats": {
                "lifetime_tokens": 25241643691,
                "peak_daily_tokens": 2988540050,
                "current_streak_days": 19,
                "longest_streak_days": 19,
                "total_threads": 3720,
                "longest_running_turn_sec": 61603,
                "fast_mode_usage_percentage": 0.09912030727295,
                "total_skills_used": 752,
                "unique_skills_used": 10,
                "most_used_reasoning_effort": "xhigh",
                "most_used_reasoning_effort_percentage": 31.16903545784292,
                "daily_usage_buckets": [
                    { "start_date": "2026-08-09", "tokens": 33848610 },
                    { "start_date": "2026-08-10", "tokens": 511198907 }
                ]
            }
        }"#;
        let parsed: CodexStatsResponse = serde_json::from_str(body).unwrap();
        let round_tripped = serde_json::to_string(&parsed).unwrap();
        assert!(!round_tripped.contains("someone"));
        assert!(!round_tripped.contains("Some One"));
        assert!(!round_tripped.contains("cdn.example"));

        assert_eq!(parsed.stats.lifetime_tokens, Some(25_241_643_691));
        assert_eq!(parsed.stats.total_threads, Some(3720));
        assert_eq!(parsed.stats.daily_usage_buckets.len(), 2);
        assert_eq!(parsed.stats.daily_usage_buckets[1].start_date, "2026-08-10");
        assert_eq!(parsed.stats.daily_usage_buckets[1].tokens, 511_198_907);
    }

    /// An account with no history yet returns the object without the buckets.
    #[test]
    fn stats_tolerate_a_missing_history() {
        let parsed: CodexStatsResponse = serde_json::from_str(r#"{ "stats": {} }"#).unwrap();
        assert!(parsed.stats.daily_usage_buckets.is_empty());
        assert!(parsed.stats.lifetime_tokens.is_none());
    }

    /// A plan with no windows at all must still parse — a free/API-mode account
    /// returns nulls rather than omitting the object.
    #[test]
    fn tolerates_absent_windows() {
        let body = r#"{ "plan_type": null, "rate_limit": null }"#;
        let parsed: CodexUsageApiResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.rate_limit.is_none());
        assert!(parsed.additional_rate_limits.is_empty());
        assert!(parsed.model_usage.is_empty());
    }
}
