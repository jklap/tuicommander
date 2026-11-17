//! The smart-selection **Run Command** action in its "in background" mode: run a command line in
//! the background, in the user's own shell environment, with no terminal involved.
//!
//! This is the counterpart of **Send Text**, which types the same kind of text into a terminal.
//! Run Command is for things that need no terminal at all (`imark README.md`, `open -a Foo`,
//! `code path`): nothing is typed anywhere, so it cannot land in an agent's prompt, and nothing
//! blocks the UI.
//!
//! How untrusted text stays out of the shell's grammar — the point of the design:
//!
//! * The command line is POSIX `sh` syntax, always, and is executed by `/bin/sh` itself — never
//!   parsed by the user's login shell, whatever that is (fish, tcsh, nu and friends parse a quoted
//!   string differently, and one of them was measured to be escapable).
//! * Matched terminal text never appears *in* the command line. The frontend rewrites each
//!   placeholder into a variable reference (`"${TUIC_M0}"`, choosing the right form for the quoting
//!   around it) and sends the values separately in `env`. A shell expands a variable's value as data
//!   and never parses it again, so no quoting the template author chose can turn a value into
//!   syntax. Only the fixed names in [`is_allowed_env_name`] are accepted, so a caller cannot use
//!   `env` to set `PATH`, `LD_PRELOAD` or anything else.
//! * The user's login shell is used only to *build the environment*: it runs a constant wrapper
//!   (`exec /bin/sh -c "$TUIC_RUN_COMMAND"`) after reading its profile and rc files, and the command
//!   line reaches `sh` through that environment variable, not through the wrapper's text.
//!
//! Policy, and why it differs from `smart_prompt::execute_shell_script`:
//!
//! * **Environment: the user's, not an allowlist.** A script from a repo-controlled Smart Prompt
//!   runs with a stripped environment so it cannot read API keys; a smart-selection rule is the
//!   user's own and exists to launch their own tools, which need their `PATH` and config. A GUI app
//!   inherits launchd's minimal `PATH`, which would otherwise not contain `/opt/homebrew/bin`,
//!   `~/.local/bin` or whatever the user's `.zshrc` adds.
//! * **No output is kept, and none is tied to TUIC.** stdin, stdout and stderr are all `/dev/null`.
//!   Not a pipe, not a file: a pipe's read end dies with TUIC, and the next write to it kills the
//!   command with `SIGPIPE` — so a tool that logs after TUIC quits would die (measured), and a file
//!   would grow without bound for a chatty one. With nothing connected, quitting TUIC changes
//!   nothing for a running command: the OS hands it to `launchd` and it carries on. The price is
//!   that a failure reports only how the command ended, not what it printed; to see its output,
//!   run it in a terminal instead.
//! * **Fire and forget, but not blind.** The command is never killed and has no deadline (`imark`
//!   stays open as long as the review does). The caller waits [`GRACE`] so the failures that
//!   matter — command not found, bad usage, a missing file — are reported by exit status; anything
//!   still running after that is left running and reported as such.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long [`run_background_command`] waits for the command to end before leaving it running.
/// Long enough for a slow interactive-shell startup plus a quick failure, short enough that a
/// long-lived GUI tool is reported as "running" without the click feeling stuck.
pub(crate) const GRACE: Duration = Duration::from_secs(5);

/// Longest command line accepted. A rule template is a few hundred bytes.
const MAX_COMMAND_BYTES: usize = 16 * 1024;

/// Longest single value, and longest total of all values. A match is a token of terminal text.
const MAX_ENV_VALUE_BYTES: usize = 8 * 1024;
const MAX_ENV_TOTAL_BYTES: usize = 32 * 1024;

/// The environment variable that carries the command line to `sh` through the login-shell wrapper.
/// Set by this module, never accepted from a caller (it is not an allowed name).
#[cfg(not(windows))]
const RUN_COMMAND_VAR: &str = "TUIC_RUN_COMMAND";

