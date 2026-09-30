//! Live filesystem watcher for the Session Diff Review feature.
//!
//! Modeled on `dir_watcher.rs`, but watches TWO roots per subscription (the
//! Claude project directory, non-recursively, for the main transcript file
//! plus brand-new session `.jsonl` files; and, if present, the watched
//! session's own `<project_dir>/<session_id>/` subfolder recursively, which
//! is where its subagent transcripts live) and is ref-counted per
//! `(project_dir, session_id)` so multiple UI subscribers to the same
//! session share one underlying watcher — the last `unwatch` tears it down.
//!
//! Deliberately NOT dynamic about a subagent subfolder that doesn't exist
//! yet: if a session has no `subagents/` directory when `watch_session_review`
//! is called, this watcher does not notice one appearing later (only a
//! change to something already inside `<project_dir>/<session_id>/` once
//! that path exists at watch-start time). A session that gains its first
//! subagent after the watch started needs a fresh `watch_session_review`
//! call (e.g. triggered by the next explicit poll/re-open) to pick it up.
//! Narrow, known gap — not fixed here.

use notify::{RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::state::AppEvent;

/// Debounce interval for transcript content changes. Shorter than
/// `dir_watcher::DEBOUNCE_MS` (500ms) — a live diff view benefits from
/// feeling responsive to an agent's own edits, and the coalescing this
/// exists for (many rapid writes from one burst of tool calls) still works
/// fine at 400ms.
const DEBOUNCE_MS: u64 = 400;

/// Most distinct `(project_dir, session_id)` watchers alive at once. Each holds
/// an OS file watcher (FSEvents stream / inotify watches), and the routes that
/// create them are reachable over HTTP, so the map must not grow with whatever
/// a client asks for. A real UI watches one session per open Session Diff tab.
pub(crate) const MAX_SESSION_REVIEW_WATCHERS: usize = 64;

/// Most subscribers sharing one watcher. A subscriber that never unwatches
/// (a crashed client) leaks one ref; this bounds what any number of them can
/// pin, and a refused `watch` never takes a ref, so the caller must not
/// `unwatch` for it.
pub(crate) const MAX_SESSION_REVIEW_WATCH_REFS: usize = 32;

/// Takes one more ref on an existing entry, unless it is at
/// [`MAX_SESSION_REVIEW_WATCH_REFS`].
fn add_ref(entry: &SessionWatchEntry) -> Result<(), String> {
    entry
        .ref_count
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_SESSION_REVIEW_WATCH_REFS).then_some(n + 1)
        })
        .map(|_| ())
        .map_err(|_| {
            format!("Too many subscribers for this session's watcher (max {MAX_SESSION_REVIEW_WATCH_REFS})")
        })
}

/// One ref-counted watcher entry, keyed by `(project_dir, claude_session_id)`
/// in `AppState::session_review_watchers`.
pub(crate) struct SessionWatchEntry {
    #[allow(dead_code)]
    watcher: Mutex<notify::RecommendedWatcher>,
    ref_count: AtomicUsize,
}

type WatchKey = (String, String);

fn watch_key(project_dir: &Path, session_id: &str) -> WatchKey {
    (
        project_dir.to_string_lossy().to_string(),
        session_id.to_string(),
    )
}

