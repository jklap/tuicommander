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

/// Print the working directory.
pub(crate) fn print_cwd_script() -> &'static str {
    if cfg!(windows) { "cd" } else { "pwd" }
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

/// Print an environment variable, or nothing when it is unset.
pub(crate) fn print_var_script(key: &str) -> String {
    if cfg!(windows) {
        format!("if defined {key} (echo %{key}%)")
    } else {
        format!("echo \"${key}\"")
    }
}

/// The variable that holds the user's home directory.
pub(crate) fn home_var() -> &'static str {
    if cfg!(windows) { "USERPROFILE" } else { "HOME" }
}

/// Sleep long enough to outlive any timeout a test sets. `cmd` has no `sleep`,
/// and its `timeout` command refuses to run with stdin redirected, so the ping
/// idiom is the portable stand-in.
pub(crate) fn sleep_script() -> String {
    if cfg!(windows) {
        format!("{} -n 61 127.0.0.1 >nul", crate::fs::system32_exe("ping.exe"))
    } else {
        "sleep 60".to_string()
    }
}

/// A stock Windows tool, spelled as an absolute path. A test about a timeout or
/// a stdin filter must not depend on the host's `PATH` — see
/// [`crate::fs::system32_exe`], which the supervisor's own kill path uses for
/// the same reason.
pub(crate) use crate::fs::system32_exe;

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