/// What a login shell runs: a constant, containing no untrusted text, that hands the command line
/// (read from [`RUN_COMMAND_VAR`]) to `/bin/sh` by absolute path so a `sh` earlier on the user's
/// `PATH` cannot stand in. The same text is valid in zsh, bash, ksh and fish.
#[cfg(not(windows))]
const LOGIN_WRAPPER: &str = r#"exec /bin/sh -c "$TUIC_RUN_COMMAND""#;

/// What happened to the command within [`GRACE`].
// On Windows `run_in_shell` always returns `Err`, so no variant is ever constructed there.
#[cfg_attr(windows, allow(dead_code))]
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum BackgroundCommandOutcome {
    /// Exited with status 0.
    Succeeded,
    /// Still running when the grace period ended. It is left running, detached.
    Running,
    /// Exited non-zero, or was killed by a signal (`exit_code` is then `None`). By shell
    /// convention 127 is "command not found" and 126 "found but not executable".
    Failed { exit_code: Option<i32> },
}

/// How the command line is handed to a shell.
#[cfg(not(windows))]
#[derive(Debug, PartialEq, Eq)]
enum ShellPlan {
    /// Start the user's shell as a login *and* interactive shell so it reads the same profile and rc
    /// files a terminal tab does, running [`LOGIN_WRAPPER`]. Only for shells the wrapper text is
    /// known to be valid in.
    Login,
    /// Run `/bin/sh -c` directly with the app's own environment. For every other shell (tcsh/csh,
    /// nu, elvish, xonsh, pwsh, dash, an unknown one): the wrapper text is not known to be valid
    /// there, and `sh` does not need a login shell to understand the command line. The cost is that
    /// the user's login `PATH` is not applied.
    Direct,
}

fn basename(shell: &str) -> &str {
    Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
}

#[cfg(not(windows))]
fn shell_plan(shell: &str) -> ShellPlan {
    match basename(shell) {
        "zsh" | "bash" | "fish" | "ksh" => ShellPlan::Login,
        _ => ShellPlan::Direct,
    }
}

/// True when a command line TYPED into `shell` is parsed the way POSIX single-quoting assumes.
/// Verified for these: sh, dash, bash, zsh (in twelve option sets) and ksh. Not true of fish (a
/// backslash before a quote inside single quotes is an escape and was measured to break out), tcsh
/// and csh (history expansion, newline rules), nu, elvish, xonsh or pwsh.
fn typed_quoting_is_sound(shell: &str) -> bool {
    #[cfg(windows)]
    {
        let _ = shell;
        false
    }
    #[cfg(not(windows))]
    {
        matches!(basename(shell), "bash" | "zsh" | "ksh" | "dash" | "sh")
    }
}

/// Answer for [`run_command_shell_support`].
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct RunShellSupport {
    /// The shell's own name (`zsh`), not its path.
    pub shell: String,
    /// Whether Run Command may type its command line into a new terminal running this shell.
    pub supported: bool,
}

/// Can Run Command's "In a new terminal" mode be used with the shell a new terminal would run?
///
/// That mode types a line into an interactive shell, so unlike the background mode it depends on
/// how that shell parses quoting; see [`typed_quoting_is_sound`]. `shell` is the user's configured
/// shell, or `None` for the default (`$SHELL`) — only the backend knows that.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn run_command_shell_support(shell: Option<String>) -> RunShellSupport {
    let resolved = crate::pty::resolve_shell(shell.filter(|s| !s.trim().is_empty()));
    RunShellSupport {
        supported: typed_quoting_is_sound(&resolved),
        shell: match basename(&resolved) {
            "" => resolved.clone(),
            name => name.to_string(),
        },
    }
}

/// The only environment names a caller may set: `TUIC_M0`..`TUIC_M9` (the match and capture
/// groups), `TUIC_D` (the folder), `TUIC_U` (user) and `TUIC_H` (host).
fn is_allowed_env_name(name: &str) -> bool {
    matches!(name, "TUIC_D" | "TUIC_U" | "TUIC_H")
        || name
            .strip_prefix("TUIC_M")
            .is_some_and(|digit| digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit())
}

