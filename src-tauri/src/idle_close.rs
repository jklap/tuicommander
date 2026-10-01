//! Delayed cleanup for managed agent terminals.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    state: String,
    turn_epoch: u64,
    last_input_ms: u64,
    last_output_ms: u64,
    last_mail_id: Option<String>,
    read_cursor: u64,
}

#[derive(Default)]
pub(crate) struct IdleCloseTracker {
    seen: HashMap<String, (Observation, u64)>,
}

impl IdleCloseTracker {
    fn observe(
        &mut self,
        session_id: &str,
        now_ms: u64,
        delay_ms: u64,
        observation: Option<Observation>,
    ) -> bool {
        let Some(observation) = observation.filter(|_| delay_ms > 0) else {
            self.seen.remove(session_id);
            return false;
        };
        match self.seen.get_mut(session_id) {
            Some((old, since)) if *old == observation => now_ms.saturating_sub(*since) >= delay_ms,
            Some(entry) => {
                *entry = (observation, now_ms);
                false
            }
            None => {
                self.seen
                    .insert(session_id.to_owned(), (observation, now_ms));
                false
            }
        }
    }
}

fn live_bg_runner_for_session(commands: &[String], session_id: &str) -> bool {
    commands.iter().any(|command| {
        let mut words = command.split_whitespace();
        words.any(|word| word == "__bg-runner")
            && command
                .split_whitespace()
                .take_while(|word| *word != "--")
                .any(|word| word == session_id)
    })
}

pub(crate) fn wake_marker_path(session_id: &str) -> std::path::PathBuf {
    crate::config::config_dir()
        .join("bg-wakes")
        .join(format!("{session_id}.json"))
}

fn bg_wake_blocks_close(session_id: &str) -> bool {
    let path = wake_marker_path(session_id);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            tracing::warn!(%session_id, %error, "idle-close cannot read bg wake marker");
            return true;
        }
    };
    let Ok(marker) = serde_json::from_str::<serde_json::Value>(&content) else {
        tracing::warn!(%session_id, "idle-close found malformed bg wake marker");
        return true;
    };
    if marker["session_id"].as_str() != Some(session_id) {
        tracing::warn!(%session_id, "idle-close found mismatched bg wake marker");
        return true;
    }
    matches!(marker["status"].as_str(), Some("failed" | "retrying"))
}

/// Nobody can answer a child whose parent has no PTY and no MCP session left.
fn parent_alive(state: &AppState, child: &str) -> bool {
    let Some(parent) = state.session_maps.session_parent.get(child) else {
        return false;
    };
    state.session_maps.sessions.contains_key(parent.value())
        || state
            .mcp
            .session_to_mcp
            .get(parent.value())
            .is_some_and(|sessions| !sessions.is_empty())
}

fn observation(state: &AppState, session_id: &str) -> Option<(Observation, u64)> {
    let parent = state.session_maps.session_parent.get(session_id)?.clone();
    if !state.session_maps.sessions.contains_key(session_id)
        || crate::mcp_http::mcp_transport::is_pending_parent(&parent)
        || state.keep_open_sessions.contains(session_id)
        || state.blocked_children.contains(session_id)
    {
        return None;
    }
    let snapshot = state.session_state_with_shell(session_id)?;
    if !matches!(snapshot.agent_state.as_deref(), Some("idle" | "completed"))
        || snapshot.background_work
    {
        return None;
    }
    let cursor = state
        .agent_read_cursor
        .get(session_id)
        .map(|v| *v)
        .unwrap_or(0);
    let inbox = state.agent_inbox.get(session_id);
    if inbox
        .as_ref()
        .is_some_and(|mail| mail.iter().any(|message| message.timestamp > cursor))
        || bg_wake_blocks_close(session_id)
    {
        return None;
    }
    let delay_minutes = crate::config::load_agents_config()
        .agents
        .get(snapshot.agent_type.as_deref()?)
        .map(|settings| settings.idle_close_minutes)
        .unwrap_or(crate::config::DEFAULT_IDLE_CLOSE_MINUTES);
    Some((
        Observation {
            state: snapshot.agent_state?,
            turn_epoch: snapshot.turn_epoch,
            last_input_ms: state
                .session_maps
                .last_input_ms
                .get(session_id)
                .map(|v| v.load(Ordering::Relaxed))
                .unwrap_or(0),
            last_output_ms: state
                .session_maps
                .last_output_ms
                .get(session_id)
                .map(|v| v.load(Ordering::Relaxed))
                .unwrap_or(0),
            last_mail_id: inbox
                .as_ref()
                .and_then(|mail| mail.back().map(|message| message.id.clone())),
            read_cursor: cursor,
        },
        u64::from(delay_minutes).saturating_mul(60_000),
    ))
}

