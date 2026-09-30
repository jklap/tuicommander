use super::*;

/// Get the platform-appropriate default shell when no override is configured.
pub(crate) fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

/// Convert a Windows drive-letter path to a WSL `/mnt/` path.
/// E.g. `C:\Users\foo\repos` → `/mnt/c/Users/foo/repos`.
/// Returns the input unchanged if it's not a Windows drive-letter path.
pub(crate) fn windows_to_wsl_path(path: &str) -> String {
    let bytes = path.as_bytes();
    // Match "X:\" or "X:/" where X is an ASCII letter
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &path[3..].replace('\\', "/");
        format!("/mnt/{drive}/{rest}")
    } else {
        path.to_string()
    }
}

/// Check whether a shell string targets WSL (e.g. `wsl.exe`, `wsl.exe -d Ubuntu`).
/// Handles both forward-slash and backslash path separators so it works
/// correctly regardless of compilation target (cross-compiled from macOS/Linux).
pub(crate) fn is_wsl_shell(shell: &str) -> bool {
    let exe = shell.split_whitespace().next().unwrap_or("");
    // Extract filename from the last path separator (either / or \)
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    // Strip .exe extension if present
    let stem = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .unwrap_or(filename);
    stem.eq_ignore_ascii_case("wsl")
}

/// Remove parent-process preferences that must not become defaults for a new
/// independent PTY. Call this immediately after constructing the command so an
/// explicit per-agent environment may still restore the variable deliberately.
pub(crate) fn sanitize_pty_parent_env(cmd: &mut CommandBuilder) {
    // TUICommander may itself be launched from Codex, whose NO_COLOR belongs
    // to that parent process. Do not leak the opt-out into independent PTY
    // sessions. Commands can still request monochrome output through their own
    // explicit CLI flags or per-command environment.
    cmd.env_remove("NO_COLOR");

    // `make dev` launches through Cargo/mbx. These keys describe TUIC's build,
    // not the PTY's next build: Cargo's executable/package metadata (CARGO,
    // CARGO_BIN_NAME, CARGO_CRATE_NAME, CARGO_PRIMARY_PACKAGE, CARGO_MANIFEST_*,
    // CARGO_PKG_*, CARGO_BIN_EXE_*, CARGO_FEATURE_*, CARGO_CFG_*), output and
    // jobserver paths (CARGO_TARGET_DIR, CARGO_TARGET_TMPDIR, OUT_DIR,
    // CARGO_MAKEFLAGS), compiler settings (CARGO_INCREMENTAL,
    // CARGO_ENCODED_RUSTFLAGS, RUSTFLAGS, RUSTC, RUSTC_LINKER, RUSTC_WRAPPER,
    // RUSTC_WORKSPACE_WRAPPER, RUSTDOC),
    // build-script metadata (HOST, TARGET, PROFILE, NUM_JOBS, OPT_LEVEL, DEBUG,
    // HOST_CC, HOST_CXX, DEP_*), and mbx's MBX_* session state. Keep user
    // preferences such as CARGO_HOME.
    for key in [
        "CARGO",
        "CARGO_TARGET_DIR",
        "CARGO_TARGET_TMPDIR",
        "CARGO_MANIFEST_DIR",
        "CARGO_MANIFEST_PATH",
        "CARGO_MANIFEST_LINKS",
        "CARGO_PRIMARY_PACKAGE",
        "CARGO_BIN_NAME",
        "CARGO_CRATE_NAME",
        "CARGO_MAKEFLAGS",
        "CARGO_INCREMENTAL",
        "CARGO_ENCODED_RUSTFLAGS",
        "OUT_DIR",
        "RUSTFLAGS",
        "RUSTC",
        "RUSTC_LINKER",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTDOC",
        "HOST",
        "TARGET",
        "PROFILE",
        "NUM_JOBS",
        "OPT_LEVEL",
        "DEBUG",
        "HOST_CC",
        "HOST_CXX",
    ] {
        cmd.env_remove(key);
    }
    let build_keys: Vec<String> = cmd
        .iter_full_env_as_str()
        .filter(|&(key, _)| {
            [
                "CARGO_PKG_",
                "CARGO_BIN_EXE_",
                "CARGO_FEATURE_",
                "CARGO_CFG_",
                "DEP_",
                "MBX_",
            ]
            .iter()
            .any(|prefix| key.starts_with(prefix))
        })
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in build_keys {
        cmd.env_remove(key);
    }
}

/// Inject the Unix-style env vars that Claude Code / Ink need to detect
/// terminal capabilities (color, kitty keyboard protocol, etc.).
/// Give the PTY the identity its agent will announce, and record which terminal
/// currently backs it.
///
/// Every session-creating path must call this. Before it existed only `create_pty`
/// injected `TUIC_SESSION`, so a tab opened through the worktree, agent-spawn or
/// HTTP paths ran with no identity at all: its bridge sent no `x-tuic-session`
/// header, the server minted an MCP-scoped UUID at `register`, and that UUID
/// matched no PTY — leaving the agent addressable by mail but unreachable through
/// its own terminal.
///
/// `tuic_session` is the caller's stable identity when it has one (a desktop tab
/// persists it across restarts for `claude --resume $TUIC_SESSION` and for goose's
/// `--name`). Paths without one fall back to the PTY key itself, which makes
/// identity and terminal trivially the same value for those sessions.
pub(crate) fn bind_pty_identity(
    state: &AppState,
    cmd: &mut CommandBuilder,
    session_id: &str,
    tuic_session: Option<&str>,
) {
    let identity = tuic_session.unwrap_or(session_id);
    cmd.env("TUIC_SESSION", identity);
    match crate::attachments::prepare_session_directory(&state.data_dir, "pty", session_id) {
        Ok(directory) => {
            cmd.env("TUIC_ATTACHMENTS_DIR", &directory);
            if let Err(error) = crate::attachments::configure_gemini_trust(cmd, &directory) {
                tracing::warn!(session_id, %error, "Could not provision Gemini attachment trust");
            }
            state
                .session_maps
                .attachment_launch_dirs
                .insert(session_id.into(), directory);
        }
        Err(error) => tracing::warn!(session_id, %error, "Could not provision session attachments"),
    }

    // Manually typed Claude inherits the same per-agent preference as TUIC spawns.
    if crate::agent_hook_launch::prevents_alt_screen("claude") {
        cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", "1");
        // The agent view is a separate full-screen renderer with its own switch.
        cmd.env("CLAUDE_CODE_DISABLE_AGENT_VIEW", "1");
    }
    cmd.env(
        "TUIC_CONFIG_DIR",
        crate::config::config_dir().to_string_lossy().as_ref(),
    );
    // Use the socket the server actually bound, including collision fallbacks.
    // Recomputing from config would let isolated child bridges reach another instance.
    #[cfg(unix)]
    {
        let socket = state.bound_socket_path.read();
        if !socket.as_os_str().is_empty() {
            cmd.env("TUIC_SOCKET", socket.as_os_str());
        }
    }
    state.bind_live_pty(identity, session_id);
}

/// Apply the same Claude screen choice after PTY identity defaults on IPC,
/// HTTP and MCP paths. Caller environment is applied afterward, so an explicit
/// Claude setting retains precedence over the TUIC opt-out.
pub(crate) fn apply_agent_screen_env(
    cmd: &mut CommandBuilder,
    env: &std::collections::HashMap<String, String>,
) {
    if !crate::agent_hook_launch::prevents_alt_screen("claude") {
        return;
    }
    let mode = env
        .get("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN")
        .map(String::as_str)
        .unwrap_or("1");
    cmd.env("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN", mode);
}

/// Inject `TUIC_*` worktree/repo context (main checkout, branch, base ref,
/// etc. — see `script_env::ScriptContext`) into a PTY spawn, so a Run Script
/// typed into the new terminal — and every command a user types afterward —
/// can see it, the same way a Setup/Archive script or a Smart Prompt child
/// already can.
///
/// A PTY's env is fixed at spawn time: if the user later `cd`s to a different
/// worktree in this same tab, these vars keep describing the spawn cwd, not
/// wherever the shell currently is. `TUIC_SESSION` has the same property, so
/// this is consistent with the rest of the terminal's identity — don't try to
/// keep it live off OSC 7, a running process's environment can't be mutated
/// from outside it.
///
/// Every `bind_pty_identity` call site should also call this one, immediately
/// after, passing the same `cwd` the PTY itself is about to be spawned in.
pub(crate) fn inject_worktree_env(cmd: &mut CommandBuilder, cwd: Option<&str>) {
    let Some(cwd) = cwd else {
        return;
    };
    let expanded = crate::cli::expand_tilde(cwd);
    crate::script_env::ScriptContext::derive(
        crate::script_env::ScriptKind::Run,
        std::path::Path::new(&expanded),
    )
    .apply_pty(cmd);
}

fn inject_unix_terminal_env(cmd: &mut CommandBuilder) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Signal kitty keyboard protocol support so apps (e.g. Claude Code / Ink)
    // detect it via heuristic precheck and proceed to query confirmation.
    cmd.env("KITTY_WINDOW_ID", "1");
    // Announce as ghostty so Claude Code's terminal detection allow-list
    // enables kitty keyboard protocol. CC ≥v2.1.52 only recognizes
    // WezTerm, ghostty, and iTerm.app — "kitty" was removed from the list.
    // ghostty is chosen because it has no iTerm/WezTerm-specific side effects.
    // On macOS this also prevents /etc/zshrc sourcing zshrc_Apple_Terminal.
    cmd.env("TERM_PROGRAM", "ghostty");
    // iTerm2 feature-reporting protocol: advertise capabilities so tools
    // (cargo, uv, mise, etc.) can detect support without a TERM_PROGRAM whitelist.
    // T2=24-bit color, P=OSC 9;4 progress, H=OSC 8 hyperlinks, U=unicode,
    // B=bracketed paste, Sy=synchronized output, M=mouse, F=focus reporting.
    cmd.env("TERM_FEATURES", "T2PHUBSyMF");
    // CC also checks TERM_PROGRAM_VERSION — missing or matching /^[0-2]\./
    // causes rejection.  Use a value that passes the gate.
    cmd.env("TERM_PROGRAM_VERSION", "3.0.0");
    // Prevent nested-session detection when TUICommander itself runs
    // inside a Claude Code session (CLAUDECODE env var would propagate).
    cmd.env_remove("CLAUDECODE");
    if let Ok(lang) = std::env::var("LANG") {
        cmd.env("LANG", lang);
    } else {
        // Fallback: ensure UTF-8 is available even when LANG is completely unset
        cmd.env("LANG", "en_US.UTF-8");
    }
    // Agent Teams: always inject feature flag so CC unlocks team tools
    cmd.env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
    // Bare `imgcat`/`imgls`/`divider` on PATH (color-tools plan, Phase 9) —
    // `tuic imgcat`/etc. already work without this; these shims exist purely
    // so a user (or script) coming from iTerm2's own conventions finds the
    // bare names too. Silently absent if the `tuic` sidecar can't be
    // resolved (e.g. a from-source checkout with no built sidecar) — never
    // worth failing a PTY spawn over. Unix only: this function also runs for a
    // WSL shell on Windows, where a host path on PATH would be meaningless.
    #[cfg(all(feature = "desktop", unix))]
    if let Some(shim_dir) = crate::image_cli_shims::shim_dir() {
        prepend_path_entry(cmd, shim_dir);
    }
}

/// Put `dir` first on the command's PATH, preserving every other entry once.
/// Reads the PATH already set on `cmd` (falling back to the process PATH), the
/// same composition `shell_integration`'s agent-shim install uses, so neither
/// prepend can drop the other's entry whichever runs first.
#[cfg(all(feature = "desktop", unix))]
fn prepend_path_entry(cmd: &mut CommandBuilder, dir: &std::path::Path) {
    let path = cmd
        .get_env("PATH")
        .map(|path| path.to_os_string())
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut entries = vec![dir.to_path_buf()];
    entries.extend(std::env::split_paths(&path).filter(|entry| entry != dir));
    if let Ok(joined) = std::env::join_paths(entries) {
        cmd.env("PATH", joined);
    }
}

#[cfg(all(test, feature = "desktop", unix))]
mod inject_unix_terminal_env_tests {
    use super::*;

    // Catches: the image-shim prepend discarding a PATH already set on the command.
    #[test]
    fn prepend_path_entry_keeps_the_commands_own_path_and_dedupes() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("PATH", "/opt/shim:/custom/bin:/usr/bin");
        prepend_path_entry(&mut cmd, std::path::Path::new("/opt/shim"));
        assert_eq!(
            cmd.get_env("PATH").unwrap().to_string_lossy(),
            "/opt/shim:/custom/bin:/usr/bin"
        );
        prepend_path_entry(&mut cmd, std::path::Path::new("/img"));
        assert_eq!(
            cmd.get_env("PATH").unwrap().to_string_lossy(),
            "/img:/opt/shim:/custom/bin:/usr/bin"
        );
    }

    /// The image-CLI shim dir, when resolvable, must be *prepended* (not
    /// appended or set alone) so it wins over any same-named binary already
    /// on the user's `PATH`, while every existing entry is preserved.
    ///
    /// `image_cli_shims::shim_dir()` caches its result in a process-wide
    /// `OnceLock` on first call, so the `set_config_dir_override` below only
    /// actually redirects it away from the real config dir when THIS is the
    /// first call in the process — true under `cargo nextest run` (one
    /// process per test, the repo's real gate), not guaranteed under a bare
    /// `cargo test` if another test calls `shim_dir()` first. Same caveat
    /// `config.rs`'s own `CONFIG_DIR_OVERRIDE` tests already carry.
    #[test]
    fn prepends_the_image_shim_dir_to_path_when_resolvable() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        // This test only asserts something meaningful when the dev/test
        // environment can actually resolve a `tuic` sidecar (same
        // requirement `resolve_sidecar_path` has everywhere else) — skip
        // cleanly rather than fail the build in an environment with none.
        let Some(shim_dir) = crate::image_cli_shims::shim_dir() else {
            return;
        };
        let mut cmd = CommandBuilder::new("/bin/sh");
        inject_unix_terminal_env(&mut cmd);
        let path = cmd
            .get_env("PATH")
            .expect("PATH must be set when a shim dir was resolved")
            .to_string_lossy()
            .to_string();
        assert!(
            path.starts_with(&shim_dir.to_string_lossy().to_string()),
            "shim dir must be the first PATH entry, got {path}"
        );
    }

    /// None of this module's own tests (or `build_shell_command`'s) assert any
    /// of the fixed-value vars `inject_unix_terminal_env` exists to set — only
    /// the `PATH` prepend above, which self-skips in an environment with no
    /// resolvable sidecar. A regression dropping any of these (e.g. losing the
    /// `TERM_PROGRAM_VERSION` version-gate value described in this function's
    /// own doc comment) would pass the suite silently.
    #[test]
    fn sets_the_fixed_value_terminal_capability_vars() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        inject_unix_terminal_env(&mut cmd);
        let get = |k: &str| cmd.get_env(k).map(|v| v.to_string_lossy().to_string());
        assert_eq!(get("TERM"), Some("xterm-256color".to_string()));
        assert_eq!(get("COLORTERM"), Some("truecolor".to_string()));
        assert_eq!(get("KITTY_WINDOW_ID"), Some("1".to_string()));
        assert_eq!(get("TERM_PROGRAM"), Some("ghostty".to_string()));
        assert_eq!(get("TERM_FEATURES"), Some("T2PHUBSyMF".to_string()));
        assert_eq!(get("TERM_PROGRAM_VERSION"), Some("3.0.0".to_string()));
        assert_eq!(
            get("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"),
            Some("1".to_string())
        );
    }

    #[test]
    fn removes_claudecode_to_prevent_nested_session_detection() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("CLAUDECODE", "1"); // simulate TUICommander itself running inside Claude Code
        inject_unix_terminal_env(&mut cmd);
        assert!(
            cmd.get_env("CLAUDECODE").is_none(),
            "CLAUDECODE must be removed so a nested PTY isn't mistaken for the same session"
        );
    }

    #[test]
    fn lang_preserves_an_existing_value() {
        let guard = TestEnvVar::set("LANG", "fr_FR.UTF-8");
        let mut cmd = CommandBuilder::new("/bin/sh");
        inject_unix_terminal_env(&mut cmd);
        assert_eq!(
            cmd.get_env("LANG").map(|v| v.to_string_lossy().to_string()),
            Some("fr_FR.UTF-8".to_string())
        );
        drop(guard);
    }

    #[test]
    fn lang_falls_back_to_utf8_when_unset() {
        let guard = TestEnvVar::unset("LANG");
        let mut cmd = CommandBuilder::new("/bin/sh");
        inject_unix_terminal_env(&mut cmd);
        assert_eq!(
            cmd.get_env("LANG").map(|v| v.to_string_lossy().to_string()),
            Some("en_US.UTF-8".to_string()),
            "a completely unset LANG must still fall back to a UTF-8 locale"
        );
        drop(guard);
    }

    /// Save/restore a real process env var for the duration of a test. `LANG`
    /// (and `SHELL`/`COMSPEC` for `resolve_shell`/`default_shell`'s own tests)
    /// are read directly from the process environment with no test-only
    /// override hook, unlike `config_dir()`'s `CONFIG_DIR_OVERRIDE` — this is
    /// the smallest reasonable substitute, same shape as `github_auth.rs`'s
    /// own private `EnvVar` test guard.
    pub(super) struct TestEnvVar(&'static str, Option<String>);

    impl TestEnvVar {
        pub(super) fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) };
            Self(key, previous)
        }

        pub(super) fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self(key, previous)
        }
    }

    impl Drop for TestEnvVar {
        fn drop(&mut self) {
            match self.1.take() {
                Some(previous) => unsafe { std::env::set_var(self.0, previous) },
                None => unsafe { std::env::remove_var(self.0) },
            }
        }
    }
}

/// Attempts made before a PTY spawn is reported as failed.
pub(crate) const PTY_SPAWN_ATTEMPTS: usize = 3;

/// Open a PTY pair and spawn a command into it, retrying transient allocation failures.
///
/// Story 059 added this retry to `create_pty` after a spawn regression, but the
/// other six production spawn sites kept a single `openpty`/`spawn_command` and
/// failed hard — so whether a burst of tab creation survived a momentarily
/// exhausted PTY table depended on *which* code path opened the terminal. This
/// helper is deliberately the retry policy and nothing else: the sites diverge
/// for real reasons (dimension clamping, shell-integration injection, env
/// sanitising, cwd inheritance) and unifying past this point would force a false
/// abstraction.
///
/// Command-spawn failures are never retried: invalid binaries, cwd, permissions,
/// and arguments do not become valid after sleeping. Async entry points use the
/// companion async wrapper so this bounded blocking backoff runs only on Tokio's
/// blocking pool.
///
/// This is also the one place the user's `custom_pty_env` setting is applied
/// (`apply_custom_pty_env`) and where `TUIC_PTY_TTY` gets stamped onto the child's
/// environment — every production caller opens its pty here, so it's the only
/// point guaranteed to run after every caller's own `cmd.env(...)` calls (which
/// happen inside `build_command()`), and, for `TUIC_PTY_TTY` specifically, the
/// only point that has the master handle (and therefore `tty_name()`) available.
/// Order is: caller env → `custom_pty_env` (can override anything above) →
/// `TUIC_PTY_TTY` (always last, never overridable).
pub(crate) fn spawn_pty_pair_with_retry<F>(
    size: PtySize,
    build_command: F,
) -> Result<
    (
        portable_pty::PtyPair,
        Box<dyn portable_pty::Child + Send + Sync>,
    ),
    String,
>
where
    F: FnOnce() -> CommandBuilder,
{
    let pty_system = native_pty_system();
    let pair = retry_transient(
        || pty_system.openpty(size),
        is_transient_pty_open_error,
        |attempt| {
            std::thread::sleep(std::time::Duration::from_millis(100 * attempt as u64));
        },
    )
    .map_err(|(attempt, error)| format!("Failed to open PTY (attempt {attempt}): {error}"))?;

    let mut cmd = build_command();
    // Global, user-configured — the one universal env-injection point. See
    // `apply_custom_pty_env`'s doc comment for why this runs here and not inside
    // `build_shell_command` (which only three of the six production spawn sites call).
    apply_custom_pty_env(&mut cmd);
    // Claude Code (and the other agents this drives) spawns its hook
    // subprocesses detached from any controlling terminal, so a hook cannot
    // discover this tty by walking its own ancestry (see
    // `crates/tuic-hook/src/tty.rs`). We created the pty and already know its
    // device path — hand it over explicitly rather than make the hook guess.
    // `tty_name()` is `#[cfg(unix)]`; hook instrumentation stays unvalidated
    // on Windows regardless (see `tty.rs`'s own Windows fallback comment).
    //
    // Two known, currently-unhandled limitations, named here rather than
    // rediscovered: (1) no multi-pane/tmux disambiguation — two agents in two
    // tmux panes under one outer pty share one TUIC_PTY_TTY, with no way to
    // attribute an emission to the right pane; (2) device-path staleness — an
    // escaped daemonized grandchild can hold a stale TUIC_PTY_TTY after its
    // pty closes, and the OS can reassign that device path to an unrelated
    // later session. Both narrow and low-severity; see todo.md.
    #[cfg(unix)]
    if let Some(tty) = pair.master.tty_name() {
        cmd.env("TUIC_PTY_TTY", tty.to_string_lossy().as_ref());
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|error| format!("Failed to spawn shell: {error}"))?;
    Ok((pair, child))
}

/// Apply the user's configured `custom_pty_env` (`AppConfig::custom_pty_env`) to a
/// spawned PTY child — the one true "every PTY" injection point, called from
/// `spawn_pty_pair_with_retry`/`_async` immediately before the `TUIC_PTY_TTY` stamp
/// so it reaches all six production spawn sites, not just the three that go
/// through `build_shell_command`.
///
/// Applied *after* every caller's own env (identity, worktree, terminal env, shell
/// integration, `PtyConfig::env`) so a custom var is a real override — the
/// motivating use case is "force this var, period," not a fallback default — but
/// *before* `TUIC_PTY_TTY`, which stays unconditional: it is internal plumbing the
/// app needs correct for its own bookkeeping and must never be user-overridable.
///
/// Backend re-validation, never trust the file: `config::valid_custom_env_key`/
/// `valid_custom_env_value` reject only what the OS cannot represent as an env var
/// (an empty/malformed key, a NUL byte) — no name denylist. See
/// `AppConfig::custom_pty_env`'s doc comment for why a denylist is deliberately
/// absent here unlike `copy_paths`/`additional_readable_dirs`.
pub(crate) fn apply_custom_pty_env(cmd: &mut CommandBuilder) {
    for entry in crate::config::resolve_custom_pty_env() {
        if !crate::config::valid_custom_env_key(&entry.key) {
            tracing::warn!(
                "Skipping invalid custom_pty_env key {:?}: not a valid env var name",
                entry.key
            );
            continue;
        }
        if !crate::config::valid_custom_env_value(&entry.value) {
            tracing::warn!(
                "Skipping custom_pty_env entry {:?}: value contains a NUL byte",
                entry.key
            );
            continue;
        }
        cmd.env(&entry.key, &entry.value);
    }
}

pub(super) fn retry_transient<T, E, O, C, S>(
    mut operation: O,
    is_transient: C,
    mut sleep_before_retry: S,
) -> Result<T, (usize, E)>
where
    O: FnMut() -> Result<T, E>,
    C: Fn(&E) -> bool,
    S: FnMut(usize),
{
    for attempt in 1..=PTY_SPAWN_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if attempt < PTY_SPAWN_ATTEMPTS && is_transient(&error) => {
                sleep_before_retry(attempt);
            }
            Err(error) => return Err((attempt, error)),
        }
    }
    unreachable!("bounded retry loop always returns")
}

pub(super) fn is_transient_pty_open_error(error: &anyhow::Error) -> bool {
    let Some(io_error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
    else {
        return false;
    };
    if matches!(
        io_error.kind(),
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    let Some(code) = io_error.raw_os_error() else {
        return false;
    };
    #[cfg(unix)]
    if matches!(
        code,
        libc::EAGAIN | libc::EINTR | libc::EMFILE | libc::ENFILE | libc::ENOSPC | libc::ENXIO
    ) {
        return true;
    }
    #[cfg(windows)]
    if matches!(code, 8 | 14 | 170 | 1450 | 1816) {
        return true;
    }
    false
}

/// Run the synchronous PTY allocation policy without occupying an async worker.
pub(crate) async fn spawn_pty_pair_with_retry_async<F>(
    size: PtySize,
    build_command: F,
) -> Result<
    (
        portable_pty::PtyPair,
        Box<dyn portable_pty::Child + Send + Sync>,
    ),
    String,
>
where
    F: FnOnce() -> CommandBuilder + Send + 'static,
{
    run_pty_spawn_blocking(move || spawn_pty_pair_with_retry(size, build_command)).await
}

pub(super) async fn run_pty_spawn_blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("PTY spawn task panicked: {error}"))?
}

