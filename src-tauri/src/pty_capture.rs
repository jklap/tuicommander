//! Raw PTY capture tap — turns "it happened again" into a fixture.
//!
//! State recognition (working / idle / awaiting) is decided from bytes an agent
//! writes once and never repeats. When a detection breaks, the evidence is
//! already gone: the per-session output ring holds the last 8 KB, which a single
//! Ink repaint overruns in seconds. Every regression therefore had to be
//! re-diagnosed by reasoning about code instead of by replaying what actually
//! arrived — and each new agent harness broke a detector in a way no test could
//! have caught, because no test had the bytes.
//!
//! This records the raw stream to disk so a failure observed on screen becomes a
//! file that `pty::tests` can replay forever. Off by default; the check when off
//! is one relaxed atomic load per chunk.
//!
//! ```text
//! curl -X POST localhost:9876/diagnostics/capture -H 'content-type: application/json' \
//!      -d '{"enabled":true}'                    # all sessions
//!      -d '{"enabled":true,"session_id":"<id>"}' # one session
//! curl localhost:9876/diagnostics/capture        # state + files written
//! ```
//!
//! Captures land in `<app config dir>/captures/<session-id>.raw` and stop
//! growing at [`MAX_CAPTURE_BYTES`] each: a fixture is a moment, not a session
//! transcript, and an unattended tap must not fill Boss's disk.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Per-session cap. Comfortably covers a full screen repaint plus the prompt
/// that follows it, which is all a state-detection fixture needs.
pub(crate) const MAX_CAPTURE_BYTES: u64 = 512 * 1024;

static ENABLED: AtomicBool = AtomicBool::new(false);

struct CaptureState {
    /// When set, only this session is recorded. `None` records every session.
    session_filter: Option<String>,
    /// Open capture files and how many bytes each has taken.
    files: HashMap<String, (std::fs::File, u64)>,
    dir: Option<PathBuf>,
}

static STATE: Mutex<Option<CaptureState>> = Mutex::new(None);

/// Whether the tap is recording. Cheap enough to call per chunk.
pub(crate) fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Start or stop recording. Starting always begins a fresh set of files:
/// a capture that silently appended to a previous run's bytes would replay as
/// one impossible stream.
pub(crate) fn set_enabled(enabled: bool, session_filter: Option<String>, dir: PathBuf) {
    let mut guard = STATE.lock();
    if enabled {
        *guard = Some(CaptureState {
            session_filter,
            files: HashMap::new(),
            dir: Some(dir),
        });
    } else {
        *guard = None;
    }
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Record one raw chunk. Called from the PTY read path, so it must never panic
/// and never block on anything but its own short-lived lock: a capture that can
/// take a session down is worse than no capture.
pub(crate) fn record(session_id: &str, data: &[u8]) {
    if !is_enabled() {
        return;
    }
    let mut guard = STATE.lock();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if let Some(filter) = &state.session_filter
        && filter != session_id
    {
        return;
    }
    let Some(dir) = state.dir.clone() else {
        return;
    };
    let entry = match state.files.get_mut(session_id) {
        Some(entry) => entry,
        None => {
            if std::fs::create_dir_all(&dir).is_err() {
                return;
            }
            let path = dir.join(format!("{session_id}.raw"));
            let Ok(file) = std::fs::File::create(&path) else {
                return;
            };
            tracing::info!("[capture] recording {session_id} to {}", path.display());
            state
                .files
                .entry(session_id.to_string())
                .or_insert((file, 0))
        }
    };
    if entry.1 >= MAX_CAPTURE_BYTES {
        return;
    }
    let room = (MAX_CAPTURE_BYTES - entry.1) as usize;
    let slice = &data[..data.len().min(room)];
    if entry.0.write_all(slice).is_ok() {
        entry.1 += slice.len() as u64;
    }
}

/// Current tap state, for `GET /diagnostics/capture`.
pub(crate) fn status() -> serde_json::Value {
    let guard = STATE.lock();
    match guard.as_ref() {
        Some(state) => serde_json::json!({
            "enabled": true,
            "session_filter": state.session_filter,
            "dir": state.dir.as_ref().map(|d| d.display().to_string()),
            "sessions": state.files.iter()
                .map(|(id, (_, len))| serde_json::json!({ "session_id": id, "bytes": len }))
                .collect::<Vec<_>>(),
        }),
        None => serde_json::json!({ "enabled": false }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tap is one global switch, so these tests would otherwise disable each
    /// other mid-run under cargo's parallel harness.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Disabled is the default, and a disabled tap writes nothing — the state a
    /// shipped build must be in.
    #[test]
    fn disabled_tap_writes_nothing() {
        let _guard = TEST_LOCK.lock();
        let dir = std::env::temp_dir().join("tuic-capture-test-disabled");
        let _ = std::fs::remove_dir_all(&dir);
        set_enabled(false, None, dir.clone());
        record("session-a", b"hello");
        assert!(!dir.exists());
    }

    #[test]
    fn capture_respects_the_session_filter_and_the_size_cap() {
        let _guard = TEST_LOCK.lock();
        let dir = std::env::temp_dir().join("tuic-capture-test-filter");
        let _ = std::fs::remove_dir_all(&dir);
        set_enabled(true, Some("wanted".into()), dir.clone());

        record("wanted", b"\x1b]777;notify;Claude Code;waiting\x07");
        record("other", b"must not be recorded");
        // Overrun the cap in one oversized chunk.
        record("wanted", &vec![b'x'; (MAX_CAPTURE_BYTES + 1024) as usize]);
        set_enabled(false, None, dir.clone());

        let wanted = std::fs::read(dir.join("wanted.raw")).expect("filtered session recorded");
        assert!(wanted.starts_with(b"\x1b]777;notify"));
        assert_eq!(
            wanted.len() as u64,
            MAX_CAPTURE_BYTES,
            "capture must stop exactly at the cap"
        );
        assert!(
            !dir.join("other.raw").exists(),
            "a filtered-out session must not be touched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Re-enabling starts a new file rather than appending to the previous run:
    /// two runs concatenated replay as a stream that never existed.
    #[test]
    fn restarting_the_tap_truncates() {
        let _guard = TEST_LOCK.lock();
        let dir = std::env::temp_dir().join("tuic-capture-test-restart");
        let _ = std::fs::remove_dir_all(&dir);

        set_enabled(true, None, dir.clone());
        record("s", b"first run");
        set_enabled(false, None, dir.clone());

        set_enabled(true, None, dir.clone());
        record("s", b"second");
        set_enabled(false, None, dir.clone());

        assert_eq!(std::fs::read(dir.join("s.raw")).unwrap(), b"second");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