/// Start (or add a subscriber to) watching `session_id`'s transcript and
/// subagent files for changes. Ref-counted: call `unwatch_session_review_internal`
/// exactly once per successful call to this, when the subscriber goes away.
pub(crate) fn watch_session_review_internal(
    project_dir: &Path,
    session_id: &str,
    repo_path: &str,
    state: &Arc<AppState>,
) -> Result<(), String> {
    let key = watch_key(project_dir, session_id);
    if let Some(entry) = state.session_review_watchers.get(&key) {
        return add_ref(&entry);
    }
    if state.session_review_watchers.len() >= MAX_SESSION_REVIEW_WATCHERS {
        return Err(format!(
            "Too many live session review watchers (max {MAX_SESSION_REVIEW_WATCHERS})"
        ));
    }
    if !project_dir.is_dir() {
        return Err(format!(
            "Claude project directory does not exist: {}",
            project_dir.display()
        ));
    }

    let transcript_name = format!("{session_id}.jsonl");
    let session_subdir = project_dir.join(session_id);

    // Compare against `notify`'s (FSEvents-on-macOS-)reported paths using the
    // canonical form on both sides — macOS routinely resolves `$TMPDIR`
    // (`/var/folders/...`) through its `/private` symlink, so a plain
    // `project_dir.to_path_buf()` used for comparison never matches a real
    // FSEvents path and every change is silently misclassified as neither
    // "this session" nor "the list" — no event ever fires. Falls back to the
    // uncanonicalized path if the directory doesn't exist (e.g. in a test
    // that intentionally exercises the not-found error), matching every
    // other best-effort `.canonicalize().ok()` fallback in this codebase.
    let project_dir_owned = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let session_subdir_owned = session_subdir
        .canonicalize()
        .unwrap_or_else(|_| session_subdir.clone());
    let transcript_name_owned = transcript_name.clone();
    let session_id_owned = session_id.to_string();
    let repo_path_owned = repo_path.to_string();
    let state_weak: Weak<AppState> = Arc::downgrade(state);

    let rt = {
        #[cfg(feature = "desktop")]
        {
            tauri::async_runtime::handle().inner().clone()
        }
        #[cfg(not(feature = "desktop"))]
        {
            tokio::runtime::Handle::current()
        }
    };

    let pending: Arc<Mutex<Option<tokio::task::AbortHandle>>> = Arc::new(Mutex::new(None));
    let session_changed = Arc::new(AtomicBool::new(false));
    let list_changed = Arc::new(AtomicBool::new(false));

    let mut watcher = notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
        let event = match result {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(source = "session_review_watcher", session_id = %session_id_owned, "Watcher error: {err}");
                return;
            }
        };

        let mut touches_session = false;
        let mut touches_list = false;
        for p in &event.paths {
            if p.starts_with(&session_subdir_owned) {
                // Anything under <project_dir>/<session_id>/ (subagents/*.jsonl).
                touches_session = true;
            } else if p.parent() == Some(project_dir_owned.as_path()) {
                let is_transcript = p
                    .file_name()
                    .map(|n| n.to_string_lossy() == transcript_name_owned)
                    .unwrap_or(false);
                if is_transcript {
                    touches_session = true;
                } else if p.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    // A different session's transcript file appeared/changed
                    // directly in the project directory.
                    touches_list = true;
                }
            }
        }
        if !touches_session && !touches_list {
            return;
        }
        if touches_session {
            session_changed.store(true, Ordering::Relaxed);
        }
        if touches_list {
            list_changed.store(true, Ordering::Relaxed);
        }

        let mut guard = pending.lock();
        if let Some(prev) = guard.take() {
            prev.abort();
        }
        let state_weak = state_weak.clone();
        let repo_path = repo_path_owned.clone();
        let session_id = session_id_owned.clone();
        let transcript_path = project_dir_owned.join(&transcript_name_owned);
        let session_flag = session_changed.clone();
        let list_flag = list_changed.clone();
        let join = rt.spawn(async move {
            tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
            let fire_session = session_flag.swap(false, Ordering::Relaxed);
            let fire_list = list_flag.swap(false, Ordering::Relaxed);
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            if fire_session {
                crate::session_review::invalidate_cached_review(&transcript_path);
                state.emit_dual(AppEvent::SessionReviewChanged {
                    repo_path: repo_path.clone(),
                    session_id: session_id.clone(),
                });
                // Fires exactly once per session: `insert` returning `None`
                // means this session had no prior entry.
                let is_first_change = state
                    .announced_edit_sessions
                    .insert(session_id.clone(), ())
                    .is_none();
                if is_first_change {
                    let tuic_session_id = state.tuic_session_for_claude_session(&session_id);
                    state.emit_dual(AppEvent::AgentEditObserved {
                        tuic_session_id,
                        claude_session_id: session_id.clone(),
                        repo_path: repo_path.clone(),
                    });
                }
            }
            if fire_list {
                state.emit_dual(AppEvent::ReviewSessionsChanged { repo_path });
            }
        });
        *guard = Some(join.abort_handle());
    })
    .map_err(|e| format!("Failed to create session review watcher: {e}"))?;

    watcher
        .watch(project_dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch project directory: {e}"))?;
    if session_subdir.is_dir() {
        // Best-effort: a session with no subagents yet has no subfolder to
        // watch, and that's fine — see the module doc comment's known gap.
        let _ = watcher.watch(&session_subdir, RecursiveMode::Recursive);
    }

    // A concurrent `watch` for the same key may have inserted its own entry
    // while this one was being built: share it (dropping this watcher) rather
    // than overwrite it and lose that subscriber's ref.
    match state.session_review_watchers.entry(key) {
        dashmap::mapref::entry::Entry::Occupied(existing) => add_ref(existing.get()),
        dashmap::mapref::entry::Entry::Vacant(slot) => {
            slot.insert(SessionWatchEntry {
                watcher: Mutex::new(watcher),
                ref_count: AtomicUsize::new(1),
            });
            Ok(())
        }
    }
}

