//! Native sleep assertions follow backend work, never WebView activity.
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::state::{AppState, SessionState};

pub struct SleepBlocker(Mutex<Option<keepawake::KeepAwake>>);

impl SleepBlocker {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn reconcile(&self, state: &AppState) -> Result<(), String> {
        self.reconcile_with(
            state,
            Instant::now,
            crate::pty::process_tree_snapshot,
            |guard, requested| {
                reconcile_guard(guard, requested, || {
                    keepawake::Builder::default()
                        .display(false)
                        .idle(true)
                        .sleep(true)
                        .reason("Agent session active")
                        .app_name("TUICommander")
                        .app_reverse_domain("com.tuicommander.app")
                        .create()
                        .map_err(|error| format!("Failed to block sleep: {error}"))
                })
            },
        )
    }

    fn reconcile_with(
        &self,
        state: &AppState,
        _now: impl Fn() -> Instant,
        enumerate: impl FnOnce() -> Option<Vec<crate::pty::ProcessTreeEntry>>,
        transition: impl FnOnce(&mut Option<keepawake::KeepAwake>, bool) -> Result<(), String>,
    ) -> Result<(), String> {
        // Serialize the observation and native transition. A late frontend
        // refresh cannot release an assertion acquired by a newer observation.
        let mut guard = self.0.lock().map_err(|error| error.to_string())?;
        let requested = !crate::agent_test::active() && sleep_requested(state, enumerate);
        transition(&mut guard, requested)
    }
}

fn reconcile_guard<G>(
    guard: &mut Option<G>,
    requested: bool,
    acquire: impl FnOnce() -> Result<G, String>,
) -> Result<(), String> {
    if !requested {
        *guard = None;
    } else if guard.is_none() {
        *guard = Some(acquire()?);
    }
    Ok(())
}

fn session_has_live_work(session: &SessionState, keep_open: bool) -> bool {
    session.shell_state.as_deref() == Some("busy")
        || (session.agent_type.is_some()
            && (keep_open
                || session.background_work
                || session.has_pending_background_probe()
                || matches!(session.agent_state.as_deref(), Some("working" | "starting"))))
}

fn sleep_requested(
    state: &AppState,
    enumerate: impl FnOnce() -> Option<Vec<crate::pty::ProcessTreeEntry>>,
) -> bool {
    if !state.config.read().prevent_sleep_when_busy {
        return false;
    }
    if state.acp.has_active_work()
        || state
            .keep_open_sessions
            .iter()
            .any(|identity| state.acp.peer_conversation(identity.key()).is_some())
    {
        return true;
    }
    let identities = crate::mcp_http::session::live_tuic_sessions_by_pty(state);
    // Snapshot live keys first; never hold a sessions shard guard while taking
    // the foreground process lock through the process-tree adapter.
    let sessions: Vec<String> = state
        .session_maps
        .sessions
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    let mut inventory = None;
    let mut enumerate = Some(enumerate);
    for id in sessions {
        let Some(session) = state.session_state_with_shell(&id) else {
            continue;
        };
        let identity = identities.get(&id).unwrap_or(&id);
        if session_has_live_work(
            &session,
            state.keep_open_sessions.contains(&id) || state.keep_open_sessions.contains(identity),
        ) {
            return true;
        }
        if let Some(agent_type) = session.agent_type.as_deref() {
            // Idle UI state is not proof of no work. Detached bg runners and
            // newly-started quiet descendants may not have changed that state.
            let processes = inventory
                .get_or_insert_with(|| enumerate.take().expect("one inventory per reconcile")());
            let Some(processes) = processes else {
                return true; // Unknown process state must not drop the hold.
            };
            if crate::pty::sleep_work_in_snapshot(state, &id, identity, agent_type, processes) {
                return true;
            }
        }
    }
    false
}

