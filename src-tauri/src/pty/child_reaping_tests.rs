use super::*;
use portable_pty::{Child, ChildKiller, ExitStatus};
use std::io;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Observes a real OS wait; neither exit readiness nor the result is simulated.
#[derive(Debug)]
struct ObservedChild {
    child: std::process::Child,
    waited: mpsc::Sender<u32>,
}

impl ChildKiller for ObservedChild {
    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        self.child.clone_killer()
    }
}

impl Child for ObservedChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Child::try_wait(&mut self.child)
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = Child::wait(&mut self.child)?;
        let _ = self.waited.send(status.exit_code());
        Ok(status)
    }

    fn process_id(&self) -> Option<u32> {
        Some(self.child.id())
    }
}

fn insert_gated_child(
    state: &AppState,
    sid: &str,
) -> (std::process::ChildStdin, mpsc::Receiver<u32>, libc::pid_t) {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "read release; exit 7"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn gated shell");
    let input = child.stdin.take().expect("child stdin");
    assert!(child.try_wait().expect("poll child").is_none());
    let pid = child.id() as libc::pid_t;
    let (waited, receiver) = mpsc::channel();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let writer = pair.master.take_writer().expect("writer");
    state
        .metrics
        .active_sessions
        .fetch_add(1, Ordering::Relaxed);
    state.session_maps.sessions.insert(
        sid.to_string(),
        Mutex::new(PtySession {
            writer: Arc::new(Mutex::new(writer)),
            master: pair.master,
            _child: Box::new(ObservedChild { child, waited }),
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: true,
            shell: "/bin/sh".to_string(),
        }),
    );
    (input, receiver, pid)
}

fn assert_os_child_reaped(pid: libc::pid_t) {
    let mut status = 0;
    // SAFETY: only observes this test's own child and writes to a live integer.
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    assert_eq!(result, -1, "the session must reap its own child");
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

/// Catches: EOF/close drops the child after try_wait(None), leaving a zombie
/// when the real process exits later. Also prevents waiting under session locks.
#[test]
fn session_removal_reaps_child_that_exits_after_first_poll() {
    for explicit_close in [false, true] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = if explicit_close {
            "reap-close"
        } else {
            "reap-eof"
        };
        let (mut input, waited, pid) = insert_gated_child(&state, sid);
        if explicit_close {
            cleanup_session(sid, &state);
        } else {
            mark_session_exited(sid, &state);
        }
        assert!(!state.session_maps.sessions.contains_key(sid));
        assert_eq!(state.metrics.active_sessions.load(Ordering::Relaxed), 0);
        // The shell cannot exit until teardown has returned and this releases it.
        input.write_all(b"release\n").expect("release child");
        drop(input);
        assert_eq!(
            waited
                .recv_timeout(Duration::from_secs(30))
                .expect("removed child must still have an OS wait owner"),
            7
        );
        assert_os_child_reaped(pid);
    }
}

/// Catches: reaping already-completed children loses the captured exit status
/// or decrements active-session accounting twice on a repeated EOF/close.
#[test]
fn session_removal_preserves_already_reaped_exit_status() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "reap-already-exited";
    let (mut input, _waited, pid) = insert_gated_child(&state, sid);
    input.write_all(b"release\n").expect("release child");
    drop(input);
    state
        .session_maps
        .sessions
        .get(sid)
        .unwrap()
        .lock()
        ._child
        .wait()
        .expect("wait child");
    mark_session_exited(sid, &state);
    assert_eq!(*state.session_maps.exit_codes.get(sid).unwrap(), 7);
    mark_session_exited(sid, &state);
    assert_eq!(state.metrics.active_sessions.load(Ordering::Relaxed), 0);
    assert_os_child_reaped(pid);
}

/// Catches: removing scrollback owners leaves hundreds of MiB of freed glibc
/// arena pages resident, on explicit close or expiry of post-mortem buffers.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn linux_glibc_session_payload_release_returns_scrollback_pages() {
    fn resident_bytes() -> usize {
        let status = std::fs::read_to_string("/proc/self/status").expect("process memory status");
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .expect("resident memory")
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap()
            * 1024
    }

    for tombstone in [false, true] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut live_grid = VtLogBuffer::new(24, 80, 1);
        live_grid.process(b"still-live");
        state
            .grid
            .vt_log_buffers
            .insert("live-session".into(), Mutex::new(live_grid));
        let ids: Vec<_> = (0..6).map(|n| format!("rss-owner-{n}")).collect();
        // Production grids are allocated by separate PTY reader threads. Use
        // real threads and real grids so this exercises glibc's thread arenas.
        let workers: Vec<_> = ids
            .iter()
            .map(|id| {
                let state = Arc::clone(&state);
                let id = id.clone();
                std::thread::spawn(move || {
                    let mut grid = VtLogBuffer::new(24, 220, 10000);
                    grid.process(&b"row\r\n".repeat(10024));
                    state.grid.vt_log_buffers.insert(id, Mutex::new(grid));
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("scrollback allocator thread");
        }
        let before = resident_bytes();
        if tombstone {
            reap_tombstones(&state, &ids);
        } else {
            for id in &ids {
                cleanup_session(id, &state);
            }
        }
        assert_eq!(state.grid.vt_log_buffers.len(), 1);
        assert_eq!(
            state
                .grid
                .vt_log_buffers
                .get("live-session")
                .unwrap()
                .lock()
                .grid_get_row_text(0),
            "still-live"
        );
        let after = resident_bytes();
        // Six 10000-row, 220-column grids hold over 300 MiB. A deliberately
        // loose OS-level bound tolerates metadata/caches without accepting the
        // measured failure where all freed grid pages stay resident.
        assert!(
            before.saturating_sub(after) > 64 * 1024 * 1024,
            "freed scrollback must return resident pages: tombstone={tombstone}, before={before}, after={after}"
        );
    }
}