/// Build a CommandBuilder for the given shell with platform-appropriate flags.
///
/// The `shell` string may contain arguments (e.g. `wsl.exe -d Ubuntu`).
/// The first whitespace-delimited token is the executable; the rest are args.
pub(crate) fn build_shell_command(shell: &str) -> CommandBuilder {
    let mut parts = shell.split_whitespace();
    let exe = parts.next().unwrap_or(shell);
    #[allow(unused_mut)]
    let mut cmd = CommandBuilder::new(exe);
    sanitize_pty_parent_env(&mut cmd);
    for arg in parts {
        cmd.arg(arg);
    }

    #[cfg(not(windows))]
    {
        // Login shell flag is Unix-only; PowerShell/cmd.exe don't support -l
        cmd.arg("-l");
        inject_unix_terminal_env(&mut cmd);
    }

    #[cfg(windows)]
    {
        // On Windows, if the shell targets WSL, inject Unix-style env vars
        // so that tools inside WSL (Claude Code, etc.) detect terminal
        // capabilities correctly. These are passed through to the Linux
        // environment by wsl.exe.
        if is_wsl_shell(shell) {
            inject_unix_terminal_env(&mut cmd);
        }
    }

    cmd
}

/// Niceness applied to every PTY child process. A child inherits the parent's
/// nice value at fork time, so deprioritizing the shell deprioritizes every
/// process it later spawns — compilers, bundlers, test runners. The intent is
/// that a heavy `cargo build` yields CPU to TUIC's own render thread and the
/// rest of the system *under contention*, while still running at full speed on
/// an idle machine (`nice` only bites when something else wants the core).
///
/// +10 was chosen over macOS QoS-background (`taskpolicy -b`), which pins the
/// workload to the E-cores on Apple Silicon and makes builds crawl even when
/// the P-cores are idle.
///
/// Overridable at launch via `TUIC_PTY_NICE` so the right value can be tuned on
/// the real app without recompiling (nice 0..19; values outside that range are
/// clamped by the kernel).
#[cfg(unix)]
const PTY_CHILD_NICE_DEFAULT: i32 = 10;

/// Resolve the nice value to apply to PTY children: `TUIC_PTY_NICE` env override
/// if set and parseable, else [`PTY_CHILD_NICE_DEFAULT`].
#[cfg(unix)]
fn pty_child_nice() -> i32 {
    std::env::var("TUIC_PTY_NICE")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(PTY_CHILD_NICE_DEFAULT)
}

/// Lower the scheduling priority of a freshly-spawned PTY child so the workloads
/// it spawns don't starve TUIC and the system.
///
/// Failure is logged and ignored: a build at the default priority is a degraded
/// experience, not a broken one. Lowering priority on a process owned by the
/// same user is always permitted, so a non-zero return here is unexpected.
///
/// Unix (macOS, Linux): `setpriority` to nice +10.
#[cfg(unix)]
pub(super) fn lower_pty_child_priority(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    let nice = pty_child_nice();
    // SAFETY: setpriority takes scalar args and is async-signal-safe; `pid` is
    // the id of the child we just spawned.
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc != 0 {
        tracing::warn!(
            pid,
            nice,
            error = %std::io::Error::last_os_error(),
            "failed to lower PTY child priority"
        );
    }
}

/// Windows: `BELOW_NORMAL_PRIORITY_CLASS` — the priority-class analog of nice
/// +10. NOT `IDLE_PRIORITY_CLASS`, which only runs the process when the system
/// is otherwise idle (the Windows equivalent of macOS QoS-background) and would
/// make builds crawl. macOS/Windows lack hard CPU affinity that works on the
/// primary target, so priority lowering is the one strategy portable to all
/// three platforms.
#[cfg(windows)]
pub(super) fn lower_pty_child_priority(pid: Option<u32>) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_SET_INFORMATION, SetPriorityClass,
    };
    let Some(pid) = pid else { return };
    // SAFETY: Win32 calls with scalar/handle args; `pid` is the id of the child
    // we just spawned. The handle is closed on every path once obtained.
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            tracing::warn!(
                pid,
                error = %std::io::Error::last_os_error(),
                "failed to open PTY child to lower priority"
            );
            return;
        }
        if SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS) == 0 {
            tracing::warn!(
                pid,
                error = %std::io::Error::last_os_error(),
                "failed to lower PTY child priority"
            );
        }
        CloseHandle(handle);
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn lower_pty_child_priority(_pid: Option<u32>) {}

/// Resolve the shell to use: explicit override > env default > platform default.
pub(crate) fn resolve_shell(override_shell: Option<String>) -> String {
    let shell = override_shell.unwrap_or_else(default_shell);
    crate::cli::expand_tilde(&shell)
}

/// Which family of shell is running inside a PTY.
///
/// Used by the frontend to decide whether control characters like Ctrl-U are
/// honoured (POSIX readline) or echoed literally (`cmd.exe`, PowerShell).
/// Classifying by the shell command rather than by host OS is the whole point
/// of story 1274-2e38: Git Bash, Cygwin, MSYS and WSL all run on Windows yet
/// support Ctrl-U, so a host-OS check alone is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ShellFamily {
    /// POSIX shell with readline semantics: sh, bash, zsh, fish, dash, ksh,
    /// and friends — including WSL (spawns a Linux shell) and Git Bash /
    /// Cygwin / MSYS (bash compiled for Windows).
    Posix,
    /// Native Windows shell that treats Ctrl-U as a literal character:
    /// cmd.exe, PowerShell, pwsh.
    WindowsNative,
    /// Shell basename didn't match any known set. Callers should fall back to
    /// the safer default for their host (on Windows: skip Ctrl-U; on
    /// Unix: send it).
    Unknown,
}

/// Classify a shell command string (as passed to `portable_pty`) into a
/// [`ShellFamily`]. Pure function — no I/O, no env lookups — so it's easy to
/// test against the set of strings the UI actually produces.
///
/// Parses the leading binary path first (supports Windows paths with spaces
/// like `C:\Program Files\Git\bin\bash.exe`), then matches the basename
/// case-insensitively with any `.exe` suffix stripped.
pub(crate) fn classify_shell(cmd: &str) -> ShellFamily {
    let trimmed = cmd.trim().trim_matches('"');
    // Locate the binary portion: if there's a case-insensitive `.exe`, take
    // everything up to and including it; otherwise split on first whitespace.
    // This keeps `C:\Program Files\...\bash.exe` intact while still trimming
    // trailing args like `wsl.exe -d Ubuntu`.
    let exe = match trimmed.to_ascii_lowercase().find(".exe") {
        Some(idx) => &trimmed[..idx + ".exe".len()],
        None => trimmed.split_whitespace().next().unwrap_or(""),
    };
    let filename = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let stem = filename
        .strip_suffix(".exe")
        .or_else(|| filename.strip_suffix(".EXE"))
        .or_else(|| filename.strip_suffix(".Exe"))
        .unwrap_or(filename)
        .to_ascii_lowercase();

    match stem.as_str() {
        // POSIX shells (same set we pattern-match elsewhere in pty.rs)
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "ash" | "tcsh" | "csh" | "mksh" => {
            ShellFamily::Posix
        }
        // WSL spawns a Linux shell — readline semantics apply.
        "wsl" => ShellFamily::Posix,
        // Native Windows shells: Ctrl-U is not line-kill.
        "cmd" | "powershell" | "pwsh" => ShellFamily::WindowsNative,
        _ => ShellFamily::Unknown,
    }
}

/// How long the agent must be silent after printing a `?`-ending line before
/// we treat it as a question waiting for input. 10s is long enough to avoid
/// false positives from AI agents that pause while thinking between API calls.
pub(super) const SILENCE_QUESTION_THRESHOLD: std::time::Duration =
    std::time::Duration::from_secs(10);

/// An idle shell may still be receiving a streamed intent. Wait one quiet tick.
pub(super) const SILENCE_INTENT_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(1);

/// Maximum non-`?` chunks allowed after a `?` candidate before considering it stale.
/// Claude Code prints 2-3 decoration chunks after a question (mode line, separator).
/// Anything beyond this threshold means the agent continued working — not waiting.
const STALE_QUESTION_CHUNKS: u32 = 10;

/// How long the agent must be silent after printing a tool-error line before
/// we treat it as a turn-ending error (fire `playError()`). Shorter than the
/// question threshold because tool errors are typically followed by immediate
/// turn end (no retry) — 5s is enough to rule out a same-chunk recovery.
pub(super) const SILENCE_TOOL_ERROR_THRESHOLD: std::time::Duration =
    std::time::Duration::from_secs(5);

/// How long a retry line ("Retrying … attempt N/M", "Unable to connect to API")
/// holds the agent BUSY after it was last seen. During an API connection-retry
/// loop the agent is mid-turn but its TUI freezes between attempts (the spinner
/// stops repainting while the network call blocks), producing no changed rows —
/// so the movement-based BUSY evidence (#446-596f) drops and the silence/ready
/// path would flip the session idle mid-retry. Each new attempt line re-arms the
/// hold; once retries stop (recovery or final failure) the hold self-expires and
/// idle detection resumes. Long enough to bridge a stalled TCP connect (~10s).
const AGENT_RETRY_HOLD: std::time::Duration = std::time::Duration::from_secs(15);

/// Detect a turn-ending tool-failure line like Claude Code's
/// `⎿  Error: Exit code 1`. Anchored to line-start with only non-letter,
/// non-quote prefix characters (whitespace, box-drawing glyphs) so source
/// code or markdown that merely quotes the literal `"Error: Exit code N"`
/// does NOT match — avoids false-positive red notifications when the user's
/// own pty.rs tests are displayed in a terminal.
pub(super) fn is_tool_error_line(line: &str) -> bool {
    lazy_static::lazy_static! {
        static ref TOOL_ERROR_RE: regex::Regex =
            regex::Regex::new(r#"^[^A-Za-z"]*Error:\s*Exit code\s+\d+"#).unwrap();
    }
    TOOL_ERROR_RE.is_match(line)
}

/// Detect an in-flight API connection-retry line, e.g. Claude's subagent SDK
/// `Unable to connect to API (ECONNRESET) · Retrying in 0s · attempt 6/10` or
/// the stream-error `retrying 5/5` form. Presence of such a line means the agent
/// is still mid-turn (auto-retrying), not idle — see `AGENT_RETRY_HOLD`. The
/// `attempt N/M` / `N/M` counter is required so plain prose mentioning "retrying"
/// or a code line containing the string does not latch the session busy.
pub(super) fn is_retry_line(line: &str) -> bool {
    lazy_static::lazy_static! {
        static ref RETRY_RE: regex::Regex = regex::Regex::new(
            r"(?i)(unable to connect to api|retrying\b[^\n]{0,40}attempt\s+\d+\s*/\s*\d+|retrying\s+\d+\s*/\s*\d+)"
        ).unwrap();
    }
    RETRY_RE.is_match(line)
}

/// How often the timer thread wakes up to check for silence.
pub(super) const SILENCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// If the wall-clock gap between two consecutive silence-timer ticks exceeds
/// this threshold, the system was likely asleep (lid closed). The tick is
/// skipped and timestamps are reset so stale elapsed times don't trigger
/// false idle transitions or completion sounds for every terminal.
pub(super) const SLEEP_WAKE_GAP: std::time::Duration = std::time::Duration::from_secs(5);

/// Grace period after a PTY resize during which parsed events (Question, RateLimit,
/// ApiError) are suppressed. The shell redraws visible output after SIGWINCH, which
/// would otherwise re-trigger notifications for content already on screen.
pub(super) const RESIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(1000);

/// How long after user input to ignore `?`-ending echo lines from the PTY.
const ECHO_SUPPRESS_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);

/// Grace period after PTY session start during which notifications (Question,
/// RateLimit, ApiError) are suppressed. When a CLI tool replays conversation
/// history (e.g. `claude --continue`), the burst of historical output contains
/// old errors and questions that would otherwise trigger stale notifications.
/// The grace ends when output pauses for STARTUP_SETTLE_SILENCE seconds,
/// indicating the replay is over and live output is starting.
pub(super) const STARTUP_SETTLE_SILENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Safety cap: startup grace never lasts longer than this, even if output
/// never pauses (e.g. continuous build log).
pub(super) const STARTUP_GRACE_MAX: std::time::Duration = std::time::Duration::from_secs(120);

/// Shell idle threshold: 500ms without real PTY output → transition busy→idle.
/// Matches the frontend's previous 500ms setTimeout in checkIdle.
pub(super) const SHELL_IDLE_MS: u64 = 500;

/// Agent idle threshold: 2.5s without real PTY output → transition busy→idle.
/// AI agents produce output in bursts with natural thinking pauses (>500ms).
/// Using the shell threshold causes visible blue→green→blue oscillation.
/// Combined with the 2s frontend debounce, this gives ~4.5s total hold.
pub(super) const AGENT_IDLE_MS: u64 = 2500;

/// Retry horizon for the payload-free orchestrator mail notice after an
/// ambiguous PTY write. Ordinary payload injection remains non-retriable.
const ORCHESTRATOR_WAKE_UNCERTAIN_RETRY: std::time::Duration = std::time::Duration::from_secs(5);

/// A ready prompt must remain visible across multiple silence-timer ticks before
/// it can end an agent turn. Ink redraws are multi-chunk (erase, then repaint),
/// so a single snapshot can briefly show the prompt without its working row.
pub(super) const AGENT_READY_CONFIRM: std::time::Duration = std::time::Duration::from_millis(1500);
/// Escape hatch for a launch-instrumented agent whose terminal-ready screen
/// remains stable after its authoritative completion signal was lost.
pub(super) const PROTOCOL_STALE_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(5 * 60);

/// Interrupt intent is only a hint: Ctrl-C/Escape may be ignored or handled
/// asynchronously. Keep it long enough to correlate the subsequent explicit
/// interrupted screen, then discard it without changing shell state.
const INTERRUPT_PENDING_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// How long a plain shell latched BUSY by OSC 133 must stay silent before its
/// foreground process group is inspected for a nested prompt. A command that is
/// genuinely running (build, test, `dd`) either prints inside this window or
/// keeps a non-shell process in the group, so the probe stays off the hot path
/// and costs nothing while work is actually happening.
pub(super) const SHELL_PROMPT_PROBE_SILENCE_MS: u64 = 3_000;

/// Maximum time active_sub_tasks can block idle transition (30s).
/// If the parser sets active_sub_tasks > 0 but the agent exits or the
/// mode-line disappears without emitting count=0, the terminal would stay
/// busy forever. After this timeout with no real output, we force-clear
/// the stale counter and allow idle transition.
const SUBTASK_STALE_MS: u64 = 30_000;

/// AtomicU8 encoding for shell_states DashMap.
pub(crate) const SHELL_NULL: u8 = 0;
pub(crate) const SHELL_BUSY: u8 = 1;
pub(crate) const SHELL_IDLE: u8 = 2;

/// Wire representation of an observed shell state. `SHELL_NULL` means no
/// lifecycle evidence has arrived yet and must remain absent/starting rather
/// than being serialized as idle.
pub(crate) fn shell_state_wire(state: u8) -> Option<&'static str> {
    match state {
        SHELL_BUSY => Some("busy"),
        SHELL_IDLE => Some("idle"),
        _ => None,
    }
}

/// Searches all changed rows (not just the last non-empty one) so a question row
/// is found even when a mode/status line with a higher row index arrives in the same chunk.
/// Applies content filters to reject lines that are clearly not questions (code comments,
/// diff context, markdown headers, code syntax).
pub(crate) fn extract_question_line(changed_rows: &[ChangedRow]) -> Option<String> {
    if !changed_rows.iter().any(|row| row.text.ends_with('?')) {
        return None;
    }
    let protocol_rows =
        collect_protocol_token_indices(changed_rows.iter().map(|r| r.text.as_str()));
    changed_rows
        .iter()
        .enumerate()
        .rev()
        .find(|(index, row)| {
            !protocol_rows.contains(index)
                && !row.text.is_empty()
                && row.text.ends_with('?')
                && is_plausible_question(&row.text)
        })
        .map(|(_, row)| row.text.clone())
}

/// Returns false for lines that are clearly not questions: code comments, diff context,
/// markdown headers, prompt-echoed user input, and lines containing code-specific syntax.
fn is_plausible_question(line: &str) -> bool {
    let trimmed = line.trim_start();
    if crate::output_parser::line_is_diff_or_code_context(line) {
        return false;
    }
    // Prompt-prefixed lines are user input echoed in the conversation, not agent questions.
    if is_prompt_line(trimmed) {
        return false;
    }
    // Comment/diff/markdown prefixes
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.starts_with('>')
    {
        return false;
    }
    // Code syntax markers — real questions don't contain these
    if line.contains("->") || line.contains("=>") || line.contains("::") {
        return false;
    }
    // Code try-syntax: word_or_> followed by (...)? — e.g. foo()?, bar(x)?, Vec<T>()?
    // But NOT human option parentheticals like (y/n)?, (yes/no)? where `(` is
    // preceded by whitespace or start-of-line, not a word character.
    lazy_static::lazy_static! {
        static ref CODE_TRY_RE: regex::Regex =
            regex::Regex::new(r"[\w>]\([^)]*\)\?").unwrap();
    }
    if CODE_TRY_RE.is_match(line) {
        return false;
    }
    true
}

/// Returns true if a changed_row text looks like a suggest token line.
/// Used to exclude suggest rows from "real output" classification so they
/// don't reset the silence timer or stale pending questions.
pub(super) fn is_suggest_row(text: &str) -> bool {
    let t = text.trim();
    t.contains("suggest:") && t.contains('|')
}

/// Verify that a question candidate is still visible among the bottom rows of the
/// terminal screen. Returns true only if the exact question text appears as a
/// complete row (trimmed) within the last `max_bottom_rows` non-empty lines.
/// This prevents ghost notifications from stale `?` lines that have scrolled off.
pub(crate) fn verify_question_on_screen(
    screen_rows: &[String],
    question: &str,
    max_bottom_rows: usize,
) -> bool {
    let q = question.trim();
    screen_rows
        .iter()
        .rev()
        .filter(|r| !r.is_empty())
        .take(max_bottom_rows)
        .any(|r| {
            let t = r.trim();
            // Exact match or prefix match (question may be truncated/wrapped on screen)
            t == q || (!q.is_empty() && t.starts_with(q))
        })
}

/// Returns true when the line is a TUIC protocol token (`suggest:` or `intent:`
/// with pipe-separated items). These are structural markers consumed by the
/// frontend, not agent chat content — they must be skipped by question detection.
fn is_protocol_token_line(text: &str) -> bool {
    let t = text.trim_start();
    (t.starts_with("suggest:") && (t.contains('[') || t.contains('|')))
        || (t.starts_with("intent:") && t.contains('|'))
}

/// Returns the set of row indices occupied by a protocol token (including
/// terminal-wrapped continuation rows). A continuation row is a row that
/// immediately follows a bracketed `suggest:` row (up to `]`), or follows a
/// legacy unbracketed token row and contains `|`. Used to exclude the entire
/// suggest/intent block from "last chat line" detection — without this, the continuation row
/// gets mistaken for real chat content and steals the question slot.
fn collect_protocol_token_indices<'a>(
    screen_rows: impl IntoIterator<Item = &'a str>,
) -> std::collections::HashSet<usize> {
    let screen_rows: Vec<&str> = screen_rows.into_iter().collect();
    let mut indices = std::collections::HashSet::new();
    for (i, row) in screen_rows.iter().enumerate() {
        if is_protocol_token_line(row) {
            indices.insert(i);
            let bracketed_suggest = row.trim_start().starts_with("suggest:") && row.contains('[');
            if bracketed_suggest && row.contains(']') {
                continue;
            }
            // Walk forward to find continuation rows (wrapped by terminal width)
            for (j, row) in screen_rows.iter().enumerate().skip(i + 1) {
                let trimmed = row.trim();
                if trimmed.is_empty() || is_separator_line(trimmed) || is_prompt_line(row) {
                    break;
                }
                // Stop at rows that start a new protocol token or chat content
                if is_protocol_token_line(row)
                    || trimmed.starts_with('>')
                    || trimmed.starts_with('›')
                    || trimmed.starts_with('❯')
                    || trimmed.starts_with('●')
                    || trimmed.starts_with('⏺')
                {
                    break;
                }
                // An unbracketed continuation needs `|`; a bracketed suggest
                // remains protocol content until its closing `]`.
                if !bracketed_suggest && !trimmed.contains('|') {
                    break;
                }
                indices.insert(j);
                if bracketed_suggest && trimmed.contains(']') {
                    break;
                }
            }
        }
    }
    indices
}

/// Find the last chat line above the prompt box and, if it is a plausible
/// `?`-ending question, return it. Suggest/intent protocol blocks (including
/// wrapped continuations) are transparently skipped because they sit between
/// the agent's question and the prompt but are not real chat content — the
/// agent emits the question first and the suggest arrives after.
///
/// Only the single last chat line is inspected. We deliberately do NOT walk
/// deeper looking for an older `?`: a multi-line scan would scavenge past
/// the current agent turn and pick up the user's own previous input (e.g.
/// `❯ tutto ok?`) or stale content from earlier in the conversation, firing
/// phantom notifications 10s after the reply.
pub(crate) fn find_last_chat_question(screen_rows: &[String]) -> Option<String> {
    let prompt_idx = screen_rows
        .iter()
        .enumerate()
        .rev()
        .find(|(_, row)| is_prompt_line(row))?
        .0;

    let protocol_indices = collect_protocol_token_indices(screen_rows.iter().map(String::as_str));

    for i in (0..prompt_idx).rev() {
        if protocol_indices.contains(&i) {
            continue;
        }
        let trimmed = screen_rows[i].trim();
        if trimmed.is_empty() || is_separator_line(trimmed) || is_chrome_row(trimmed) {
            continue;
        }
        // First non-skip row above the prompt — this is the last chat line.
        // Check it for a question, otherwise give up: we do not scavenge
        // deeper into the buffer.
        if trimmed.ends_with('?') && is_plausible_question(trimmed) {
            return Some(trimmed.to_string());
        }
        return None;
    }
    None
}

/// Whether the screen has a current input box and, if so, whether the last chat
/// content above it is a question. This distinction matters to the silence
/// fallback: `None` from `find_last_chat_question` can mean either "no prompt
/// anchor" or "the current turn ends in non-question content". Only the former
/// may use a changed-row fallback; the latter must not scavenge an older question
/// from scrollback.
pub(super) fn current_chat_question(screen_rows: &[String]) -> CurrentChatQuestion {
    if screen_rows.iter().any(|row| is_prompt_line(row)) {
        CurrentChatQuestion::PromptAnchored(find_last_chat_question(screen_rows))
    } else {
        CurrentChatQuestion::NoPromptAnchor
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CurrentChatQuestion {
    NoPromptAnchor,
    PromptAnchored(Option<String>),
}

/// Relative strength of evidence backing a busy/idle/awaiting verdict. A
/// higher rank overrides a lower one; equal-or-lower rank evidence is
/// rejected rather than clobbering something stronger already recorded for
/// the opposite verdict (see `TurnEvidence::record_busy`/`record_idle`).
///
/// Ordering matches #744-138c: wall-clock silence is the weakest signal,
/// screen-content classification is stronger, a background-process check is
/// stronger still, and a turn-granular protocol marker (OSC 7770, a submitted
/// line on a ready-adapter agent, a `suggest:`/completion marker) is
/// authoritative.
///
/// Rank is about what a signal *knows*, not how it travelled (#745-8ff1).
/// OSC 133 is the cautionary case and is deliberately **not** Protocol rank:
/// it is shell integration, so `133;C` fires when a foreground command starts
/// and `133;D` when it exits — on a long-lived TUI agent, once at launch and
/// once at death. It knows a process is running and nothing about turns, so it
/// records at [`EvidenceRank::Screen`] and a stable Ready screen may close it.
///
/// [`EvidenceRank::Process`] appears on the idle side only, from the two
/// places that read the process table: the foreground probe (`"process"`,
/// `foreground_probe`) and the `"protocol-stale"` give-up. Nothing records it
/// for busy, and **child exit deliberately records nothing at all** — an exit
/// removes the session rather than transitioning it (#771-4733).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceRank {
    Silence,
    Screen,
    Process,
    Protocol,
}

/// A single piece of ranked evidence, with the detector name that produced it
/// (used for `activity_source` in transition logs) and when it was recorded.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evidence {
    pub(crate) rank: EvidenceRank,
    pub(crate) source: &'static str,
    pub(crate) at: std::time::Instant,
}