fn sweep_with_snapshot(
    state: &Arc<AppState>,
    tracker: &mut IdleCloseTracker,
    now_ms: u64,
    mut snapshot: impl FnMut() -> Option<Vec<String>>,
) {
    let children: Vec<String> = state
        .session_maps
        .session_parent
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    tracker
        .seen
        .retain(|session_id, _| children.contains(session_id));
    state
        .blocked_children
        .retain(|session_id| children.contains(session_id) && parent_alive(state, session_id));
    let mut runner_commands: Option<Option<Vec<String>>> = None;
    for session_id in children {
        let candidate = observation(state, &session_id);
        let (observed, delay) = match candidate {
            Some(value) => value,
            None => {
                tracker.observe(&session_id, now_ms, 0, None);
                continue;
            }
        };
        if !tracker.observe(&session_id, now_ms, delay, Some(observed.clone())) {
            continue;
        }
        // Cheap state may have changed while the timer matured; avoid a process
        // scan unless this child still qualifies for closure.
        if observation(state, &session_id)
            .as_ref()
            .is_none_or(|(current, _)| current != &observed)
        {
            continue;
        }
        let Some(commands) = runner_commands.get_or_insert_with(&mut snapshot).as_ref() else {
            // A failed inventory cannot establish that no detached runner exists.
            continue;
        };
        if live_bg_runner_for_session(commands, &session_id) {
            tracker.seen.remove(&session_id);
            continue;
        }
        // Recheck mail, output, marker and state after the process inventory.
        if observation(state, &session_id)
            .as_ref()
            .is_none_or(|(current, _)| current != &observed)
        {
            continue;
        }
        let name = state
            .peer_agents
            .get(&session_id)
            .map(|peer| peer.name.clone())
            .unwrap_or_else(|| session_id.clone());
        crate::pty::push_state_change_to_parent(
            state,
            &session_id,
            serde_json::json!({
                "type": "state_change", "state": "closed", "reason": "idle_timeout",
                "session_id": session_id, "name": name,
            }),
        );
        crate::mcp_http::mcp_transport::close_idle_managed_session(state, &session_id);
        tracker.seen.remove(&session_id);
    }
}

#[cfg(test)]
fn sweep_with_commands(
    state: &Arc<AppState>,
    tracker: &mut IdleCloseTracker,
    now_ms: u64,
    runner_commands: &[String],
) {
    sweep_with_snapshot(state, tracker, now_ms, || Some(runner_commands.to_vec()));
}

async fn sweep_step(
    state: Arc<AppState>,
    mut tracker: IdleCloseTracker,
    now_ms: u64,
    snapshot: impl FnMut() -> Option<Vec<String>> + Send + 'static,
) -> IdleCloseTracker {
    match tokio::task::spawn_blocking(move || {
        sweep_with_snapshot(&state, &mut tracker, now_ms, snapshot);
        tracker
    })
    .await
    {
        Ok(tracker) => tracker,
        Err(error) => {
            tracing::error!("idle-close sweep failed: {error}");
            IdleCloseTracker::default()
        }
    }
}

