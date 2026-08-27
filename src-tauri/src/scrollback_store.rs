//! On-disk persistence of terminal scrollback, for restoring the last few
//! hundred/thousand lines of output above a fresh prompt across app restarts.
//!
//! Opt-in via `AppConfig::restore_scrollback` (default off): output routed
//! through a terminal can contain secrets, and this stores it as plaintext
//! JSON in the config directory. One file per tab, keyed by its stable
//! `tuic_session` UUID (the same identity `SavedTerminal.tuicSession` already
//! carries through session restore), so a restored tab's saved output survives
//! independently of the ephemeral PTY session id.

use crate::state::{AppState, LogLine, log_lines_to_ansi};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SCROLLBACK_DIR: &str = "scrollback";
const CURRENT_VERSION: u32 = 1;

fn scrollback_dir() -> PathBuf {
    crate::config::config_dir().join(SCROLLBACK_DIR)
}

fn scrollback_path(tuic_session: &str) -> PathBuf {
    scrollback_dir().join(format!("{tuic_session}.json"))
}

/// One tab's persisted scrollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredScrollback {
    #[serde(default = "current_version")]
    pub(crate) version: u32,
    /// Unix epoch milliseconds at capture time.
    pub(crate) saved_at_ms: u64,
    /// The terminal width the lines were captured at. Recorded as diagnostic
    /// metadata (visible via `GET`-style inspection of the saved file) — not
    /// currently read by `replay_bytes` or anything else, since each `LogLine`
    /// already carries its own `cols` and replay doesn't reflow.
    pub(crate) cols: u16,
    pub(crate) lines: Vec<LogLine>,
}

fn current_version() -> u32 {
    CURRENT_VERSION
}

/// Persist `lines` for `tuic_session`, replacing any previous save. `cols` is
/// the terminal width the lines were captured at, recorded for diagnostic
/// purposes only (see `StoredScrollback::cols`).
pub(crate) fn save(
    tuic_session: &str,
    lines: &[LogLine],
    cols: u16,
    saved_at_ms: u64,
) -> Result<(), String> {
    let doc = StoredScrollback {
        version: CURRENT_VERSION,
        saved_at_ms,
        cols,
        lines: lines.to_vec(),
    };
    let json = serde_json::to_vec(&doc).map_err(|e| format!("Failed to serialize: {e}"))?;
    crate::config::persist_atomic(&scrollback_path(tuic_session), &json)
}