/// The current turn's ranked evidence. Busy and idle are mutually exclusive —
/// recording one clears the other unless the incoming evidence is too weak to
/// outrank what is already held (see `record_busy`/`record_idle`). Awaiting is
/// independent (a session can be busy or idle while a question is pending).
///
/// This is the single model #744-138c replaces the nine independently
/// mutated `SilenceState` booleans with: `completion_declared` and
/// `explicit_idle` both become Protocol-rank `idle` evidence with different
/// `source` tags; `explicit_busy`/`hook_busy`/`turn_started_by_input` become
/// Protocol-rank `busy` evidence tagged `"hook-busy"`/`"osc133-busy"`/
/// `"user-submit"`; `idle_confirmed` is derived from the recorded idle
/// evidence's rank/source rather than stored; `ready_since` is the `at` of a
/// `"agent-ready-screen"` idle evidence (preserved across repeated
/// observations, reset by any other evidence — see `record_idle`).
#[derive(Debug, Clone, Default)]
pub(crate) struct TurnEvidence {
    pub(super) busy: Option<Evidence>,
    pub(super) idle: Option<Evidence>,
    pub(super) awaiting: Option<Evidence>,
    pub(super) activity_seen: bool,
}

impl TurnEvidence {
    /// Record busy evidence. Rejected (no-op, returns `false`) if idle
    /// evidence of strictly higher rank is already held — e.g. a stale
    /// Working screen row (`Screen` rank) cannot reopen a turn an explicit
    /// OSC idle marker or a declared completion (`Protocol` rank) already
    /// closed, unless the caller has already decided the reopen is valid and
    /// passes an elevated rank for it (see `apply_working_evidence`).
    pub(super) fn record_busy(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.idle.is_some_and(|idle| idle.rank > rank)
            || self.busy.is_some_and(|busy| busy.rank > rank)
        {
            return false;
        }
        self.busy = Some(Evidence {
            rank,
            source,
            at: std::time::Instant::now(),
        });
        self.idle = None;
        true
    }

    /// Record idle evidence. Rejected if busy evidence of strictly higher
    /// rank is already held. `at` is preserved across repeated observations
    /// of the same (rank, source) — this is the `AGENT_READY_CONFIRM`
    /// debounce clock a caller reads via the returned `Evidence`.
    pub(super) fn record_idle(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.busy.is_some_and(|busy| busy.rank > rank)
            || self.idle.is_some_and(|idle| idle.rank > rank)
        {
            return false;
        }
        let at = match self.idle {
            Some(existing) if existing.rank == rank && existing.source == source => existing.at,
            _ => std::time::Instant::now(),
        };
        self.idle = Some(Evidence { rank, source, at });
        self.busy = None;
        true
    }

    /// Drop any held idle evidence and its debounce clock without recording
    /// new busy evidence. Used when a detector determines its own evidence is
    /// currently invalid (e.g. an unstable/unknown screen, or a ready screen
    /// gated by `injection_delivery_uncertain`/API-retry/no-activity-yet).
    pub(super) fn clear_idle(&mut self) {
        self.idle = None;
    }

    /// Record idle evidence unconditionally, bypassing the busy-rank gate in
    /// `record_idle`. Used only by call sites that have already performed
    /// their own precise, narrower busy-evidence gate (e.g. `note_ready_screen`
    /// only withholds ready-confirmation for `"hook-busy"`/`"user-submit"`
    /// busy evidence with no activity seen yet — a bare `"osc133-busy"`
    /// marker must NOT block screen-confirmed readiness, unlike the generic
    /// rank gate `record_idle` applies for e.g. the silence-timeout fallback).
    pub(super) fn force_idle(&mut self, rank: EvidenceRank, source: &'static str) -> Evidence {
        let at = match self.idle {
            Some(existing) if existing.rank == rank && existing.source == source => existing.at,
            _ => std::time::Instant::now(),
        };
        let evidence = Evidence { rank, source, at };
        self.idle = Some(evidence);
        self.busy = None;
        evidence
    }

    /// Record awaiting (question/dialog) evidence. Rejected if awaiting
    /// evidence of strictly higher rank is already held — this is the
    /// generic form of the old state.rs sticky guard "a low-confidence
    /// (silence-heuristic) question must not overwrite an already-active
    /// high-confidence one": confident sources are `Protocol` rank, heuristic
    /// ones `Screen` rank, so a `Screen` observation is rejected while a
    /// `Protocol` one is held, and any same-or-higher rank observation
    /// updates (a confident question's text can still change).
    pub(crate) fn record_awaiting(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        if self.awaiting.is_some_and(|existing| existing.rank > rank) {
            return false;
        }
        self.awaiting = Some(Evidence {
            rank,
            source,
            at: std::time::Instant::now(),
        });
        true
    }

    pub(crate) fn clear_awaiting(&mut self) {
        self.awaiting = None;
    }

    /// The rank of the currently recorded awaiting evidence, if any. Used by
    /// callers that only clear on a WEAK (non-`Protocol`) awaiting verdict —
    /// mirrors the old `!question_confident` guard in state.rs's status-line
    /// and question-cleared handling.
    pub(crate) fn awaiting_rank(&self) -> Option<EvidenceRank> {
        self.awaiting.map(|a| a.rank)
    }

    /// Who recorded the current awaiting evidence. A `progress done` clears
    /// only a badge that a `progress blocked` raised (#1537-6c4b).
    pub(crate) fn awaiting_source(&self) -> Option<&'static str> {
        self.awaiting.map(|a| a.source)
    }

    /// True while the recorded idle evidence is strong/current enough to act
    /// on downstream (standby, peer injection): an explicit protocol marker
    /// or a screen adapter's confirmed ready/interrupted state, or a plain
    /// (non-agent) shell's silence timeout. An agent silence-timeout with no
    /// screen confirmation is NOT confirmed — mirrors the old `idle_confirmed`.
    pub(super) fn idle_confirmed(&self) -> bool {
        match self.idle {
            Some(Evidence {
                rank: EvidenceRank::Silence,
                source,
                ..
            }) => source == "silence-timeout-shell",
            Some(_) => true,
            None => false,
        }
    }
}

/// The verdict `decide()` reaches for the busy/idle shell-state axis.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Transition {
    ToBusy,
    ToIdle(Evidence),
}

/// The single arbiter every busy/idle transition site routes through
/// (#744-138c). Pure: given the evidence recorded so far and whether the
/// shell is currently busy, says whether — and on what evidence — it should
/// flip. All the interesting gating (rank comparisons against the opposite
/// verdict, debounce, staleness) already happened when the evidence was
/// recorded (`record_busy`/`record_idle`); this function only compares the
/// surviving evidence against the current shell state.
pub(super) fn decide(
    evidence: &TurnEvidence,
    shell_is_busy: bool,
    _now: std::time::Instant,
) -> Option<Transition> {
    if shell_is_busy {
        evidence.idle.map(Transition::ToIdle)
    } else {
        evidence.busy.map(|_| Transition::ToBusy)
    }
}

#[derive(Clone)]
pub(super) struct OpenIntent {
    pub(super) text: String,
    /// The unjoined anchor line distinguishes a new intent from an Ink frame
    /// that temporarily rewrites or moves its continuation rows.
    pub(super) anchor_text: String,
    pub(super) start_row: usize,
    pub(super) end_row: usize,
}

#[cfg(test)]
thread_local! {
    pub(super) static INTENT_CANDIDATE_GRID_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static INTENT_CONTINUATION_GRID_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn incomplete_intent_title(text: &str) -> bool {
    text.rsplit_once('(')
        .is_some_and(|(_, suffix)| !suffix.contains(')') && suffix.split_whitespace().count() <= 3)
}

/// Counts only VTE line breaks. CSI cursor moves and CR are repaint operations.
#[derive(Default)]
pub(super) struct IntentBreaks {
    pub(super) any: bool,
    pub(super) strong: bool,
    previous_cr: bool,
}

impl vte::Perform for IntentBreaks {
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.previous_cr = true,
            b'\n' => {
                self.any = true;
                self.strong |= self.previous_cr;
                self.previous_cr = false;
            }
            0x0b | 0x0c => {
                self.any = true;
                self.strong = true;
                self.previous_cr = false;
            }
            _ => self.previous_cr = false,
        }
    }

    fn print(&mut self, _char: char) {
        self.previous_cr = false;
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && matches!(byte, b'D' | b'E') {
            self.any = true;
            self.strong = true;
        }
        self.previous_cr = false;
    }
}

/// What kind of evidence-recorder call a [`TrailEntry`] describes. Distinct
/// from [`EvidenceRank`] (which axis, how strong) — this is which recorder
/// was invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrailKind {
    /// `record_busy`.
    Busy,
    /// `record_idle`, or `force_idle` (`forced` distinguishes the two).
    Idle,
    /// A one-shot busy-evidence clear (the `evidence.busy = None` sites) —
    /// not itself rank-gated, so always `accepted: true`, no `outranked_by`.
    ClearBusy,
    /// `clear_idle` — drops idle evidence without recording busy.
    ClearIdle,
    /// `record_awaiting`.
    Awaiting,
    /// `clear_awaiting` — not itself rank-gated, always `accepted: true`.
    ClearAwaiting,
    /// `note_user_submission`'s turn boundary. Carries no rank/source of its
    /// own; a reader segments the trail into turns by this marker instead of
    /// a plumbed epoch (deliberately not added — see `DecisionTrail`'s doc
    /// comment).
    UserSubmit,
}

/// One entry in a [`DecisionTrail`]. Every field is `Copy`, so a push never
/// allocates. `rank`/`source` are `None` only for `ClearBusy`/`UserSubmit`,
/// which carry no evidence of their own.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrailEntry {
    pub(crate) at: std::time::Instant,
    pub(crate) kind: TrailKind,
    pub(crate) rank: Option<EvidenceRank>,
    pub(crate) source: Option<&'static str>,
    /// Whether the recorder accepted this attempt. Always `true` for
    /// `force_idle` (bypasses the gate by design) and for the non-rank-gated
    /// kinds; `record_busy`/`record_idle` can be `false`.
    pub(crate) accepted: bool,
    /// True when this `Idle` entry came from `force_idle` rather than the
    /// gated `record_idle`. Always `false` for other kinds.
    pub(crate) forced: bool,
    /// The `(rank, source)` of the opposite-verdict evidence that rejected
    /// this attempt, when `accepted` is `false`. This is the single most
    /// useful field in the trail: a rejection is otherwise invisible, since
    /// every pre-existing call site discarded `record_busy`/`record_idle`'s
    /// returned `bool`.
    pub(crate) outranked_by: Option<(EvidenceRank, &'static str)>,
}

/// Fixed capacity of a [`DecisionTrail`] ring: ~48 bytes/entry, so 64 entries
/// is a few KB per session, bounded by `MAX_CONCURRENT_SESSIONS`.
pub(super) const TRAIL_CAPACITY: usize = 64;

/// A bounded, always-on ring of the last [`TRAIL_CAPACITY`] evidence-recorder
/// calls a session's [`SilenceState`] made — including REJECTED attempts,
/// which `record_busy`/`record_idle`'s `bool` return makes invisible today
/// (every call site before this discarded it). This is exactly what a "why is
/// this session in the wrong state" dump needs: what was attempted, at what
/// rank, and — when rejected — what outranked it.
///
/// Lives here, as a sibling field of `SilenceState::evidence`, NOT inside
/// `TurnEvidence` itself: `TurnEvidence` is `Clone`d to escape the lock on the
/// PTY reader's hot path (`apply_working_evidence`, and the reader chunk path
/// — once per chunk), and today that clone is ~4 words of `Copy` data. A ring
/// inside it would deep-copy several KB on every chunk. `SilenceState` itself
/// is only ever reached behind its own mutex, so a push here costs one write
/// no existing caller wasn't already paying for — no extra lock, no
/// allocation (every field of `TrailEntry` is `Copy`).
///
/// Always-on by design, not gated behind a runtime toggle: a badge is
/// reported wrong *after* it's already stuck, so a trail you'd have to
/// enable first would be empty exactly when it's needed.
///
/// Carries no turn/session id: the recorders that push here (`SilenceState`
/// methods and their callers) have no epoch in scope, and plumbing one
/// through every call site was evaluated and rejected in favor of a `Copy`
/// ring plus a `UserSubmit` marker entry at the real turn boundary
/// (`note_user_submission`) — a reader segments the trail by that marker
/// instead. Do not "fix" this by threading an epoch through every recorder.
#[derive(Debug, Clone)]
pub(crate) struct DecisionTrail(std::collections::VecDeque<TrailEntry>);

impl DecisionTrail {
    fn new() -> Self {
        Self(std::collections::VecDeque::with_capacity(TRAIL_CAPACITY))
    }

    /// Collapses an exact repeat of the immediately-previous outcome (same
    /// `kind`/`rank`/`source`/`accepted`/`forced`/`outranked_by` — `at` is
    /// deliberately excluded from the comparison, see below) into that same
    /// entry instead of pushing a new one.
    ///
    /// Without this, a no-op call floods the bounded ring with duplicate
    /// noise on the PTY reader's hot path: `note_busy_evidence()`'s
    /// `clear_idle()` call fires on every "working"/"real activity" chunk
    /// regardless of whether there was any idle evidence to clear, and
    /// `record_busy` re-affirms the identical (rank, source) on every chunk
    /// of a long streaming turn. Confirmed by review: either alone can evict
    /// the entire 64-entry ring within under a second of active output,
    /// wiping out the one rejected-evidence entry ("outranked_by") someone
    /// opened the dump specifically to find — exactly the failure mode this
    /// trail exists to prevent.
    ///
    /// A repeat keeps the FIRST occurrence's `at`, not the latest — so its
    /// reported age answers "how long has this been true", not "when was it
    /// last redundantly re-recorded", which matches every other timestamp in
    /// this payload (e.g. `Evidence.at` has the identical "preserved across
    /// repeats" behavior in `record_idle`/`force_idle` for the same reason).
    fn push(&mut self, entry: TrailEntry) {
        if let Some(last) = self.0.back()
            && last.kind == entry.kind
            && last.rank == entry.rank
            && last.source == entry.source
            && last.accepted == entry.accepted
            && last.forced == entry.forced
            && last.outranked_by == entry.outranked_by
        {
            return;
        }
        if self.0.len() == TRAIL_CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(entry);
    }

    /// Snapshot for the state-explain payload, oldest first.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &TrailEntry> {
        self.0.iter()
    }
}

impl Default for DecisionTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// The last `Notification`-sourced `state=awaiting` classification this
/// session made — the same five values `process_chunk` already logs via
/// `tracing::debug!` ("Notification-sourced state=awaiting classified"), just
/// also kept for query instead of log-only. `notification_type` needs an
/// owned `String` (unlike the `&'static str` sources elsewhere in this file),
/// so this is deliberately NOT part of `DecisionTrail`'s `Copy` ring — a
/// `Notification` hook fire is rare, so one small allocation here is fine, as
/// long as it never rides the per-chunk path (it doesn't: this is written
/// only from the one call site that already computes all five values).
#[derive(Debug, Clone)]
pub(crate) struct NotificationClassification {
    pub(crate) at: std::time::Instant,
    pub(crate) notification_type: Option<String>,
    pub(crate) has_message: bool,
    pub(crate) shell_already_idle: bool,
    /// `Some(bool)` only when this fire actually produced a `Question` event
    /// (`payload == "awaiting"`); mirrors the `confident` local already
    /// logged at the call site.
    pub(crate) confident: Option<bool>,
    /// Whether the classification suppressed this fire outright (no event
    /// reached state.rs's reducer at all).
    pub(crate) suppressed: bool,
}

#[derive(Clone)]
pub(crate) struct SilenceState {
    /// A title-less marker is kept until a real line or turn boundary closes it.
    pub(super) open_intent: Option<OpenIntent>,
    /// Exact-value repaint dedup; a different value permits an earlier value again.
    pub(super) last_intent: Option<(String, Option<String>)>,
    /// When the last chunk of output was received from the PTY.
    pub(crate) last_output_at: std::time::Instant,
    /// The last line ending with `?` that hasn't been resolved yet.
    pub(crate) pending_question_line: Option<String>,
    /// Whether a Question event has already been emitted for the current pending line
    /// (either by the instant regex detector or by the silence timer).
    pub(crate) question_already_emitted: bool,
    /// When the last resize was requested. Used to suppress re-parsing of redrawn output.
    pub(super) last_resize_at: Option<std::time::Instant>,
    /// Deadline until which `on_chunk` ignores `?`-ending lines (suppresses PTY echo).
    /// Set by `suppress_user_input()` so the echo of user-typed text doesn't
    /// re-enable silence-based question detection.
    pub(crate) suppress_echo_until: Option<std::time::Instant>,
    /// When the last chunk of ANY kind (real or chrome-only) was processed.
    /// Used by the backup idle timer to distinguish "no output at all" (reader
    /// blocked on read()) from "only chrome-only ticks arriving". The backup
    /// timer should only fire when truly no chunks arrive.
    pub(crate) last_chunk_at: std::time::Instant,
    /// When the last StatusLine (spinner) event was seen. If recent,
    /// silence-based question detection is suppressed — spinner means the agent is working.
    pub(crate) last_status_line_at: Option<std::time::Instant>,
    /// How many non-`?` chunks arrived after the current pending question candidate.
    /// Used to detect stale candidates: if the agent continued producing significant
    /// output after the `?` line, it was not a real question.
    output_chunks_after_question: u32,
    /// The text of the last question emitted (by silence timer or check_silence).
    /// Used to prevent re-emission of the same question when scrolling causes the
    /// `?` line to reappear in changed_rows at a different row position.
    /// Cleared on user input (new conversation cycle).
    last_emitted_text: Option<String>,
    /// When this session was created. Used with `startup_settled` to suppress
    /// notifications during the initial output burst (e.g. `--continue` replay).
    pub(super) created_at: std::time::Instant,
    /// True once the session has settled after the initial output burst.
    /// Settled = output paused for STARTUP_SETTLE_SILENCE seconds, or
    /// STARTUP_GRACE_MAX has elapsed since creation.
    pub(crate) startup_settled: bool,
    /// The last `Error: Exit code N` line seen, awaiting silence verification.
    /// Cleared if real output (non-chrome, non-error) arrives — that means the
    /// agent recovered and the error is not turn-ending.
    pending_tool_error: Option<String>,
    /// Error lines already surfaced via `ToolError` in the current "input epoch"
    /// (since the last user line submit / session start). Persists across
    /// `clear_tool_error_on_recovery` so that scroll-induced reappearances of
    /// the same error in `changed_rows` do not re-fire the notification.
    /// Cleared on explicit user input so a recurring failure in a later turn
    /// can notify again.
    surfaced_tool_errors: std::collections::HashSet<String>,
    /// Parked request to reopen `OutputParser`'s error dedup. The parser lives
    /// in the reader thread's `ChunkProcessor` and never sees the input path, so
    /// a submitted line leaves the request here and `process_chunk` drains it
    /// before the next parse. Same "the user is engaging again" epoch as
    /// `surfaced_tool_errors`, for the parser-side half of the same dedup.
    parser_dedup_reset_pending: bool,
    /// Parsed `suggest:` items awaiting silence-based flush. The parser detects
    /// the token synchronously with output, but we hold the event here until
    /// `check_suggest` confirms the turn has ended (`SILENCE_SUGGEST_THRESHOLD`
    /// elapsed since the last real output chunk). Eliminates the frontend
    /// `pendingSuggest` race: the event never reaches the UI before idle.
    pub(super) pending_suggest_items: Option<Vec<String>>,
    /// Input-turn epoch associated with `pending_suggest_items`.
    pub(super) pending_suggest_turn_epoch: u64,
    /// Choices were published, while the completion payload remains parked.
    pub(super) pending_suggest_emitted: bool,
    /// Timestamp when `pending_suggest_items` was parked. Currently for
    /// diagnostics only — the flush decision is driven by `last_output_at`,
    /// not the park time.
    pending_suggest_at: Option<std::time::Instant>,
    /// The agent emitted the protocol's explicit end-of-task marker for the
    /// current input epoch. Unlike the pending item payload, this survives the
    /// one-shot Suggest event drain so status/list can distinguish completed
    /// work from a merely quiet ready prompt.
    ///
    /// Deliberately NOT folded into `TurnEvidence` (#744-138c): a `suggest:`
    /// marker is consulted by OTHER decisions (`apply_working_evidence`'s
    /// reopen gate, `completion_adjusted_screen_activity`) as "the agent
    /// already declared this turn done", but it must NOT itself flip the
    /// shell atomic to idle — it is parsed mid-output, often while the shell
    /// still reads BUSY. Recording it as `idle` evidence would make `decide()`
    /// treat parsing the marker as proof the shell is idle, which is not what
    /// today's behavior is.
    pub(super) completion_declared: bool,
    /// Input-turn epoch that declared completion.
    pub(super) completion_turn_epoch: u64,
    /// Claude's own hook payload (the `bgtasks` OSC 7770 verb, scraped from
    /// `background_tasks` on `Stop`/`StopFailure`) declared at least one
    /// background task still running as of `declared_background_work_turn_epoch`.
    /// Deliberately separate from `SessionState::background_work` (the OS
    /// process-tree observation): that field is demand-gated by a 1s
    /// process-tree refresher which would silently overwrite a hook-derived
    /// value on its very next tick (see `set_background_work_for_epoch_with_hook`).
    /// This declaration instead follows `completion_declared`'s shape exactly
    /// — single writer, epoch-stamped, self-expiring, untouched by any
    /// polling loop.
    pub(super) declared_background_work: bool,
    /// Input-turn epoch that made the declaration above.
    pub(super) declared_background_work_turn_epoch: u64,
    /// Ranked busy/idle/awaiting evidence for the current turn (#744-138c).
    /// Replaces eight independently-mutated booleans (explicit_busy, hook_busy,
    /// explicit_idle, idle_confirmed, turn_started_by_input, turn_activity_seen,
    /// ready_since, and the screen/protocol distinction previously spread
    /// across them) with one struct: every busy/idle/awaiting transition
    /// records `Evidence` here and is arbitrated by `decide()`, instead of
    /// each call site toggling its own subset of the old flags.
    pub(super) evidence: TurnEvidence,
    /// Always-on ring of evidence-recorder calls, including rejections. See
    /// `DecisionTrail`'s doc comment for why it's a sibling of `evidence`
    /// rather than a field inside it.
    pub(super) trail: DecisionTrail,
    /// The last `Notification`-sourced `state=awaiting` classification, kept
    /// for query. See `NotificationClassification`'s doc comment.
    pub(crate) last_notification_classification: Option<NotificationClassification>,
    /// True only after OSC 7770 `state=` was observed (OSC 133 shell markers do
    /// not prove that an agent's configured hooks are actually running). This
    /// is session-lifetime latch metadata, not per-turn evidence — it never
    /// resets, so it does not belong in `TurnEvidence`.
    pub(super) hook_state_seen: bool,
    /// The latest agent hook, retained across a submitted-input evidence reset.
    last_hook_state: Option<u8>,
    /// Last screen classification and when it was computed, shared between the
    /// reader chunk path (which computes it fresh on every chunk) and the
    /// silence timer (which reuses this instead of re-classifying, so
    /// `detect_agent_screen_activity` runs at most once per session per
    /// `SILENCE_CHECK_INTERVAL` — see `cached_screen_activity()`).
    pub(super) cached_screen_activity: AgentScreenActivity,
    /// Output offsets of semantic screen transitions, retained even when the
    /// reader paints Ready and Working before the confirmation worker wakes.
    pub(super) last_ready_screen_offset: u64,
    pub(super) last_working_screen_offset: u64,
    /// Ring offset when the latest hook `state=busy` was handled, kept after a
    /// Stop hook clears the busy evidence so a confirmation worker still sees
    /// the turn. Hooks run before the reader writes their own chunk to the ring,
    /// so a busy carried by the chunk after Enter is stamped AT the Enter offset.
    pub(super) last_hook_busy_offset: Option<u64>,
    /// Recent user request to interrupt (Ctrl-C or bare Escape). This never
    /// changes shell state by itself; it only strengthens a matching interrupted
    /// screen emitted by the agent.
    pub(super) interrupt_requested_at: Option<std::time::Instant>,
    /// Debounce clock for `note_ready_screen`: first observation of a stable
    /// agent ready prompt. Not part of `TurnEvidence` — it is a pending
    /// observation, not yet evidence; only committed via `force_idle` once
    /// stable for `AGENT_READY_CONFIRM`, so busy evidence is not cleared early.
    pub(super) screen_ready_pending_since: Option<std::time::Instant>,
    /// Monotonic owner for an IDLE→BUSY transition reserved by terminal
    /// injection. The saved bool is the confirmed-idle value to restore only
    /// when no PTY byte was written and this claim still owns the state.
    pub(super) active_injection_claim: Option<(u64, bool)>,
    next_injection_claim: u64,
    /// A payload may have been partially written or flushed without a complete
    /// Enter. Such sessions remain conservatively BUSY and are surfaced in
    /// status; automatic retry would risk duplicate or corrupted input.
    pub(crate) injection_delivery_uncertain: bool,
    pub(super) injection_uncertain_since: Option<std::time::Instant>,
    injection_uncertainty_retryable: bool,
    /// Deadline until which an in-flight API connection-retry holds the agent
    /// BUSY. Armed by `mark_api_retry` when `is_retry_line` matches a changed
    /// row; blocks both the ready-screen and silence idle paths until it expires
    /// or is cleared by recovery/user input. See `AGENT_RETRY_HOLD`.
    pub(super) api_retry_hold_until: Option<std::time::Instant>,
}

