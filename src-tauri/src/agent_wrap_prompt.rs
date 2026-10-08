//! Consent flow for wrapping a user's own `claude`/`codex`/`goose` shell
//! function with TUIC's launch-flag injection.
//!
//! `shell_integration.rs`'s deferred zsh integration detects when the user
//! already has their own function for one of these agents and there is no
//! recorded decision for that exact function body — see that module's doc
//! comment for the full mechanism. It reports this via the OSC 7770
//! `userwrap=<agent>:<fingerprint>` verb, handled in `pty.rs`, which validates
//! both halves and calls [`request`] here. The fingerprint (`cksum` of the
//! function body) is stored with the answer, so consent covers exactly the
//! function the user was asked about: a changed function is asked about again
//! and is never wrapped on an old "yes".
//!
//! [`request`]/[`resolve`] are deliberately NOT built on `McpConfirm`'s
//! oneshot/timeout shape (`state.rs`'s `AppEvent::McpConfirm`,
//! `mcp_http::resolve_mcp_confirm`): that mechanism collapses "explicit No"
//! and "dismissed without answering" into the same `bool`, which is exactly
//! the distinction this feature needs to keep — an explicit answer persists
//! `Some(_)` (with the fingerprint), while a dismiss only snoozes for this app
//! run and persists nothing. Nothing here needs a caller blocked waiting on
//! the answer either, so there's no oneshot channel and no timeout.

use crate::state::{AppEvent, AppState};
use std::time::{Duration, Instant};

/// How long a pending prompt holds its agent's single slot. Only one prompt
/// per agent can be pending, so without an expiry a prompt nobody answers —
/// including one opened by forged terminal output — would block the genuine
/// one for the whole app run. After this, a fresh detection replaces it (the
/// old dialog is dismissed) and an answer to the old id is a no-op.
pub(crate) const PENDING_WRAP_PROMPT_TTL: Duration = Duration::from_secs(10 * 60);

/// One pending prompt: the id clients answer with, the fingerprint of the
/// user function it asks about (persisted with the answer), and when it was
/// opened (`PENDING_WRAP_PROMPT_TTL`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingWrapPrompt {
    pub(crate) request_id: String,
    pub(crate) fingerprint: String,
    pub(crate) opened_at: Instant,
}

impl PendingWrapPrompt {
    fn expired(&self) -> bool {
        self.opened_at.elapsed() >= PENDING_WRAP_PROMPT_TTL
    }
}

/// Error `resolve` returns for an agent outside the allow-list — a caller
/// mistake (HTTP 400), as opposed to a failed save (500).
pub(crate) fn unsupported_agent_error(agent_type: &str) -> String {
    format!("wrapping a user-defined shell function is unsupported for '{agent_type}'")
}

/// Whether `agent_type` may have a user function wrapped at all — the one
/// allow-list every entry point checks.
pub(crate) fn is_wrappable_agent(agent_type: &str) -> bool {
    crate::shell_integration::WRAP_USER_FUNCTION_AGENTS.contains(&agent_type)
}

/// Whether the persisted decision already covers the function with
/// `fingerprint` — the same rule the zsh integration's `__tuic_user_fn_mode`
/// applies (`wrap` needs the exact fingerprint; a fingerprint-less `skip`
/// covers any function).
fn decided_for(agent_type: &str, fingerprint: &str) -> bool {
    let config = crate::config::load_agents_config();
    let Some(settings) = config.agents.get(agent_type) else {
        return false;
    };
    match settings.wrap_user_function {
        Some(true) => settings.wrap_user_function_hash.as_deref() == Some(fingerprint),
        Some(false) => settings
            .wrap_user_function_hash
            .as_deref()
            .is_none_or(|h| h == fingerprint),
        None => false,
    }
}

