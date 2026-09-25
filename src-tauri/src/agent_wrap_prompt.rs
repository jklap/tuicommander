//! Consent flow for wrapping a user's own `claude`/`codex`/`goose` shell
//! function with TUIC's launch-flag injection.
//!
//! `shell_integration.rs`'s deferred zsh integration detects when the user
//! already has their own function for one of these agents and the decision
//! is still undecided (`AgentSettings::wrap_user_function` is `None`) —
//! see that module's doc comment for the full mechanism. It reports this via
//! a new OSC 7770 `userwrap=<agent>` verb, handled in `pty.rs`, which calls
//! [`request`] here.
//!
//! [`request`]/[`resolve`] are deliberately NOT built on `McpConfirm`'s
//! oneshot/timeout shape (`state.rs`'s `AppEvent::McpConfirm`,
//! `mcp_http::resolve_mcp_confirm`): that mechanism collapses "explicit No"
//! and "dismissed without answering" into the same `bool`, which is exactly
//! the distinction this feature needs to keep — an explicit No persists
//! `Some(false)` forever, while a dismiss only snoozes for this app run and
//! leaves the setting `None`. Nothing here needs a caller blocked waiting on
//! the answer either, so there's no oneshot channel and no timeout.

use crate::state::{AppEvent, AppState};