pub(crate) fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut tracker = IdleCloseTracker::default();
        loop {
            interval.tick().await;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            tracker = sweep_step(
                Arc::clone(&state),
                tracker,
                now_ms,
                crate::pty::live_bg_runner_commands,
            )
            .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn live_child(state: &Arc<AppState>, session_id: &str, worktree: std::path::PathBuf) {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open PTY");
        let child = pair
            .slave
            .spawn_command(CommandBuilder::new("true"))
            .expect("start child");
        let writer = pair.master.take_writer().expect("PTY writer");
        state.session_maps.sessions.insert(
            session_id.into(),
            parking_lot::Mutex::new(crate::state::PtySession {
                writer: Arc::new(parking_lot::Mutex::new(writer)),
                master: pair.master,
                _child: child,
                paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                worktree: Some(crate::state::WorktreeInfo {
                    name: "child".into(),
                    path: worktree.clone(),
                    branch: Some("child".into()),
                    base_repo: worktree,
                }),
                cwd: None,
                display_name: Some("worker".into()),
                display_name_is_custom: false,
                display_name_from_spawn: true,
                is_remote: false,
                shell: "true".into(),
            }),
        );
        state.session_maps.session_states.insert(
            session_id.into(),
            crate::state::SessionState {
                agent_type: Some("claude".into()),
                suggested_actions: Some(vec![]),
                ..Default::default()
            },
        );
    }

    /// The parent is an MCP peer without a PTY, as an orchestrator usually is.
    fn live_parent(state: &Arc<AppState>) {
        state
            .mcp
            .session_to_mcp
            .insert("parent".into(), vec!["parent-mcp".into()]);
    }

    fn idle() -> Observation {
        Observation {
            state: "idle".into(),
            turn_epoch: 1,
            last_input_ms: 100,
            last_output_ms: 200,
            last_mail_id: None,
            read_cursor: 0,
        }
    }

    #[test]
    fn idle_managed_child_closes_at_fifteen_minutes_and_activity_restarts_window() {
        let mut tracker = IdleCloseTracker::default();
        assert!(!tracker.observe("child", 0, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 899_999, 900_000, Some(idle())));
        let mut after_output = idle();
        after_output.last_output_ms = 300;
        assert!(!tracker.observe("child", 899_999, 900_000, Some(after_output.clone())));
        assert!(!tracker.observe("child", 1_799_998, 900_000, Some(after_output.clone())));
        assert!(tracker.observe("child", 1_799_999, 900_000, Some(after_output)));
    }

    #[test]
    fn active_work_or_mail_or_keep_open_never_ages_into_close() {
        let mut tracker = IdleCloseTracker::default();
        assert!(!tracker.observe("child", 0, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 1_000_000, 900_000, None));
        assert!(!tracker.observe("child", 1_000_001, 900_000, Some(idle())));
        assert!(!tracker.observe("child", 1_000_002, 0, Some(idle())));
    }

    #[test]
    fn detached_bg_job_owned_by_child_blocks_idle_close_until_runner_exits() {
        let child = "9023a0e6-229b-4322-ba92-93507eb00d15";
        let other = "917ec723-27e5-47fb-8a6a-801f65af6cf6";
        let commands = vec![format!(
            "/usr/local/bin/tuic __bg-runner /some/log {child} -- cargo test"
        )];
        assert!(live_bg_runner_for_session(&commands, child));
        assert!(!live_bg_runner_for_session(&commands, other));
        assert!(!live_bg_runner_for_session(&[], child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_skips_process_snapshot_until_managed_child_reaches_deadline() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-snapshot-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let mut tracker = IdleCloseTracker::default();
        let snapshots = std::cell::Cell::new(0);
        let mut count_snapshot = || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![])
        };
        sweep_with_snapshot(&state, &mut tracker, 0, &mut count_snapshot);
        assert_eq!(snapshots.get(), 0, "no managed child needs a process scan");

        live_child(&state, "managed-child", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("managed-child".into(), "parent".into());
        state
            .session_maps
            .session_states
            .get_mut("managed-child")
            .unwrap()
            .background_work = true;
        sweep_with_snapshot(&state, &mut tracker, 900_000, &mut count_snapshot);
        assert_eq!(
            snapshots.get(),
            0,
            "background work is not an idle-close candidate"
        );
        state
            .session_maps
            .session_states
            .get_mut("managed-child")
            .unwrap()
            .background_work = false;
        sweep_with_snapshot(&state, &mut tracker, 900_000, &mut count_snapshot);
        sweep_with_snapshot(&state, &mut tracker, 1_799_999, &mut count_snapshot);
        assert_eq!(
            snapshots.get(),
            0,
            "an idle child is not due before its delay"
        );
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "managed-child");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_rechecks_live_runner_at_deadline_before_closing() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-runner-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let mut tracker = IdleCloseTracker::default();
        let snapshots = std::cell::Cell::new(0);
        sweep_with_snapshot(&state, &mut tracker, 0, || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![])
        });
        assert_eq!(
            snapshots.get(),
            0,
            "runner inventory waits until the close deadline"
        );
        sweep_with_snapshot(&state, &mut tracker, 900_000, || {
            snapshots.set(snapshots.get() + 1);
            Some(vec![format!("tuic __bg-runner /log {child} -- cargo test")])
        });
        assert_eq!(
            snapshots.get(),
            1,
            "the destructive step checks live runners"
        );
        assert!(state.session_maps.sessions.contains_key(child));
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, child);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_sweep_resumes_after_a_panicking_process_snapshot() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-panic-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let tracker = sweep_step(Arc::clone(&state), IdleCloseTracker::default(), 0, || {
            Some(vec![])
        })
        .await;
        let tracker = sweep_step(Arc::clone(&state), tracker, 900_000, || {
            panic!("injected process inventory failure")
        })
        .await;
        assert!(state.session_maps.sessions.contains_key(child));
        let tracker = sweep_step(Arc::clone(&state), tracker, 900_001, || Some(vec![])).await;
        sweep_step(Arc::clone(&state), tracker, 1_800_001, || Some(vec![])).await;
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn only_managed_child_closes_after_delay_and_parent_gets_reason_while_worktree_stays() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let root = crate::test_support::test_temp_root();
        let temp = tempfile::Builder::new()
            .prefix("idle-close-")
            .tempdir_in(root)
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let worktree = temp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let marker = worktree.join("preserve.txt");
        std::fs::write(&marker, "committed work").unwrap();
        live_child(&state, "managed-child", worktree.clone());
        live_child(&state, "manual-session", worktree);
        live_child(&state, "pending-parent", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("managed-child".into(), "parent".into());
        state
            .session_maps
            .session_parent
            .insert("pending-parent".into(), "pending-mcp:unbound".into());
        state.peer_agents.insert(
            "managed-child".into(),
            crate::state::PeerAgent {
                tuic_session: "managed-child".into(),
                mcp_session_id: String::new(),
                name: "worker".into(),
                project: None,
                registered_at: 0,
            },
        );
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 899_999, &[]);
        assert!(state.session_maps.sessions.contains_key("managed-child"));
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(!state.session_maps.sessions.contains_key("managed-child"));
        assert!(state.session_maps.sessions.contains_key("manual-session"));
        assert!(state.session_maps.sessions.contains_key("pending-parent"));
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "committed work");
        let notices = state.agent_inbox.get("parent").unwrap();
        let notice: serde_json::Value =
            serde_json::from_str(&notices.back().unwrap().content).unwrap();
        assert_eq!(notice["reason"], "idle_timeout");
        assert_eq!(notice["name"], "worker");
        drop(notices);
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "manual-session");
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "pending-parent");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mail_background_work_keep_open_and_detached_runner_each_prevent_close() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let root = crate::test_support::test_temp_root();
        let temp = tempfile::Builder::new()
            .prefix("idle-close-")
            .tempdir_in(root)
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        live_child(&state, "child-with-work", temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert("child-with-work".into(), "parent".into());
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        state.keep_open_sessions.insert("child-with-work".into());
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        state.keep_open_sessions.remove("child-with-work");
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        state
            .session_maps
            .session_states
            .get_mut("child-with-work")
            .unwrap()
            .background_work = true;
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        state
            .session_maps
            .session_states
            .get_mut("child-with-work")
            .unwrap()
            .background_work = false;
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        let runner = vec!["tuic __bg-runner /log child-with-work -- make check".into()];
        sweep_with_commands(&state, &mut tracker, 2_700_000, &runner);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        state.push_agent_inbox(
            "child-with-work",
            crate::state::AgentMessage {
                id: "mail".into(),
                from_tuic_session: "parent".into(),
                from_name: "parent".into(),
                content: "follow up".into(),
                timestamp: 2_700_001,
                delivered_via_channel: false,
            },
        );
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(state.session_maps.sessions.contains_key("child-with-work"));
        crate::mcp_http::mcp_transport::close_idle_managed_session(&state, "child-with-work");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_whose_last_mail_to_parent_is_blocked_stays_open_until_a_newer_mail() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-blocked-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "managed-child";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        live_parent(&state);
        let mail = |from: &str, content: &str, timestamp| crate::state::AgentMessage {
            id: format!("mail-{timestamp}"),
            from_tuic_session: from.into(),
            from_name: from.into(),
            content: content.into(),
            timestamp,
            delivered_via_channel: false,
        };
        state.push_agent_inbox("parent", mail(child, "BLOCKED: rb box unreachable", 10));
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        assert!(
            state.session_maps.sessions.contains_key(child),
            "a BLOCKED child waiting for its parent must not be idle-closed"
        );
        state.push_agent_inbox(child, mail("parent", "box is back", 20));
        state.agent_read_cursor.insert(child.into(), 20);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        sweep_with_commands(&state, &mut tracker, 3_600_000, &[]);
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_or_retrying_bg_wake_keeps_child_open_until_a_successful_wake() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = tempfile::Builder::new()
            .prefix("idle-close-wake-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        let child = "9023a0e6-229b-4322-ba92-93507eb00d15";
        live_child(&state, child, temp.path().to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
        let wakes = crate::config::config_dir().join("bg-wakes");
        std::fs::create_dir_all(&wakes).unwrap();
        let marker = wakes.join(format!("{child}.json"));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"failed"}}"#),
        )
        .unwrap();
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(&state, &mut tracker, 0, &[]);
        sweep_with_commands(&state, &mut tracker, 900_000, &[]);
        assert!(state.session_maps.sessions.contains_key(child));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"retrying"}}"#),
        )
        .unwrap();
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        assert!(state.session_maps.sessions.contains_key(child));
        std::fs::write(
            &marker,
            format!(r#"{{"session_id":"{child}","status":"queued"}}"#),
        )
        .unwrap();
        sweep_with_commands(&state, &mut tracker, 1_800_000, &[]);
        sweep_with_commands(&state, &mut tracker, 2_700_000, &[]);
        assert!(!state.session_maps.sessions.contains_key(child));
    }

    // ---- critic-1319: adversarial cases for the BLOCKED hold and the close reason ----

    #[cfg(unix)]
    fn critic_mail(
        id: &str,
        from: &str,
        content: &str,
        timestamp: u64,
    ) -> crate::state::AgentMessage {
        crate::state::AgentMessage {
            id: id.into(),
            from_tuic_session: from.into(),
            from_name: from.into(),
            content: content.into(),
            timestamp,
            delivered_via_channel: false,
        }
    }

    #[cfg(unix)]
    fn critic_child(state: &Arc<AppState>, child: &str, dir: &std::path::Path) {
        live_parent(state);
        live_child(state, child, dir.to_path_buf());
        state
            .session_maps
            .session_parent
            .insert(child.into(), "parent".into());
    }

    #[cfg(unix)]
    fn critic_temp() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("idle-close-critic-")
            .tempdir_in(crate::test_support::test_temp_root())
            .unwrap()
    }

    /// Two sweeps 15 minutes apart: the first starts the window, the second matures it.
    #[cfg(unix)]
    fn critic_sweep_to_maturity(state: &Arc<AppState>) {
        let mut tracker = IdleCloseTracker::default();
        sweep_with_commands(state, &mut tracker, 0, &[]);
        sweep_with_commands(state, &mut tracker, 900_000, &[]);
    }

    #[derive(Clone)]
    struct CriticLogSink(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for CriticLogSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CriticLogSink {
        type Writer = CriticLogSink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Catches: idle close still logging `close_requested` (or the reason being dropped),
    /// so the cause of an idle kill is invisible in the log (criterion 3).
    #[cfg(unix)]
    #[test]
    fn idle_close_of_an_unblocked_child_logs_reason_idle_close() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "quiet-child", temp.path());
        let output = Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(CriticLogSink(output.clone()))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || critic_sweep_to_maturity(&state));
        assert!(!state.session_maps.sessions.contains_key("quiet-child"));
        let log = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(log.contains("Closing session"), "{log}");
        assert!(log.contains("reason=\"idle_close\""), "{log}");
        assert!(!log.contains("close_requested"), "{log}");
    }

    /// Catches: the hold keying on any earlier BLOCKED instead of the LAST mail, so a
    /// child that reported BLOCKED and later finished with RESULT is kept open for ever.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_that_mailed_blocked_then_result_is_closed() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox("parent", critic_mail("m2", "c", "RESULT: done", 20));
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("c"));
    }

    /// Catches: a lifecycle notice (`tuic-auto-*`) that TUIC itself files for the child
    /// after its BLOCKED being treated as the child's last mail, so the hold is lost and
    /// the blocked child is closed.
    #[cfg(unix)]
    #[tokio::test]
    async fn lifecycle_notice_after_blocked_does_not_release_the_hold() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox(
            "parent",
            critic_mail("tuic-auto-state-1", "c", "state idle", 20),
        );
        critic_sweep_to_maturity(&state);
        assert!(state.session_maps.sessions.contains_key("c"));
    }

    /// Catches: the hold matching on the parent's inbox without filtering by sender, so
    /// one child's BLOCKED keeps every sibling open.
    #[cfg(unix)]
    #[tokio::test]
    async fn sibling_blocked_does_not_hold_a_child_with_no_mail() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "quiet", temp.path());
        state.push_agent_inbox(
            "parent",
            critic_mail("m1", "other", "BLOCKED: box down", 10),
        );
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("quiet"));
    }

    /// Catches: the hold living only in the parent's bounded inbox (capacity 100): once
    /// the BLOCKED mail is evicted by later traffic from other children, the blocked
    /// child is closed while it still waits for an answer.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_hold_survives_parent_inbox_overflow_from_other_senders() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        for index in 0..crate::state::AGENT_INBOX_CAPACITY {
            state.push_agent_inbox(
                "parent",
                critic_mail(
                    &format!("noise-{index}"),
                    "other",
                    "RESULT: x",
                    11 + index as u64,
                ),
            );
        }
        critic_sweep_to_maturity(&state);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "BLOCKED mail was evicted from the parent inbox and the child was closed"
        );
    }

    /// Catches: comparing the parent inbox's logical clock with the child inbox's clock.
    /// A burst of same-millisecond mail makes the parent clock run ahead of wall time, so
    /// a reply stamped with the real time is judged older than BLOCKED and never releases.
    #[cfg(unix)]
    #[tokio::test]
    async fn reply_releases_the_hold_even_when_parent_clock_ran_ahead() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        for index in 0..20 {
            state.push_agent_inbox(
                "parent",
                critic_mail(&format!("burst-{index}"), "other", "RESULT: x", 1_000),
            );
        }
        // Stored as 1_020 by the per-recipient logical clock.
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 1_000));
        // The parent answers 5 ms later in wall time.
        state.push_agent_inbox("c", critic_mail("reply", "parent", "box is back", 1_005));
        state.agent_read_cursor.insert("c".into(), 1_005);
        critic_sweep_to_maturity(&state);
        assert!(
            !state.session_maps.sessions.contains_key("c"),
            "the parent replied, the hold must be released"
        );
    }

    // ---- critic-1319 round 2: lifecycle of the blocked_children set ----

    /// Catches: the hold surviving the child's PTY. The set is only swept by the idle
    /// sweep, so a session re-created under the same durable id before the next sweep
    /// inherits a stale BLOCKED hold and is never closed.
    #[cfg(unix)]
    #[tokio::test]
    async fn closing_a_blocked_child_drops_its_hold_before_the_id_is_reused() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        assert!(state.blocked_children.contains("c"));
        crate::pty::close_pty_core(&state, "c", false);
        critic_child(&state, "c", temp.path());
        critic_sweep_to_maturity(&state);
        assert!(
            !state.session_maps.sessions.contains_key("c"),
            "a new session under a reused id inherited the dead session's BLOCKED hold"
        );
    }

    /// Catches: any non-lifecycle mail to the child releasing the hold. A sibling's
    /// chatter is not the parent answering; once the child has read it, the child is
    /// still blocked on its parent and must stay open.
    #[cfg(unix)]
    #[tokio::test]
    async fn mail_from_a_sibling_does_not_release_the_blocked_hold() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.push_agent_inbox("c", critic_mail("s1", "sibling", "fyi", 20));
        state.agent_read_cursor.insert("c".into(), 20);
        critic_sweep_to_maturity(&state);
        assert!(
            state.session_maps.sessions.contains_key("c"),
            "a sibling's mail released a hold that only the parent can end"
        );
    }

    /// Catches: a dead parent leaving its blocked child held for ever, with nobody
    /// left to answer it.
    #[cfg(unix)]
    #[tokio::test]
    async fn blocked_child_of_a_dead_parent_is_closed_like_any_idle_child() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let temp = critic_temp();
        let _config = crate::config::set_config_dir_override(temp.path().join("config"));
        critic_child(&state, "c", temp.path());
        state.push_agent_inbox("parent", critic_mail("m1", "c", "BLOCKED: box down", 10));
        state.mcp.session_to_mcp.remove("parent");
        critic_sweep_to_maturity(&state);
        assert!(!state.session_maps.sessions.contains_key("c"));
    }
}