/// Check the values a caller wants set in the command's environment.
fn validate_env(env: &HashMap<String, String>) -> Result<(), String> {
    let mut total = 0usize;
    for (name, value) in env {
        if !is_allowed_env_name(name) {
            return Err(format!(
                "Environment variable {name:?} is not allowed (only TUIC_M0..TUIC_M9, TUIC_D, TUIC_U, TUIC_H)"
            ));
        }
        if value.contains('\0') {
            return Err(format!("Value of {name} contains a NUL byte"));
        }
        if value.len() > MAX_ENV_VALUE_BYTES {
            return Err(format!(
                "Value of {name} is longer than {MAX_ENV_VALUE_BYTES} bytes"
            ));
        }
        total += value.len();
    }
    if total > MAX_ENV_TOTAL_BYTES {
        return Err(format!(
            "Values are longer than {MAX_ENV_TOTAL_BYTES} bytes in total"
        ));
    }
    Ok(())
}

/// Check the request and resolve the working directory.
///
/// `cwd` is the terminal's directory the match came from, so a relative path in the match
/// (`docs/a.md`) resolves exactly as it would in that terminal. With none, the user's home.
fn validate(command: &str, cwd: Option<&str>) -> Result<PathBuf, String> {
    if command.trim().is_empty() {
        return Err("Command is empty".to_string());
    }
    if command.contains('\0') {
        return Err("Command contains a NUL byte".to_string());
    }
    if command.len() > MAX_COMMAND_BYTES {
        return Err(format!("Command is longer than {MAX_COMMAND_BYTES} bytes"));
    }
    let dir = match cwd.map(str::trim).filter(|c| !c.is_empty()) {
        Some(dir) => PathBuf::from(crate::cli::expand_tilde(dir)),
        None => PathBuf::from(crate::cli::expand_tilde("~")),
    };
    if !dir.is_dir() {
        return Err(format!(
            "Working directory does not exist: {}",
            dir.display()
        ));
    }
    Ok(dir)
}

/// Run `command` (POSIX `sh` syntax, matched text referenced as variables) per `shell`'s
/// [`ShellPlan`], from `cwd`, with `env` added to the environment; wait up to `grace`; report.
///
/// Split from [`run_background_command`] so a test can pass an explicit shell, a short grace and
/// extra environment (e.g. a scratch `HOME`, which `env` validation would refuse a caller).
#[cfg(not(windows))]
pub(crate) async fn run_in_shell(
    shell: &str,
    command: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
    grace: Duration,
) -> Result<BackgroundCommandOutcome, String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use tokio::process::Command;

    let mut cmd = match shell_plan(shell) {
        ShellPlan::Login => {
            let mut cmd = Command::new(shell);
            cmd.args(["-l", "-i", "-c", LOGIN_WRAPPER])
                .env(RUN_COMMAND_VAR, command);
            cmd
        }
        ShellPlan::Direct => {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg(command);
            cmd
        }
    };
    cmd.current_dir(cwd)
        .envs(env)
        // Nothing connected to TUIC: see the module docs for why this must not be a pipe or file.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Never kill it: a long-lived GUI tool must outlive this call, and dropping the handle
        // must not signal it.
        .kill_on_drop(false);
    // Its own process group, so it is not part of the app's and a signal aimed at the app's group
    // does not reach it.
    cmd.as_std_mut().process_group(0);

    let program = cmd.as_std().get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {program}: {e}"))?;

    match tokio::time::timeout(grace, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(BackgroundCommandOutcome::Succeeded),
        Ok(Ok(status)) => Ok(BackgroundCommandOutcome::Failed {
            exit_code: status.code(),
        }),
        Ok(Err(e)) => Err(format!("Failed while waiting for the command: {e}")),
        Err(_) => {
            // Still running: leave it, but keep reaping it so it does not linger as a zombie
            // while TUIC lives (if TUIC quits first, the OS reaps it).
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(BackgroundCommandOutcome::Running)
        }
    }
}

#[cfg(windows)]
pub(crate) async fn run_in_shell(
    _shell: &str,
    _command: &str,
    _cwd: &Path,
    _env: &HashMap<String, String>,
    _grace: Duration,
) -> Result<BackgroundCommandOutcome, String> {
    // DEFERRED (2026-11-16) — Windows. The user's default shell there is cmd or PowerShell, and the
    // command line is POSIX `sh` syntax run by `/bin/sh`, which does not exist. Needs a decision on
    // what Run Command means there (a bundled sh? PowerShell with its own variable syntax?) before
    // it can be offered.
    Err("Run Command is not supported on Windows yet".to_string())
}