/// Every zsh tab with an undecided setting fires this on its own precmd
/// bootstrap, so a workspace with many tabs open at once must not open the
/// same dialog once per tab — this enforces exactly one in-flight prompt per
/// agent type, app-wide.
///
/// Takes `&AppState` (not `&Arc<AppState>`) so it works unchanged from both
/// `pty.rs`'s `process_chunk` (which only ever has `&AppState`) and the
/// Tauri command/HTTP route call sites (which hold `Arc<AppState>` — `&arc`
/// coerces to `&AppState` at the call site via `Deref`).
pub(crate) fn request(state: &AppState, agent_type: &str) {
    // Another tab (or a prior run) already decided — nothing to ask.
    if crate::agent_hook_launch::wrap_user_function(agent_type).is_some() {
        return;
    }
    // Dismissed earlier this app run — snoozed until restart.
    if state.agent_wrap_snoozed.contains(agent_type) {
        return;
    }
    // Already pending from another tab's detection.
    if state.agent_wrap_pending.contains_key(agent_type) {
        return;
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    // Re-check-and-insert isn't atomic across the two DashMap operations
    // above and this one, so a concurrent second detection for the same
    // agent could still race past both checks — accepted: the loser's
    // `insert` below simply overwrites the winner's `request_id` with its
    // own, which only means the OLDER dialog's answer resolves nothing (a
    // no-op on an already-removed id — see `resolve`), not that two dialogs
    // ever end up open at once for the same client. A user reloading/
    // retrying the (now-orphaned) first dialog just gets nothing to happen;
    // there's still exactly one *visible* prompt per agent per client.
    state
        .agent_wrap_pending
        .insert(agent_type.to_string(), request_id.clone());
    state.emit_dual(AppEvent::AgentWrapPrompt {
        request_id,
        agent_type: agent_type.to_string(),
    });
}

/// Resolve a pending "wrap my shell function?" prompt and tell every client
/// to dismiss it. Shared by the Tauri command and the HTTP route so the two
/// transports cannot drift — same pattern as `resolve_mcp_confirm`.
///
/// `decision`: `Some(true)`/`Some(false)` persists the answer to
/// `AgentSettings::wrap_user_function` (future shell spawns only — see
/// `shell_integration.rs`'s scope note); `None` means the prompt was
/// dismissed without an answer, which persists nothing and snoozes the
/// agent for the rest of this app run instead.
///
/// An unknown or already-resolved `request_id` is ignored — every client
/// racing to answer the same request only lets one of them win, and a stale
/// answer for an id nothing is waiting on has nothing to do.
///
/// Returns `Err` when an explicit answer (`Some(_)`) failed to persist to
/// disk — the caller should surface this rather than let the frontend
/// believe the decision was saved when it wasn't.
pub(crate) fn resolve(
    state: &AppState,
    request_id: &str,
    agent_type: &str,
    decision: Option<bool>,
) -> Result<(), String> {
    // Belt-and-suspenders: `agent_wrap_pending` is only ever populated by
    // `request`, whose only caller (`pty.rs`'s `userwrap` OSC verb) already
    // validates against this same allow-list, so an unrecognized agent_type
    // can never actually match a pending entry below. Checking it explicitly
    // here too — matching `agent_hook_commands::set_agent_wrap_user_function`'s
    // own guard — means this function stays safe even if a future call site
    // ever inserts into `agent_wrap_pending` without going through `request`.
    if !matches!(agent_type, "claude" | "codex" | "goose") {
        return Err(format!(
            "wrapping a user-defined shell function is unsupported for '{agent_type}'"
        ));
    }
    match state.agent_wrap_pending.get(agent_type) {
        Some(pending) if pending.as_str() == request_id => {}
        _ => return Ok(()),
    }
    state.agent_wrap_pending.remove(agent_type);

    let save_result = match decision {
        Some(value) => {
            let mut cfg = crate::config::load_agents_config();
            cfg.agents
                .entry(agent_type.to_string())
                .or_default()
                .wrap_user_function = Some(value);
            crate::config::save_agents_config(cfg)
        }
        None => {
            state.agent_wrap_snoozed.insert(agent_type.to_string());
            Ok(())
        }
    };

    // Broadcast unconditionally, even on a save failure: every other client
    // still needs to take its copy of the dialog down (the request is no
    // longer pending either way, and there's nothing productive for a second
    // client to do with a stale, already-claimed request). Only the
    // answering caller — who can retry — gets the error back.
    state.emit_dual(AppEvent::AgentWrapPromptResolved {
        request_id: request_id.to_string(),
        agent_type: agent_type.to_string(),
        decision,
    });
    save_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_an_agent_type_outside_the_allow_list() {
        // Defense-in-depth (code review, 2026-09-25): `agent_wrap_pending`
        // is only ever populated by `request`'s own validated allow-list
        // today, so this can't currently be reached via a real pending
        // entry — but `resolve` must not trust that indirectly. No pending
        // state at all is set up here on purpose: an invalid agent_type
        // must be rejected before any pending-map lookup even happens.
        let state = test_state();
        let err = resolve(&state, "irrelevant-id", "not-a-real-agent", Some(true))
            .expect_err("an unrecognized agent_type must be rejected, not silently no-op'd");
        assert!(err.contains("not-a-real-agent"));
    }
    use std::sync::Arc;

    fn test_state() -> Arc<AppState> {
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    #[test]
    #[serial_test::serial]
    fn request_opens_exactly_one_prompt_per_agent() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();

        request(&state, "claude");
        assert_eq!(state.agent_wrap_pending.len(), 1);
        let first_id = state.agent_wrap_pending.get("claude").unwrap().clone();

        // A second detection for the same agent (another tab) must not
        // replace the pending request or open a second prompt.
        request(&state, "claude");
        assert_eq!(state.agent_wrap_pending.len(), 1);
        assert_eq!(
            *state.agent_wrap_pending.get("claude").unwrap(),
            first_id,
            "a second detection for an already-pending agent must not mint a new request"
        );
    }

    #[test]
    #[serial_test::serial]
    fn request_is_a_noop_once_already_decided() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();

        let mut cfg = crate::config::load_agents_config();
        cfg.agents
            .entry("claude".to_string())
            .or_default()
            .wrap_user_function = Some(true);
        crate::config::save_agents_config(cfg).unwrap();

        request(&state, "claude");
        assert!(
            state.agent_wrap_pending.is_empty(),
            "must not prompt once the setting is already decided"
        );
    }

    #[test]
    #[serial_test::serial]
    fn request_is_a_noop_while_snoozed() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        state.agent_wrap_snoozed.insert("claude".to_string());

        request(&state, "claude");
        assert!(
            state.agent_wrap_pending.is_empty(),
            "must not re-prompt an agent snoozed this app run"
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_true_persists_and_clears_pending() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude");
        let id = state.agent_wrap_pending.get("claude").unwrap().clone();

        resolve(&state, &id, "claude", Some(true)).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert_eq!(
            crate::agent_hook_launch::wrap_user_function("claude"),
            Some(true)
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_false_persists_and_clears_pending() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "codex");
        let id = state.agent_wrap_pending.get("codex").unwrap().clone();

        resolve(&state, &id, "codex", Some(false)).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert_eq!(
            crate::agent_hook_launch::wrap_user_function("codex"),
            Some(false)
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_none_snoozes_without_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "goose");
        let id = state.agent_wrap_pending.get("goose").unwrap().clone();

        resolve(&state, &id, "goose", None).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert!(state.agent_wrap_snoozed.contains("goose"));
        assert_eq!(
            crate::agent_hook_launch::wrap_user_function("goose"),
            None,
            "a dismiss must never persist a decision"
        );

        // Snoozed — a fresh detection this run must not re-prompt.
        request(&state, "goose");
        assert!(state.agent_wrap_pending.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn resolve_surfaces_a_save_failure_instead_of_swallowing_it() {
        // Point the config dir at a plain file, not a directory — writing
        // agents.json underneath it can't succeed. Previously `resolve`
        // discarded save_agents_config's Result, so a caller (and therefore
        // the frontend) had no way to learn the decision never persisted.
        let file = tempfile::NamedTempFile::new().unwrap();
        let _guard = crate::config::set_config_dir_override(file.path().to_path_buf());
        let state = test_state();

        request(&state, "claude");
        let id = state.agent_wrap_pending.get("claude").unwrap().clone();

        let result = resolve(&state, &id, "claude", Some(true));

        assert!(
            result.is_err(),
            "a failed save must be reported, not swallowed"
        );
        // Still removed from pending — a failed persist must not leave the
        // prompt stuck open forever with no way to retry.
        assert!(state.agent_wrap_pending.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn resolve_is_a_noop_for_an_unknown_or_already_resolved_request_id() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude");
        let id = state.agent_wrap_pending.get("claude").unwrap().clone();

        resolve(&state, &id, "claude", Some(true)).unwrap();
        // Second resolve of the same id: already removed, must not re-persist
        // or panic. Flip the on-disk value first so a wrongly-repeated
        // resolve would be observable.
        let mut cfg = crate::config::load_agents_config();
        cfg.agents
            .entry("claude".to_string())
            .or_default()
            .wrap_user_function = Some(false);
        crate::config::save_agents_config(cfg).unwrap();

        resolve(&state, &id, "claude", Some(true)).unwrap();
        assert_eq!(
            crate::agent_hook_launch::wrap_user_function("claude"),
            Some(false),
            "resolving an already-resolved request_id must be a no-op"
        );
    }
}