/// Load a tab's saved scrollback, if any. Corrupt or unreadable files are
/// treated as absent rather than propagated as an error — restore is a
/// best-effort convenience, never a reason to block opening the terminal.
pub(crate) fn load(tuic_session: &str) -> Option<StoredScrollback> {
    let data = std::fs::read(scrollback_path(tuic_session)).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Delete one tab's saved scrollback. A no-op if none exists.
pub(crate) fn clear(tuic_session: &str) {
    let _ = std::fs::remove_file(scrollback_path(tuic_session));
}

/// Delete every saved scrollback file — the "Clear saved scrollback" settings
/// action with no session scoped.
pub(crate) fn clear_all() -> Result<(), String> {
    let dir = scrollback_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Failed to read scrollback directory: {e}")),
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

/// Delete saved scrollback files whose last-modified time is older than
/// `max_age` — the backstop against the directory growing forever from tabs
/// that were closed without ever being explicitly cleared. Best-effort: a
/// file whose mtime can't be read is left alone rather than guessed at.
pub(crate) fn prune(max_age: std::time::Duration) {
    let dir = scrollback_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now.duration_since(modified).is_ok_and(|age| age > max_age) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A cheap fingerprint of a `VtLogBuffer`'s current content, for the capture
/// dedup in [`capture_session`]. Combines `total_lines()` with a hash of the
/// current on-screen rows: `total_lines()` alone never moves for a full-screen
/// program that redraws via cursor addressing without ever scrolling a new
/// line into history (e.g. `htop`, `vim`, an inline TUI on the primary
/// screen) — a dedup keyed on it alone would freeze that session's saved
/// scrollback at whatever was on screen the first time it was captured.
///
/// Exposed so [`crate::pty::create_pty`] can seed the dedup mark immediately
/// after replaying saved scrollback into a fresh buffer — otherwise the very
/// next capture sees no mark at all, treats the replay as new content, and
/// re-persists it (append separator and all), compounding on every restart
/// of a tab nobody touched.
pub(crate) fn capture_fingerprint(vt: &crate::state::VtLogBuffer) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    vt.total_lines().hash(&mut hasher);
    for line in vt.screen_log_lines() {
        line.text().hash(&mut hasher);
    }
    hasher.finish()
}

/// Capture `session_id`'s current scrollback (durable log tail + unflushed
/// on-screen rows) and persist it under `tuic_session`, unless
/// [`capture_fingerprint`] is unchanged since the last capture — the dedup
/// that keeps an idle app's periodic sweep from rewriting every session's
/// file every tick.
///
/// No-op if there's no live `VtLogBuffer` for `session_id` (already closed
/// and reaped) or if the capture would be empty.
pub(crate) fn capture_session(
    state: &AppState,
    session_id: &str,
    tuic_session: &str,
    max_lines: usize,
    saved_at_ms: u64,
) {
    let Some(vt_lock) = state.vt_log_buffers.get(session_id) else {
        return;
    };
    let vt = vt_lock.lock();
    let fingerprint = capture_fingerprint(&vt);
    if let Some(mark) = state.scrollback_capture_marks.get(session_id)
        && *mark == fingerprint
    {
        return; // Buffer unchanged since the last capture — skip the write.
    }
    let total = vt.total_lines();
    let (mut lines, _) = vt.lines_since_owned(total.saturating_sub(max_lines), max_lines);
    lines.extend(vt.screen_log_lines());
    if lines.len() > max_lines {
        let excess = lines.len() - max_lines;
        lines.drain(0..excess);
    }
    drop(vt);
    state
        .scrollback_capture_marks
        .insert(session_id.to_string(), fingerprint);
    if lines.is_empty() {
        return;
    }
    let cols = lines
        .last()
        .map(|l| l.cols)
        .filter(|&c| c > 0)
        .unwrap_or(80);
    if let Err(e) = save(tuic_session, &lines, cols, saved_at_ms) {
        tracing::warn!(session_id, tuic_session, "Failed to save scrollback: {e}");
    }
}

/// Clear saved scrollback — one tab (`session` = its `tuic_session`) or, when
/// omitted, every saved tab. Backs the desktop "Clear saved scrollback"
/// settings action; `DELETE /scrollback` is the HTTP/MCP equivalent
/// (`mcp_http/config_routes.rs::clear_saved_scrollback_http`).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn clear_saved_scrollback(session: Option<String>) -> Result<(), String> {
    match session {
        Some(tuic_session) => {
            clear(&tuic_session);
            Ok(())
        }
        None => clear_all(),
    }
}

/// Capture every live session's scrollback in one pass — the periodic sweep
/// (called on a timer while the app runs) and the final flush (called once at
/// `RunEvent::Exit`). No-op entirely when `restore_scrollback` is off.
pub(crate) fn sweep_all(state: &AppState) {
    let (enabled, max_lines) = {
        let cfg = state.config.read();
        (
            cfg.restore_scrollback,
            cfg.restore_scrollback_lines as usize,
        )
    };
    if !enabled {
        return;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Collect first: capture_session only reads live_pty_by_tuic_session
    // transitively through state, but holding a DashMap iterator open while
    // also taking other locks on the same map elsewhere risks a deadlock —
    // collecting up front keeps the iteration borrow short-lived.
    let live: Vec<(String, String)> = state
        .live_pty_by_tuic_session
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect();
    for (tuic_session, session_id) in live {
        capture_session(state, &session_id, &tuic_session, max_lines, now_ms);
    }
}

/// Render a saved scrollback into replay bytes: the restored content, then a
/// dim separator row marking where restored history ends and the live
/// session begins.
pub(crate) fn replay_bytes(saved: &StoredScrollback) -> Vec<u8> {
    let mut out = log_lines_to_ansi(&saved.lines);
    let timestamp = format_saved_at(saved.saved_at_ms);
    out.extend_from_slice(
        format!("\x1b[2m───── restored from previous session · {timestamp} ─────\x1b[0m\r\n")
            .as_bytes(),
    );
    out
}

/// Best-effort human-readable rendering of a saved-at timestamp. Falls back to
/// the raw epoch value if `humantime`-style formatting isn't worth pulling in
/// a dependency for — this is a cosmetic separator, not a parsed value.
fn format_saved_at(saved_at_ms: u64) -> String {
    let secs = saved_at_ms / 1000;
    match std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs)) {
        Some(_) => {
            // No chrono dependency needed here — a coarse relative rendering
            // is enough for a separator row a human skims once.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(saved_at_ms);
            let age_secs = now_ms.saturating_sub(saved_at_ms) / 1000;
            if age_secs < 60 {
                "moments ago".to_string()
            } else if age_secs < 3600 {
                format!("{}m ago", age_secs / 60)
            } else if age_secs < 86400 {
                format!("{}h ago", age_secs / 3600)
            } else {
                format!("{}d ago", age_secs / 86400)
            }
        }
        None => "unknown time".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LogSpan, tests_support::make_test_app_state};

    fn sample_lines() -> Vec<LogLine> {
        vec![LogLine {
            spans: vec![LogSpan {
                text: "hello".to_string(),
                ..Default::default()
            }],
            cols: 80,
            chrome: false,
        }]
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        save("tuic-1", &sample_lines(), 80, 12345).unwrap();

        let loaded = load("tuic-1").expect("saved scrollback loads");
        assert_eq!(loaded.version, CURRENT_VERSION);
        assert_eq!(loaded.cols, 80);
        assert_eq!(loaded.saved_at_ms, 12345);
        assert_eq!(loaded.lines, sample_lines());
    }

    #[tokio::test]
    async fn clear_saved_scrollback_command_with_a_session_clears_only_that_one() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save("tuic-a", &sample_lines(), 80, 1).unwrap();
        save("tuic-b", &sample_lines(), 80, 1).unwrap();

        clear_saved_scrollback(Some("tuic-a".to_string()))
            .await
            .unwrap();

        assert!(load("tuic-a").is_none());
        assert!(load("tuic-b").is_some());
    }

    #[tokio::test]
    async fn clear_saved_scrollback_command_with_none_clears_every_tab() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save("tuic-a", &sample_lines(), 80, 1).unwrap();
        save("tuic-b", &sample_lines(), 80, 1).unwrap();

        clear_saved_scrollback(None).await.unwrap();

        assert!(load("tuic-a").is_none());
        assert!(load("tuic-b").is_none());
    }

    #[test]
    fn load_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        assert!(load("does-not-exist").is_none());
    }

    #[test]
    fn load_returns_none_for_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        std::fs::create_dir_all(scrollback_dir()).unwrap();
        std::fs::write(scrollback_path("bad"), b"not json").unwrap();

        assert!(load("bad").is_none());
    }

    #[test]
    fn clear_removes_only_the_named_session() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        save("keep", &sample_lines(), 80, 1).unwrap();
        save("drop", &sample_lines(), 80, 1).unwrap();

        clear("drop");

        assert!(load("keep").is_some());
        assert!(load("drop").is_none());
    }

    #[test]
    fn clear_all_removes_every_saved_session() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        save("a", &sample_lines(), 80, 1).unwrap();
        save("b", &sample_lines(), 80, 1).unwrap();

        clear_all().unwrap();

        assert!(load("a").is_none());
        assert!(load("b").is_none());
    }

    #[test]
    fn clear_all_is_a_noop_when_directory_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        assert!(clear_all().is_ok());
    }

    #[test]
    fn prune_removes_only_files_older_than_max_age() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        save("fresh", &sample_lines(), 80, 1).unwrap();
        save("also-fresh", &sample_lines(), 80, 1).unwrap();

        // Backdate one file's mtime well past the prune threshold.
        let old_path = scrollback_path("fresh");
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let old_file = std::fs::File::open(&old_path).unwrap();
        old_file
            .set_modified(old_time)
            .expect("set_modified should be supported for this test");

        prune(std::time::Duration::from_secs(60));

        assert!(load("fresh").is_none(), "aged-out file should be pruned");
        assert!(load("also-fresh").is_some(), "recent file should survive");
    }

    #[test]
    fn capture_session_writes_scrollback_from_a_live_vt_log_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        let mut vt = crate::state::VtLogBuffer::new(3, 80, 1000);
        for i in 0..10 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), parking_lot::Mutex::new(vt));

        capture_session(&state, "sess-1", "tuic-1", 1000, 999);

        let loaded = load("tuic-1").expect("capture should have saved a file");
        assert!(!loaded.lines.is_empty());
        assert_eq!(loaded.saved_at_ms, 999);
    }

    #[test]
    fn capture_session_skips_write_when_buffer_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        let mut vt = crate::state::VtLogBuffer::new(3, 80, 1000);
        for i in 0..10 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), parking_lot::Mutex::new(vt));

        capture_session(&state, "sess-1", "tuic-1", 1000, 1);
        let path = scrollback_path("tuic-1");
        let mtime1 = std::fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        capture_session(&state, "sess-1", "tuic-1", 1000, 2); // no new output

        let mtime2 = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            mtime1, mtime2,
            "unchanged buffer must not trigger a rewrite"
        );
    }

    // --- capture_fingerprint (regression: full-screen redraws with no new
    // scrolled lines must still be detected as changed) ---

    #[test]
    fn capture_fingerprint_changes_when_on_screen_content_changes_without_scrolling() {
        // A screen tall enough that neither write below scrolls anything into
        // history — total_lines() alone would stay at 0 for both, exactly the
        // htop/vim/inline-TUI scenario the old total_lines()-only dedup missed.
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 1000);
        vt.process(b"first frame");
        let fp1 = capture_fingerprint(&vt);

        vt.process(b"\x1b[Hsecond frame, totally different"); // cursor home + redraw
        let fp2 = capture_fingerprint(&vt);

        assert_eq!(
            vt.total_lines(),
            0,
            "test setup: nothing should have scrolled"
        );
        assert_ne!(
            fp1, fp2,
            "an on-screen-only redraw must change the fingerprint even though total_lines() didn't move"
        );
    }

    #[test]
    fn capture_fingerprint_is_stable_for_unchanged_content() {
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 1000);
        vt.process(b"stable content");
        assert_eq!(capture_fingerprint(&vt), capture_fingerprint(&vt));
    }

    #[test]
    fn capture_session_detects_a_full_screen_redraw_that_never_scrolls() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        let mut vt = crate::state::VtLogBuffer::new(24, 80, 1000);
        vt.process(b"first frame");
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), parking_lot::Mutex::new(vt));

        capture_session(&state, "sess-1", "tuic-1", 1000, 1);
        let first_saved = load("tuic-1").expect("first capture should save");

        {
            let entry = state.vt_log_buffers.get("sess-1").unwrap();
            entry
                .lock()
                .process(b"\x1b[Hsecond frame, totally different content");
        }
        capture_session(&state, "sess-1", "tuic-1", 1000, 2);
        let second_saved = load("tuic-1").expect("second capture should also save");

        assert_ne!(
            first_saved.lines, second_saved.lines,
            "a redraw that never scrolls a new line into history must still be re-captured"
        );
    }

    // --- Replay reseed (regression: a fresh replay must not look like new
    // content to the very next capture) ---

    #[test]
    fn seeding_the_mark_after_a_replay_prevents_an_immediate_recapture() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // Simulate create_pty's replay: build a buffer from saved lines, then
        // seed the dedup mark exactly as create_pty now does immediately after
        // vt_log.process(replay_bytes(...)) — before the session is inserted.
        let mut vt = crate::state::VtLogBuffer::new(3, 80, 1000);
        for i in 0..10 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        let (replayed_lines, _) = vt.lines_since_owned(0, usize::MAX);
        save("tuic-1", &replayed_lines, 80, 1).unwrap();

        let mut fresh = crate::state::VtLogBuffer::new(3, 80, 1000);
        fresh.process(&log_lines_to_ansi(&replayed_lines));
        let seeded_fingerprint = capture_fingerprint(&fresh);

        let state = make_test_app_state();
        state
            .scrollback_capture_marks
            .insert("sess-1".to_string(), seeded_fingerprint);
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), parking_lot::Mutex::new(fresh));

        let saved_before = load("tuic-1").unwrap();
        capture_session(&state, "sess-1", "tuic-1", 1000, 2);
        let saved_after = load("tuic-1").unwrap();

        assert_eq!(
            saved_before.saved_at_ms, saved_after.saved_at_ms,
            "seeding the mark at replay time must make the very next capture a no-op, \
             not a re-save of the replayed content (which would compound the separator \
             on every restart of an untouched tab)"
        );
    }

    #[test]
    fn without_seeding_a_replay_would_be_recaptured_immediately() {
        // Companion to the test above: proves the seeding step is load-bearing
        // by showing what happens without it (the bug the fix addresses).
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let mut vt = crate::state::VtLogBuffer::new(3, 80, 1000);
        for i in 0..10 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        let (replayed_lines, _) = vt.lines_since_owned(0, usize::MAX);

        let mut fresh = crate::state::VtLogBuffer::new(3, 80, 1000);
        fresh.process(&log_lines_to_ansi(&replayed_lines));

        let state = make_test_app_state();
        // No mark seeded this time.
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), parking_lot::Mutex::new(fresh));

        assert!(load("tuic-1").is_none());
        capture_session(&state, "sess-1", "tuic-1", 1000, 1);
        assert!(
            load("tuic-1").is_some(),
            "without seeding, the replayed content looks like new output and gets re-saved"
        );
    }

    #[test]
    fn capture_session_is_a_noop_when_no_live_buffer_exists() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        capture_session(&state, "no-such-session", "tuic-1", 1000, 1);

        assert!(load("tuic-1").is_none());
    }

    fn vt_with_scrolled_lines() -> parking_lot::Mutex<crate::state::VtLogBuffer> {
        let mut vt = crate::state::VtLogBuffer::new(3, 80, 1000);
        for i in 0..10 {
            vt.process(format!("line {i}\r\n").as_bytes());
        }
        parking_lot::Mutex::new(vt)
    }

    #[test]
    fn sweep_all_is_a_noop_when_restore_scrollback_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), vt_with_scrolled_lines());
        state
            .live_pty_by_tuic_session
            .insert("tuic-1".to_string(), "sess-1".to_string());
        // restore_scrollback defaults to false — sweep_all must not write anything.

        sweep_all(&state);

        assert!(load("tuic-1").is_none());
    }

    #[test]
    fn sweep_all_captures_every_live_session_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        state.config.write().restore_scrollback = true;
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), vt_with_scrolled_lines());
        state
            .vt_log_buffers
            .insert("sess-2".to_string(), vt_with_scrolled_lines());
        state
            .live_pty_by_tuic_session
            .insert("tuic-1".to_string(), "sess-1".to_string());
        state
            .live_pty_by_tuic_session
            .insert("tuic-2".to_string(), "sess-2".to_string());

        sweep_all(&state);

        assert!(load("tuic-1").is_some());
        assert!(load("tuic-2").is_some());
    }

    #[test]
    fn sweep_all_respects_the_configured_line_cap() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let state = make_test_app_state();
        {
            let mut cfg = state.config.write();
            cfg.restore_scrollback = true;
            cfg.restore_scrollback_lines = 2;
        }
        state
            .vt_log_buffers
            .insert("sess-1".to_string(), vt_with_scrolled_lines());
        state
            .live_pty_by_tuic_session
            .insert("tuic-1".to_string(), "sess-1".to_string());

        sweep_all(&state);

        let loaded = load("tuic-1").expect("capture should have saved a file");
        assert!(loaded.lines.len() <= 2);
    }

    #[test]
    fn replay_bytes_appends_a_separator_after_restored_content() {
        let saved = StoredScrollback {
            version: CURRENT_VERSION,
            saved_at_ms: 0,
            cols: 80,
            lines: sample_lines(),
        };
        let bytes = replay_bytes(&saved);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("hello"));
        assert!(text.contains("restored from previous session"));
    }
}