impl SilenceState {
    /// Keep semantic transitions and their output offsets consistent across
    /// reader chunks and late foreground-identity discovery.
    pub(super) fn record_screen_activity(&mut self, activity: AgentScreenActivity, offset: u64) {
        if self.cached_screen_activity != activity {
            match activity {
                AgentScreenActivity::Ready => self.last_ready_screen_offset = offset,
                AgentScreenActivity::Working => self.last_working_screen_offset = offset,
                _ => {}
            }
        }
        self.cached_screen_activity = activity;
    }

    pub(super) fn close_open_intent(&mut self) -> Option<ParsedEvent> {
        let open = self.open_intent.take()?;
        let mut text = open.text;
        if incomplete_intent_title(&text)
            && let Some(open_paren) = text.rfind('(')
            && !text[..open_paren].trim().is_empty()
        {
            text.truncate(open_paren);
            text = text.trim_end().to_string();
        }
        self.accept_intent(text, None)
    }

    pub(super) fn accept_intent(
        &mut self,
        text: String,
        title: Option<String>,
    ) -> Option<ParsedEvent> {
        let value = (text, title);
        if self.last_intent.as_ref() == Some(&value) {
            return None;
        }
        self.last_intent = Some(value.clone());
        Some(ParsedEvent::Intent {
            text: value.0,
            title: value.1,
        })
    }

    pub(crate) fn new() -> Self {
        Self {
            open_intent: None,
            last_intent: None,
            last_output_at: std::time::Instant::now(),
            pending_question_line: None,
            question_already_emitted: false,
            last_chunk_at: std::time::Instant::now(),
            last_resize_at: None,
            suppress_echo_until: None,
            last_status_line_at: None,
            output_chunks_after_question: 0,
            last_emitted_text: None,
            created_at: std::time::Instant::now(),
            startup_settled: false,
            pending_tool_error: None,
            surfaced_tool_errors: std::collections::HashSet::new(),
            parser_dedup_reset_pending: false,
            pending_suggest_items: None,
            pending_suggest_turn_epoch: 0,
            pending_suggest_emitted: false,
            pending_suggest_at: None,
            completion_declared: false,
            completion_turn_epoch: 0,
            declared_background_work: false,
            declared_background_work_turn_epoch: 0,
            evidence: TurnEvidence::default(),
            trail: DecisionTrail::new(),
            last_notification_classification: None,
            hook_state_seen: false,
            last_hook_state: None,
            cached_screen_activity: AgentScreenActivity::Unknown,
            last_ready_screen_offset: 0,
            last_working_screen_offset: 0,
            last_hook_busy_offset: None,
            interrupt_requested_at: None,
            screen_ready_pending_since: None,
            active_injection_claim: None,
            next_injection_claim: 0,
            injection_delivery_uncertain: false,
            injection_uncertain_since: None,
            injection_uncertainty_retryable: false,
            api_retry_hold_until: None,
        }
    }

    pub(super) fn begin_injection_claim(&mut self, prior_idle_confirmed: bool) -> u64 {
        self.next_injection_claim = self.next_injection_claim.wrapping_add(1).max(1);
        let token = self.next_injection_claim;
        self.active_injection_claim = Some((token, prior_idle_confirmed));
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        token
    }

    /// A claim that leaves the agent busy: refused while another claim is live
    /// or an earlier write is uncertain, since no atom CAS orders it.
    pub(super) fn begin_mid_turn_injection_claim(&mut self) -> Option<u64> {
        if self.active_injection_claim.is_some() || self.injection_delivery_uncertain {
            return None;
        }
        Some(self.begin_injection_claim(false))
    }

    /// Drop a mid-turn claim whose write never started. Nothing else moves: the
    /// agent was busy before the claim and still is.
    pub(super) fn release_injection_claim(&mut self, token: u64) {
        if self
            .active_injection_claim
            .is_some_and(|(owner, _)| owner == token)
        {
            self.active_injection_claim = None;
        }
    }

    pub(super) fn commit_injection_claim(&mut self, token: u64) -> bool {
        if self
            .active_injection_claim
            .is_some_and(|(owner, _)| owner == token)
        {
            self.active_injection_claim = None;
            self.injection_delivery_uncertain = false;
            self.injection_uncertain_since = None;
            self.injection_uncertainty_retryable = false;
            true
        } else {
            false
        }
    }

    pub(super) fn rollback_injection_claim(&mut self, token: u64) -> Option<bool> {
        let (_, prior_idle_confirmed) = self
            .active_injection_claim
            .filter(|(owner, _)| *owner == token)?;
        if self.evidence.activity_seen || self.busy_source_is("hook-busy") {
            self.active_injection_claim = None;
            return None;
        }
        self.active_injection_claim = None;
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        // Restore the pre-claim idle confirmation without restoring the exact
        // prior `Evidence` (not retained) — a synthetic marker reproducing the
        // same `idle_confirmed()` verdict is all any reader consults.
        let (rank, source) = if prior_idle_confirmed {
            (EvidenceRank::Protocol, "restored-confirmed-idle")
        } else {
            (EvidenceRank::Silence, "silence-timeout-agent")
        };
        self.force_idle(rank, source);
        Some(prior_idle_confirmed)
    }

    pub(super) fn mark_injection_uncertain(&mut self, token: u64) {
        self.mark_injection_uncertain_with_retry(token, false);
    }

    pub(super) fn mark_orchestrator_notice_uncertain(&mut self, token: u64) {
        self.mark_injection_uncertain_with_retry(token, true);
    }

    fn mark_injection_uncertain_with_retry(&mut self, token: u64, retryable: bool) {
        if self
            .active_injection_claim
            .is_some_and(|(owner, _)| owner == token)
        {
            self.active_injection_claim = None;
            self.injection_delivery_uncertain = true;
            self.injection_uncertain_since = Some(std::time::Instant::now());
            self.injection_uncertainty_retryable = retryable;
            self.screen_ready_pending_since = None;
        }
    }

    fn invalidate_injection_claim(&mut self) {
        self.active_injection_claim = None;
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
    }

    /// Only the ego adapter proves both Ready and an empty composer. Require
    /// a repaint after uncertainty was recorded and the normal stable-screen
    /// debounce; elapsed silence over a pre-write screen proves nothing.
    pub(super) fn recover_uncertain_injection_on_empty_composer(&mut self) -> bool {
        if !self.injection_delivery_uncertain
            || self.is_api_retry_active()
            || self
                .injection_uncertain_since
                .is_none_or(|since| self.last_chunk_at <= since)
        {
            self.screen_ready_pending_since = None;
            return false;
        }
        let since = self
            .screen_ready_pending_since
            .get_or_insert_with(std::time::Instant::now);
        if since.elapsed() < AGENT_READY_CONFIRM {
            return false;
        }
        self.invalidate_injection_claim();
        self.evidence
            .force_idle(EvidenceRank::Screen, "injection-ready");
        self.last_status_line_at = None;
        self.interrupt_requested_at = None;
        self.evidence.activity_seen = false;
        true
    }

    pub(super) fn expire_orchestrator_notice_uncertainty(&mut self) -> bool {
        if !self.injection_delivery_uncertain
            || !self.injection_uncertainty_retryable
            || self
                .injection_uncertain_since
                .is_none_or(|since| since.elapsed() < ORCHESTRATOR_WAKE_UNCERTAIN_RETRY)
        {
            return false;
        }
        self.injection_delivery_uncertain = false;
        self.injection_uncertain_since = None;
        self.injection_uncertainty_retryable = false;
        self.screen_ready_pending_since = None;
        true
    }

    /// Any recorded busy evidence currently comes from `source` (one of the
    /// explicit markers: `"hook-busy"`/`"osc133-busy"`/`"user-submit"`).
    pub(super) fn busy_source_is(&self, source: &str) -> bool {
        self.evidence.busy.is_some_and(|busy| busy.source == source)
    }

    /// An **explicit** busy marker (OSC hook/shell-integration, or a submitted
    /// line on a ready-adapter agent) is currently in effect. Narrower than
    /// "any busy evidence": screen/raw-activity evidence is deliberately
    /// one-shot (see `apply_working_evidence` and the reader chunk path) and
    /// never sets this, exactly as the old `explicit_busy` was never set by
    /// `note_working_screen`/`note_real_activity`.
    ///
    /// **This is a provenance predicate, not a rank predicate** — the three
    /// sources it accepts do not share a rank, and its doc used to claim they
    /// were all Protocol. They are not: `note_explicit_state` records
    /// `osc133-busy` at [`EvidenceRank::Screen`], because OSC 133 is *shell*
    /// integration. `133;C` fires when a foreground command starts and `133;D`
    /// when it exits, so on a long-lived TUI agent it fires once at launch and
    /// once at death — process-granularity evidence that knows nothing about
    /// turns. A caller deciding whether evidence may hold a turn against the
    /// screen wants `self.evidence.busy.rank`, never this. Reading this as a
    /// rank test is what produced the contradiction #745-8ff1 settled: it made
    /// a stuck `osc133-busy` outlive a stable Ready screen for the whole
    /// process, which is #535-d4f5.
    pub(super) fn explicit_busy(&self) -> bool {
        matches!(
            self.evidence.busy.map(|b| b.source),
            Some("hook-busy" | "osc133-busy" | "user-submit")
        )
    }

    /// A Protocol-rank explicit idle marker (OSC hook/shell-integration) is
    /// the current idle evidence. Narrower than "any idle evidence": a
    /// screen-confirmed ready/interrupted state does not count — callers that
    /// need "is idle confirmed at all" want `idle_confirmed()` instead.
    pub(super) fn explicit_idle(&self) -> bool {
        matches!(
            self.evidence.idle.map(|i| i.source),
            Some("hook-idle" | "osc133-idle")
        )
    }

    pub(crate) fn idle_confirmed(&self) -> bool {
        self.evidence.idle_confirmed()
    }

