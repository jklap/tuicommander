//! App-specific fixtures built on the shared platform-neutral test helpers.

pub(crate) use crate::fs::system32_exe;
pub(crate) use tuic_test_support::{
    chain, copy_file_script, dir_outside_home, fail_with_stderr_script, fake_ssh_script,
    host_shell, normalize_newlines, print_file_script, print_var_script, replay_file_command,
    short_socket_test_temp_root, slashed, sleep_argv, sleep_script, test_temp_root, touch_script,
};

/// Feed everything a PTY master produces to `sink`, in the small chunks the
/// production reader thread sees, and return once the master goes quiet.
///
/// Quiet, not EOF: a unix master reports EOF once the slave is closed and the
/// child has exited, but a Windows ConPTY master stays readable for as long as
/// the console host lives, so `read` never answers `Ok(0)` there. The three
/// tests that drove a real PTY with an EOF loop did not fail on Windows — they
/// hung, and nextest killed them at 120s.
///
/// The drain must also answer the terminal's side of the conversation, which is
/// why it takes the master's writer. A ConPTY's console host opens by asking
/// the terminal where the cursor is — DSR, `ESC[6n` — and writes nothing at all
/// until it gets a reply. With no reply these tests saw exactly those four
/// bytes and then silence, so the child's output never existed to assert on.
/// Production answers the same query through alacritty's `Event::PtyWrite`
/// (see `pty.rs`); a buffer under test has no terminal, so the drain is the
/// terminal.
///
/// Three named budgets, per the rule that one deadline may not serve two roles.
/// `STARTUP` waits for the first byte the *child* wrote — the handshake above
/// does not count, or the drain would start its quiet countdown against a
/// console host that has not spoken yet. `QUIET` decides the child has finished
/// writing, and `DEADLINE` is the outer bound that says the read itself is
/// stuck.
pub(crate) fn drain_pty(
    mut reader: Box<dyn std::io::Read + Send>,
    mut terminal: Box<dyn std::io::Write + Send>,
    mut sink: impl FnMut(&[u8]),
) {
    const STARTUP: std::time::Duration = std::time::Duration::from_secs(15);
    const QUIET: std::time::Duration = std::time::Duration::from_millis(500);
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    // Detached: on Windows this thread stays parked in `read` after the child
    // exits, and the test process is what ends it.
    std::thread::spawn(move || {
        // 64 bytes, so the parser under test sees a chunk boundary in the
        // middle of the escape sequences rather than one tidy buffer.
        let mut raw = [0u8; 64];
        loop {
            match reader.read(&mut raw) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(raw[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            }
        }
    });

    let deadline = std::time::Instant::now() + DEADLINE;
    let mut budget = STARTUP;
    while std::time::Instant::now() < deadline {
        // A timeout means the child stopped writing; a disconnect means the
        // reader reached EOF. Both end the drain.
        match rx.recv_timeout(budget) {
            Ok(chunk) => {
                let payload = answer_cursor_queries(&chunk, &mut terminal);
                // Nothing but the handshake: the child still has not spoken,
                // so the startup budget keeps running.
                if payload.is_empty() {
                    continue;
                }
                sink(&payload);
                budget = QUIET;
            }
            Err(_) => return,
        }
    }
}

/// Reply to every cursor-position query in `chunk` and return what is left.
///
/// The reply is always `1;1`: these tests open a fresh PTY and replay a file
/// into it, so the console host is asking about a cursor that has not moved.
/// The query is matched inside one chunk because the console host writes it as
/// a single write of its own, which the drain's read returns on its own.
fn answer_cursor_queries(chunk: &[u8], terminal: &mut (impl std::io::Write + ?Sized)) -> Vec<u8> {
    const QUERY: &[u8] = b"\x1b[6n";
    if !chunk.windows(QUERY.len()).any(|w| w == QUERY) {
        return chunk.to_vec();
    }
    let mut rest = Vec::with_capacity(chunk.len());
    let mut at = 0;
    while at < chunk.len() {
        if chunk[at..].starts_with(QUERY) {
            let _ = terminal.write_all(b"\x1b[1;1R");
            let _ = terminal.flush();
            at += QUERY.len();
        } else {
            rest.push(chunk[at]);
            at += 1;
        }
    }
    rest
}

// ---------------------------------------------------------------------------
// Sessions a test can deliver to
// ---------------------------------------------------------------------------
//
// These used to live in `pty::tests`, where only that module could reach them.
// Anything else that needed a session an agent could be delivered to — the
// hands-free dictation surface, for one — had no way to build one, and the
// alternative was making a test module public. They are the same helpers,
// moved; `pty::tests` imports them from here rather than keeping copies.

/// A session `session_is_agent` accepts, with its shell state and silence
/// tracker pre-seeded. `shell` is one of `pty`'s `SHELL_*` constants; an idle
/// one is confirmed idle, so the injection gate can claim it.
pub(crate) fn agent_session(state: &crate::state::AppState, sid: &str, shell: u8) {
    use std::sync::atomic::AtomicU8;
    state
        .session_maps
        .shell_states
        .insert(sid.to_string(), AtomicU8::new(shell));
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("claude".to_string()),
            ..Default::default()
        },
    );
    let mut silence = crate::pty::SilenceState::new();
    if shell == crate::pty::SHELL_IDLE {
        silence.confirm_idle();
    }
    state.session_maps.silence_states.insert(
        sid.to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(silence)),
    );
}