/// Every zsh tab with an undecided function fires this on its own precmd
/// bootstrap, so a workspace with many tabs open at once must not open the
/// same dialog once per tab — this enforces exactly one in-flight prompt per
/// agent type, app-wide.
///
/// Callers have already validated `agent_type` and `fingerprint` (the OSC
/// verb is untrusted terminal output); both are re-checked here so a future
/// call site cannot skip that.
/// Parse a `userwrap` OSC payload, `<agent>:<fingerprint>:<tuic_session>`, read
/// from PTY `session_id`'s output, and accept it only when every part is valid
/// AND the session token is this PTY's own identity (its key, or the
/// `$TUIC_SESSION` bound to it). Terminal output is untrusted: any program can
/// print the sequence — and, by also forging `133;D` first, get past the
/// "a foreground command is running" filter — but it cannot know this terminal's
/// identity, so a forged request is dropped instead of opening the consent
/// dialog. A payload without the token (an older integration script) is
/// refused too; the script is rewritten on every spawn.
pub(crate) fn bound_userwrap_payload<'a>(
    state: &AppState,
    session_id: &str,
    payload: &'a str,
) -> Option<(&'a str, &'a str)> {
    let mut parts = payload.splitn(3, ':');
    let agent = parts.next()?;
    let fingerprint = parts.next()?;
    let owner = parts.next()?;
    if !is_wrappable_agent(agent)
        || !crate::shell_integration::is_user_function_fingerprint(fingerprint)
        || owner.is_empty()
        || owner.len() > 128
        || !owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    let bound =
        owner == session_id || state.live_pty_for_peer(owner).as_deref() == Some(session_id);
    bound.then_some((agent, fingerprint))
}