    /// Record awaiting (question/dialog) evidence — see
    /// `TurnEvidence::record_awaiting`. Exposed on `SilenceState` (rather than
    /// the `evidence` field, which stays private) so state.rs's PtyParsed
    /// dispatcher can share the same ranked model without pty.rs giving up
    /// direct control of the busy/idle axis. Wrapped (like `record_busy`/
    /// `record_idle`) to log a `DecisionTrail` entry, including rejections —
    /// a rejected awaiting attempt (a low-confidence screen-scrape question
    /// blocked by an already-held confident one) is exactly the kind of fact
    /// the trail exists to surface, and this axis is the one this repo's own
    /// AGENTS.md calls out repeatedly as the hardest to debug.
    /// Shared constructor for every `DecisionTrail` push site, so the 7-field
    /// `TrailEntry` literal (and the `at: Instant::now()` it always wants)
    /// lives in exactly one place. Adding a field to `TrailEntry` only means
    /// touching this one call, not each of the 8 sites that used to build it
    /// inline (a review finding: a future field would otherwise silently
    /// default/stale-out at any site the change missed).
    fn push_trail(
        &mut self,
        kind: TrailKind,
        rank: Option<EvidenceRank>,
        source: Option<&'static str>,
        accepted: bool,
        forced: bool,
        outranked_by: Option<(EvidenceRank, &'static str)>,
    ) {
        self.trail.push(TrailEntry {
            at: std::time::Instant::now(),
            kind,
            rank,
            source,
            accepted,
            forced,
            outranked_by,
        });
    }

    pub(crate) fn record_awaiting(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_awaiting(rank, source);
        let outranked_by = (!accepted)
            .then(|| self.evidence.awaiting.filter(|a| a.rank > rank))
            .flatten()
            .map(|e| (e.rank, e.source));
        self.push_trail(
            TrailKind::Awaiting,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    pub(crate) fn clear_awaiting(&mut self) {
        self.evidence.clear_awaiting();
        self.push_trail(TrailKind::ClearAwaiting, None, None, true, false, None);
    }

    pub(crate) fn awaiting_rank(&self) -> Option<EvidenceRank> {
        self.evidence.awaiting_rank()
    }

    pub(crate) fn awaiting_source(&self) -> Option<&'static str> {
        self.evidence.awaiting_source()
    }

    /// BUSY evidence came from an observed agent hook (OSC 7770 `state=busy`
    /// with a live hook), not a bare OSC 133 shell-integration marker.
    #[cfg(test)]
    pub(super) fn hook_busy(&self) -> bool {
        self.busy_source_is("hook-busy")
    }

    /// A user/injected prompt started the current turn on a ready-adapter
    /// agent (`note_user_submission(true)`).
    #[cfg(test)]
    pub(super) fn turn_started_by_input(&self) -> bool {
        self.busy_source_is("user-submit")
    }

    /// Wraps `TurnEvidence::record_busy`, appending a `DecisionTrail` entry —
    /// including on rejection, which the bare `bool` return leaves invisible
    /// to every existing call site. Every call to `evidence.record_busy` in
    /// this module goes through this wrapper instead, so the trail can never
    /// silently miss one.
    pub(super) fn record_busy(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_busy(rank, source);
        let outranked_by = (!accepted)
            .then(|| {
                self.evidence
                    .idle
                    .filter(|idle| idle.rank > rank)
                    .or(self.evidence.busy.filter(|busy| busy.rank > rank))
                    .map(|e| (e.rank, e.source))
            })
            .flatten();
        self.push_trail(
            TrailKind::Busy,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    /// Wraps `TurnEvidence::record_idle` — see `record_busy` above.
    pub(super) fn record_idle(&mut self, rank: EvidenceRank, source: &'static str) -> bool {
        let accepted = self.evidence.record_idle(rank, source);
        let outranked_by = (!accepted)
            .then(|| {
                self.evidence
                    .busy
                    .filter(|busy| busy.rank > rank)
                    .or(self.evidence.idle.filter(|idle| idle.rank > rank))
                    .map(|e| (e.rank, e.source))
            })
            .flatten();
        self.push_trail(
            TrailKind::Idle,
            Some(rank),
            Some(source),
            accepted,
            false,
            outranked_by,
        );
        accepted
    }

    /// Wraps `TurnEvidence::force_idle` — always accepted, bypasses the
    /// rank gate by design, so `outranked_by` is always `None`.
    pub(super) fn force_idle(&mut self, rank: EvidenceRank, source: &'static str) -> Evidence {
        let evidence = self.evidence.force_idle(rank, source);
        self.push_trail(TrailKind::Idle, Some(rank), Some(source), true, true, None);
        evidence
    }

    /// Wraps `TurnEvidence::clear_idle`.
    pub(super) fn clear_idle(&mut self) {
        self.evidence.clear_idle();
        self.push_trail(TrailKind::ClearIdle, None, None, true, false, None);
    }

    /// One-shot busy-evidence clear (the `evidence.busy = None` call sites),
    /// recorded in the trail as `ClearBusy`. Callers already gate this on
    /// their own condition (e.g. "was this busy evidence's source the one
    /// this chunk just applied") — this only performs the clear and logs it.
    pub(super) fn clear_busy_evidence(&mut self) {
        self.evidence.busy = None;
        self.push_trail(TrailKind::ClearBusy, None, None, true, false, None);
    }

    pub(super) fn note_explicit_state(&mut self, state: u8, hook_state: bool) {
        self.invalidate_injection_claim();
        self.hook_state_seen |= hook_state;
        if hook_state {
            self.last_hook_state = Some(state);
        }
        self.screen_ready_pending_since = None;
        match state {
            SHELL_BUSY => {
                let source = if hook_state {
                    "hook-busy"
                } else {
                    "osc133-busy"
                };
                let rank = if hook_state {
                    EvidenceRank::Protocol
                } else {
                    EvidenceRank::Screen
                };
                self.record_busy(rank, source);
                self.last_status_line_at = Some(std::time::Instant::now());
            }
            SHELL_IDLE => {
                let source = if hook_state {
                    "hook-idle"
                } else {
                    "osc133-idle"
                };
                let rank = if hook_state {
                    EvidenceRank::Protocol
                } else {
                    EvidenceRank::Screen
                };
                self.record_idle(rank, source);
                self.last_status_line_at = None;
                self.interrupt_requested_at = None;
                self.evidence.activity_seen = false;
            }
            _ => {}
        }
    }

    fn note_busy_evidence(&mut self) {
        self.clear_idle();
        self.screen_ready_pending_since = None;
    }

    pub(super) fn note_working_screen(&mut self) {
        self.invalidate_injection_claim();
        self.note_busy_evidence();
        self.evidence.activity_seen = true;
        // Keep silence-based question/tool-error detection aligned with shell
        // activity. Previously the working marker refreshed last_output_ms but
        // not SilenceState, allowing contradictory question events.
        self.last_status_line_at = Some(std::time::Instant::now());
    }

    pub(super) fn note_real_activity(&mut self) {
        self.invalidate_injection_claim();
        self.note_busy_evidence();
        self.evidence.activity_seen = true;
    }

    pub(super) fn note_ready_screen(&mut self) -> bool {
        // Only Protocol-rank busy evidence may hold a turn against a stable
        // Ready screen. Deliberately `== Protocol` and not `>= Screen`: the
        // ladder is used asymmetrically here, because Process-rank evidence
        // (the foreground probe) is about the process, not the turn. The probe
        // runs AFTER this returns true and refines the recorded evidence from
        // `Screen` to `Process` (#771-4733); it never gets to override a
        // Protocol-rank hold, which is what this guard exists to protect.
        //
        // Do NOT widen this to accept `explicit_busy()`'s sources (#745-8ff1).
        // That set includes `osc133-busy`, which is shell integration and knows
        // only that a foreground command is running — on a long-lived TUI agent
        // it is set once at launch and cleared only at exit, so honouring it
        // here strands the tab BUSY for the whole process (#535-d4f5). The
        // three `*_recovers_long_lived_shell_busy` tests build a byte-identical
        // `SilenceState`, and a `SilenceState` carries no agent type, so they
        // must all agree; widening this guard turns grok green and goose and
        // opencode red, which moves the contradiction instead of resolving it.
        if self.evidence.busy.is_some_and(|busy| {
            busy.rank == EvidenceRank::Protocol
                && (busy.source != "user-submit" || !self.evidence.activity_seen)
        }) {
            self.screen_ready_pending_since
                .get_or_insert_with(std::time::Instant::now);
            return false;
        }
        if self.injection_delivery_uncertain || self.is_api_retry_active() {
            self.screen_ready_pending_since = None;
            return false;
        }
        let now = std::time::Instant::now();
        let since = *self.screen_ready_pending_since.get_or_insert(now);
        if since.elapsed() < AGENT_READY_CONFIRM {
            return false;
        }
        // The guard above decides when Ready may release busy evidence. An
        // existing idle marker still owns its rank: a timer tick must not
        // downgrade a completed protocol turn so the next repaint reopens it.
        self.clear_busy_evidence();
        self.record_idle(EvidenceRank::Screen, "agent-ready-screen");
        self.last_status_line_at = None;
        self.interrupt_requested_at = None;
        self.evidence.activity_seen = false;
        true
    }

    pub(super) fn note_interrupted_screen(&mut self) -> bool {
        let pending = self
            .interrupt_requested_at
            .is_some_and(|at| at.elapsed() < INTERRUPT_PENDING_TTL);
        if pending {
            self.force_idle(EvidenceRank::Screen, "interrupted-screen");
            self.last_status_line_at = None;
            self.screen_ready_pending_since = None;
            self.interrupt_requested_at = None;
            self.evidence.activity_seen = false;
            return true;
        }
        self.note_ready_screen()
    }

    pub(super) fn note_unknown_screen(&mut self) {
        self.screen_ready_pending_since = None;
        if self
            .interrupt_requested_at
            .is_some_and(|at| at.elapsed() >= INTERRUPT_PENDING_TTL)
        {
            self.interrupt_requested_at = None;
        }
    }

    pub(crate) fn note_interrupt_requested(&mut self) {
        self.interrupt_requested_at = Some(std::time::Instant::now());
        self.screen_ready_pending_since = None;
    }

    pub(crate) fn note_user_submission(&mut self, protocol_instrumented: bool) {
        // An observed human Enter resolves a previous uncertain composer.
        // Queued writes call this before their confirmation wait, so a failed
        // write still marks itself uncertain afterward.
        self.invalidate_injection_claim();
        self.interrupt_requested_at = None;
        self.completion_declared = false;
        self.note_busy_evidence();
        // The real turn boundary — the trail carries no epoch of its own (see
        // `DecisionTrail`'s doc comment), so a reader segments it into turns
        // by this marker instead.
        self.push_trail(TrailKind::UserSubmit, None, None, true, false, None);
        if protocol_instrumented {
            self.record_busy(EvidenceRank::Protocol, "user-submit");
            self.last_status_line_at = Some(std::time::Instant::now());
            self.evidence.activity_seen = false;
        }
    }

    pub(super) fn protocol_busy_is_stale(&self) -> bool {
        self.evidence.busy.is_some_and(|busy| {
            busy.rank == EvidenceRank::Protocol
                && busy.at.elapsed() >= PROTOCOL_STALE_TIMEOUT
                && self.last_output_at.elapsed() >= PROTOCOL_STALE_TIMEOUT
                && self
                    .screen_ready_pending_since
                    .is_some_and(|ready| ready.elapsed() >= PROTOCOL_STALE_TIMEOUT)
        })
    }

    pub(super) fn unacknowledged_submission_is_stale(&self) -> bool {
        self.last_hook_state == Some(SHELL_IDLE)
            && !self.injection_delivery_uncertain
            && self.evidence.busy.is_some_and(|busy| {
                busy.source == "user-submit"
                    && busy.at.elapsed() >= PROTOCOL_STALE_TIMEOUT
                    && self.last_output_at.elapsed() >= PROTOCOL_STALE_TIMEOUT
            })
    }

    #[cfg(test)]
    pub(crate) fn confirm_idle(&mut self) {
        self.force_idle(EvidenceRank::Protocol, "test-confirmed-idle");
    }

    /// Test-only: idle, but not confirmed (mirrors an agent silence-timeout
    /// with no ready-screen adapter — `idle_confirmed()` derives `false`).
    #[cfg(test)]
    pub(crate) fn force_idle_unconfirmed(&mut self) {
        self.force_idle(EvidenceRank::Silence, "silence-timeout-agent");
    }

    /// Called by resize_pty when the terminal is resized.
    /// Marks the start of a grace period during which parsed events are suppressed.
    pub(crate) fn on_resize(&mut self) {
        self.last_resize_at = Some(std::time::Instant::now());
    }

    /// Returns true if we are within the resize grace period.
    /// Parsed events (Question, RateLimit, ApiError) should be suppressed during this window
    /// because the shell redraws visible output after SIGWINCH, causing false re-detections.
    pub(crate) fn is_resize_grace(&self) -> bool {
        self.last_resize_at
            .map(|t| t.elapsed() < RESIZE_GRACE)
            .unwrap_or(false)
    }

    /// Returns true if we are still in the startup grace period.
    /// During this window, notifications are suppressed to avoid reacting to
    /// historical output replayed by `--continue` or similar session restore.
    pub(crate) fn is_startup_grace(&self) -> bool {
        !self.startup_settled
    }

    /// Check if the startup grace should end and update the flag.
    /// Called by the silence timer thread every second.
    pub(crate) fn check_startup_settle(&mut self) {
        if self.startup_settled {
            return;
        }
        // Safety cap: always settle after STARTUP_GRACE_MAX
        if self.created_at.elapsed() >= STARTUP_GRACE_MAX {
            self.startup_settled = true;
            self.pending_suggest_items = None;
            self.pending_suggest_at = None;
            return;
        }
        // Settle after STARTUP_SETTLE_SILENCE without output
        if self.last_output_at.elapsed() >= STARTUP_SETTLE_SILENCE {
            self.startup_settled = true;
            self.pending_suggest_items = None;
            self.pending_suggest_at = None;
        }
    }

    /// Called by the reader thread after each chunk.
    /// `regex_found_question`: true if `parse()` already emitted a Question event.
    /// `last_question_line`: the last `?`-ending line in the chunk, if any.
    /// `has_status_line`: true if the chunk contained a StatusLine parsed event.
    /// `status_line_only`: true if the chunk contained ONLY status-line/mode-line updates.
    ///   Mode-line timer ticks (elapsed time updating every second) are not significant
    ///   output — they must not reset the silence timer or the spinner timestamp,
    ///   or questions asked by Ink agents will never be detected.
    pub(crate) fn on_chunk(
        &mut self,
        regex_found_question: bool,
        last_question_line: Option<String>,
        has_status_line: bool,
        status_line_only: bool,
        suggest_only: bool,
    ) {
        // Always track that a chunk arrived — used by the backup idle timer
        // to distinguish "reader blocked on read()" from "chrome ticks arriving".
        self.last_chunk_at = std::time::Instant::now();

        // Suggest-only chunks are not significant output — they are protocol
        // tokens consumed by the frontend, not real agent text.
        let insignificant = status_line_only || suggest_only;

        if !insignificant {
            self.last_output_at = std::time::Instant::now();
        }

        // Only mark spinner active when the status line accompanies real output.
        // Mode-line timer ticks and suggest-only chunks are not agent activity
        // and must not suppress question detection.
        if has_status_line && !insignificant {
            self.last_status_line_at = Some(std::time::Instant::now());
        }

        // Within the echo suppress window, ignore `?`-ending lines — they are
        // the PTY echoing back user-typed text, not agent questions.
        let in_echo_window = self
            .suppress_echo_until
            .map(|deadline| std::time::Instant::now() < deadline)
            .unwrap_or(false);

        if regex_found_question {
            // The instant detector already fired — suppress the silence timer.
            self.pending_question_line = None;
            self.question_already_emitted = true;
            self.output_chunks_after_question = 0;
        } else if let Some(line) = last_question_line {
            if in_echo_window {
                // Ignore — this is the PTY echo of user input.
            } else if self.question_already_emitted
                && (self.pending_question_line.as_deref() == Some(&line)
                    || self.last_emitted_text.as_deref() == Some(&line))
            {
                // Same `?` text as already emitted (either still pending, or
                // previously emitted and reappearing because new output scrolled
                // it to a different row). Don't reset — otherwise the silence
                // timer will re-fire for every scroll of the same question.
            } else {
                // New candidate for silence-based detection.
                self.pending_question_line = Some(line);
                self.question_already_emitted = false;
                self.output_chunks_after_question = 0;
            }
        } else if self.pending_question_line.is_some() && !insignificant {
            // Non-`?` chunk with real output after a pending candidate — track staleness.
            // Insignificant chunks (mode-line ticks, suggest tokens) are NOT real output
            // and must not count toward staleness, or they will clear the pending question
            // before the silence timer has a chance to detect it.
            self.output_chunks_after_question = self.output_chunks_after_question.saturating_add(1);
            // Once stale, clear pending so the repaint guard won't block the
            // same question text from being detected again in a future session.
            if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
                self.pending_question_line = None;
            }
        }
    }

    /// Called by write_pty when the user submits a line of input.
    /// Clears any pending question candidate since it was typed by the user, not the agent.
    /// Also opens a time window to ignore the PTY echo of the typed text.
    pub(crate) fn suppress_user_input(&mut self) {
        self.pending_question_line = None;
        // A new turn may legitimately ask the same text again. Re-open the
        // screen-anchored strategy while retaining `last_emitted_text`; the
        // unanchored changed-row fallback uses that memory to reject historical
        // repaints of the prior turn.
        self.question_already_emitted = false;
        self.suppress_echo_until = Some(std::time::Instant::now() + ECHO_SUPPRESS_WINDOW);
    }

    /// Returns true if a spinner/status-line was seen recently.
    /// Uses the same threshold as silence detection (10s) so that agents with
    /// pauses between status-line updates (API calls, file reads) don't trigger
    /// false question notifications during those gaps.
    fn is_spinner_active(&self) -> bool {
        self.explicit_busy()
            || self
                .last_status_line_at
                .map(|t| t.elapsed() < SILENCE_QUESTION_THRESHOLD)
                .unwrap_or(false)
    }

    /// Returns true if any chunk (real or chrome-only) was received recently.
    /// The backup idle timer uses this to avoid false idle transitions when the
    /// reader thread IS processing chunks (even chrome-only status-line ticks).
    /// Status-line ticking proves the agent is alive — the backup timer should
    /// only fire when truly no chunks arrive (reader blocked on read()).
    /// The 2s threshold matches the frontend debounce hold (BUSY_HOLD_MS).
    #[allow(dead_code)] // called from tests; kept for backup-idle-timer reintegration
    pub(crate) fn has_recent_chunks(&self) -> bool {
        self.last_chunk_at.elapsed() < std::time::Duration::from_secs(2)
    }

    /// Called by the timer thread. Returns the question text if the silence
    /// threshold has been reached and we haven't emitted yet.
    pub(crate) fn check_silence(&mut self) -> Option<String> {
        if self.question_already_emitted {
            return None;
        }
        // Spinner active means the agent is working — not waiting for input.
        if self.is_spinner_active() {
            return None;
        }
        // Too much output after the `?` line — the agent continued working,
        // so the `?` was not a real question (e.g. code comment, markdown).
        if self.output_chunks_after_question > STALE_QUESTION_CHUNKS {
            return None;
        }
        if let Some(ref line) = self.pending_question_line
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
        {
            if self.last_emitted_text.as_deref() == Some(line.as_str()) {
                return None;
            }
            self.question_already_emitted = true;
            self.last_emitted_text = Some(line.clone());
            return Some(line.clone());
        }
        None
    }

    /// Clear a stale question candidate that failed screen verification.
    /// Prevents the timer from re-checking the same stale candidate every second.
    pub(crate) fn clear_stale_question(&mut self) {
        self.pending_question_line = None;
        self.question_already_emitted = true;
    }

    /// Register an `Error: Exit code N` line seen in visible output. The silence
    /// timer will emit a ToolError event if the session goes idle without any
    /// real-output chunk clearing the candidate (= agent did not recover).
    ///
    /// Idempotent across scroll-induced re-appearances: if this exact line has
    /// already surfaced in the current input epoch, we drop it. Without this,
    /// Ink-based TUIs (Claude Code, Codex) cause `changed_rows` to include the
    /// old error line every time the viewport scrolls, re-arming the candidate
    /// and re-firing the red notification long after the user has resumed.
    pub(crate) fn mark_tool_error_candidate(&mut self, line: String) {
        if self.surfaced_tool_errors.contains(&line) {
            return;
        }
        if self.pending_tool_error.as_deref() == Some(&line) {
            return;
        }
        self.pending_tool_error = Some(line);
    }

    /// Arm (or re-arm) the API connection-retry BUSY hold. Called when
    /// `is_retry_line` matches a changed row: the agent is auto-retrying a failed
    /// API call and is still mid-turn even though its TUI has frozen between
    /// attempts. See `AGENT_RETRY_HOLD` for why this is needed.
    pub(crate) fn mark_api_retry(&mut self) {
        self.api_retry_hold_until = Some(std::time::Instant::now() + AGENT_RETRY_HOLD);
    }

    /// True while an in-flight API retry holds the agent BUSY. Consulted by the
    /// ready-screen and silence idle paths to suppress a premature idle flip.
    pub(crate) fn is_api_retry_active(&self) -> bool {
        self.api_retry_hold_until
            .is_some_and(|deadline| std::time::Instant::now() < deadline)
    }

    /// Called on every real-output chunk that is NOT an error line. Clears the
    /// pending tool-error candidate: if the agent produced real output after an
    /// error, it recovered (e.g. retry) and the error is not turn-ending.
    ///
    /// Does NOT reset `surfaced_tool_errors` — recovery is a transient backend
    /// signal; the user-facing "I've already told you about this error" state
    /// must survive it and only reset on explicit user input.
    pub(crate) fn clear_tool_error_on_recovery(&mut self) {
        self.pending_tool_error = None;
        // Real non-error, non-retry output means the agent recovered from the
        // connection-retry loop — release the BUSY hold so idle detection resumes.
        self.api_retry_hold_until = None;
    }

    /// Clear the "already surfaced" memory so the next occurrence of any error
    /// line — including one we've already fired — can notify again. Called
    /// from `write_pty` when the user submits a line (or Ctrl+C), mirroring
    /// the api-error dedup reset in `OutputParser::parse_clean_lines`.
    pub(crate) fn reset_tool_error_memory(&mut self) {
        self.pending_tool_error = None;
        self.surfaced_tool_errors.clear();
        self.api_retry_hold_until = None;
    }

    /// Park a request to reopen `OutputParser::reset_input_dedup`. Called on the
    /// input thread; the reader consumes it with `take_parser_dedup_reset` on the
    /// next chunk, which is the first moment the parser is reachable again.
    pub(crate) fn request_parser_dedup_reset(&mut self) {
        self.parser_dedup_reset_pending = true;
    }

    /// Consume a parked parser-dedup reset. Returns true at most once per
    /// submitted line.
    pub(crate) fn take_parser_dedup_reset(&mut self) -> bool {
        std::mem::take(&mut self.parser_dedup_reset_pending)
    }

    /// Called by the timer thread. Returns the error text if the silence
    /// threshold has been reached and we haven't emitted yet. Semantics mirror
    /// `check_silence` but use the shorter tool-error threshold.
    pub(crate) fn check_tool_error(&mut self) -> Option<String> {
        if self.is_spinner_active() {
            return None;
        }
        let should_fire = self.pending_tool_error.is_some()
            && self.last_output_at.elapsed() >= SILENCE_TOOL_ERROR_THRESHOLD;
        if !should_fire {
            return None;
        }
        let line = self.pending_tool_error.take()?;
        self.surfaced_tool_errors.insert(line.clone());
        Some(line)
    }

    /// Park `suggest:` items parsed from output. The silence timer will flush
    /// them to the frontend once the shell state transitions to idle — this
    /// is the single source of truth for "turn ended". A newer set overwrites
    /// an older pending set: if the agent updates its suggestions mid-turn,
    /// we deliver the latest.
    pub(crate) fn mark_suggest_candidate(&mut self, items: Vec<String>, turn_epoch: u64) {
        if items.is_empty() {
            return;
        }
        self.completion_declared = true;
        self.completion_turn_epoch = turn_epoch;
        self.pending_suggest_items = Some(items);
        self.pending_suggest_emitted = false;
        self.pending_suggest_turn_epoch = turn_epoch;
        self.pending_suggest_at = Some(std::time::Instant::now());
    }

    pub(super) fn drain_pending_suggest_with_epoch(&mut self) -> Option<(u64, Vec<String>)> {
        self.pending_suggest_at = None;
        self.pending_suggest_items
            .take()
            .map(|items| (self.pending_suggest_turn_epoch, items))
    }

    /// Drain parked suggest items. No gates — trust the caller to invoke only
    /// when the shell state is IDLE (the silence timer does exactly that).
    /// Returns the items once and clears the park slot; a second call returns
    /// `None` until new items are parked.
    #[cfg(test)]
    pub(crate) fn drain_pending_suggest(&mut self) -> Option<Vec<String>> {
        self.drain_pending_suggest_with_epoch()
            .map(|(_, items)| items)
    }

    /// Drop any parked suggest on user input. Parallels `reset_tool_error_memory`:
    /// the user is engaging again, so stale suggestions from the previous turn
    /// must not fire after a new input cycle starts.
    ///
    /// Deliberately does NOT touch `declared_background_work` — see
    /// `reset_declared_background_work`'s doc comment for why that's a
    /// separate method, not folded in here. This method is also called from
    /// `apply_working_evidence`'s "reopen a stale completed/idle turn on
    /// renewed screen evidence" path, which is not a real new turn boundary.
    pub(crate) fn reset_suggest_memory(&mut self) {
        self.pending_suggest_items = None;
        self.pending_suggest_turn_epoch = 0;
        self.pending_suggest_emitted = false;
        self.pending_suggest_at = None;
        self.completion_declared = false;
        self.completion_turn_epoch = 0;
    }

    /// Working evidence retracts completion, not an undelivered choice list.
    /// Claude's idle hook can precede its last Stop-hook spinner repaint; the
    /// pending choices still belong to this turn and must reach the ready UI.
    /// Submitted input uses reset_suggest_memory and discards them instead.
    pub(super) fn reopen_suggest_completion(&mut self) {
        if self.pending_suggest_emitted {
            self.reset_suggest_memory();
        } else {
            self.completion_declared = false;
            self.completion_turn_epoch = 0;
        }
    }

    /// Clear a `bgtasks`-declared background-work claim. Call this ONLY at a
    /// genuine new-turn boundary (a real line/interrupt submitted to the
    /// agent) — never from `apply_working_evidence`'s reopening path.
    ///
    /// This used to be folded into `reset_suggest_memory` on the assumption
    /// that "a new turn invalidates any prior declaration" — true, but
    /// `reset_suggest_memory` is ALSO called when renewed screen evidence
    /// reopens a stale completed/idle Claude turn (`apply_working_evidence`),
    /// which is not a new turn at all: for an agent orchestrating background
    /// teammates, waking up to poll them is expected and doesn't mean the
    /// teammates finished. Folding the two together meant every such poll
    /// silently erased a still-accurate `declared_background_work=true` the
    /// moment the parent's own screen showed renewed activity, until the
    /// next `Stop` hook happened to re-assert it — see the `ai-usage`
    /// Agent-Teams incident (2026-09-16) this split was extracted from.
    pub(crate) fn reset_declared_background_work(&mut self) {
        self.declared_background_work = false;
        self.declared_background_work_turn_epoch = 0;
    }

    #[cfg(test)]
    pub(crate) fn completion_declared(&self) -> bool {
        self.completion_declared
    }

    pub(crate) fn completion_declared_for_epoch(&self, turn_epoch: u64) -> bool {
        self.completion_declared && self.completion_turn_epoch == turn_epoch
    }

    /// A final marker can arrive while the same redraw still contains a spinner.
    /// Only settled/delivered completion is a boundary that new work can reopen.
    pub(crate) fn completion_settled_for_epoch(&self, turn_epoch: u64) -> bool {
        self.completion_declared_for_epoch(turn_epoch)
            && (self.pending_suggest_items.is_none()
                || self.pending_suggest_emitted
                || self.idle_confirmed())
    }

    /// Set from the `bgtasks` OSC 7770 verb (`pty.rs`'s `TermEvent::Tuic`
    /// dispatch) — Claude's own hook declared at least one background task
    /// with a "still running" status as of `turn_epoch`.
    pub(crate) fn set_declared_background_work(&mut self, active: bool, turn_epoch: u64) {
        self.declared_background_work = active;
        self.declared_background_work_turn_epoch = turn_epoch;
    }

    /// Mirrors `completion_declared_for_epoch`: only true for the CURRENT
    /// turn — a new turn silently invalidates a stale declaration even
    /// without an explicit clear (see `reset_declared_background_work`).
    pub(crate) fn declared_background_work_for_epoch(&self, turn_epoch: u64) -> bool {
        self.declared_background_work && self.declared_background_work_turn_epoch == turn_epoch
    }

    /// Returns true if the session has been silent long enough and the spinner
    /// is not active. Used by the timer thread before reading the screen.
    pub(crate) fn is_silent(&self) -> bool {
        !self.question_already_emitted
            && !self.is_spinner_active()
            && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
    }

    /// Retraction is independent from question emission. Once a low-confidence
    /// wait is active, the timer must keep reconciling it even though
    /// `question_already_emitted` deliberately blocks another SET.
    pub(super) fn is_quiet_for_question_retraction(&self) -> bool {
        !self.is_spinner_active() && self.last_output_at.elapsed() >= SILENCE_QUESTION_THRESHOLD
    }

    /// Mark that a question has been emitted (prevents re-emission).
    /// Stores the emitted text so that scroll-induced reappearances of the same
    /// `?` line in changed_rows are recognized as duplicates, not new questions.
    pub(crate) fn mark_emitted(&mut self, text: &str) {
        self.question_already_emitted = true;
        self.last_emitted_text = Some(text.to_string());
    }
}

/// Attempt a shell state transition using compare_exchange.
/// Returns true if the transition was performed (and a ShellState event should be emitted).
/// Attempt an atomic shell-state transition.
///
/// When `notify_parent` is true and the transition is BUSY→IDLE, pushes a
/// state_change message to the parent's inbox (used during normal idle detection).
/// Pass `notify_parent=false` from process-exit paths — the sole "exited"
/// notification from `mark_session_exited` is sufficient; suppressing the
/// intermediate "idle" avoids the orchestrator double-firing on exit.
///
/// RE-ENTRANCY INVARIANT (CONC-C, story 099-6526): this fn does its own
/// `shell_states.get(session_id)` below. Callers MUST NOT hold a `shell_states`
/// Ref for the same key across this call — a held Ref plus this second get on the
/// same shard can deadlock under parking_lot writer-fairness when a concurrent
/// session create/destroy is queued to write the shard between the two reads.
/// Load what you need, drop the Ref, then call. Internally the Ref is dropped
/// BEFORE any post-transition work for the same reason:
/// `flush_pending_injections_blocking` re-reads `shell_states` through
/// `should_inject_now` on this very thread. (`push_state_change_to_parent` reaches
/// the same read via `deliver_notice_to_pty`, but hands it to the injection
/// worker, so it is no longer this thread's re-entrancy to manage.)
pub(super) fn try_shell_transition(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
) -> bool {
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    try_shell_transition_for_epoch(
        state,
        session_id,
        expected,
        new,
        notify_parent,
        observed_turn_epoch,
    )
}

pub(super) fn try_shell_transition_for_epoch(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
    observed_turn_epoch: Option<u64>,
) -> bool {
    try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state,
            session_id,
            expected,
            new,
            notify_parent,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas: || {},
            before_parent_dispatch: || {},
        },
    )
}

#[cfg(test)]
pub(super) fn try_shell_transition_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    expected: u8,
    new: u8,
    notify_parent: bool,
    after_cas: F,
) -> bool {
    let observed_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    try_shell_transition_with_hooks(
        ShellTransitionRequest {
            state,
            session_id,
            expected,
            new,
            notify_parent,
            observed_turn_epoch,
        },
        ShellTransitionHooks {
            after_epoch_snapshot: || {},
            after_cas,
            before_parent_dispatch: || {},
        },
    )
}

#[derive(Clone, Copy)]
pub(super) struct ShellTransitionRequest<'a> {
    pub(super) state: &'a crate::state::AppState,
    pub(super) session_id: &'a str,
    pub(super) expected: u8,
    pub(super) new: u8,
    pub(super) notify_parent: bool,
    pub(super) observed_turn_epoch: Option<u64>,
}

pub(super) struct ShellTransitionHooks<B: FnOnce(), A: FnOnce(), D: FnOnce()> {
    pub(super) after_epoch_snapshot: B,
    pub(super) after_cas: A,
    pub(super) before_parent_dispatch: D,
}

pub(super) fn try_shell_transition_with_hooks<B: FnOnce(), A: FnOnce(), D: FnOnce()>(
    transition: ShellTransitionRequest<'_>,
    hooks: ShellTransitionHooks<B, A, D>,
) -> bool {
    // One lifecycle lock covers CAS through the authoritative parent inbox
    // enqueue. Submitted-turn reservations take the same lock, so a new epoch
    // cannot begin between an IDLE CAS and the preceding turn's notification.
    (hooks.after_epoch_snapshot)();
    let silence = transition
        .state
        .session_maps
        .silence_states
        .get(transition.session_id)
        .map(|entry| Arc::clone(entry.value()));
    let (transitioned, parent_dispatch) = {
        let mut silence_guard = silence.as_ref().map(|silence| silence.lock());
        let silence_state = silence_guard.as_deref_mut();
        try_shell_transition_locked(transition, silence_state, hooks.after_cas)
    };
    if let Some(dispatch) = parent_dispatch {
        (hooks.before_parent_dispatch)();
        dispatch_parent_lifecycle(transition.state, dispatch);
    }
    transitioned
}

/// Perform a shell transition while the caller owns the lifecycle lock.
/// `note_submitted_input` uses this form so epoch mutation and IDLE→BUSY are
/// one critical section instead of recursively acquiring `SilenceState`.
pub(super) fn try_shell_transition_locked<F: FnOnce()>(
    transition: ShellTransitionRequest<'_>,
    mut silence_state: Option<&mut SilenceState>,
    after_cas: F,
) -> (bool, Option<ParentLifecycleDispatch>) {
    let ShellTransitionRequest {
        state,
        session_id,
        expected,
        new,
        notify_parent,
        observed_turn_epoch,
    } = transition;
    if expected == SHELL_BUSY
        && new == SHELL_IDLE
        && observed_turn_epoch.is_some_and(|observed| {
            state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.turn_epoch != observed)
        })
    {
        return (false, None);
    }
    let ok = match state.session_maps.shell_states.get(session_id) {
        Some(atom) => atom
            .compare_exchange(
                expected,
                new,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok(),
        None => return (false, None),
    };
    let mut parent_dispatch = None;
    if ok {
        after_cas();
    }
    // Ref dropped here — post-transition work below re-enters shell_states.
    if ok {
        if new == SHELL_BUSY
            && let Some(silence) = silence_state.as_mut()
        {
            silence.note_busy_evidence();
            invalidate_background_probe_boundary_locked(state, session_id);
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        // Insert with the correct timestamp immediately so concurrent
        // readers never observe a transient 0 between or_insert and store.
        state
            .session_maps
            .shell_state_since_ms
            .entry(session_id.to_string())
            .and_modify(|a| a.store(now_ms, std::sync::atomic::Ordering::Relaxed))
            .or_insert_with(|| std::sync::atomic::AtomicU64::new(now_ms));
        let evidence = silence_state.as_ref().and_then(|silence| {
            if new == SHELL_BUSY {
                silence.evidence.busy
            } else {
                silence.evidence.idle
            }
        });
        tracing::debug!(
            session_id,
            activity_source = evidence.map(|item| item.source).unwrap_or("transition"),
            rank = ?evidence.map(|item| item.rank),
            "Shell state → {}",
            shell_state_wire(new).unwrap_or("unknown")
        );
        // Notify orchestrator when an agent goes idle (BUSY→IDLE only).
        // Plain shell sessions are excluded — only registered agent sessions qualify.
        if notify_parent && expected == SHELL_BUSY && new == SHELL_IDLE {
            let session_lifecycle = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|s| (s.agent_type.is_some(), s.turn_epoch));
            let completion_declared = session_lifecycle.is_some_and(|(_, turn_epoch)| {
                silence_state
                    .as_ref()
                    .is_some_and(|silence| silence.completion_declared_for_epoch(turn_epoch))
            });
            let is_agent = session_lifecycle.is_some_and(|(is_agent, _)| is_agent);
            let has_background_work = state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.background_work);
            let background_probe_pending = state
                .session_maps
                .session_states
                .get(session_id)
                .is_some_and(|session| session.has_pending_background_probe());
            // `silence_state` (the caller's already-locked guard) — NOT
            // `state.declared_background_work_for`, which would try to
            // re-lock the same non-reentrant mutex and deadlock.
            let declared_background_work = session_lifecycle.is_some_and(|(_, turn_epoch)| {
                silence_state
                    .as_ref()
                    .is_some_and(|silence| silence.declared_background_work_for_epoch(turn_epoch))
            });
            if is_agent
                && !completion_declared
                && !has_background_work
                && !background_probe_pending
                && !declared_background_work
            {
                parent_dispatch = enqueue_state_change_to_parent(
                    state,
                    session_id,
                    serde_json::json!({
                        "type": "state_change",
                        "state": "idle",
                        "session_id": session_id,
                    }),
                );
            }
        }
    }
    (ok, parent_dispatch)
}

/// Decision from `should_transition_idle`.
///
/// `force_cleared_subtasks` is true only on the stale-subtask recovery path —
/// callers must emit `ActiveSubtasks { count: 0 }` so the frontend store and
/// notification gate reset (story 1366-2b3e/H1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IdleDecision {
    pub(super) should_transition: bool,
    pub(super) force_cleared_subtasks: bool,
    pub(super) turn_epoch: Option<u64>,
}

impl IdleDecision {
    pub(super) const NO: Self = Self {
        should_transition: false,
        force_cleared_subtasks: false,
        turn_epoch: None,
    };

    pub(super) const fn yes(turn_epoch: Option<u64>) -> Self {
        Self {
            should_transition: true,
            force_cleared_subtasks: false,
            turn_epoch,
        }
    }
}

/// Current wall-clock time as milliseconds since the Unix epoch.
pub(super) fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Check whether the session should transition to idle (busy → idle).
/// Conditions: last real output > threshold ago AND no active sub-tasks.
/// Agent sessions use a longer threshold (AGENT_IDLE_MS) because AI agents
/// produce output in bursts with natural thinking pauses between them.
pub(super) fn should_transition_idle(
    state: &crate::state::AppState,
    session_id: &str,
) -> IdleDecision {
    should_transition_idle_with_hook(state, session_id, || {})
}