/// A process-owned thread survives hidden, unloaded and blocked WebViews.
/// Weak state ownership lets shutdown drop the native guard normally.
pub(crate) fn spawn(state: &Arc<AppState>, blocker: Arc<SleepBlocker>) -> std::io::Result<()> {
    if crate::agent_test::active() {
        return Ok(());
    }
    let state = Arc::downgrade(state);
    std::thread::Builder::new()
        .name("sleep-prevention".into())
        .spawn(move || {
            let mut last_error = None;
            while let Some(state) = state.upgrade() {
                match blocker.reconcile(&state) {
                    Ok(()) => last_error = None,
                    Err(error) => {
                        if last_error.as_ref() != Some(&error) {
                            tracing::warn!(%error, "Backend sleep prevention failed; will retry");
                        }
                        last_error = Some(error);
                    }
                }
                drop(state);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        })?;
    Ok(())
}

/// Refresh backend-owned sleep prevention. Neither legacy command overrides
/// live work or the persisted user setting.
#[tauri::command]
pub(crate) async fn block_sleep(
    blocker: tauri::State<'_, Arc<SleepBlocker>>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let blocker = blocker.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || blocker.reconcile(&state))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn unblock_sleep(
    blocker: tauri::State<'_, Arc<SleepBlocker>>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let blocker = blocker.inner().clone();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || blocker.reconcile(&state))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LiveIdleAgent(AppState);

    impl LiveIdleAgent {
        fn new() -> Self {
            let state = crate::state::tests_support::make_test_app_state();
            crate::state::tests_support::insert_dummy_session(&state, "idle");
            state.session_maps.session_states.insert(
                "idle".into(),
                SessionState {
                    agent_type: Some("ego".into()),
                    agent_state: Some("idle".into()),
                    ..Default::default()
                },
            );
            state.session_maps.shell_states.insert(
                "idle".into(),
                std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
            );
            state.config.write().prevent_sleep_when_busy = true;
            Self(state)
        }
    }

    impl Drop for LiveIdleAgent {
        fn drop(&mut self) {
            if let Some((_, session)) = self.0.session_maps.sessions.remove("idle") {
                let mut session = session.lock();
                let _ = session._child.kill();
                let _ = session._child.wait();
            }
        }
    }

    // Catches: idle ticks and legacy refreshes each enumerate the full process table.
    #[test]
    fn idle_sleep_reconcile_bounds_process_inventory_reads() {
        let fixture = LiveIdleAgent::new();
        let blocker = SleepBlocker::new();
        let start = Instant::now();
        let mut reads = 0;
        for second in [0, 0, 1, 2, 3, 4] {
            blocker
                .reconcile_with(
                    &fixture.0,
                    || start + std::time::Duration::from_secs(second),
                    || {
                        reads += 1;
                        None
                    },
                    |_, requested| {
                        assert!(requested, "unknown inventory must retain power");
                        Ok(())
                    },
                )
                .unwrap();
        }
        assert_eq!(
            reads, 1,
            "idle reconciliation must share one five-second inventory attempt"
        );
        blocker
            .reconcile_with(
                &fixture.0,
                || start + std::time::Duration::from_secs(5),
                || {
                    reads += 1;
                    None
                },
                |_, requested| {
                    assert!(requested);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(reads, 2, "retry unknown inventory at the cadence boundary");
    }

    #[test]
    fn idle_pty_background_work_retains_sleep_assertion() {
        let session = crate::state::SessionState {
            agent_type: Some("ego".into()),
            shell_state: Some("idle".into()),
            agent_state: Some("idle".into()),
            background_work: true,
            ..Default::default()
        };
        assert!(
            session_has_live_work(&session, false),
            "idle composer must not release sleep prevention while its background job remains live"
        );
    }

    #[test]
    fn idle_and_awaiting_agents_do_not_hold_power_without_live_work() {
        for agent_state in ["idle", "completed", "awaiting_input"] {
            let session = SessionState {
                agent_type: Some("claude".into()),
                shell_state: Some("idle".into()),
                agent_state: Some(agent_state.into()),
                ..Default::default()
            };
            assert!(!session_has_live_work(&session, false), "{agent_state}");
            assert!(
                session_has_live_work(&session, true),
                "keep_open {agent_state}"
            );
        }
    }

    #[test]
    fn pending_descendant_probe_and_working_turn_cannot_drop_power() {
        let mut session = SessionState {
            agent_type: Some("ego".into()),
            shell_state: Some("idle".into()),
            turn_epoch: 7,
            background_probe_turn_epoch: Some(7),
            ..Default::default()
        };
        assert!(session_has_live_work(&session, false));
        session.background_probe_turn_epoch = Some(6);
        assert!(
            !session_has_live_work(&session, false),
            "old-turn probe is not work"
        );
        for activity in ["working", "starting"] {
            session.agent_state = Some(activity.into());
            assert!(session_has_live_work(&session, false), "{activity}");
        }
        session.agent_state = None;
        session.agent_type = None;
        assert!(
            !session_has_live_work(&session, true),
            "retained keep_open is not a live agent"
        );
        session.shell_state = Some("busy".into());
        assert!(
            session_has_live_work(&session, false),
            "non-agent shell command"
        );
    }

    #[test]
    fn disabled_setting_and_closed_session_metadata_cannot_hold_power() {
        let state = crate::state::tests_support::make_test_app_state();
        state.session_maps.session_states.insert(
            "closed".into(),
            SessionState {
                agent_type: Some("ego".into()),
                background_work: true,
                ..Default::default()
            },
        );
        state.keep_open_sessions.insert("closed".into());
        state.config.write().prevent_sleep_when_busy = true;
        assert!(
            !sleep_requested(&state, crate::pty::process_tree_snapshot),
            "removed PTY is not live work"
        );
        state.config.write().prevent_sleep_when_busy = false;
        assert!(
            !sleep_requested(&state, crate::pty::process_tree_snapshot),
            "user opt-out wins"
        );
    }

    #[test]
    fn native_guard_is_retained_until_backend_work_ends_and_failed_acquire_retries() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let mut guard = None;
        assert!(
            reconcile_guard(&mut guard, true, || Err::<Guard, _>("unavailable".into())).is_err()
        );
        assert!(guard.is_none());
        reconcile_guard(&mut guard, true, || Ok(Guard(drops.clone()))).unwrap();
        reconcile_guard(&mut guard, true, || panic!("duplicate native assertion")).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        reconcile_guard(&mut guard, false, || panic!("release must not acquire")).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}
