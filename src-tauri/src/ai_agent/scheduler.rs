//! Cron scheduler for recurring agent tasks.
//!
//! Persists jobs to `ai-cron.json` in the app config dir. A tokio interval
//! task checks cron expressions every 30 s. Triggered jobs always run with
//! `TrustLevel::Standard` regardless of any global unsafe-mode toggle.

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Notify;

use crate::state::AppState;

const CONFIG_FILE: &str = "ai-cron.json";
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
pub(crate) const DEFAULT_MAX_DURATION_SECS: u64 = 300;

// ── Job definition ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScheduledJob {
    pub id: String,
    pub cron_expr: String,
    pub goal: String,
    pub target_session: Option<String>,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub one_shot: bool,
}

fn default_max_duration() -> u64 {
    DEFAULT_MAX_DURATION_SECS
}
fn default_enabled() -> bool {
    true
}

impl ScheduledJob {
    pub fn parse_schedule(&self) -> Result<Schedule, String> {
        Schedule::from_str(&self.cron_expr)
            .map_err(|e| format!("Invalid cron expression '{}': {e}", self.cron_expr))
    }
}

// ── Scheduler state ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SchedulerConfig {
    #[serde(default)]
    pub jobs: Vec<ScheduledJob>,
}

pub(crate) struct Scheduler {
    state: Arc<AppState>,
    last_fire: parking_lot::Mutex<HashMap<String, DateTime<Utc>>>,
    stop: Arc<Notify>,
}