pub(crate) fn request(state: &AppState, agent_type: &str, fingerprint: &str) {
    if !is_wrappable_agent(agent_type)
        || !crate::shell_integration::is_user_function_fingerprint(fingerprint)
    {
        return;
    }
    // Another tab (or a prior run) already decided for this exact function.
    if decided_for(agent_type, fingerprint) {
        return;
    }
    // Dismissed earlier this app run — snoozed until restart.
    if state.agent_wrap_snoozed.contains(agent_type) {
        return;
    }
    // Already pending from another tab's detection — unless it has expired,
    // in which case its dialog is dismissed and this detection replaces it.
    if let Some((_, stale)) = state
        .agent_wrap_pending
        .remove_if(agent_type, |_, pending| pending.expired())
    {
        state.emit_dual(AppEvent::AgentWrapPromptResolved {
            request_id: stale.request_id,
            agent_type: agent_type.to_string(),
            decision: None,
        });
    }
    if state.agent_wrap_pending.contains_key(agent_type) {
        return;
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    // Re-check-and-insert isn't atomic across the DashMap operations above
    // and this one, so a concurrent second detection for the same agent could
    // still race past both checks — accepted: the loser's `insert` simply
    // overwrites the winner's entry, which only means the OLDER dialog's
    // answer resolves nothing (a no-op on an id no longer pending — see
    // `resolve`), never that a stale answer is persisted.
    state.agent_wrap_pending.insert(
        agent_type.to_string(),
        PendingWrapPrompt {
            request_id: request_id.clone(),
            fingerprint: fingerprint.to_string(),
            opened_at: Instant::now(),
        },
    );
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
/// `AgentSettings::wrap_user_function` together with the pending prompt's
/// fingerprint (`wrap_user_function_hash`) — future shell spawns only; `None`
/// means the prompt was dismissed without an answer, which persists nothing
/// and snoozes the agent for the rest of this app run instead.
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
    // `request`, which validates against this same allow-list.
    if !is_wrappable_agent(agent_type) {
        return Err(unsupported_agent_error(agent_type));
    }
    let Some((_, pending)) = state
        .agent_wrap_pending
        .remove_if(agent_type, |_, pending| pending.request_id == request_id)
    else {
        return Ok(());
    };
    // An answer that arrives after the prompt's TTL persists nothing: the
    // dialog may have been opened by forged output long ago, and a fresh
    // detection will ask again. It is still taken down below.
    if pending.expired() {
        state.emit_dual(AppEvent::AgentWrapPromptResolved {
            request_id: request_id.to_string(),
            agent_type: agent_type.to_string(),
            decision: None,
        });
        return Ok(());
    }

    let save_result = match decision {
        Some(value) => {
            let mut cfg = crate::config::load_agents_config();
            let base = cfg.clone();
            let settings = cfg.agents.entry(agent_type.to_string()).or_default();
            settings.wrap_user_function = Some(value);
            settings.wrap_user_function_hash = Some(pending.fingerprint);
            crate::config::save_agents_config(base, cfg)
        }
        None => {
            state.agent_wrap_snoozed.insert(agent_type.to_string());
            Ok(())
        }
    };

    // Broadcast unconditionally, even on a save failure: every other client
    // still needs to take its copy of the dialog down (the request is no
    // longer pending either way). Only the answering caller — who can retry —
    // gets the error back.
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
    use std::sync::Arc;

    const FP: &str = "1234567-89";

    fn test_state() -> Arc<AppState> {
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    fn pending_id(state: &AppState, agent: &str) -> String {
        state
            .agent_wrap_pending
            .get(agent)
            .unwrap()
            .request_id
            .clone()
    }

    fn stored(agent: &str) -> (Option<bool>, Option<String>) {
        let cfg = crate::config::load_agents_config();
        let s = cfg.agents.get(agent).cloned().unwrap_or_default();
        (s.wrap_user_function, s.wrap_user_function_hash)
    }

    fn store(agent: &str, value: Option<bool>, hash: Option<&str>) {
        let mut cfg = crate::config::load_agents_config();
        let base = cfg.clone();
        let s = cfg.agents.entry(agent.to_string()).or_default();
        s.wrap_user_function = value;
        s.wrap_user_function_hash = hash.map(str::to_string);
        crate::config::save_agents_config(base, cfg).unwrap();
    }

    #[test]
    fn resolve_rejects_an_agent_type_outside_the_allow_list() {
        // No pending state at all on purpose: an invalid agent_type must be
        // rejected before any pending-map lookup happens.
        let state = test_state();
        let err = resolve(&state, "irrelevant-id", "not-a-real-agent", Some(true))
            .expect_err("an unrecognized agent_type must be rejected, not silently no-op'd");
        assert!(err.contains("not-a-real-agent"));
    }

    #[test]
    #[serial_test::serial]
    fn request_ignores_an_invalid_agent_or_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "grok", FP);
        request(&state, "claude", "12;rm -rf ~");
        request(&state, "claude", "");
        assert!(state.agent_wrap_pending.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn request_opens_exactly_one_prompt_per_agent() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();

        request(&state, "claude", FP);
        assert_eq!(state.agent_wrap_pending.len(), 1);
        let first_id = pending_id(&state, "claude");

        // A second detection for the same agent (another tab) must not
        // replace the pending request or open a second prompt.
        request(&state, "claude", FP);
        assert_eq!(state.agent_wrap_pending.len(), 1);
        assert_eq!(
            pending_id(&state, "claude"),
            first_id,
            "a second detection for an already-pending agent must not mint a new request"
        );
    }

    #[test]
    #[serial_test::serial]
    fn request_is_a_noop_once_decided_for_the_same_function() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        store("claude", Some(true), Some(FP));

        request(&state, "claude", FP);
        assert!(
            state.agent_wrap_pending.is_empty(),
            "must not prompt once this exact function is decided"
        );
    }

    #[test]
    #[serial_test::serial]
    fn request_asks_again_when_the_function_changed() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        store("claude", Some(true), Some(FP));

        request(&state, "claude", "999-1");
        assert!(
            state.agent_wrap_pending.contains_key("claude"),
            "a yes for one function body must not cover a different one"
        );
    }

    #[test]
    #[serial_test::serial]
    fn a_wrap_without_a_fingerprint_still_asks() {
        // "Wrap" picked in Settings records no fingerprint: no function has
        // been consented to yet, so the shell asks and so does this.
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        store("codex", Some(true), None);

        request(&state, "codex", FP);
        assert!(state.agent_wrap_pending.contains_key("codex"));
    }

    #[test]
    #[serial_test::serial]
    fn a_fingerprint_less_leave_alone_covers_any_function() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        store("goose", Some(false), None);

        request(&state, "goose", FP);
        assert!(state.agent_wrap_pending.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn request_is_a_noop_while_snoozed() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        state.agent_wrap_snoozed.insert("claude".to_string());

        request(&state, "claude", FP);
        assert!(
            state.agent_wrap_pending.is_empty(),
            "must not re-prompt an agent snoozed this app run"
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_true_persists_with_the_fingerprint_and_clears_pending() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude", FP);
        let id = pending_id(&state, "claude");

        resolve(&state, &id, "claude", Some(true)).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert_eq!(stored("claude"), (Some(true), Some(FP.to_string())));
    }

    #[test]
    #[serial_test::serial]
    fn resolve_false_persists_with_the_fingerprint_and_clears_pending() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "codex", FP);
        let id = pending_id(&state, "codex");

        resolve(&state, &id, "codex", Some(false)).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert_eq!(stored("codex"), (Some(false), Some(FP.to_string())));
    }

    #[test]
    #[serial_test::serial]
    fn resolve_none_snoozes_without_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "goose", FP);
        let id = pending_id(&state, "goose");

        resolve(&state, &id, "goose", None).unwrap();

        assert!(state.agent_wrap_pending.is_empty());
        assert!(state.agent_wrap_snoozed.contains("goose"));
        assert_eq!(
            stored("goose"),
            (None, None),
            "a dismiss must never persist a decision"
        );

        // Snoozed — a fresh detection this run must not re-prompt.
        request(&state, "goose", FP);
        assert!(state.agent_wrap_pending.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn resolve_surfaces_a_save_failure_instead_of_swallowing_it() {
        // Point the config dir at a plain file, not a directory — writing
        // agents.json underneath it can't succeed.
        let file = tempfile::NamedTempFile::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(file.path().to_path_buf());
        let state = test_state();

        request(&state, "claude", FP);
        let id = pending_id(&state, "claude");

        let result = resolve(&state, &id, "claude", Some(true));

        assert!(
            result.is_err(),
            "a failed save must be reported, not swallowed"
        );
        // Still removed from pending — a failed persist must not leave the
        // prompt stuck open forever with no way to retry.
        assert!(state.agent_wrap_pending.is_empty());
    }

    fn age_pending(state: &AppState, agent: &str) {
        let mut entry = state.agent_wrap_pending.get_mut(agent).unwrap();
        entry.opened_at = Instant::now()
            .checked_sub(PENDING_WRAP_PROMPT_TTL + Duration::from_secs(1))
            .unwrap();
    }

    /// A prompt nobody answered (e.g. one opened by forged output) must not
    /// hold its agent's slot forever: after the TTL a fresh detection
    /// replaces it.
    #[test]
    #[serial_test::serial]
    fn an_expired_pending_prompt_is_replaced_by_a_fresh_detection() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude", "111-1");
        let stale_id = pending_id(&state, "claude");
        age_pending(&state, "claude");

        request(&state, "claude", FP);
        let fresh = state.agent_wrap_pending.get("claude").unwrap().clone();
        assert_ne!(fresh.request_id, stale_id);
        assert_eq!(fresh.fingerprint, FP);
    }

    #[test]
    #[serial_test::serial]
    fn an_answer_after_the_ttl_persists_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude", FP);
        let id = pending_id(&state, "claude");
        age_pending(&state, "claude");

        resolve(&state, &id, "claude", Some(true)).unwrap();
        assert!(state.agent_wrap_pending.is_empty());
        assert_eq!(stored("claude"), (None, None));
    }

    #[test]
    #[serial_test::serial]
    fn resolve_is_a_noop_for_an_unknown_or_already_resolved_request_id() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let state = test_state();
        request(&state, "claude", FP);
        let id = pending_id(&state, "claude");

        resolve(&state, "not-the-id", "claude", Some(true)).unwrap();
        assert!(
            state.agent_wrap_pending.contains_key("claude"),
            "a wrong id must not consume the pending prompt"
        );
        assert_eq!(stored("claude"), (None, None));

        resolve(&state, &id, "claude", Some(true)).unwrap();
        // Second resolve of the same id: already removed, must not re-persist.
        store("claude", Some(false), Some(FP));
        resolve(&state, &id, "claude", Some(true)).unwrap();
        assert_eq!(
            stored("claude"),
            (Some(false), Some(FP.to_string())),
            "resolving an already-resolved request_id must be a no-op"
        );
    }
}