pub(super) fn should_transition_idle_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    after_silence_evidence: F,
) -> IdleDecision {
    // Capture the originating turn before reading the silence evidence. A new
    // submission updates the epoch before stamping last_output_ms; either this
    // decision sees the fresh timestamp, or the transition rejects its stale
    // epoch. Reading these in the opposite order can pair old silence with the
    // new turn and immediately idle a just-submitted task.
    //
    // Read the snapshot in a scoped block so the DashMap shard read-lock is
    // released before we take a write-lock below — same shard would otherwise
    // deadlock the runtime in the force-clear branch.
    let (is_agent, sub_tasks, turn_epoch) = {
        let session = state.session_maps.session_states.get(session_id);
        (
            session
                .as_ref()
                .map(|s| s.agent_type.is_some())
                .unwrap_or(false),
            session.as_ref().map(|s| s.active_sub_tasks).unwrap_or(0),
            session.as_ref().map(|s| s.turn_epoch),
        )
    };
    let last_ms = state
        .session_maps
        .last_output_ms
        .get(session_id)
        .map(|ts| ts.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    after_silence_evidence();
    if last_ms == 0 {
        return IdleDecision::NO;
    }
    let threshold = if is_agent {
        AGENT_IDLE_MS
    } else {
        SHELL_IDLE_MS
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let elapsed = now.saturating_sub(last_ms);
    if elapsed < threshold {
        return IdleDecision::NO;
    }
    if sub_tasks == 0 {
        return IdleDecision::yes(turn_epoch);
    }
    // Sub-tasks are active but no output for SUBTASK_STALE_MS — the mode-line
    // disappeared without emitting count=0 (agent exited, user cleared, etc.).
    // Force-clear the stale counter so we don't stay busy forever.
    if elapsed >= SUBTASK_STALE_MS {
        if let Some(mut entry) = state.session_maps.session_states.get_mut(session_id) {
            entry.active_sub_tasks = 0;
        }
        return IdleDecision {
            should_transition: true,
            force_cleared_subtasks: true,
            turn_epoch,
        };
    }
    IdleDecision::NO
}

pub(super) fn stamp_last_output_now(state: &crate::state::AppState, session_id: &str, now_ms: u64) {
    if let Some(ts) = state.session_maps.last_output_ms.get(session_id) {
        ts.store(now_ms, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Apply positive working evidence immediately. In particular this repairs an
/// already-false-idle session: working evidence is an edge into BUSY, not merely
/// a keepalive that only runs while the state happens to be busy. An explicit
/// idle marker (agent hook) outranks it until the next busy evidence.
///
/// Two evidence strengths call this (#446-596f):
/// - `"working-screen"` — presence-based `• Working (… esc to interrupt)`;
///   this holds an open turn but cannot reopen a completed Codex turn because
///   completed screens can retain a stale static Working row.
/// - `"working-screen-movement"` — that exact semantic row occurred among the
///   post-cutoff `changed_rows`. Text-equality diffing means this fires only
///   while the row actually animates or its elapsed time advances, so it can
///   safely reopen an internal Codex continuation that had no PTY submission.
pub(super) fn apply_working_evidence(
    state: &crate::state::AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
    now_ms: u64,
    source: &'static str,
) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|session| session.agent_type.clone());
    let can_reopen_completed = agent_type.as_deref() == Some("claude")
        || (agent_type.as_deref() == Some("codex") && source == "working-screen-movement");
    let (reopened_completion, evidence_snapshot) = {
        let mut sl = silence.lock();
        let turn_completed = state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| sl.completion_settled_for_epoch(session.turn_epoch));
        if turn_completed && !can_reopen_completed {
            return;
        }
        if sl.explicit_idle() && !can_reopen_completed {
            return;
        }
        let reopen = can_reopen_completed && (turn_completed || sl.explicit_idle());
        if reopen {
            // Claude can emit Stop/suggest before a blocking Stop hook finishes;
            // Codex can start an internal continuation without a PTY submission.
            // Current semantic movement is stronger than either stale boundary.
            sl.reopen_suggest_completion();
        }
        sl.note_working_screen();
        invalidate_background_probe_boundary_locked(state, session_id);
        // One-shot evidence: it must win THIS decision (working-screen evidence
        // is deliberately allowed to override even Protocol-rank idle once the
        // reopen checks above already cleared it), but must not persist and
        // block a later, unrelated idle evidence recording — see the comment
        // on the reader chunk path's `real_activity` CAS for the same reasoning.
        let rank = if reopen {
            EvidenceRank::Protocol
        } else {
            EvidenceRank::Screen
        };
        sl.record_busy(rank, source);
        (reopen, sl.evidence.clone())
    };
    if reopened_completion
        && let Some(mut session) = state.session_maps.session_states.get_mut(session_id)
    {
        session.suggested_actions = None;
    }
    stamp_last_output_now(state, session_id, now_ms);
    let prev = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire));
    if let Some(prev) = prev
        && let Some(Transition::ToBusy) = decide(
            &evidence_snapshot,
            prev == SHELL_BUSY,
            std::time::Instant::now(),
        )
        && try_shell_transition(state, session_id, prev, SHELL_BUSY, true)
    {
        emit_shell_state(state, session_id, "busy");
    }
    let mut silence = silence.lock();
    if silence
        .evidence
        .busy
        .is_some_and(|busy| busy.source == source)
    {
        silence.clear_busy_evidence();
    }
}

/// A submitted line to a known agent is strong BUSY evidence even before the
/// first model token or spinner repaint. Adapter-backed agents hold that state
/// until a ready screen/explicit Stop; unknown agents retain the timing fallback.
pub(crate) fn note_submitted_input(state: &AppState, session_id: &str) {
    note_submitted_input_with_hook(state, session_id, || {});
}

pub(super) fn note_submitted_input_with_hook<F: FnOnce()>(
    state: &AppState,
    session_id: &str,
    after_epoch: F,
) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    let Some(agent_type) = agent_type else {
        if let Some(sl) = state.session_maps.silence_states.get(session_id) {
            let mut silence = sl.lock();
            silence.note_user_submission(false);
            silence.reset_suggest_memory();
            silence.reset_declared_background_work();
        }
        return;
    };

    let silence = state
        .session_maps
        .silence_states
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(SilenceState::new())))
        .clone();
    {
        // Lock order for submitted turns is SilenceState → SessionState → shell
        // atomics. Completion drains and Suggest parsing use the same order.
        let mut silence = silence.lock();
        if let Some(mut session) = state.session_maps.session_states.get_mut(session_id) {
            session.turn_epoch = session.turn_epoch.wrapping_add(1);
            session.suggested_actions = None;
            // The denominator for marker compliance (#4421): one submitted turn
            // is one chance for the agent to emit its markers.
            state.note_marker(session_id, crate::state::MarkerKind::TurnSubmitted);
        }
        after_epoch();
        // The gate is the ready-screen adapter, NOT `hook_instrumented`: those
        // are different properties and swapping them regressed every
        // ready-adapter agent that runs without hooks. A Protocol-rank
        // "user-submit" needs something that can later retract it, and the
        // adapter is that something — a hook-instrumented agent emits its own
        // hook-busy anyway, so gating on hooks both misses the agents that need
        // this evidence and is redundant for the ones that do not.
        silence.note_user_submission(has_ready_screen_adapter(Some(&agent_type)));
        silence.reset_suggest_memory();
        silence.reset_declared_background_work();
        stamp_last_output_now(state, session_id, now_epoch_ms());
        let prev = state
            .session_maps
            .shell_states
            .get(session_id)
            .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire));
        let transitioned = prev.is_some_and(|prev| {
            prev != SHELL_BUSY
                && try_shell_transition_locked(
                    ShellTransitionRequest {
                        state,
                        session_id,
                        expected: prev,
                        new: SHELL_BUSY,
                        notify_parent: true,
                        observed_turn_epoch: None,
                    },
                    Some(&mut silence),
                    || {},
                )
                .0
        });
        if transitioned {
            emit_shell_state(state, session_id, "busy");
        }
    };
}

/// Emit a ShellState parsed event via both event bus and Tauri IPC.
pub(super) fn emit_shell_state(
    state: &crate::state::AppState,
    session_id: &str,
    shell_state: &str,
) {
    let agent_type = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| s.agent_type.clone());
    let parsed = ParsedEvent::ShellState {
        state: shell_state.to_string(),
        agent_type,
    };
    match serde_json::to_value(&parsed) {
        Ok(json) => {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: json.into(),
            });
        }
        Err(e) => tracing::error!(session_id, "Failed to serialize ShellState event: {e}"),
    }
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("pty-parsed-{session_id}"), &parsed);
    }
}

/// Apply an authoritative shell-state marker and emit the new state if it
/// changed. Shared by OSC 133 A/C and OSC 7770 `state=` handlers. Returns
/// whether this call caused a real transition (as opposed to a same-state
/// re-affirmation or a stale/unknown-session no-op) — callers that need to
/// distinguish a genuine idle↔busy edge from a redundant re-affirmation
/// (e.g. turn-level block synthesis) use this instead of re-deriving it.
pub(super) fn transition_explicit_shell_state(
    state: &crate::state::AppState,
    session_id: &str,
    target: u8,
    label: &str,
    hook_state: bool,
) -> bool {
    transition_explicit_shell_state_impl(state, session_id, target, label, hook_state, || {}, false)
}

#[cfg(test)]
pub(super) fn transition_explicit_shell_state_with_hook<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    target: u8,
    label: &str,
    hook_state: bool,
    before_transaction: F,
) -> bool {
    transition_explicit_shell_state_impl(
        state,
        session_id,
        target,
        label,
        hook_state,
        before_transaction,
        true,
    )
}

fn transition_explicit_shell_state_impl<F: FnOnce()>(
    state: &crate::state::AppState,
    session_id: &str,
    target: u8,
    label: &str,
    hook_state: bool,
    before_transaction: F,
    flush_on_idle: bool,
) -> bool {
    // A hook busy/idle transition proves the agent is no longer blocked on a
    // question, so it retracts the awaiting badge. Emit ONLY when a badge is
    // actually set: the badge is sticky state, not a stream, so this is an edge
    // — one event per real clear, never one per transition.
    let clears_awaiting = hook_state
        && matches!(target, SHELL_BUSY | SHELL_IDLE)
        && state
            .session_maps
            .session_states
            .get(session_id)
            .is_some_and(|session| session.awaiting_input);
    if clears_awaiting {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: serde_json::json!({ "type": "protocol-question-cleared" }).into(),
        });
    }
    let evidence_turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch);
    before_transaction();
    let silence = state
        .session_maps
        .silence_states
        .get(session_id)
        .map(|entry| Arc::clone(entry.value()));
    let (transitioned, parent_dispatch) = {
        let mut silence_guard = silence.as_ref().map(|silence| silence.lock());
        if target == SHELL_IDLE
            && evidence_turn_epoch.is_some_and(|observed| {
                state
                    .session_maps
                    .session_states
                    .get(session_id)
                    .is_some_and(|session| session.turn_epoch != observed)
            })
        {
            return false;
        }
        if let Some(silence) = silence_guard.as_mut() {
            silence.note_explicit_state(target, hook_state);
            if hook_state && target == SHELL_BUSY {
                silence.last_hook_busy_offset = state
                    .session_maps
                    .output_buffers
                    .get(session_id)
                    .map(|ring| ring.lock().total_written);
            }
            if target == SHELL_BUSY {
                invalidate_background_probe_boundary_locked(state, session_id);
            }
        }
        if target == SHELL_BUSY {
            stamp_last_output_now(state, session_id, now_epoch_ms());
        }
        let prev = match state.session_maps.shell_states.get(session_id) {
            Some(atom) => atom.load(std::sync::atomic::Ordering::Acquire),
            None => return false,
        };
        if prev == target {
            return false;
        }
        if prev == SHELL_BUSY
            && target == SHELL_IDLE
            && let Some(turn_epoch) = evidence_turn_epoch
        {
            arm_explicit_idle_background_probe(state, session_id, turn_epoch);
        }
        tracing::debug!(
            session_id = %session_id,
            prev,
            target,
            label,
            hook_state,
            "shell_state edge attempt (research: unexpected state transitions)"
        );
        let (transitioned, parent_dispatch) = try_shell_transition_locked(
            ShellTransitionRequest {
                state,
                session_id,
                expected: prev,
                new: target,
                notify_parent: true,
                observed_turn_epoch: evidence_turn_epoch,
            },
            silence_guard.as_deref_mut(),
            || {},
        );
        (transitioned, parent_dispatch)
    };
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    tracing::debug!(
        session_id = %session_id,
        target,
        label,
        transitioned,
        "shell_state edge result (research: unexpected state transitions)"
    );
    if transitioned {
        emit_shell_state(state, session_id, label);
        // Publish IDLE before a queued delivery claims IDLE→BUSY again. Reversing
        // this order leaves the backend BUSY while the frontend's last event is
        // the stale IDLE emitted by this caller.
        if target == SHELL_IDLE {
            reevaluate_orchestrator_mail_wake(state, session_id);
            if flush_on_idle {
                flush_pending_injections_blocking(state, session_id);
            }
        }
    }
    transitioned
}

/// Emit an ActiveSubtasks parsed event via both event bus and Tauri IPC.
/// Used by the stale-subtasks recovery path to keep the frontend store in
/// sync after `should_transition_idle` force-clears the in-memory counter.
pub(super) fn emit_active_subtasks(
    state: &crate::state::AppState,
    session_id: &str,
    count: u32,
    task_type: &str,
) {
    let parsed = ParsedEvent::ActiveSubtasks {
        count,
        task_type: task_type.to_string(),
    };
    match serde_json::to_value(&parsed) {
        Ok(json) => {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: session_id.to_string(),
                parsed: json.into(),
            });
        }
        Err(e) => tracing::error!(session_id, "Failed to serialize ActiveSubtasks event: {e}"),
    }
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(&format!("pty-parsed-{session_id}"), &parsed);
    }
}

/// Extract a signal number from portable_pty's signal string.
/// Format is typically "Killed: 9", "Interrupt: 2", or "Signal 15".
pub(crate) fn parse_signal_number(sig: &str) -> i32 {
    sig.rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0)
}

pub(super) fn parse_osc7_cwd(url: &str) -> Result<String, ()> {
    let rest = url.strip_prefix("file://").ok_or(())?;
    let path_start = rest.find('/').ok_or(())?;
    let raw = &rest[path_start..];
    if raw.is_empty() {
        return Err(());
    }
    let decoded = percent_decode(raw)?;
    let path = if decoded.len() > 1 && decoded.ends_with('/') {
        &decoded[..decoded.len() - 1]
    } else {
        &decoded
    };
    if !path.starts_with('/') {
        return Err(());
    }
    Ok(path.to_string())
}

fn percent_decode(s: &str) -> Result<String, ()> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]).ok_or(())?;
            let lo = hex_val(bytes[i + 2]).ok_or(())?;
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(super) fn parse_osc133_exit_code(command: char, params: &str) -> Option<i32> {
    if command == 'D' && !params.is_empty() {
        params.parse::<i32>().ok()
    } else {
        None
    }
}

/// Detect Claude Code tool call headers: `⏺ ToolName(args)`.
/// The ⏺ (U+23FA) bullet followed by a capitalized word and `(` is unique to
/// CC's expanded tool-call rendering — agent prose after ⏺ starts with a
/// lowercase word or a proper noun without parens.
pub(super) fn is_cc_tool_call_header(text: &str) -> bool {
    let trimmed = text.trim_start();
    let rest = if let Some(r) = trimmed.strip_prefix('\u{23FA}') {
        r
    } else {
        return false;
    };
    let rest = rest.trim_start();
    if rest.is_empty() {
        return false;
    }
    // Must start with uppercase ASCII (ToolName) or `mcp__` prefix.
    let first = rest.as_bytes()[0];
    if !first.is_ascii_uppercase() && !rest.starts_with("mcp__") {
        return false;
    }
    // Find the opening paren — everything before it must be a single
    // identifier (no spaces). Rejects prose like "Boss, ci sono (molti)".
    rest.find('(').is_some_and(|pos| {
        let before = &rest[..pos];
        !before.is_empty()
            && before
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// Detect a `❯ ` prompt line in a Claude Code fullscreen transcript-mode `[`
/// dump — see `synthesize_transcript_dump_block_events`'s doc comment.
/// Returns the prompt text (everything after "❯ ", possibly empty).
pub(super) fn transcript_dump_prompt_text(text: &str) -> Option<&str> {
    text.trim_start().strip_prefix("\u{276F} ")
}

/// Detect a `✻ ... · done ...` turn-completion line in a `[` dump.
pub(super) fn is_transcript_dump_turn_end(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('\u{273B}') && trimmed.contains("\u{b7} done")
}

/// Synthesize `AgentBlock` events from a Claude Code fullscreen
/// transcript-mode `[` dump — the "write the full conversation to native
/// scrollback" gesture (`Ctrl+O` to enter transcript mode, then `[`).
/// Verified live (throwaway session via the HTTP API, 2026-09-15): pressing
/// `[` genuinely exits the alternate screen buffer and prints the whole
/// conversation as plain, human-formatted text into REAL primary-screen
/// scrollback — `❯ <prompt>` starts each turn, `✻ ... · done <time>` ends
/// it. Unlike live/transcript-mode fullscreen (which has no real scrollback
/// to anchor into at all — see `is_alternate_screen()`'s callers elsewhere in
/// this file), this dump IS real, monotonically-increasing primary content,
/// so blocks synthesized from it get `on_alt_screen: false` and work with
/// every row-anchored feature (gutter, scrollbar, fold, jump-nav,
/// block-scoped search) exactly like a real shell block would.
///
/// Deliberately narrow: only for a hook-instrumented Claude Code session
/// (`agent_type == "claude" && hook_instrumented`) — a `❯ ` prompt glyph is
/// not exclusive to this dump (this repo's own zsh prompt can use one), so
/// this stays scoped to exactly the one agent/scenario it was verified
/// against. The caller is also expected to only invoke this for chunks where
/// the alternate screen is NOT active — this function does not check that
/// itself, since by construction (`on_alt_screen: false` always) it has
/// nothing else to gate on.
///
/// Per the docs, `[` re-dumps the *entire* conversation from scratch each
/// time it's pressed — every `❯ ` line here is just treated as a fresh block
/// boundary, the same as `synthesize_cc_block_events` does for `⏺` headers,
/// regardless of whether the same logical turn was already recorded by an
/// earlier dump. What stops a repeat gesture from leaving a second,
/// overlapping copy of the same blocks in `commandBlocks[]` forever
/// (2026-09-15) is `is_new_generation`, not text-level dedup: the caller
/// tracks whether the alternate screen was visited since the last dump
/// activity (only possible between two dumps, since `[` is only reachable
/// from transcript mode) and passes `true` exactly once, for the chunk that
/// resumes dump activity after such a visit. This function marks the first
/// `start` event it synthesizes from that chunk `new_dump_generation: true`;
/// the frontend responds by pruning every prior `fromTranscriptDump: true`
/// block (including a still-open `activeBlock`) before adding the new one —
/// see `terminals.ts`'s `handleOsc133`. A dump that only ever *adds* turns to
/// the screen without an intervening alt-screen visit (impossible today,
/// since Claude Code's fullscreen renderer owns the whole session between
/// dumps, but not a structural assumption this function makes) would
/// correctly keep `is_new_generation: false` and never prune.
///
/// `teardown_end_line` closes a still-open dump block the moment this
/// function's own gate (`agent_type`/`hook_instrumented`) stops holding —
/// mirrors `synthesize_cc_block_events`'s `!agent_active` branch. Without
/// this, `last_dump_block_line` could stay `Some` forever if the gate flips
/// before a `✻ … · done` marker ever appears (the agent exits). A dump left
/// open by leaving transcript mode instead (no gate flip) is handled by the
/// generation-prune above once the next dump starts, not by this teardown.
pub(super) fn synthesize_transcript_dump_block_events(
    changed_rows: &[crate::state::ChangedRow],
    total_scrolled: usize,
    agent_type: Option<&str>,
    hook_instrumented: bool,
    teardown_end_line: usize,
    last_dump_block_line: &mut Option<usize>,
    is_new_generation: bool,
) -> Vec<ParsedEvent> {
    if agent_type != Some("claude") || !hook_instrumented {
        if let Some(prev) = last_dump_block_line.take() {
            return vec![ParsedEvent::AgentBlock {
                action: "end".into(),
                line: teardown_end_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: false,
            }];
        }
        return Vec::new();
    }
    let mut events = Vec::new();
    // Only the very first `start` event synthesized after `is_new_generation`
    // came in true carries `new_dump_generation: true` — consumed here so a
    // chunk containing several `❯` rows doesn't repeat the signal for every one.
    let mut pending_new_generation = is_new_generation;
    for row in changed_rows {
        let abs_line = total_scrolled + row.row_index;
        if let Some(prompt_text) = transcript_dump_prompt_text(&row.text) {
            if Some(abs_line) == *last_dump_block_line {
                continue;
            }
            // Mirrors synthesize_cc_block_events's clamp: the new block's
            // start must never precede the end just emitted for the block
            // it's closing.
            let start_line = if let Some(prev) = *last_dump_block_line {
                let end_line = abs_line.max(prev + 1);
                events.push(ParsedEvent::AgentBlock {
                    action: "end".into(),
                    line: end_line as i64,
                    exit_code: None,
                    prompt_text: None,
                    on_alt_screen: false,
                    from_transcript_dump: true,
                    new_dump_generation: false,
                });
                end_line
            } else {
                abs_line
            };
            events.push(ParsedEvent::AgentBlock {
                action: "start".into(),
                line: start_line as i64,
                exit_code: None,
                prompt_text: if prompt_text.is_empty() {
                    None
                } else {
                    Some(prompt_text.to_string())
                },
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: std::mem::take(&mut pending_new_generation),
            });
            *last_dump_block_line = Some(start_line);
        } else if is_transcript_dump_turn_end(&row.text)
            && let Some(prev) = last_dump_block_line.take()
        {
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: abs_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen: false,
                from_transcript_dump: true,
                new_dump_generation: false,
            });
        }
    }
    events
}

/// Synthesize `AgentBlock` start/end events from Claude Code `⏺ ToolName(args)`
/// tool-call headers — the fallback block source for sessions without hook
/// instrumentation (see `has_tuic_state_integration`; the turn-level idle↔busy
/// edge is the primary source and is unconditionally preferred once present).
///
/// `end` always carries the *exclusive* upper bound of the block being
/// closed — the next header's absolute line, or (on agent teardown) one past
/// the last written row — never the closing block's own start line and never
/// `abs_line - 1`. `CommandBlock.endLine` is exclusive throughout the
/// frontend (fold height, block-scoped search), so a block whose `endLine`
/// equals its own `promptLine` silently breaks folding.
///
/// `total_scrolled` (the grid's eviction-stable running total — see
/// `TerminalGrid::screen_origin`) replaced a plain `history_size` here
/// 2026-09-15: `history_size` plateaus once the scrollback ring saturates
/// while `row_index` keeps cycling through the same on-screen rows, so two
/// unrelated headers far apart in real time could land on the identical
/// `abs_line` — not just risk `end < start` for the block being closed right
/// now (the `.max(prev + 1)` clamp below still guards that narrower case),
/// but alias a long-past block's stored row onto a brand new one. See
/// src-tauri/AGENTS.md > Command Blocks > "Scrollback-ring eviction".
///
/// `on_alt_screen` reflects whether the alternate screen buffer was active
/// for this chunk. `total_scrolled`/`row_index` are read from whichever screen
/// (primary or alt) was active at that instant — a fullscreen TUI never grows
/// real alt-screen history, so `abs_line` there is a transient on-screen
/// cursor row, not a valid scrollback anchor. Tagged through unchanged so
/// row-anchored consumers can skip these blocks without this function having
/// to know anything about rendering.
pub(super) fn synthesize_cc_block_events(
    changed_rows: &[crate::state::ChangedRow],
    total_scrolled: usize,
    agent_active: bool,
    teardown_end_line: usize,
    on_alt_screen: bool,
    last_agent_block_line: &mut Option<usize>,
) -> Vec<ParsedEvent> {
    let mut events = Vec::new();
    if !agent_active {
        if let Some(prev) = last_agent_block_line.take() {
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: teardown_end_line.max(prev + 1) as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen,
                from_transcript_dump: false,
                new_dump_generation: false,
            });
        }
        return events;
    }
    for row in changed_rows {
        if !is_cc_tool_call_header(&row.text) {
            continue;
        }
        let abs_line = total_scrolled + row.row_index;
        if Some(abs_line) == *last_agent_block_line {
            continue;
        }
        // The new block's start must never precede the end just emitted for
        // the block it's closing — otherwise the same scrollback-saturation
        // regression that requires clamping `end` (see doc comment above)
        // produces an `end` ahead of an unclamped, regressed `start`,
        // overlapping the two blocks. Reuse the clamped end line as the new
        // start whenever there was a previous block to close.
        let start_line = if let Some(prev) = *last_agent_block_line {
            let end_line = abs_line.max(prev + 1);
            events.push(ParsedEvent::AgentBlock {
                action: "end".into(),
                line: end_line as i64,
                exit_code: None,
                prompt_text: None,
                on_alt_screen,
                from_transcript_dump: false,
                new_dump_generation: false,
            });
            end_line
        } else {
            abs_line
        };
        events.push(ParsedEvent::AgentBlock {
            action: "start".into(),
            line: start_line as i64,
            exit_code: None,
            prompt_text: None,
            on_alt_screen,
            from_transcript_dump: false,
            new_dump_generation: false,
        });
        *last_agent_block_line = Some(start_line);
    }
    events
}