impl Scheduler {
    /// `stop` is the AppState-owned handle (`AppState::scheduler_stop`), shared
    /// across start/stop cycles so `reconcile_after_config_change` can wake a
    /// running scheduler without holding a reference to this `Scheduler`.
    pub fn new(state: Arc<AppState>, stop: Arc<Notify>) -> Self {
        Self {
            state,
            last_fire: parking_lot::Mutex::new(HashMap::new()),
            stop,
        }
    }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => self.tick().await,
                _ = self.stop.notified() => {
                    tracing::info!("Scheduler stopped");
                    break;
                }
            }
        }
    }

    async fn tick(&self) {
        let config: SchedulerConfig = crate::config::load_json_config(CONFIG_FILE);
        let now = Utc::now();

        for job in &config.jobs {
            if !job.enabled {
                continue;
            }
            let schedule = match job.parse_schedule() {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(job_id = %job.id, "Skipping job: {e}");
                    continue;
                }
            };

            if !should_fire(&schedule, &job.id, now, &self.last_fire) {
                continue;
            }

            self.last_fire.lock().insert(job.id.clone(), now);

            if super::conversation_engine::ACTIVE_CONVERSATIONS
                .contains_key(job.target_session.as_deref().unwrap_or(""))
            {
                tracing::info!(job_id = %job.id, "Skipping: target session busy");
                continue;
            }

            tracing::info!(job_id = %job.id, goal = %job.goal, "Firing scheduled job");
            self.fire_job(job).await;
        }
    }

    async fn fire_job(&self, job: &ScheduledJob) {
        let session_id = match &job.target_session {
            Some(sid) if self.state.session_maps.sessions.contains_key(sid) => sid.clone(),
            _ => {
                match crate::pty::spawn_session_for_agent(
                    &self.state,
                    None,
                    Some(format!("cron:{}", job.id)),
                )
                .await
                {
                    Ok(sid) => sid,
                    Err(e) => {
                        tracing::error!(job_id = %job.id, "Failed to spawn session: {e}");
                        return;
                    }
                }
            }
        };

        use super::conversation_engine::{Autonomy, ConversationConfig, start_conversation};
        use std::collections::HashSet;

        let config = ConversationConfig {
            autonomy: Autonomy::Autonomous,
            max_steps: None,
            temperature: 0.7,
            model_override: None,
            bypassed_tools: HashSet::new(),
            reasoning: super::conversation_engine::ReasoningLevel::default(),
            compact_after_tokens: Some(super::engine::DEFAULT_COMPACT_THRESHOLD_TOKENS),
        };

        let timeout = std::time::Duration::from_secs(job.max_duration_secs);
        let job_id = job.id.clone();
        let job_goal = job.goal.clone();
        let one_shot = job.one_shot;
        let state = self.state.clone();

        match start_conversation(
            self.state.clone(),
            session_id.clone(),
            job.goal.clone(),
            config,
        )
        .await
        {
            Ok(mut rx) => {
                tracing::info!(job_id = %job_id, session_id, "Scheduled agent started");
                // Consume events in a background task; enforce max_duration_secs.
                tokio::spawn(async move {
                    let deadline = tokio::time::sleep(timeout);
                    tokio::pin!(deadline);
                    let mut timed_out = false;
                    loop {
                        tokio::select! {
                            _ = &mut deadline => {
                                tracing::warn!(job_id = %job_id, "Scheduled job timed out after {}s", timeout.as_secs());
                                if let Err(e) = super::conversation_engine::cancel_conversation(&session_id) {
                                    tracing::warn!(job_id = %job_id, "Failed to cancel timed-out job: {e}");
                                }
                                timed_out = true;
                                break;
                            }
                            msg = rx.recv() => {
                                match msg {
                                    Ok(event) => {
                                        tracing::debug!(job_id = %job_id, event = ?event, "Scheduled job event");
                                        match &event {
                                            super::conversation_engine::ConversationEvent::Completed { .. }
                                            | super::conversation_engine::ConversationEvent::Error { .. } => break,
                                            _ => {}
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    }
                    // Disable one-shot jobs after completion.
                    if one_shot {
                        let result = crate::config::ConfigFile::<SchedulerConfig>::new(CONFIG_FILE)
                            .update(|cfg| {
                                if let Some(j) = cfg.jobs.iter_mut().find(|j| j.id == job_id) {
                                    j.enabled = false;
                                    true
                                } else {
                                    false
                                }
                            });
                        if let Err(e) = result {
                            tracing::warn!(job_id = %job_id, "Failed to disable one-shot job: {e}");
                        }
                    }
                    let _ = state
                        .event_bus
                        .send(crate::state::AppEvent::ScheduledJobCompleted {
                            job_id: job_id.clone(),
                            goal: job_goal,
                            timed_out,
                        });
                });
            }
            Err(e) => {
                tracing::error!(job_id = %job_id, "Failed to start agent: {e}");
            }
        }
    }
}

fn should_fire(
    schedule: &Schedule,
    job_id: &str,
    now: DateTime<Utc>,
    last_fire: &parking_lot::Mutex<HashMap<String, DateTime<Utc>>>,
) -> bool {
    let guard = last_fire.lock();
    let after = guard
        .get(job_id)
        .copied()
        .unwrap_or(now - chrono::Duration::seconds(TICK_INTERVAL.as_secs() as i64 + 1));
    drop(guard);

    schedule.after(&after).take(1).any(|next| next <= now)
}

// ── Config persistence commands ──────────────────────────────────

pub(crate) fn load_config() -> SchedulerConfig {
    crate::config::load_json_config(CONFIG_FILE)
}

pub(crate) fn save_config(config: &SchedulerConfig) -> Result<(), String> {
    for job in &config.jobs {
        job.parse_schedule()?;
    }
    crate::config::ConfigFile::<SchedulerConfig>::new(CONFIG_FILE).save(config)
}

fn has_enabled_jobs(config: &SchedulerConfig) -> bool {
    config.jobs.iter().any(|job| job.enabled)
}

/// Spawn the tick loop if it isn't already running and the config has at
/// least one enabled job. Idempotent — safe to call at boot and after every
/// config save. A config with zero enabled jobs (the common case for most
/// installs, which never touch scheduling) spawns nothing instead of ticking
/// every 30s and re-reading `ai-cron.json` from disk for the process lifetime.
pub(crate) fn ensure_running(state: &Arc<AppState>) {
    if !has_enabled_jobs(&load_config()) {
        return;
    }
    if state
        .ai
        .scheduler_running
        .swap(true, std::sync::atomic::Ordering::AcqRel)
    {
        return; // already running
    }
    let sched_state = state.clone();
    let stop = state.ai.scheduler_stop.clone();
    tokio::spawn(async move {
        let scheduler = Scheduler::new(sched_state, stop);
        scheduler.run().await;
    });
}

/// Call after every `save_scheduler_config`: starts the loop if the new
/// config has an enabled job and it wasn't running, or stops it if the new
/// config has none and it was. No-op otherwise.
pub(crate) fn reconcile_after_config_change(state: &Arc<AppState>, config: &SchedulerConfig) {
    if has_enabled_jobs(config) {
        ensure_running(state);
    } else if state
        .ai
        .scheduler_running
        .swap(false, std::sync::atomic::Ordering::AcqRel)
    {
        state.ai.scheduler_stop.notify_one();
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_job(enabled: bool) -> ScheduledJob {
        ScheduledJob {
            id: "job1".into(),
            cron_expr: "0 0 * * * *".into(),
            goal: "test".into(),
            target_session: None,
            max_duration_secs: 300,
            enabled,
            one_shot: false,
        }
    }

    #[test]
    fn has_enabled_jobs_ignores_disabled_ones() {
        assert!(!has_enabled_jobs(&SchedulerConfig { jobs: vec![] }));
        assert!(!has_enabled_jobs(&SchedulerConfig {
            jobs: vec![sample_job(false)]
        }));
        assert!(has_enabled_jobs(&SchedulerConfig {
            jobs: vec![sample_job(false), sample_job(true)]
        }));
    }

    /// Serializes tests that mutate the global config-dir override, mirroring
    /// the lock in `ai_agent::knowledge::persist_tests` — both write
    /// per-test-isolated files under it, and cargo runs tests in parallel by
    /// default.
    static CONFIG_DIR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[tokio::test]
    async fn scheduler_starts_only_once_a_job_is_enabled_and_stops_once_none_are() {
        let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let _g = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let running = || {
            state
                .ai
                .scheduler_running
                .load(std::sync::atomic::Ordering::Acquire)
        };

        // No ai-cron.json on disk yet — load_config() defaults to zero jobs.
        ensure_running(&state);
        assert!(
            !running(),
            "must not spawn the 30s tick loop when there are zero enabled jobs"
        );

        let with_job = SchedulerConfig {
            jobs: vec![sample_job(true)],
        };
        save_config(&with_job).unwrap();
        reconcile_after_config_change(&state, &with_job);
        assert!(
            running(),
            "must spawn the tick loop once a config with an enabled job is saved"
        );

        // Calling again while already running must not double-spawn — the
        // swap in ensure_running short-circuits, which this exercises via the
        // idempotent public entry point rather than reaching into internals.
        reconcile_after_config_change(&state, &with_job);
        assert!(running(), "must stay running while a job is still enabled");

        let empty = SchedulerConfig::default();
        save_config(&empty).unwrap();
        reconcile_after_config_change(&state, &empty);
        assert!(
            !running(),
            "must stop the tick loop once the last enabled job is removed"
        );
    }

    #[test]
    fn parse_valid_cron() {
        let job = ScheduledJob {
            id: "test".into(),
            cron_expr: "0 0 * * * *".into(),
            goal: "run tests".into(),
            target_session: None,
            max_duration_secs: 300,
            enabled: true,
            one_shot: false,
        };
        assert!(job.parse_schedule().is_ok());
    }

    #[test]
    fn parse_invalid_cron() {
        let job = ScheduledJob {
            id: "bad".into(),
            cron_expr: "not a cron".into(),
            goal: String::new(),
            target_session: None,
            max_duration_secs: 300,
            enabled: true,
            one_shot: false,
        };
        assert!(job.parse_schedule().is_err());
    }

    #[test]
    fn should_fire_when_due() {
        let schedule = Schedule::from_str("* * * * * *").unwrap(); // every second
        let last = parking_lot::Mutex::new(HashMap::new());
        let now = Utc::now();
        assert!(should_fire(&schedule, "j1", now, &last));
    }

    #[test]
    fn should_not_fire_when_recently_fired() {
        let schedule = Schedule::from_str("0 0 * * * *").unwrap(); // top of every hour
        let last = parking_lot::Mutex::new(HashMap::new());
        let now = Utc::now();
        last.lock().insert("j1".into(), now);
        // Just fired — next occurrence is ~1h away, so shouldn't fire
        assert!(!should_fire(&schedule, "j1", now, &last));
    }

    #[test]
    fn disabled_job_skipped_in_config() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "off".into(),
                cron_expr: "* * * * * *".into(),
                goal: "noop".into(),
                target_session: None,
                max_duration_secs: 300,
                enabled: false,
                one_shot: false,
            }],
        };
        assert!(!config.jobs[0].enabled);
    }

    #[test]
    fn serde_round_trip() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "build".into(),
                cron_expr: "0 0 * * * *".into(),
                goal: "cargo build".into(),
                target_session: Some("sess-1".into()),
                max_duration_secs: 600,
                enabled: true,
                one_shot: false,
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: SchedulerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.jobs.len(), 1);
        assert_eq!(loaded.jobs[0].id, "build");
        assert_eq!(loaded.jobs[0].max_duration_secs, 600);
    }

    #[test]
    fn defaults_on_deserialize() {
        let json = r#"{"jobs":[{"id":"x","cron_expr":"0 0 * * * *","goal":"test"}]}"#;
        let config: SchedulerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.jobs[0].max_duration_secs, DEFAULT_MAX_DURATION_SECS);
        assert!(config.jobs[0].enabled);
        assert!(config.jobs[0].target_session.is_none());
    }

    #[test]
    fn save_rejects_invalid_cron() {
        let config = SchedulerConfig {
            jobs: vec![ScheduledJob {
                id: "bad".into(),
                cron_expr: "invalid".into(),
                goal: "test".into(),
                target_session: None,
                max_duration_secs: 300,
                enabled: true,
                one_shot: false,
            }],
        };
        assert!(save_config(&config).is_err());
    }
}