/// Everything written to a PTY, kept so a test can assert both what reached the
/// composer and what deliberately did not.
#[cfg(unix)]
pub(crate) struct RecordingWriter {
    pub(crate) bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

#[cfg(unix)]
impl std::io::Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The three line-discipline states `write_terminal_reply` must tell apart.
/// `Cbreak` is not a curiosity: it is the one where `ICANON` and `ECHO`
/// disagree, so it is the only case that can prove the gate keys on the right
/// flag. A fresh `openpty` is `Cooked`.
#[cfg(unix)]
#[derive(Clone, Copy)]
pub(crate) enum TtyMode {
    /// `ICANON` + `ECHO` — a reply is painted on screen and never delivered.
    Cooked,
    /// `ICANON` off, `ECHO` on — ugly, but the reply IS read immediately.
    Cbreak,
    /// Both off — what an agent sets before it queries.
    Raw,
}

/// A live PTY whose every byte is recorded.
#[cfg(unix)]
pub(crate) fn insert_recording_session(
    state: &crate::state::AppState,
    session_id: &str,
) -> std::sync::Arc<std::sync::Mutex<Vec<u8>>> {
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        state,
        session_id,
        Box::new(RecordingWriter {
            bytes: std::sync::Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );
    bytes
}

/// Register a real `openpty` session in `session_maps.sessions`, with its line
/// discipline set to `mode` and its writes going to `writer`.
#[cfg(unix)]
pub(crate) fn insert_session_with_writer(
    state: &crate::state::AppState,
    session_id: &str,
    writer: Box<dyn std::io::Write + Send>,
    mode: TtyMode,
) {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    if !matches!(mode, TtyMode::Cooked) {
        let fd = pair.master.as_raw_fd().expect("master fd");
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) }, 0);
        let mut termios = unsafe { termios.assume_init() };
        termios.c_lflag &= !libc::ICANON;
        if matches!(mode, TtyMode::Raw) {
            termios.c_lflag &= !libc::ECHO;
        }
        assert_eq!(
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) },
            0,
            "setting the line discipline must succeed"
        );
    }
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let child = pair.slave.spawn_command(command).expect("spawn shell");
    state.session_maps.sessions.insert(
        session_id.to_string(),
        parking_lot::Mutex::new(crate::state::PtySession {
            writer: Arc::new(parking_lot::Mutex::new(writer)),
            master: pair.master,
            _child: child,
            paused: Arc::new(AtomicBool::new(false)),
            worktree: None,
            cwd: None,
            display_name: None,
            display_name_is_custom: false,
            display_name_from_spawn: false,
            is_remote: false,
            shell: "/bin/sh".to_string(),
        }),
    );
}