/// Retract a low-confidence `awaiting_input` once the screen is quiet and no
/// question is visible any more.
///
/// The three existing clears all need an event that may never arrive: a typed
/// non-empty line (`UserInput`), a choice-prompt key (`resolve_choice_prompt_input`),
/// or a parsed `status-line`. A user who answers an approval dialog with a bare
/// Enter, or an agent whose spinner never parses as a status line, produces
/// none of them — the badge then reads "question" for the rest of the session
/// with the prompt long gone from the screen.
///
/// Only the heuristic (`confident == false`) state is retracted. A confident
/// question stays sticky on purpose: grok keeps repainting while it waits, so
/// "not on screen right now" is not proof that it was answered. A live
/// `choice_prompt` owns its own resolution and is left alone.
pub(super) fn emit_question_cleared_if_stale(state: &Arc<AppState>, session_id: &str) {
    let turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .and_then(|s| {
            // Research note (2026-09-01): this guard is BY DESIGN never allowed to
            // retract a confident question or one with an open choice_prompt — see the
            // module-level docs on why (a confident source can still repaint while
            // genuinely waiting; screen absence alone isn't proof of an answer). If a
            // session is stuck "awaiting" and this log line below never appears for
            // it, that's the tell: the badge is confident/choice-prompt-owned, so this
            // backstop was never going to be the thing that clears it — look at the
            // hook busy re-affirmation path (`tuic_state_awaiting_event`) or
            // `resolve_choice_prompt_input`/`choice-cleared` instead.
            if s.awaiting_input && (s.question_confident || s.choice_prompt.is_some()) {
                tracing::debug!(
                    session_id = %session_id,
                    confident = s.question_confident,
                    has_choice_prompt = s.choice_prompt.is_some(),
                    "silence_timer: awaiting_input is stale-eligible on screen but the \
                     confident/choice_prompt guard blocks this backstop from clearing it \
                     (research: unexpected state transitions)"
                );
            }
            (s.awaiting_input && !s.question_confident && s.choice_prompt.is_none())
                .then_some(s.turn_epoch)
        });
    let Some(turn_epoch) = turn_epoch else {
        return;
    };
    tracing::debug!(
        session_id = %session_id,
        "silence_timer: retracting stale awaiting_input (no question on screen)"
    );
    let parsed = ParsedEvent::QuestionCleared;
    if let Ok(mut json) = serde_json::to_value(&parsed) {
        if let Some(object) = json.as_object_mut() {
            object.insert("_turn_epoch".to_string(), turn_epoch.into());
        }
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            let _ = app.emit(&format!("pty-parsed-{session_id}"), &json);
        }
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: json.into(),
        });
    }
}

/// Publish the explicit end-of-task marker only after the shell has settled.
/// A completed lifecycle event is emitted from the same drain point, so an
/// orchestrator never has to reinterpret an ambiguous BUSY→IDLE transition.
pub(super) fn emit_pending_suggest_if_idle(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
) -> bool {
    let shell_is_idle = state
        .session_maps
        .shell_states
        .get(session_id)
        .map(|atom| atom.load(std::sync::atomic::Ordering::Acquire) == SHELL_IDLE)
        .unwrap_or(false);
    // Serialize completion emission against note_submitted_input, which takes
    // this same lock before advancing SessionState.turn_epoch and clearing the
    // old turn. Whichever owns the lock first defines the lifecycle order.
    let mut silence_state = silence.lock();
    let Some((current_turn_epoch, background_work, background_probe_pending)) = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| {
            (
                session.turn_epoch,
                session.background_work,
                session.has_pending_background_probe(),
            )
        })
    else {
        return false;
    };
    // A ready agent can finish its turn while descendants keep the shell busy.
    // Publish choices independently; retain the payload for the existing
    // completion gate and background-settlement path.
    let agent_is_ready = silence_state.cached_screen_activity == AgentScreenActivity::Ready
        && silence_state.last_output_at.elapsed() >= AGENT_READY_CONFIRM;
    if !shell_is_idle && !agent_is_ready {
        return false;
    }
    if silence_state.pending_suggest_turn_epoch == current_turn_epoch
        && !silence_state.pending_suggest_emitted
        && let Some(items) = silence_state.pending_suggest_items.clone()
    {
        emit_suggest_event(state, session_id, current_turn_epoch, items);
        silence_state.pending_suggest_emitted = true;
    }
    // `background_probe_pending`: an open question (a ready screen or explicit
    // idle marker was seen, but no process snapshot newer than that observation
    // has confirmed or denied a live descendant yet) counts as work, same as
    // every other reader of these three signals (`state.rs::session_state_with_shell`,
    // `try_shell_transition_locked`'s parent-idle suppression) — publishing
    // "completed" before the probe resolves would race a real background
    // descendant that just hasn't been confirmed yet.
    if !shell_is_idle
        || background_work
        || background_probe_pending
        || silence_state.declared_background_work_for_epoch(current_turn_epoch)
    {
        return false;
    }
    let Some((turn_epoch, items)) = silence_state.drain_pending_suggest_with_epoch() else {
        return false;
    };
    if turn_epoch != current_turn_epoch {
        if silence_state.completion_turn_epoch == turn_epoch {
            silence_state.completion_declared = false;
            silence_state.completion_turn_epoch = 0;
        }
        return false;
    }
    if !silence_state.pending_suggest_emitted {
        emit_suggest_event(state, session_id, turn_epoch, items);
    }
    let parent_dispatch = enqueue_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "state_change",
            "state": "completed",
            "session_id": session_id,
        }),
    );
    drop(silence_state);
    if let Some(dispatch) = parent_dispatch {
        dispatch_parent_lifecycle(state, dispatch);
    }
    reevaluate_orchestrator_mail_wake(state, session_id);
    true
}

pub(super) fn emit_open_intent_if_idle(
    state: &AppState,
    silence: &Arc<Mutex<SilenceState>>,
    session_id: &str,
) {
    if state
        .session_maps
        .shell_states
        .get(session_id)
        .is_none_or(|shell| shell.load(Ordering::Acquire) != SHELL_IDLE)
    {
        return;
    }
    let turn_epoch = state
        .session_maps
        .session_states
        .get(session_id)
        .map(|session| session.turn_epoch)
        .unwrap_or(0);
    let event = {
        let mut silence = silence.lock();
        (silence.last_output_at.elapsed() >= SILENCE_INTENT_THRESHOLD)
            .then(|| silence.close_open_intent())
            .flatten()
    };
    if let Some(event) = event {
        publish_intent_event(state, session_id, &event, turn_epoch);
    }
}

/// Drop every trace of a peer identity nobody can reach any more, and tell
/// subscribers the address is gone.
///
/// Two callers below retire an identity for different reasons — the PTY backing
/// it died, or the last child naming it as parent did — and both must clear the
/// same maps. Keeping one list here is the same discipline
/// [`remove_live_session_state`] enforces for per-session state: a new
/// peer-keyed map belongs in this function and nowhere else.
fn retire_peer_identity(state: &AppState, tuic_session: &str) {
    if state.hibernated_peers.contains(tuic_session) {
        return;
    }
    crate::mcp_http::remote_peer::unregister_peer(state, tuic_session);
    state.orchestrator_peers.remove(tuic_session);
    state.agent_inbox.remove(tuic_session);
    state.agent_inbox_evictions.remove(tuic_session);
    state.agent_read_cursor.remove(tuic_session);
    state.active_agent_waiters.remove(tuic_session);
    state.pending_injections.remove(tuic_session);
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::PeerUnregistered {
            tuic_session: tuic_session.to_string(),
        });
}

/// Per-session state owned by the running process: streams, input, shell status,
/// and the swarm identities the PTY was backing. Reaped the moment the process
/// dies, whether or not a readable tombstone outlives it.
///
/// This and [`remove_post_mortem_session_state`] are the *only* two enumerations
/// of per-session maps. Three call sites compose them — `cleanup_session` runs
/// both, `tombstone_transient_cleanup` runs this one, `spawn_tombstone_sweeper`
/// runs the other. Each used to keep its own hand-written list, and the three had
/// drifted: an explicit close left every peer identity behind, and a session that
/// exited normally leaked its terminal alias for the life of the process.
/// **A new per-session map belongs in one of these two functions and nowhere else.**
fn flush_open_intent_before_session_removal(session_id: &str, state: &AppState) {
    if let Some(silence) = state.session_maps.silence_states.get(session_id) {
        let event = silence.lock().close_open_intent();
        drop(silence);
        if let Some(event) = event {
            let turn_epoch = state
                .session_maps
                .session_states
                .get(session_id)
                .map(|session| session.turn_epoch)
                .unwrap_or(0);
            publish_intent_event(state, session_id, &event, turn_epoch);
        }
    }
}

pub(super) fn remove_live_session_state(session_id: &str, state: &AppState) {
    flush_open_intent_before_session_removal(session_id, state);
    if let Err(error) = crate::stories::StoryStore::release_closed_session(session_id) {
        tracing::warn!(session_id = %session_id, error = %error, "Could not release story claim for closed session");
    }
    state.suspend_metadata.remove(session_id);
    state.session_maps.attachment_launch_dirs.remove(session_id);
    // Reverse-map removal keeps this O(1) instead of scanning
    // `claude_session_map` for a matching value. Only drop the forward entry
    // (and the Session Diff "first edit announced" mark) while this session
    // still owns it: a second tab that resumed the same Claude session owns
    // both now.
    if let Some((_, claude_session_id)) = state.tuic_to_claude_session.remove(session_id)
        && state
            .claude_session_map
            .remove_if(&claude_session_id, |_, owner| owner == session_id)
            .is_some()
    {
        state.announced_edit_sessions.remove(&claude_session_id);
    }
    state.ws_clients.remove(session_id);
    // Drop the per-session PTY event channel alongside ws_clients. Any final
    // SessionClosed already emitted stays buffered for live subscribers (broadcast
    // drains buffered messages before signalling Closed), so no close frame is lost.
    state.session_maps.pty_event_channels.remove(session_id);
    #[cfg(feature = "desktop")]
    state.grid.channels.remove(session_id);
    state.grid.watch.remove(session_id);
    state.grid.gates.remove(session_id);
    state.grid.pending_scroll.remove(session_id);
    state.session_maps.kitty_states.remove(session_id);
    state.session_maps.input_buffers.remove(session_id);
    state.session_maps.input_transactions.remove(session_id);
    state.session_maps.silence_states.remove(session_id);
    state.session_maps.shell_states.remove(session_id);
    state.session_maps.last_prompts.remove(session_id);
    state.session_maps.pty_descriptions.remove(session_id);
    state.session_maps.terminal_rows.remove(session_id);
    state.session_maps.resize_locks.remove(session_id);
    state.session_maps.pty_accent_colors.remove(session_id);
    // Input mode and shell integration describe the process that just died.
    state.session_maps.slash_mode.remove(session_id);
    state.session_maps.last_input_ms.remove(session_id);
    // Watchdog attribution counters — see `state::drain_counter_map`. Live-only:
    // once the session is gone there is nothing left to attribute a rate to.
    state.session_maps.session_event_counts.remove(session_id);
    state.session_maps.session_output_bytes.remove(session_id);
    state.session_maps.session_ws_lag.remove(session_id);
    // These three integration/flag markers must not leak — every session that
    // ever spoke OSC 133 or OSC 7770 left a permanent dead entry keyed by its
    // UUID otherwise, on both this path and the explicit close/kill path
    // (`cleanup_session`, which composes this function).
    state.session_maps.has_osc133_integration.remove(session_id);
    state
        .session_maps
        .has_tuic_state_integration
        .remove(session_id);
    state.session_maps.turn_error_flags.remove(session_id);
    // Swarm maps — inserted at spawn/register time, must be cleaned on exit.
    state.session_maps.shell_state_since_ms.remove(session_id);
    // A peer that announced its own `$TUIC_SESSION` is filed under that identity,
    // not under the PTY key — so the `peer_agents.remove(session_id)` below has
    // never matched it, and its registration outlived the terminal for the whole
    // process lifetime. Retire the identities this PTY was backing as well.
    for orphaned in state.unbind_live_pty(session_id) {
        retire_peer_identity(state, &orphaned);
    }
    state.pending_injections.remove(session_id);
    state.recent_queue_keys.remove(session_id);
    state.pending_initial_prompts.remove(session_id);
    state.managed_trust_dialogs.remove(session_id);
    state.active_agent_waiters.remove(session_id);
    if !state.hibernated_peers.contains(session_id) {
        crate::mcp_http::remote_peer::unregister_peer(state, session_id);
        state.orchestrator_peers.remove(session_id);
        state.agent_inbox.remove(session_id);
        state.agent_inbox_evictions.remove(session_id);
        // The inbox read position is meaningless once the inbox is gone.
        state.agent_read_cursor.remove(session_id);
    }
    #[cfg(unix)]
    state.session_maps.standby_sessions.remove(session_id);
    // DEFERRED (2026-08-25) — a parent identity retained ONLY because this child
    // named it (`peer_identity_is_reapable`) is never re-examined once the child
    // goes: the reaper walks the peers of the MCP session it is collecting, and the
    // parent's was collected long ago. The address then survives for the life of
    // the process as the phantom `retire_repaired_phantom_identity` describes —
    // advertised by `list_peers`, swallowing every message sent to it.
    //
    // Retiring it HERE was tried and is wrong twice over: `mark_session_exited` has
    // just pushed this child's `state_change` into that inbox, and a headerless
    // orchestrator can still reclaim the identity later with `register
    // replaces=<old_uuid>` to collect exactly that mail. Both make "no live PTY, no
    // live MCP session, no child" too weak a test for deletion. The real fix is a
    // periodic sweep over ALL peers with a mail-retention rule, which is a policy
    // decision, not a cleanup tweak.
    state.session_maps.session_parent.remove(session_id);
    state.keep_open_sessions.remove(session_id);
    state.blocked_children.remove(session_id);
    // mcp_to_session maps mcp_session_id → tuic_session. The reverse index
    // session_to_mcp lets us drop O(k) entries (k = mcp sessions for this
    // tuic_session, typically 1) instead of scanning every entry.
    if let Some((_, mcp_sids)) = state.mcp.session_to_mcp.remove(session_id) {
        for sid in &mcp_sids {
            state.mcp.to_session.remove(sid);
        }
    }
}

/// Per-session state a tombstone keeps readable after the process is gone: the
/// buffers, the exit code, and the alias the tab still shows. Reaped when the tombstone ages
/// out — or immediately, when the session is closed outright.
///
/// See [`remove_live_session_state`] for why these are the only two lists.
pub(super) fn remove_post_mortem_session_state(session_id: &str, state: &AppState) -> bool {
    let output_removed = state
        .session_maps
        .output_buffers
        .remove(session_id)
        .is_some();
    let grid_removed = state.grid.vt_log_buffers.remove(session_id).is_some();
    let raw_removed = state.grid.pty_raw_rings.remove(session_id).is_some();
    state.session_maps.last_output_ms.remove(session_id);
    state.session_maps.exit_codes.remove(session_id);
    state.session_maps.term_aliases.remove(session_id);
    state.session_maps.marker_stats.remove(session_id);
    state.session_maps.session_visibility.remove(session_id);
    crate::background_mode::hidden::forget_session(state, session_id);
    state
        .session_maps
        .scrollback_capture_marks
        .remove(session_id);
    output_removed || grid_removed || raw_removed
}

/// Return freed scrollback pages glibc otherwise keeps resident in thread arenas.
/// Called only after payload owners and their map guards have been dropped.
fn trim_unused_session_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: malloc_trim is glibc's thread-safe allocator operation. It
        // releases only unused pages, preserving all live session allocations.
        // A zero return simply means there were no releasable pages.
        let _ = unsafe { libc::malloc_trim(0) };
    }
}

/// Remove the live session and transfer any pending child wait out of its maps.
fn remove_pty_session(session_id: &str, state: &AppState) {
    let Some((_, session)) = state.session_maps.sessions.remove(session_id) else {
        return;
    };
    state
        .metrics
        .active_sessions
        .fetch_sub(1, Ordering::Relaxed);
    release_pty_session(session_id, session.into_inner());
}

/// Release terminal resources without abandoning an unreaped child.
fn release_pty_session(session_id: &str, session: PtySession) {
    // Moving only the child drops the master, writer and session metadata here.
    // EOF can precede process exit: dropping a std::process::Child does not wait
    // for it, and one try_wait(None) must not abandon its eventual zombie.
    let mut child = session._child;
    // Drop terminal handles before spawning the waiter.
    drop(session.master);
    drop(session.writer);
    match child.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(source = "pty", session_id, %error, "Poll removed PTY child failed; waiting for it");
        }
    }
    let session_id = session_id.to_string();
    std::thread::spawn(move || {
        // No AppState, map guard, session lock or PTY handle survives here. A
        // process that closed its terminal but is still alive cannot stall other
        // sessions or keep its terminal buffers resident while we wait for it.
        loop {
            match child.wait() {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    tracing::warn!(source = "pty", session_id, %error, "Wait for removed PTY child failed");
                    break;
                }
            }
        }
    });
}

/// Fully remove session state from all DashMaps.
/// Called on explicit close/kill — caller has already consumed any output they need.
#[cfg(test)]
pub(crate) fn cleanup_session(session_id: &str, state: &AppState, reason: &str) {
    flush_open_intent_before_session_removal(session_id, state);
    // Before `remove_live_session_state`, same ordering requirement as
    // `close_pty_core`/`kill_pty_core`: that call reaps
    // `state.pty_event_channels`, and emitting after it would silently drop
    // the session-scoped WS "closed" frame.
    emit_session_closed(state, session_id, reason);
    remove_pty_session(session_id, state);
    crate::attachments::cleanup_session(&state.data_dir, "pty", session_id);
    remove_live_session_state(session_id, state);
    if remove_post_mortem_session_state(session_id, state) {
        trim_unused_session_heap();
    }
}

/// Reap the state the dead process owned, and stamp `last_output_ms` so the
/// tombstone sweeper can age the entry out. What a post-mortem read needs stays —
/// see [`remove_post_mortem_session_state`].
pub(super) fn tombstone_transient_cleanup(session_id: &str, state: &AppState) {
    crate::attachments::cleanup_session(&state.data_dir, "pty", session_id);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .entry(session_id.to_string())
        .or_insert_with(|| AtomicU64::new(0))
        .store(now_ms, Ordering::Relaxed);
    // Capture scrollback under this session's $TUIC_SESSION identity before
    // remove_live_session_state unbinds it below. This is the only point that
    // catches a tab closed mid-run — every exit path (close, kill, process
    // died on its own) funnels through here.
    let (restore_scrollback, max_scrollback_lines) = {
        let cfg = state.config.read();
        (
            cfg.restore_scrollback,
            cfg.restore_scrollback_lines as usize,
        )
    };
    if restore_scrollback && let Some(tuic_session) = state.tuic_session_for_live_pty(session_id) {
        crate::scrollback_store::capture_session(
            state,
            session_id,
            &tuic_session,
            max_scrollback_lines,
            now_ms,
        );
    }
    remove_live_session_state(session_id, state);
}

/// Keeps `output_buffers`, `vt_log_buffers`, `last_output_ms`, and `exit_codes`
/// alive so MCP consumers can read final output + exit status post-mortem.
/// Tombstones are reaped by `spawn_tombstone_sweeper` after `TOMBSTONE_TTL_MS`.
/// Drive any task tracking `session_id` to its terminal state. This is what makes
/// a task handle worth polling: the outcome is recorded even if no client was
/// waiting when the agent finished.
///
/// A missing exit code is read as success — the session is gone and we have no
/// evidence of failure, so an orchestrator should collect a result rather than
/// see a phantom error.
fn finish_session_tasks(state: &AppState, session_id: &str, exit_code: Option<i32>) {
    let failed = exit_code.is_some_and(|code| code != 0);
    for task_id in state.tasks.live_ids_for_session(session_id) {
        let (status, update) = if failed {
            (
                crate::tasks::TaskStatus::Failed,
                crate::tasks::TaskUpdate {
                    error: Some(format!(
                        "agent session exited with code {}",
                        exit_code.unwrap_or_default()
                    )),
                    ..Default::default()
                },
            )
        } else {
            (
                crate::tasks::TaskStatus::Completed,
                crate::tasks::TaskUpdate {
                    result: Some(serde_json::json!({
                        "session_id": session_id,
                        "exit_code": exit_code,
                    })),
                    ..Default::default()
                },
            )
        };
        if let Err(e) = state.tasks.set_status(&task_id, status, update) {
            // Terminal already (a cancel that raced the exit) is expected, not an
            // error worth a warning — `live_ids_for_session` just read it as live.
            tracing::debug!(source = "tasks", task_id = %task_id, error = %e, "Task not finished on session exit");
        }
    }
}

pub(crate) fn mark_session_exited(session_id: &str, state: &Arc<AppState>) {
    // Capture exit code before dropping the session entry.
    // portable_pty::ExitStatus carries both exit_code() and signal().
    // Signal-killed processes get 128+signum (shell convention) so the
    // caller can distinguish SIGKILL (137) from normal exit(1).
    if let Some(entry) = state.session_maps.sessions.get(session_id)
        && let Ok(Some(status)) = entry.value().lock()._child.try_wait()
    {
        let code = if let Some(sig) = status.signal() {
            let signum = parse_signal_number(sig);
            128 + signum
        } else {
            status.exit_code() as i32
        };
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), code);
    }
    flush_open_intent_before_session_removal(session_id, state);
    remove_pty_session(session_id, state);

    // Notify orchestrator (if any) that this agent has exited.
    let exit_code = state
        .session_maps
        .exit_codes
        .get(session_id)
        .map(|e| *e.value());
    finish_session_tasks(state, session_id, exit_code);
    push_state_change_to_parent(
        state,
        session_id,
        serde_json::json!({
            "type": "state_change",
            "state": "exited",
            "session_id": session_id,
            "exit_code": exit_code,
        }),
    );

    // A managed workflow child may exit without calling workflow_report. Persist
    // the interruption before presenting another run state; exit code zero is
    // not evidence that its story criteria passed.
    if crate::config::config_dir()
        .join("workflow_runs.sqlite3")
        .is_file()
    {
        match crate::workflows::RunStore::open()
            .and_then(|store| store.interrupt_agent_session(session_id))
        {
            Ok(changed_runs) => {
                for run in changed_runs {
                    crate::workflows::emit_run_changed(state, &run.project, &run.id, run.sequence);
                }
            }
            Err(error) => {
                tracing::warn!(source = "workflows", session_id, %error, "record workflow agent exit")
            }
        }
    }

    // SIMP-1: drain HTML tabs registered by this session and emit close.
    // Same helper used by `session(close)` and `session(kill)` so all three
    // exit paths drain `session_html_tabs` identically (no orphan tabs).
    crate::mcp_http::mcp_transport::emit_close_html_tabs(state, session_id);

    tombstone_transient_cleanup(session_id, state);
}

/// Time a tombstoned session's buffers remain readable after process exit.
pub(crate) const TOMBSTONE_TTL_MS: u64 = 5 * 60 * 1000; // 5 minutes

/// Session ids whose tombstone has aged past the TTL.
///
/// Discovery walks `last_output_ms`, not `output_buffers`: an explicit close runs
/// the full cleanup, and the reader thread can afterwards reach EOF and re-stamp
/// the timestamp through the tombstone path. That leaves a lone entry with no
/// buffers — invisible to a buffer-driven walk, and so never reaped at all. The
/// stamp is the one thing every tombstone has.
pub(super) fn aged_out_tombstones(state: &AppState, now_ms: u64) -> Vec<String> {
    // A tombstone is: a stamp present, session entry absent, aged past TTL.
    state
        .session_maps
        .last_output_ms
        .iter()
        .filter_map(|entry| {
            let id = entry.key();
            if state.session_maps.sessions.contains_key(id) {
                return None;
            }
            let last_ms = entry.value().load(Ordering::Relaxed);
            if last_ms == 0 || now_ms.saturating_sub(last_ms) < TOMBSTONE_TTL_MS {
                return None;
            }
            Some(id.clone())
        })
        .collect()
}

/// Reap each candidate's post-mortem state.
///
/// Liveness is re-checked here, not only at selection: the HTTP spawn path accepts
/// a caller-supplied id, so an aged id can be reclaimed by a live session between
/// the two. Reaping it then would delete that session's buffers and alias.
///
/// DEFERRED (2026-08-18) — the re-check narrows the window, it does not close it:
/// a reclaim landing between this load and the removal still loses. Closing it
/// needs a per-id generation stamped at insert and compared at removal, which is
/// a `sessions` API change; not worth it while ids are random UUIDs in practice.
pub(super) fn reap_tombstones(state: &AppState, candidates: &[String]) {
    let mut released_payload = false;
    for id in candidates {
        if state.session_maps.sessions.contains_key(id) {
            continue;
        }
        released_payload |= remove_post_mortem_session_state(id, state);
        tracing::debug!(source = "pty", session_id = %id, "Tombstone reaped");
    }
    if released_payload {
        trim_unused_session_heap();
    }
}

/// Background sweeper that reaps tombstoned session buffers once they age out.
/// Started once at boot from the HTTP server runtime.
pub(crate) fn spawn_tombstone_sweeper(state: Arc<AppState>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            reap_tombstones(&state, &aged_out_tombstones(&state, now_ms));
        }
    });
}

