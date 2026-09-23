//! Shell snippets and path shapes the tests need on every platform.
//!
//! The suite drives real child processes in a dozen modules, and until the
//! Windows job started running them every one of those tests spelled its script
//! in POSIX: `pwd`, `cat`, `touch`, `sleep`, `printenv`, `sh -c`. On Windows the
//! production code hands the same script to `cmd /C`, which understands none of
//! them, so the tests failed on the shell rather than on the behaviour they
//! exist to check.
//!
//! These helpers keep that difference in one place. They are deliberately
//! small: each returns the script the host shell understands for one step, so a
//! test still reads as the thing it asserts.

/// The shell and the flag that makes it read a script from its argv — the same
/// pair the production spawners pick.
pub(crate) fn host_shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Print a file's contents.
pub(crate) fn print_file_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type {path}")
    } else {
        format!("cat {path}")
    }
}

/// Create an empty file.
pub(crate) fn touch_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type nul > {path}")
    } else {
        format!("touch {path}")
    }
}

/// Copy a file. `cmd` has no `cp`, and its `copy` announces the file count on
/// stdout unless it is silenced.
pub(crate) fn copy_file_script(source: &str, destination: &str) -> String {
    if cfg!(windows) {
        format!("copy /y {source} {destination} >nul")
    } else {
        format!("cp {source} {destination}")
    }
}

/// Run one script and then another, in one command line. `cmd` separates with
/// `&`; in `sh` that would background the first.
pub(crate) fn chain(first: &str, second: &str) -> String {
    if cfg!(windows) {
        format!("{first}& {second}")
    } else {
        format!("{first}; {second}")
    }
}

/// Print an environment variable, or nothing when it is unset.
pub(crate) fn print_var_script(key: &str) -> String {
    if cfg!(windows) {
        format!("if defined {key} (echo %{key}%)")
    } else {
        format!("echo \"${key}\"")
    }
}

/// The program and arguments that sleep longer than any timeout a test sets.
/// `cmd` has no `sleep`, and its `timeout` command refuses to run with stdin
/// redirected, so the ping idiom is the portable stand-in.
///
/// A test that spawns the sleeper itself wants this rather than
/// [`sleep_script`]: a shell wrapper does not take its child with it when it
/// is killed, so `cmd /C ping` leaves the ping behind and nextest reports the
/// test as leaky.
pub(crate) fn sleep_argv() -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            crate::fs::system32_exe("ping.exe"),
            vec!["-n".to_string(), "61".to_string(), "127.0.0.1".to_string()],
        )
    } else {
        ("sleep".to_string(), vec!["60".to_string()])
    }
}

/// The same sleep, spelled for the host shell.
pub(crate) fn sleep_script() -> String {
    let (program, args) = sleep_argv();
    let call = format!("{program} {}", args.join(" "));
    if cfg!(windows) {
        format!("{call} >nul")
    } else {
        call
    }
}

/// A stock Windows tool, spelled as an absolute path. A test about a timeout or
/// a stdin filter must not depend on the host's `PATH` — see
/// [`crate::fs::system32_exe`], which the supervisor's own kill path uses for
/// the same reason.
pub(crate) use crate::fs::system32_exe;

/// Write a fake SSH executable to a stable path and warm it before use.
///
/// Fresh executable inodes can spend minutes in endpoint-security scanning on
/// macOS. Reusing one path per behavior keeps that scan outside test deadlines.
pub(crate) fn fake_ssh_script(name: &str, posix: &str, windows: &str) -> std::path::PathBuf {
    const WARMUP_VAR: &str = "TUIC_FAKE_SSH_WARMUP";

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-ssh");
    std::fs::create_dir_all(&dir).expect("create fake-ssh dir");
    let (extension, desired) = if cfg!(windows) {
        (
            "cmd",
            format!("@echo off\r\nif defined {WARMUP_VAR} exit /b 0\r\n{windows}\r\n"),
        )
    } else {
        (
            "sh",
            format!("#!/bin/sh\n[ -n \"${WARMUP_VAR}\" ] && exit 0\n{posix}\n"),
        )
    };
    let path = dir.join(format!("{name}.{extension}"));

    if std::fs::read_to_string(&path).is_ok_and(|found| found == desired) {
        return path;
    }

    let staging = dir.join(format!("{name}.{extension}.{}", std::process::id()));
    std::fs::write(&staging, &desired).expect("write fake ssh script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake ssh script");
    }
    std::fs::rename(&staging, &path).expect("install fake ssh script");

    let _ = std::process::Command::new(&path)
        .env(WARMUP_VAR, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    path
}

/// Write `line` to stderr and exit with `code`.
pub(crate) fn fail_with_stderr_script(line: &str, code: i32) -> String {
    if cfg!(windows) {
        format!("echo {line} 1>&2& exit /b {code}")
    } else {
        format!("echo {line} >&2; exit {code}")
    }
}

/// `cmd` ends every line with CRLF and `sh` with LF. No test here is about
/// which one the shell chose.
pub(crate) fn normalize_newlines(output: &str) -> String {
    output.replace("\r\n", "\n")
}

/// A command that replays a file's bytes into the PTY it is spawned on.
///
/// The tests that drive a real PTY need a specific byte stream, and spelling
/// that stream as a shell script means writing it twice — once in `sh`, once in
/// `cmd`, which has neither `printf` nor `seq`. Writing the bytes from Rust and
/// playing them back keeps one spelling and tests the same stream everywhere.
pub(crate) fn replay_file_command(path: &std::path::Path) -> portable_pty::CommandBuilder {
    let (shell, flag) = host_shell();
    let spelled = if cfg!(windows) {
        // Bare, because the quotes would never reach `cmd`. `CommandBuilder`
        // wraps any argument holding a space or a quote and escapes the inner
        // quotes as `\"`, which is the C runtime's convention and not one
        // `cmd` knows: it answered "The filename, directory name, or volume
        // label syntax is incorrect." The surviving wrapper is the pair
        // `cmd /C` strips by itself, so an unquoted path arrives intact.
        assert!(
            !path.to_string_lossy().contains(' '),
            "a space in {} cannot survive `cmd /C` through CommandBuilder; \
             give the test a temp directory without one",
            path.display(),
        );
        // Backslashes too: `Path::join` keeps whatever separator the literal
        // used, so a path built from a `"src/fixtures/…"` literal reaches
        // `cmd` half-and-half, and `cmd` reads `/fixtures` as a switch — "The
        // syntax of the command is incorrect."
        path.display().to_string().replace('/', "\\")
    } else {
        format!("\"{}\"", path.display())
    };
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.arg(flag);
    command.arg(print_file_script(&spelled));
    command
}

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

/// A path with `/` separators, whatever the host used. Tests spell the suffix
/// or fragment they expect once, in the form every platform can read, and
/// compare against this rather than against two spellings.
pub(crate) fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// A directory that exists and is outside the home directory. `/tmp` is neither
/// absolute nor outside home on Windows, where the temp directory lives under
/// the user profile.
pub(crate) fn dir_outside_home() -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
        )
    } else {
        std::path::PathBuf::from("/tmp")
    }
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
