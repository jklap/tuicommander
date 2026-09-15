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
pub(crate) fn sleep_script() -> &'static str {
    if cfg!(windows) {
        "ping -n 61 127.0.0.1 >nul"
    } else {
        "sleep 60"
    }
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
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.arg(flag);
    command.arg(print_file_script(&format!("\"{}\"", path.display())));
    command
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
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()),
        )
    } else {
        std::path::PathBuf::from("/tmp")
    }
}