/// Shared lookup for `last_prompts`, kept in one place so the IPC-exposed
/// getter (`get_last_prompt`, `pty/commands.rs`) and the internal consumer
/// (`ChunkProcessor::handle_tuic_state`'s `AgentBlock.prompt_text`) can't drift
/// if the lookup semantics ever change (trimming, a different word-count
/// threshold, etc.).
pub(crate) fn last_prompt_text(state: &AppState, session_id: &str) -> Option<String> {
    state
        .session_maps
        .last_prompts
        .get(session_id)
        .map(|v| v.clone())
}

/// Shared resize core for the Tauri command and the HTTP route (story 056-7545).
///
/// Order matters: the grid must adopt the new dimensions BEFORE the PTY ioctl
/// delivers SIGWINCH to the child. With the old PTY-first order the child could
/// repaint for the new width while the grid still wrapped at the old one (the
/// window grows with vt-lock contention under bursty output); wide lines then
/// autowrapped in the narrow grid, breaking Ink's cursor-up arithmetic and
/// stranding intermediate render rows in scrollback as duplicated blocks.
///
/// Same-dims calls are a no-op (returns None): they would otherwise deliver a
/// gratuitous SIGWINCH (full Ink repaint) per redundant caller (MCP/HTTP/multi
/// -client — the desktop frontend already guards, others don't).
///
/// Returns the post-resize full frame to flush, if the grid was resized.
/// Which thread last ran the reflow for a session.
///
/// Keyed by session, not process-wide: the suite runs tests in parallel, and a
/// single slot would report some other test's thread. Test-only — it is how a
/// test proves the reflow left the caller's thread without reaching into tokio.
#[cfg(test)]
static RESIZE_THREADS: std::sync::LazyLock<dashmap::DashMap<String, std::thread::ThreadId>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

#[cfg(test)]
pub(crate) fn resize_thread(session_id: &str) -> Option<std::thread::ThreadId> {
    RESIZE_THREADS.get(session_id).map(|t| *t)
}

/// [`resize_session_core`] on the blocking pool.
///
/// The reflow is the single most expensive thing a terminal does: it rewraps the
/// whole ring — up to 10,000 rows — and then serializes a full frame, all while
/// holding the VT mutex the PTY reader wants. Run inline in a `#[tauri::command]`
/// that is on macOS the main thread, a drag-resize froze the WebView for the
/// length of every reflow it triggered.
///
/// Shared by both transports, like [`vt_try_read`]: the HTTP route is `async` but
/// its await point is worthless if the body blocks a tokio worker for a whole
/// rewrap. Neither transport can quietly go back to blocking.
pub(crate) async fn resize_session_off_thread(
    state: &Arc<AppState>,
    session_id: String,
    rows: u16,
    cols: u16,
    cell_width_px: Option<u16>,
    cell_height_px: Option<u16>,
) -> Result<Option<crate::grid_gate::GridFrame>, String> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        resize_session_core(
            &state,
            &session_id,
            rows,
            cols,
            cell_width_px,
            cell_height_px,
        )
    })
    .await
    .map_err(|e| format!("resize failed: {e}"))?
}

pub(crate) fn resize_session_core(
    state: &AppState,
    session_id: &str,
    rows: u16,
    cols: u16,
    cell_width_px: Option<u16>,
    cell_height_px: Option<u16>,
) -> Result<Option<crate::grid_gate::GridFrame>, String> {
    if rows == 0 || cols == 0 {
        return Err("Invalid dimensions: rows and cols must be > 0".to_string());
    }
    #[cfg(test)]
    {
        RESIZE_THREADS.insert(session_id.to_string(), std::thread::current().id());
    }
    // Update cell pixel metrics unconditionally, ahead of the no-op dimension
    // guard below — a `resize_pty` call carrying only new device-pixel cell
    // metrics (e.g. a DPR change with no row/col change) still lands them for
    // future CSI 14t/16t replies, even on a call that no-ops for TIOCSWINSZ.
    if let (Some(w), Some(h)) = (cell_width_px, cell_height_px)
        && let Some(vt_log) = state.grid.vt_log_buffers.get(session_id)
    {
        vt_log.lock().set_cell_pixel_size(w, h);
    }
    // Serialize the whole grid+PTY resize for this session under one lock so two
    // concurrent differing resizes (Tauri `resize_pty` + HTTP route) cannot interleave
    // their two critical sections and leave the grid and PTY at mismatched dimensions
    // (CONC-B, story 100-e303). Clone the Arc and drop the DashMap Ref before locking
    // so we never hold a `resize_locks` shard guard across the resize.
    let resize_lock = state
        .session_maps
        .resize_locks
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new((0, 0))))
        .clone();
    let mut applied = resize_lock.lock();
    // Seed the last-applied dims from the live grid the first time we see this session:
    // at creation the grid and PTY share the openpty size, so a first resize that only
    // matches the startup dims no-ops instead of firing a gratuitous SIGWINCH. `(0, 0)`
    // is the never-applied sentinel (real dims are guarded > 0 above).
    if *applied == (0, 0)
        && let Some(vt_log) = state.grid.vt_log_buffers.get(session_id)
    {
        let vt = vt_log.lock();
        *applied = (vt.grid_screen_lines() as u16, vt.grid_columns() as u16);
    }
    // No-op guard compares against the last dims that actually reached the PTY, not just
    // the grid: a prior call that resized the grid but then failed `master.resize` leaves
    // them divergent, and a grid-only guard would skip the PTY forever (CONC-B criterion 2).
    if *applied == (rows, cols) {
        return Ok(None);
    }
    // Resize the grid and capture a fresh full frame, holding the vt lock so no
    // PTY chunk can land between the check and the resize. `resize`
    // marks the grid fully damaged, so `serialize_dirty_rows` yields the whole
    // viewport. The caller must flush it: the reader thread only sends frames on
    // PTY data or the ticker, so a resize/zoom over idle or static content would
    // otherwise leave the viewport blank until a scroll forces
    // `terminal_request_frame`. If the grid already matches (PTY-only retry after a
    // prior `master.resize` failure) skip the grid work but still re-apply the PTY.
    let resize_frame = match state.grid.vt_log_buffers.get(session_id) {
        Some(vt_log) => {
            let mut vt = vt_log.lock();
            if vt.grid_screen_lines() == rows as usize && vt.grid_columns() == cols as usize {
                None
            } else {
                vt.resize(rows, cols);
                Some(vt.serialize_dirty_rows())
            }
        }
        None => None,
    };
    // Update terminal rows for cursor-up clamping in the reader thread.
    if let Some(r) = state.session_maps.terminal_rows.get(session_id) {
        r.store(rows, Ordering::Relaxed);
    }
    // Mark resize in silence state so the reader thread suppresses re-parsed events
    // from the shell's prompt redraw triggered by SIGWINCH.
    if let Some(ss) = state.session_maps.silence_states.get(session_id) {
        ss.lock().on_resize();
    }
    // Only now signal the child (TIOCSWINSZ → SIGWINCH): everything it repaints
    // from here on meets a grid that already wraps at the new width.
    let entry = state
        .session_maps
        .sessions
        .get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let (cell_w, cell_h) = match state.grid.vt_log_buffers.get(session_id) {
        Some(vt_log) => vt_log.lock().cell_pixel_size(),
        None => (
            crate::terminal_grid::DEFAULT_CELL_WIDTH_PX,
            crate::terminal_grid::DEFAULT_CELL_HEIGHT_PX,
        ),
    };
    entry
        .lock()
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: cols.saturating_mul(cell_w),
            pixel_height: rows.saturating_mul(cell_h),
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;
    // Record the dims only now that they've reached the PTY, still under `applied`, so
    // a racing resize either waits behind this lock or observes a consistent value.
    // On a `master.resize` failure above we return via `?` WITHOUT updating `applied`,
    // so a later retry re-applies the PTY instead of no-opping on a grid-only match.
    *applied = (rows, cols);
    Ok(resize_frame)
}

/// Periodically checks all sessions for standby eligibility.
/// A session enters standby when:
/// 1. standby_timeout_minutes > 0
/// 2. session_visibility == false (tab not focused)
/// 3. shell_state == SHELL_IDLE
/// 4. idle duration >= timeout
/// 5. not already in standby
/// 6. startup_settled == true
#[cfg(unix)]
pub(super) fn background_activity_blocks_standby(state: &AppState, session_id: &str) -> bool {
    background_activity_blocks_standby_with_silence(state, session_id, None)
}

/// `locked_silence`: pass the caller's already-held `SilenceState` guard when
/// one is held (e.g. `standby_session`'s `_lifecycle_guard`) — `None` re-locks
/// internally via `AppState::declared_background_work_for`. Never lock
/// `state.session_maps.silence_states` again here unconditionally: `SilenceState`'s mutex
/// is not reentrant, and `standby_session` calls this while already holding it.
#[cfg(unix)]
pub(super) fn background_activity_blocks_standby_with_silence(
    state: &AppState,
    session_id: &str,
    locked_silence: Option<&SilenceState>,
) -> bool {
    let Some((turn_epoch, base_activity)) =
        state
            .session_maps
            .session_states
            .get(session_id)
            .map(|session| {
                (
                    session.turn_epoch,
                    session.background_work || session.has_pending_background_probe(),
                )
            })
    else {
        return false;
    };
    let declared_background_work = match locked_silence {
        Some(silence) => silence.declared_background_work_for_epoch(turn_epoch),
        None => state.declared_background_work_for(session_id, turn_epoch),
    };
    base_activity || declared_background_work
}

#[cfg(unix)]
pub(crate) fn spawn_standby_checker(state: Arc<AppState>) {
    use std::time::Duration;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let timeout_min = state.config.read().standby_timeout_minutes;
            if timeout_min == 0 {
                // Standby disabled: wake any sessions still parked (SIGSTOP'd)
                // from a previous non-zero timeout. Otherwise their stopped
                // badge persists until the user manually focuses each tab
                // (to-test.md:236, story 095).
                wake_all_standby(&state);
                continue;
            }
            let timeout_ms = u64::from(timeout_min) * 60_000;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let vis_count = state.session_maps.session_visibility.len();
            let sessions_count = state.session_maps.sessions.len();
            tracing::trace!(
                vis_count,
                sessions_count,
                timeout_min,
                "Standby checker tick"
            );

            let visibility: Vec<(String, bool)> = state
                .session_maps
                .session_visibility
                .iter()
                .map(|entry| (entry.key().clone(), *entry.value()))
                .collect();
            for (session_id, visible) in &visibility {
                if *visible {
                    continue;
                }
                if state
                    .session_maps
                    .standby_sessions
                    .contains_key(session_id.as_str())
                {
                    continue;
                }

                let shell_raw = state
                    .session_maps
                    .shell_states
                    .get(session_id.as_str())
                    .map(|a| a.load(Ordering::Acquire));
                let is_idle = shell_raw == Some(SHELL_IDLE);
                if !is_idle {
                    continue;
                }

                // For agents with a verified ready-screen adapter, a silence-only
                // idle is not strong enough to SIGSTOP the process group. Require
                // explicit Stop/OSC or a stable ready screen. Legacy agents that
                // lack an adapter retain their prior timeout behavior.
                let is_agent = state
                    .session_maps
                    .session_states
                    .get(session_id.as_str())
                    .map(|s| s.agent_type.is_some())
                    .unwrap_or(false);
                if is_agent && !idle_is_confirmed(&state, session_id.as_str()) {
                    tracing::trace!(
                        session_id = session_id.as_str(),
                        "Standby skipped: agent idle is heuristic-only"
                    );
                    continue;
                }
                if background_activity_blocks_standby(&state, session_id.as_str()) {
                    tracing::trace!(
                        session_id = session_id.as_str(),
                        "Standby skipped: background work or probe pending"
                    );
                    continue;
                }

                let idle_since = state
                    .session_maps
                    .shell_state_since_ms
                    .get(session_id.as_str())
                    .map(|a| a.load(Ordering::Acquire))
                    .unwrap_or(now_ms);
                let idle_ms = now_ms.saturating_sub(idle_since);
                if idle_ms < timeout_ms {
                    continue;
                }

                let settled = state
                    .session_maps
                    .silence_states
                    .get(session_id.as_str())
                    .map(|e| e.lock().startup_settled)
                    .unwrap_or(false);
                if !settled {
                    continue;
                }

                tracing::debug!(
                    session_id = session_id.as_str(),
                    idle_ms,
                    "Standby: all conditions met, stopping"
                );
                if let Err(e) = standby_session(&state, session_id) {
                    tracing::warn!(session_id, error = %e, "Standby failed");
                }
            }
        }
    });
}

/// SIGSTOP a session only when its PTY child owns the foreground process group.
/// Returns Ok(true) if stopped, Ok(false) if already in standby or session gone.
#[cfg(unix)]
pub(crate) fn standby_session(state: &AppState, session_id: &str) -> Result<bool, String> {
    // Serialize SIGSTOP with Show and remote attach: current visibility must
    // invalidate a checker tick selected before the session became visible.
    let visibility = state.background_visibility.lock();
    if visibility.watched(state, session_id)
        || visibility.recently_active(state, session_id)
        || state
            .session_maps
            .session_visibility
            .get(session_id)
            .is_some_and(|visible| *visible)
    {
        return Ok(false);
    }
    if state.session_maps.standby_sessions.contains_key(session_id) {
        return Ok(false);
    }
    let timeout_ms = u64::from(state.config.read().standby_timeout_minutes) * 60_000;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if state
        .session_maps
        .last_input_ms
        .get(session_id)
        .is_some_and(|last| now_ms.saturating_sub(last.load(Ordering::Relaxed)) < timeout_ms)
    {
        return Ok(false);
    }
    // Serialize the final eligibility check with background-work updates. This
    // closes the gap between the periodic check above and the actual SIGSTOP.
    let silence = state
        .session_maps
        .silence_states
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(SilenceState::new())))
        .clone();
    let _lifecycle_guard = silence.lock();
    if background_activity_blocks_standby_with_silence(state, session_id, Some(&_lifecycle_guard)) {
        return Ok(false);
    }
    let pgid = {
        let entry = state
            .session_maps
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {session_id}"))?;
        let session = entry.value().lock();
        let pgid = session
            .master
            .process_group_leader()
            .ok_or_else(|| "No process group leader".to_string())?;
        // A job-control shell reclaims the terminal when its foreground job
        // stops. Only that shell can foreground the job again; SIGCONT alone
        // cannot restore its terminal ownership. Leave shell-managed jobs live.
        if session._child.process_id().map(|pid| pid as i32) != Some(pgid) {
            return Ok(false);
        }
        pgid
    };
    if pgid <= 1 || pgid == unsafe { libc::getpgid(0) } {
        return Err(format!("Unsafe pgid {pgid} — refusing SIGSTOP"));
    }
    let ret = unsafe { libc::kill(-pgid, libc::SIGSTOP) };
    if ret != 0 {
        return Err(format!(
            "SIGSTOP failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    state
        .session_maps
        .standby_sessions
        .insert(session_id.to_string(), pgid);
    tracing::info!(session_id, pgid, "Session entered standby (SIGSTOP)");
    emit_standby_event(state, session_id, true);
    Ok(true)
}

/// SIGCONT a session in standby. Returns Ok(true) if woken, Ok(false) if not in standby.
#[cfg(unix)]
pub(crate) fn wake_session(state: &AppState, session_id: &str) -> Result<bool, String> {
    let Some((_, pgid)) = state.session_maps.standby_sessions.remove(session_id) else {
        return Ok(false);
    };
    if !state.session_maps.sessions.contains_key(session_id) {
        return Err(format!("Session not found: {session_id}"));
    }
    if pgid <= 1 || pgid == unsafe { libc::getpgid(0) } {
        return Err(format!("Unsafe pgid {pgid} — refusing SIGCONT"));
    }
    let ret = unsafe { libc::kill(-pgid, libc::SIGCONT) };
    if ret != 0 {
        let error = std::io::Error::last_os_error();
        state
            .session_maps
            .standby_sessions
            .insert(session_id.to_string(), pgid);
        return Err(format!("SIGCONT failed: {}", error));
    }
    tracing::info!(session_id, pgid, "Session woken from standby (SIGCONT)");
    emit_standby_event(state, session_id, false);
    Ok(true)
}

/// Wake every session currently in standby. Used when the user disables standby
/// (timeout=0) so already-parked sessions resume instead of staying SIGSTOP'd.
/// Returns the number of sessions for which a wake was attempted.
///
/// Keys are collected into a Vec first: `wake_session` calls
/// `standby_sessions.remove`, and mutating a DashMap while holding an `iter()`
/// shard guard on the same map deadlocks. A session killed between the snapshot
/// and the wake is handled by `wake_session` (removes its entry, then returns
/// Err on the missing session — no panic).
#[cfg(unix)]
pub(crate) fn wake_all_standby(state: &AppState) -> usize {
    let parked: Vec<String> = state
        .session_maps
        .standby_sessions
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for session_id in &parked {
        if let Err(e) = wake_session(state, session_id) {
            tracing::warn!(session_id, error = %e, "Standby wake-all (timeout=0) failed");
        }
    }
    parked.len()
}

/// Was desktop-only with no bus arm — a browser/PWA client never saw the
/// standby badge at all (D.3).
#[cfg(unix)]
fn emit_standby_event(state: &AppState, session_id: &str, standby: bool) {
    state.emit_dual(crate::state::AppEvent::SessionStandby {
        session_id: session_id.to_string(),
        standby,
    });
}

/// SIGKILL the foreground process group of a PTY session.
///
/// An agent (e.g. claude) runs as a *grandchild* inside the PTY's shell and, under
/// job control, sits in its own foreground process group. SIGKILL on the shell
/// alone leaves that group orphaned — the cloned reader fd keeps the pty master
/// open, so the kernel never delivers SIGHUP to the foreground group, and the
/// agent is reparented to init and keeps running. killpg nukes the agent plus
/// every descendant in one shot. The shell (the session leader, in its own
/// process group) is reaped separately by the caller's `_child.kill()`.
#[cfg(unix)]
fn kill_foreground_process_group(session: &PtySession, session_id: &str) {
    signal_foreground_process_group(session, session_id, libc::SIGKILL);
}

#[cfg(unix)]
fn signal_foreground_process_group(session: &PtySession, session_id: &str, signal: i32) {
    let Some(pgid) = session.master.process_group_leader() else {
        return;
    };
    // Never signal pid <= 1 or our own group — that would take down TUIC itself.
    if pgid <= 1 || pgid == unsafe { libc::getpgid(0) } {
        tracing::warn!(session_id, pgid, "Refusing killpg on unsafe pgid");
        return;
    }
    if unsafe { libc::kill(-pgid, signal) } != 0 {
        let err = std::io::Error::last_os_error();
        // ESRCH just means the group already exited — not worth a warning.
        if err.raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!(session_id, pgid, signal, "killpg failed: {err}");
        }
    }
}

/// Wait up to 100ms for `session`'s child to exit gracefully (the caller has
/// already delivered an interrupt, or is relying on whatever the process does with the
/// grace window on its own), then force-kill it — nuking the foreground
/// process group first — if it's still alive. Without the fallback, agents
/// that ignore Ctrl-C (e.g. claude) become orphans: the cloned reader fd
/// keeps the pty master alive, the slave never sees EOF, and the reader
/// thread spins forever. Pulled out of `close_pty_core` as a named step.
/// (wip also routed `cleanup_session` through it; on this tree that function
/// is a test-only wrapper with no production caller — explicit close goes
/// through `close_pty_core` — so it keeps `remove_pty_session`.)
fn terminate_child_with_grace(session: &mut PtySession, session_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => return, // Process exited cleanly
            Ok(None) if std::time::Instant::now() >= deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    tracing::info!(
        source = "session",
        session_id = %session_id,
        reason = "close_timeout",
        "Closing session: sending SIGKILL after Ctrl-C grace period"
    );
    // Nuke the agent's foreground process group first; SIGKILL on the shell
    // alone leaves the agent (a grandchild) orphaned. See
    // kill_foreground_process_group.
    #[cfg(unix)]
    kill_foreground_process_group(session, session_id);

    if let Err(e) = session._child.kill() {
        tracing::warn!(session_id = %session_id, "terminate_child_with_grace SIGKILL fallback failed: {e}");
    }
    // Brief wait so try_wait can observe the termination and record the code.
    let kill_deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= kill_deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
}

/// Close a PTY session core: signals an interrupt, waits briefly for graceful exit,
/// captures the exit code for the tombstone, and leaves `output_buffers` +
/// `vt_log_buffers` + `last_output_ms` + `exit_codes` alive so post-mortem
/// MCP reads can still return final output and exit status.
///
/// Shared between the Tauri `close_pty` command and the MCP `close` action —
/// both paths must tombstone identically, or post-mortem reads break.
/// Returns the worktree path when `cleanup_worktree` is true and the session
/// had one, so the caller can run `remove_worktree_internal` outside this fn.
pub(crate) fn close_pty_core(
    state: &AppState,
    session_id: &str,
    cleanup_worktree: bool,
) -> Option<crate::state::WorktreeInfo> {
    close_pty_core_with_reason(state, session_id, cleanup_worktree, "close_requested")
}

/// `close_pty_core` with the cause that the close log line reports.
pub(crate) fn close_pty_core_with_reason(
    state: &AppState,
    session_id: &str,
    cleanup_worktree: bool,
    reason: &str,
) -> Option<crate::state::WorktreeInfo> {
    flush_open_intent_before_session_removal(session_id, state);
    let (_, session_mutex) = state.session_maps.sessions.remove(session_id)?;
    state
        .metrics
        .active_sessions
        .fetch_sub(1, Ordering::Relaxed);
    let mut session = session_mutex.into_inner();

    tracing::info!(
        source = "session",
        session_id = %session_id,
        reason,
        "Closing session: sending Ctrl-C"
    );

    // Teardown must never enter the writer path: either the mutex or a kernel
    // write can be stuck behind a child that no longer drains its terminal.
    // Deliver the interrupt out of band, then retain the existing kill deadline.
    #[cfg(unix)]
    signal_foreground_process_group(&session, session_id, libc::SIGINT);
    // ConPTY has no portable out-of-band Ctrl-C. Terminate the owned child
    // rather than risk an unbounded console write before the kill fallback.
    #[cfg(windows)]
    if let Err(error) = session._child.kill() {
        tracing::warn!(session_id, %error, "close_pty_core termination failed");
    }

    terminate_child_with_grace(&mut session, session_id);

    // Capture exit code for the tombstone before dropping the child handle.
    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    // Announce the close BEFORE tombstone_transient_cleanup: it removes
    // `state.pty_event_channels` (via `remove_live_session_state`), so
    // emitting after it means `emit_pty_event` finds no per-session channel
    // and the session-scoped WS `"closed"` frame is silently never
    // delivered — see `mcp_http::session::close_session`'s doc comment on
    // this exact hazard. `reason` above is the log cause; the wire reason of
    // a requested close is always `closed`.
    emit_session_closed(state, session_id, "closed");

    // Preserve output_buffers, vt_log_buffers, last_output_ms, exit_codes.
    // Tombstone sweeper reaps them after TOMBSTONE_TTL_MS.
    tombstone_transient_cleanup(session_id, state);

    let worktree_to_cleanup = if cleanup_worktree {
        session.worktree.clone()
    } else {
        None
    };

    release_pty_session(session_id, session);

    worktree_to_cleanup
}

/// Force-kill a PTY session and tombstone it. Used by the MCP `kill` action.
/// Unlike `close_pty_core`, skips the Ctrl-C grace period — sends SIGKILL
/// immediately. The child exits near-instantly so `try_wait` captures the
/// exit code before the tombstone is stamped.
pub(crate) fn kill_pty_core(state: &AppState, session_id: &str) -> bool {
    flush_open_intent_before_session_removal(session_id, state);
    let Some((_, session_mutex)) = state.session_maps.sessions.remove(session_id) else {
        return false;
    };
    state
        .metrics
        .active_sessions
        .fetch_sub(1, Ordering::Relaxed);
    let mut session = session_mutex.into_inner();

    tracing::info!(
        source = "session",
        session_id = %session_id,
        reason = "kill_requested",
        "Killing session: sending SIGKILL"
    );

    // Nuke the agent's foreground process group first; SIGKILL on the shell
    // alone leaves the agent (a grandchild) orphaned. See
    // kill_foreground_process_group.
    #[cfg(unix)]
    kill_foreground_process_group(&session, session_id);

    if let Err(e) = session._child.kill() {
        tracing::warn!(session_id = %session_id, "SIGKILL failed: {e}");
    }

    // Give the kernel a brief window to reap the child so try_wait sees it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    loop {
        match session._child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => break,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }

    if let Ok(Some(status)) = session._child.try_wait() {
        state
            .session_maps
            .exit_codes
            .insert(session_id.to_string(), status.exit_code() as i32);
    }

    // See `close_pty_core`'s identical comment: must precede
    // `tombstone_transient_cleanup`, which reaps the per-session WS channel
    // this emit needs to deliver the session-scoped "closed" frame.
    emit_session_closed(state, session_id, "killed");

    tombstone_transient_cleanup(session_id, state);
    release_pty_session(session_id, session);
    true
}