/// Drop this subscriber's reference; tears the watcher down once the last
/// one unwatches. Also clears `announced_edit_sessions` for this session at
/// that point, so a later re-watch announces its next first change again.
pub(crate) fn unwatch_session_review_internal(
    project_dir: &Path,
    session_id: &str,
    state: &Arc<AppState>,
) {
    let key = watch_key(project_dir, session_id);
    let mut should_remove = false;
    if let Some(entry) = state.session_review_watchers.get(&key) {
        // Never below zero: an unwatch with no matching ref is a no-op rather
        // than stealing another subscriber's ref.
        let prev = entry
            .ref_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
        should_remove = prev == Ok(1);
    }
    // Re-checked under the shard lock: a `watch` that took a fresh ref between
    // the decrement above and here keeps the watcher alive.
    if should_remove
        && state
            .session_review_watchers
            .remove_if(&key, |_, entry| {
                entry.ref_count.load(Ordering::Acquire) == 0
            })
            .is_some()
    {
        state.announced_edit_sessions.remove(session_id);
    }
}

// --- Tauri commands (desktop only — see mcp_http::session_review_routes for
// the HTTP-transport equivalent, which calls the `_internal` functions above
// directly with its own `State<Arc<AppState>>`) ---

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn watch_session_review(
    repo_path: String,
    session_id: String,
    claude_config_dir: Option<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    crate::session_review::validate_session_id(&session_id)?;
    let state = app_handle.state::<Arc<AppState>>();
    let project_dir =
        crate::session_review::project_dir_for(&repo_path, claude_config_dir.as_deref())
            .ok_or_else(|| "Could not determine Claude project directory".to_string())?;
    watch_session_review_internal(&project_dir, &session_id, &repo_path, &state)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn unwatch_session_review(
    repo_path: String,
    session_id: String,
    claude_config_dir: Option<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    crate::session_review::validate_session_id(&session_id)?;
    let state = app_handle.state::<Arc<AppState>>();
    let project_dir =
        crate::session_review::project_dir_for(&repo_path, claude_config_dir.as_deref())
            .ok_or_else(|| "Could not determine Claude project directory".to_string())?;
    unwatch_session_review_internal(&project_dir, &session_id, &state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_test_state() -> Arc<AppState> {
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    fn wait_for<F: Fn() -> bool>(cond: F, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    #[tokio::test]
    async fn watch_nonexistent_project_dir_returns_error() {
        let state = make_test_state();
        let result = watch_session_review_internal(
            Path::new("/tmp/nonexistent-session-review-watcher-test-12345"),
            "session-a",
            "/repo",
            &state,
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ref_counting_shares_one_watcher_and_tears_down_on_last_unwatch() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();
        fs::write(project_dir.join("session-a.jsonl"), "").unwrap();
        let state = make_test_state();

        watch_session_review_internal(project_dir, "session-a", "/repo", &state).unwrap();
        watch_session_review_internal(project_dir, "session-a", "/repo", &state).unwrap();
        let key = watch_key(project_dir, "session-a");
        assert_eq!(
            state
                .session_review_watchers
                .get(&key)
                .unwrap()
                .ref_count
                .load(Ordering::Relaxed),
            2
        );

        unwatch_session_review_internal(project_dir, "session-a", &state);
        assert!(state.session_review_watchers.contains_key(&key));

        unwatch_session_review_internal(project_dir, "session-a", &state);
        assert!(!state.session_review_watchers.contains_key(&key));
    }

    /// The watch routes are reachable over HTTP, and every distinct session id
    /// costs an OS file watcher: the map must stop growing at its cap.
    #[tokio::test]
    async fn refuses_a_new_watcher_past_the_distinct_session_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();
        let state = make_test_state();

        for i in 0..MAX_SESSION_REVIEW_WATCHERS {
            watch_session_review_internal(project_dir, &format!("session-{i}"), "/repo", &state)
                .unwrap();
        }
        let err = watch_session_review_internal(project_dir, "one-too-many", "/repo", &state)
            .unwrap_err();
        assert!(err.contains("Too many"), "{err}");
        assert_eq!(
            state.session_review_watchers.len(),
            MAX_SESSION_REVIEW_WATCHERS
        );

        // An existing watcher still takes another subscriber at the cap.
        watch_session_review_internal(project_dir, "session-0", "/repo", &state).unwrap();
        // Freeing a slot lets a new session in again.
        unwatch_session_review_internal(project_dir, "session-1", &state);
        watch_session_review_internal(project_dir, "one-too-many", "/repo", &state).unwrap();
    }

    /// Subscribers that never unwatch (a crashed client) cannot pin an
    /// unbounded ref count, and an unwatch with no ref behind it (e.g. after a
    /// refused watch) never steals another subscriber's ref.
    #[tokio::test]
    async fn caps_refs_per_watcher_and_never_decrements_below_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();
        let state = make_test_state();
        let key = watch_key(project_dir, "session-a");
        let refs = || {
            state
                .session_review_watchers
                .get(&key)
                .map(|e| e.ref_count.load(Ordering::Acquire))
        };

        for _ in 0..MAX_SESSION_REVIEW_WATCH_REFS {
            watch_session_review_internal(project_dir, "session-a", "/repo", &state).unwrap();
        }
        assert!(watch_session_review_internal(project_dir, "session-a", "/repo", &state).is_err());
        assert_eq!(refs(), Some(MAX_SESSION_REVIEW_WATCH_REFS));

        for _ in 0..MAX_SESSION_REVIEW_WATCH_REFS {
            unwatch_session_review_internal(project_dir, "session-a", &state);
        }
        assert_eq!(refs(), None, "last ref tears the watcher down");
        // A stray unwatch for a key with no watcher is a no-op.
        unwatch_session_review_internal(project_dir, "session-a", &state);
        assert_eq!(refs(), None);
    }

    #[tokio::test]
    async fn a_transcript_write_emits_session_review_changed_and_agent_edit_observed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();
        let transcript = project_dir.join("session-a.jsonl");
        fs::write(&transcript, "").unwrap();
        let state = make_test_state();
        let mut rx = state.event_bus.subscribe();

        watch_session_review_internal(project_dir, "session-a", "/repo", &state).unwrap();
        fs::write(&transcript, "{\"type\":\"user\"}\n").unwrap();

        let mut saw_review_changed = false;
        let mut saw_edit_observed = 0u32;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
                Ok(Ok(AppEvent::SessionReviewChanged { session_id, .. }))
                    if session_id == "session-a" =>
                {
                    saw_review_changed = true;
                }
                Ok(Ok(AppEvent::AgentEditObserved {
                    claude_session_id, ..
                })) if claude_session_id == "session-a" => {
                    saw_edit_observed += 1;
                }
                _ => {}
            }
            if saw_review_changed && saw_edit_observed > 0 {
                break;
            }
        }
        assert!(saw_review_changed, "expected a SessionReviewChanged event");
        assert_eq!(
            saw_edit_observed, 1,
            "AgentEditObserved must fire exactly once per session"
        );

        // A second write must NOT fire AgentEditObserved again.
        fs::write(&transcript, "{\"type\":\"user\"}\n{\"type\":\"user\"}\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_millis(800);
        while std::time::Instant::now() < deadline {
            if let Ok(Ok(AppEvent::AgentEditObserved {
                claude_session_id, ..
            })) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await
                && claude_session_id == "session-a"
            {
                saw_edit_observed += 1;
            }
        }
        assert_eq!(
            saw_edit_observed, 1,
            "AgentEditObserved must not re-fire on a later change"
        );
        let _ = wait_for(|| true, Duration::from_millis(1)); // keep tmp alive to this point
    }

    #[tokio::test]
    async fn unwatch_clears_the_announced_flag_so_a_rewatch_announces_again() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();
        fs::write(project_dir.join("session-a.jsonl"), "").unwrap();
        let state = make_test_state();

        state
            .announced_edit_sessions
            .insert("session-a".to_string(), ());
        watch_session_review_internal(project_dir, "session-a", "/repo", &state).unwrap();
        unwatch_session_review_internal(project_dir, "session-a", &state);
        assert!(!state.announced_edit_sessions.contains_key("session-a"));
    }

    #[test]
    fn tuic_session_map_upsert_lookup_and_removal() {
        let state = make_test_state();
        assert_eq!(state.tuic_session_for_claude_session("claude-1"), None);

        state
            .claude_session_map
            .insert("claude-1".to_string(), "tuic-1".to_string());
        state
            .tuic_to_claude_session
            .insert("tuic-1".to_string(), "claude-1".to_string());
        assert_eq!(
            state.tuic_session_for_claude_session("claude-1"),
            Some("tuic-1".to_string())
        );

        // Simulate the removal path a PTY-session-close cleanup takes.
        if let Some((_, claude_id)) = state.tuic_to_claude_session.remove("tuic-1") {
            state.claude_session_map.remove(&claude_id);
        }
        assert_eq!(state.tuic_session_for_claude_session("claude-1"), None);
    }
}