/// Run `command` in the background, in the user's shell environment.
///
/// `command` is POSIX `sh` syntax; matched text must appear in it only as the variable references
/// whose values are in `env` (see the module docs). `cwd` is the directory to run from (the
/// terminal's); `None` means the user's home. Returns once the command ended or [`GRACE`] elapsed,
/// whichever is first — see [`BackgroundCommandOutcome`]. `Err` is only for a command that could
/// not be started at all, or a request that was refused.
///
/// `shell` is the shell the user chose in Settings → Shell, so this runs in the same shell a new
/// terminal would; `None` or empty means the default (`$SHELL`), as for a terminal.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn run_background_command(
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    shell: Option<String>,
) -> Result<BackgroundCommandOutcome, String> {
    let env = env.unwrap_or_default();
    let dir = validate(&command, cwd.as_deref())?;
    validate_env(&env)?;
    let shell = crate::pty::resolve_shell(shell.filter(|s| !s.trim().is_empty()));
    run_in_shell(&shell, &command, &dir, &env, GRACE).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
    }

    fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn env_names_are_limited_to_the_fixed_set() {
        for ok in [
            "TUIC_M0", "TUIC_M5", "TUIC_M9", "TUIC_D", "TUIC_U", "TUIC_H",
        ] {
            assert!(is_allowed_env_name(ok), "{ok}");
        }
        for bad in [
            "PATH",
            "HOME",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "BASH_ENV",
            "ENV",
            "ZDOTDIR",
            "TUIC_RUN_COMMAND",
            "TUIC_M",
            "TUIC_M10",
            "TUIC_Ma",
            "TUIC_MX0",
            "tuic_m0",
            "TUIC_D1",
            "TUIC_",
            "",
        ] {
            assert!(!is_allowed_env_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn validate_env_refuses_a_disallowed_name_a_nul_and_oversized_values() {
        assert!(validate_env(&HashMap::new()).is_ok());
        assert!(validate_env(&env_of(&[("TUIC_M0", "a b"), ("TUIC_D", "/x")])).is_ok());
        let err = validate_env(&env_of(&[("PATH", "/evil")])).unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
        assert!(
            validate_env(&env_of(&[("TUIC_M0", "a\0b")]))
                .unwrap_err()
                .contains("NUL")
        );
        let big = "x".repeat(MAX_ENV_VALUE_BYTES + 1);
        assert!(
            validate_env(&env_of(&[("TUIC_M0", &big)]))
                .unwrap_err()
                .contains("longer")
        );
        // Every value is individually fine, the total is not.
        let each = "x".repeat(MAX_ENV_VALUE_BYTES);
        let many: HashMap<String, String> = (0..10)
            .map(|i| (format!("TUIC_M{i}"), each.clone()))
            .collect();
        assert!(validate_env(&many).unwrap_err().contains("in total"));
    }

    #[test]
    fn validate_rejects_empty_nul_and_oversized_commands() {
        let dir = scratch();
        let cwd = dir.path().to_str().unwrap();
        assert!(validate("", Some(cwd)).unwrap_err().contains("empty"));
        assert!(validate("  \n ", Some(cwd)).unwrap_err().contains("empty"));
        assert!(validate("echo \0", Some(cwd)).unwrap_err().contains("NUL"));
        let long = "x".repeat(MAX_COMMAND_BYTES + 1);
        assert!(validate(&long, Some(cwd)).unwrap_err().contains("longer"));
        assert!(validate(&"x".repeat(MAX_COMMAND_BYTES), Some(cwd)).is_ok());
    }

    #[test]
    fn validate_resolves_the_working_directory() {
        let dir = scratch();
        let cwd = dir.path().to_str().unwrap();
        assert_eq!(validate("true", Some(cwd)).unwrap(), dir.path());
        // Not there, and not a directory: both are refused rather than silently run elsewhere.
        let missing = dir.path().join("nope");
        assert!(
            validate("true", Some(missing.to_str().unwrap()))
                .unwrap_err()
                .contains("does not exist")
        );
        let file = dir.path().join("f");
        std::fs::write(&file, "x").unwrap();
        assert!(validate("true", Some(file.to_str().unwrap())).is_err());
        // None or blank falls back to the home directory, which exists.
        assert!(validate("true", None).is_ok());
        assert!(validate("true", Some("   ")).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn only_shells_the_wrapper_text_is_valid_in_get_a_login_shell() {
        for shell in ["/bin/zsh", "/opt/homebrew/bin/bash", "fish", "/usr/bin/ksh"] {
            assert_eq!(shell_plan(shell), ShellPlan::Login, "{shell}");
        }
        // Everything else runs /bin/sh directly: tcsh/csh/nu/elvish/xonsh/pwsh parse the wrapper
        // differently, and sh/dash need no login shell to understand a sh command line.
        for shell in [
            "/bin/sh",
            "/bin/dash",
            "/bin/tcsh",
            "/bin/csh",
            "nu",
            "/usr/bin/elvish",
            "xonsh",
            "pwsh",
            "/weird/thing",
            "",
        ] {
            assert_eq!(shell_plan(shell), ShellPlan::Direct, "{shell:?}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn typed_quoting_is_trusted_only_for_the_shells_it_was_verified_in() {
        for shell in [
            "/bin/sh",
            "/bin/dash",
            "/bin/bash",
            "/bin/zsh",
            "ksh",
            "/opt/homebrew/bin/zsh",
        ] {
            assert!(typed_quoting_is_sound(shell), "{shell}");
        }
        for shell in [
            "fish",
            "/bin/tcsh",
            "/bin/csh",
            "nu",
            "elvish",
            "xonsh",
            "pwsh",
            "/weird/thing",
            "",
        ] {
            assert!(!typed_quoting_is_sound(shell), "{shell:?}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn the_shell_support_answer_names_the_shell_and_honours_an_explicit_choice() {
        let fish = run_command_shell_support(Some("/opt/homebrew/bin/fish".into()));
        assert_eq!(
            fish,
            RunShellSupport {
                shell: "fish".into(),
                supported: false
            }
        );
        let zsh = run_command_shell_support(Some("/bin/zsh".into()));
        assert_eq!(
            zsh,
            RunShellSupport {
                shell: "zsh".into(),
                supported: true
            }
        );
        // No explicit shell: the default is answered for, whatever it is.
        let default = run_command_shell_support(None);
        assert!(!default.shell.is_empty());
        // A blank setting counts as none.
        assert_eq!(run_command_shell_support(Some("  ".into())), default);
    }

    #[cfg(not(windows))]
    mod run {
        use super::*;

        const SH: &str = "/bin/sh";
        const QUICK: Duration = Duration::from_secs(5);

        async fn run(command: &str) -> BackgroundCommandOutcome {
            let dir = scratch();
            run_in_shell(SH, command, dir.path(), &HashMap::new(), QUICK)
                .await
                .unwrap()
        }

        async fn run_with(
            command: &str,
            dir: &Path,
            env: &HashMap<String, String>,
        ) -> BackgroundCommandOutcome {
            run_in_shell(SH, command, dir, env, QUICK).await.unwrap()
        }

        #[tokio::test]
        async fn a_command_that_exits_zero_succeeded() {
            assert_eq!(run("exit 0").await, BackgroundCommandOutcome::Succeeded);
        }

        #[tokio::test]
        async fn a_failing_command_reports_its_exit_code() {
            assert_eq!(
                run("exit 3").await,
                BackgroundCommandOutcome::Failed { exit_code: Some(3) }
            );
        }

        #[tokio::test]
        async fn a_missing_program_is_the_shells_127() {
            assert_eq!(
                run("tuic-no-such-program-xyz").await,
                BackgroundCommandOutcome::Failed {
                    exit_code: Some(127)
                }
            );
        }

        #[tokio::test]
        async fn a_command_killed_by_a_signal_has_no_exit_code() {
            assert_eq!(
                run("kill -9 $$").await,
                BackgroundCommandOutcome::Failed { exit_code: None }
            );
        }

        #[tokio::test]
        async fn nothing_is_connected_to_tuic_so_quitting_cannot_kill_it_with_sigpipe() {
            // The measured bug this design exists for: with a pipe, a command that writes after
            // TUIC has gone is killed by SIGPIPE. Here stdin/stdout/stderr must be the null
            // device — not pipes, not files, not a terminal.
            let check = "[ ! -p /dev/stdin ] && [ ! -p /dev/stdout ] && [ ! -p /dev/stderr ] \
                         && [ -c /dev/stdout ] && [ -c /dev/stderr ]";
            assert_eq!(run(check).await, BackgroundCommandOutcome::Succeeded);
        }

        #[tokio::test]
        async fn a_command_that_prints_a_lot_is_not_blocked_or_slowed() {
            // Twenty megabytes to each stream: with an undrained pipe this would block forever.
            let started = std::time::Instant::now();
            let out = run("yes | head -c 20000000; yes err | head -c 20000000 >&2; exit 0").await;
            assert_eq!(out, BackgroundCommandOutcome::Succeeded);
            assert!(started.elapsed() < Duration::from_secs(4));
        }

        #[tokio::test]
        async fn a_command_still_running_after_the_grace_period_is_left_running() {
            let dir = scratch();
            let started = std::time::Instant::now();
            let out = run_in_shell(
                SH,
                "exec sleep 2",
                dir.path(),
                &HashMap::new(),
                Duration::from_millis(150),
            )
            .await
            .unwrap();
            assert_eq!(out, BackgroundCommandOutcome::Running);
            // It returned at the grace period, not when the command ended.
            assert!(started.elapsed() < Duration::from_millis(1500));
        }

        #[tokio::test]
        async fn it_runs_in_the_given_directory() {
            let dir = scratch();
            // macOS reaches the temp dir through a symlink; compare the real locations.
            let real = std::fs::canonicalize(dir.path()).unwrap();
            let command = format!("[ \"$(pwd -P)\" = '{}' ]", real.to_str().unwrap());
            assert_eq!(
                run_with(&command, dir.path(), &HashMap::new()).await,
                BackgroundCommandOutcome::Succeeded
            );
        }

        #[tokio::test]
        async fn it_inherits_the_app_environment_rather_than_an_allowlist() {
            // The full PATH of this process, not a stripped one.
            let path = std::env::var("PATH").unwrap();
            let command = "[ \"$PATH\" = \"$TUIC_M0\" ]";
            let dir = scratch();
            let out = run_with(command, dir.path(), &env_of(&[("TUIC_M0", &path)])).await;
            assert_eq!(out, BackgroundCommandOutcome::Succeeded);
        }

        #[tokio::test]
        async fn a_backgrounded_child_does_not_hold_up_the_report() {
            let started = std::time::Instant::now();
            // The shell exits at once; the `sleep` it started carries on.
            let out = run("sleep 2 & exit 0").await;
            assert_eq!(out, BackgroundCommandOutcome::Succeeded);
            assert!(started.elapsed() < Duration::from_millis(1500));
        }

        #[tokio::test]
        async fn a_login_shell_that_cannot_start_is_an_error_not_a_failed_outcome() {
            let dir = scratch();
            // Named like a shell the wrapper supports, so it is spawned as the login shell.
            let err = run_in_shell(
                "/nonexistent/zsh",
                "true",
                dir.path(),
                &HashMap::new(),
                QUICK,
            )
            .await
            .unwrap_err();
            assert!(err.contains("Failed to start"), "{err}");
        }

        /// The point of the design: a value reaches the command as DATA, whatever the template
        /// author's quoting around the placeholder. Each case writes what the command saw into a
        /// file, and a marker file would appear if the value were ever parsed as shell syntax.
        #[tokio::test]
        async fn a_value_is_data_in_every_quoting_context_and_never_syntax() {
            let hostile = |marker: &Path| {
                [
                    format!("$(touch {})", marker.display()),
                    format!("`touch {}`", marker.display()),
                    format!("a; touch {}", marker.display()),
                    format!("'; touch {} #", marker.display()),
                    format!("\" ; touch {} #", marker.display()),
                    format!("\\' ; touch {} #", marker.display()),
                    format!("x\ntouch {}", marker.display()),
                ]
            };
            // How the frontend writes a placeholder, per the quoting around it in the template, and
            // what that command writes for a value `v`.
            type Expect = fn(&str) -> String;
            let contexts: [(&str, &str, Expect); 4] = [
                ("unquoted", "printf %s \"${TUIC_M0}\" > out", |v| {
                    v.to_string()
                }),
                (
                    "inside double quotes",
                    "printf %s \"<${TUIC_M0}>\" > out",
                    |v| format!("<{v}>"),
                ),
                (
                    // The frontend closes the single quote around the reference.
                    "inside single quotes",
                    "printf %s '<'\"${TUIC_M0}\"'>' > out",
                    |v| format!("<{v}>"),
                ),
                (
                    // The frontend does not model heredocs, so a heredoc body is written like any
                    // unquoted word: the reference carries its own double quotes, and they show up
                    // in the body. That is wrong for the author (a visible quote pair) but safe,
                    // and this case pins both halves: the value is still data, quotes included.
                    "heredoc body",
                    "cat > out <<EOF\n\"${TUIC_M0}\"\nEOF",
                    |v| format!("\"{v}\""),
                ),
            ];
            for (label, command, expect) in contexts {
                let dir = scratch();
                let marker = dir.path().join("pwned");
                for value in hostile(&marker) {
                    let out = run_with(command, dir.path(), &env_of(&[("TUIC_M0", &value)])).await;
                    assert_eq!(
                        out,
                        BackgroundCommandOutcome::Succeeded,
                        "{label}: {value:?}"
                    );
                    assert!(!marker.exists(), "{label}: value was executed: {value:?}");
                    let written = std::fs::read_to_string(dir.path().join("out")).unwrap();
                    let expected = expect(&value);
                    assert_eq!(
                        written.trim_end_matches('\n'),
                        expected.trim_end_matches('\n'),
                        "{label}"
                    );
                }
            }
        }

        #[tokio::test]
        async fn without_the_variable_indirection_the_same_text_would_have_run_a_command() {
            // Control for the test above: pasted INTO the command line, the same text executes.
            let dir = scratch();
            let marker = dir.path().join("pwned");
            let command = format!("printf %s $(touch {}) > out", marker.display());
            run_with(&command, dir.path(), &HashMap::new()).await;
            assert!(
                marker.exists(),
                "the control must show the injection is real"
            );
        }
    }

    /// The login-shell wrapper, through the REAL shells where they are installed. The values that
    /// broke POSIX single-quoting in fish must round-trip as data through every supported shell,
    /// because they are never parsed by it at all. A scratch `HOME`/`ZDOTDIR` keeps the developer's
    /// own rc files out of the test.
    #[cfg(not(windows))]
    mod wrapper {
        use super::*;

        fn find(name: &str) -> Option<PathBuf> {
            ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
                .iter()
                .map(|dir| Path::new(dir).join(name))
                .find(|p| p.is_file())
        }

        fn scratch_home(dir: &Path) -> HashMap<String, String> {
            let home = dir.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let mut env = HashMap::new();
            env.insert("HOME".into(), home.to_str().unwrap().into());
            env.insert("ZDOTDIR".into(), home.to_str().unwrap().into());
            env
        }

        #[tokio::test]
        async fn values_that_broke_fish_quoting_round_trip_as_data_in_every_installed_shell() {
            let mut tested = Vec::new();
            for name in ["zsh", "bash", "fish", "ksh"] {
                let Some(shell) = find(name) else {
                    eprintln!("skipping {name}: not installed");
                    continue;
                };
                let dir = scratch();
                let marker = dir.path().join("pwned");
                for value in [
                    format!("\\' ; touch {} #", marker.display()),
                    format!("$(touch {})", marker.display()),
                    format!("x(touch {})", marker.display()),
                    format!("/(e:'touch {}':)", marker.display()),
                ] {
                    let mut env = scratch_home(dir.path());
                    env.insert("TUIC_M0".into(), value.clone());
                    let command = "printf %s \"${TUIC_M0}\" > out";
                    let out = run_in_shell(
                        shell.to_str().unwrap(),
                        command,
                        dir.path(),
                        &env,
                        Duration::from_secs(10),
                    )
                    .await
                    .unwrap();
                    assert_eq!(
                        out,
                        BackgroundCommandOutcome::Succeeded,
                        "{name}: {value:?}"
                    );
                    assert!(!marker.exists(), "{name}: executed {value:?}");
                    assert_eq!(
                        std::fs::read_to_string(dir.path().join("out")).unwrap(),
                        value,
                        "{name}"
                    );
                }
                tested.push(name);
            }
            eprintln!("login-shell wrapper tested in: {tested:?}");
            // A host with none of them would otherwise pass without checking anything.
            assert!(
                !tested.is_empty(),
                "none of zsh, bash, fish or ksh was found, so the login-shell wrapper was not exercised"
            );
        }

        #[tokio::test]
        async fn the_command_line_is_run_by_bin_sh_whatever_the_login_shell_is() {
            // POSIX-only syntax that fish and tcsh would reject outright: it succeeds only if
            // /bin/sh, not the login shell, parsed the line.
            let mut tested = 0;
            for name in ["zsh", "bash", "fish", "ksh"] {
                let Some(shell) = find(name) else { continue };
                tested += 1;
                let dir = scratch();
                let env = scratch_home(dir.path());
                let out = run_in_shell(
                    shell.to_str().unwrap(),
                    "for i in 1 2 3; do : ; done; [ \"$(echo ok)\" = ok ] && exit 0 || exit 9",
                    dir.path(),
                    &env,
                    Duration::from_secs(10),
                )
                .await
                .unwrap();
                assert_eq!(out, BackgroundCommandOutcome::Succeeded, "{name}");
            }
            assert!(
                tested > 0,
                "no login shell was found, so nothing was exercised"
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_is_refused_with_a_clear_message() {
        let dir = scratch();
        let err = run_in_shell("cmd", "echo hi", dir.path(), &HashMap::new(), GRACE)
            .await
            .unwrap_err();
        assert!(err.contains("not supported on Windows"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn the_shell_the_caller_names_is_the_one_used_not_the_environments() {
        let dir = scratch();
        let cwd = Some(dir.path().to_str().unwrap().to_string());
        // `sh` takes the direct plan, so this runs.
        let out =
            run_background_command("exit 0".into(), cwd.clone(), None, Some("/bin/sh".into()))
                .await
                .unwrap();
        assert_eq!(out, BackgroundCommandOutcome::Succeeded);
        // A zsh that does not exist takes the login plan and cannot start. If `$SHELL` were used
        // instead, this would succeed.
        let err = run_background_command(
            "exit 0".into(),
            cwd,
            None,
            Some("/nonexistent-dir/zsh".into()),
        )
        .await
        .unwrap_err();
        assert!(err.contains("Failed to start"), "{err}");
    }

    #[test]
    fn the_outcome_serializes_in_the_shape_the_frontend_reads() {
        assert_eq!(
            serde_json::to_value(BackgroundCommandOutcome::Succeeded).unwrap(),
            serde_json::json!({"status": "succeeded"})
        );
        assert_eq!(
            serde_json::to_value(BackgroundCommandOutcome::Running).unwrap(),
            serde_json::json!({"status": "running"})
        );
        assert_eq!(
            serde_json::to_value(BackgroundCommandOutcome::Failed { exit_code: Some(2) }).unwrap(),
            serde_json::json!({"status": "failed", "exit_code": 2})
        );
        assert_eq!(
            serde_json::to_value(BackgroundCommandOutcome::Failed { exit_code: None }).unwrap(),
            serde_json::json!({"status": "failed", "exit_code": null})
        );
        assert_eq!(
            serde_json::to_value(RunShellSupport {
                shell: "zsh".into(),
                supported: true
            })
            .unwrap(),
            serde_json::json!({"shell": "zsh", "supported": true})
        );
    }
}
