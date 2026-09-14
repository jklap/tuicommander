//! OSC 133 shell integration scripts for command block detection.
//!
//! Injects shell hooks (precmd/preexec for zsh, PROMPT_COMMAND/DEBUG trap for bash)
//! that emit FinalTerm/iTerm2-compatible OSC 133 markers:
//!   A = prompt start, C = pre-execution, D = command finished (with exit code)
//!
//! Injection strategy per shell:
//!   zsh  — ZDOTDIR trick: point ZDOTDIR at a wrapper dir whose .zshenv sources
//!          the integration script then delegates to the real dotfiles.
//!   bash — (future) BASH_ENV or --init-file
//!   fish — (future) XDG_CONFIG_HOME/fish/conf.d/ auto-source

use std::path::Path;

/// Zsh shell integration script.
const ZSH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
__tuic_precmd() {
  local ec=$?
  if [[ -n "$__tuic_cmd" ]]; then
    printf '\e]133;D;%d\a' "$ec"
    unset __tuic_cmd
  fi
  printf '\e]133;A\a'
}
__tuic_preexec() {
  printf '\e]133;C\a'
  __tuic_cmd=1
}
[[ " ${precmd_functions[*]} " == *" __tuic_precmd "* ]] || precmd_functions+=(__tuic_precmd)
[[ " ${preexec_functions[*]} " == *" __tuic_preexec "* ]] || preexec_functions+=(__tuic_preexec)
# OSC 7770 TUIC protocol helpers
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  claude() {
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) command claude "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then command claude "$@" --settings "$TUIC_CLAUDE_SETTINGS"; else command claude "$@"; fi
  }
  codex() {
    local a prev; for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then command codex "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then command codex "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else command codex "$@"; fi
  }
  goose() {
    local a; for a in "$@"; do
      case "$a" in --name|-n|--resume|-r) command goose "$@"; return;; esac
    done
    case "$1" in
      session|run) command goose "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) command goose "$@";;
    esac
  }
fi
"#;

/// Bash shell integration script.
const BASH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
__tuic_precmd() {
  local ec=$?
  if [[ -n "$__tuic_cmd" ]]; then
    printf '\e]133;D;%d\a' "$ec"
    unset __tuic_cmd
  fi
  printf '\e]133;A\a'
  __tuic_preexec_ready=1
}
__tuic_preexec_trap() {
  [[ -n "$__tuic_preexec_ready" ]] || return
  unset __tuic_preexec_ready
  printf '\e]133;C\a'
  __tuic_cmd=1
}
if [[ -z "$__tuic_installed" ]]; then
  __tuic_installed=1
  PROMPT_COMMAND="__tuic_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  trap '__tuic_preexec_trap' DEBUG
fi
# OSC 7770 TUIC protocol helpers
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  claude() {
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) command claude "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then command claude "$@" --settings "$TUIC_CLAUDE_SETTINGS"; else command claude "$@"; fi
  }
  codex() {
    local a prev; for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then command codex "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then command codex "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else command codex "$@"; fi
  }
  goose() {
    local a; for a in "$@"; do
      case "$a" in --name|-n|--resume|-r) command goose "$@"; return;; esac
    done
    case "$1" in
      session|run) command goose "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) command goose "$@";;
    esac
  }
fi
"#;

/// Fish shell integration script.
const FISH_INTEGRATION: &str = r#"# TUIC Shell Integration — OSC 133 command block markers + OSC 7770 helpers
function __tuic_prompt --on-event fish_prompt
  set -l ec $status
  if set -q __tuic_cmd
    printf '\e]133;D;%d\a' $ec
    set -e __tuic_cmd
  end
  printf '\e]133;A\a'
end
function __tuic_preexec --on-event fish_preexec
  printf '\e]133;C\a'
  set -g __tuic_cmd 1
end
# OSC 7770 TUIC protocol helpers
function tuic_state;   printf '\e]7770;state=%s\a' $argv[1]; end
function tuic_suggest; printf '\e]7770;suggest=%s\a' (string join " " $argv); end
function tuic_intent;  printf '\e]7770;intent=%s\a' (string join " " $argv); end
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if set -q TUIC_SESSION
  function claude --wraps claude
    for a in $argv
      switch $a
        case --settings '--settings=*' --bare
          command claude $argv; return
      end
    end
    if set -q TUIC_CLAUDE_SETTINGS
      command claude $argv --settings $TUIC_CLAUDE_SETTINGS
    else
      command claude $argv
    end
  end
  function codex --wraps codex
    set -l prev
    for a in $argv
      if test "$prev" = -c; and string match -q 'notify=*' -- $a
        command codex $argv; return
      end
      if string match -q -- '-cnotify=*' $a; or string match -q -- '--config=notify=*' $a
        command codex $argv; return
      end
      set prev $a
    end
    if set -q TUIC_CODEX_NOTIFY
      command codex $argv -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"
    else
      command codex $argv
    end
  end
  function goose --wraps goose
    for a in $argv
      switch $a
        case --name -n --resume -r
          command goose $argv
          return
      end
    end
    switch $argv[1]
      case session run
        command goose $argv[1] --name $TUIC_SESSION $argv[2..]
      case '*'
        command goose $argv
    end
  end
end
"#;

/// Template for the ZDOTDIR `.zshenv` wrapper.  At runtime `{script}` is
/// replaced with the absolute path to `tuic-integration.zsh`.
const ZDOTDIR_ZSHENV: &str = r#"# TUIC ZDOTDIR wrapper — sources integration then restores real dotfiles
source "{script}"
ZDOTDIR="${TUIC_ORIGINAL_ZDOTDIR:-$HOME}"
[[ -f "$ZDOTDIR/.zshenv" ]] && source "$ZDOTDIR/.zshenv"
"#;

/// Zsh dotfile names that ZDOTDIR affects.  We create passthrough wrappers
/// for each so the user's config loads normally from the original ZDOTDIR.
const ZSH_DOTFILES: &[&str] = &[".zprofile", ".zshrc", ".zlogin", ".zlogout"];

/// Write shell integration files to `app_data_dir/shell-integration/` and
/// apply the appropriate injection env vars to `cmd`.
///
/// For zsh this sets up the ZDOTDIR trick.  For other shells it sets an env
/// var pointing to the integration script (manual sourcing for now).
pub(crate) fn inject(app_data_dir: &Path, shell: &str, cmd: &mut portable_pty::CommandBuilder) {
    let base = app_data_dir.join("shell-integration");
    if std::fs::create_dir_all(&base).is_err() {
        return;
    }
    if crate::agent_hook_launch::enabled("claude") {
        cmd.env(
            "TUIC_CLAUDE_SETTINGS",
            app_data_dir.join("agent-hooks/claude.json"),
        );
    }
    if crate::agent_hook_launch::enabled("codex") {
        cmd.env(
            "TUIC_CODEX_NOTIFY",
            app_data_dir.join("agent-hooks/codex-notify.sh"),
        );
    }

    if crate::pty::is_wsl_shell(shell) {
        // WSL default shell is bash. Inject bash integration with
        // translated paths so /mnt/c/... references work inside WSL.
        inject_bash_wsl(&base, cmd);
    } else if shell.contains("zsh") {
        inject_zsh(&base, cmd);
    } else if shell.contains("bash") {
        inject_bash(&base, cmd);
    } else if shell.contains("fish") {
        inject_fish(&base, cmd);
    }
}

fn write_if_changed(path: &Path, content: &str) -> bool {
    let needs_write = std::fs::read_to_string(path)
        .map(|existing| existing != content)
        .unwrap_or(true);
    if needs_write {
        std::fs::write(path, content).is_ok()
    } else {
        true
    }
}

fn inject_zsh(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Write the integration script
    let script_path = base.join("tuic-integration.zsh");
    if !write_if_changed(&script_path, ZSH_INTEGRATION) {
        return;
    }

    // Create ZDOTDIR wrapper directory
    let zdotdir = base.join("zdotdir");
    if std::fs::create_dir_all(&zdotdir).is_err() {
        return;
    }

    // .zshenv — sources integration, then restores real ZDOTDIR and sources real .zshenv
    let zshenv_content = ZDOTDIR_ZSHENV.replace("{script}", &script_path.to_string_lossy());
    if !write_if_changed(&zdotdir.join(".zshenv"), &zshenv_content) {
        return;
    }

    // Passthrough wrappers for other dotfiles (so user config still loads)
    for dotfile in ZSH_DOTFILES {
        let mut wrapper = format!(
            "# TUIC passthrough — load real {dotfile}\n\
             [[ -f \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\" ]] && \
             source \"${{TUIC_ORIGINAL_ZDOTDIR:-$HOME}}/{dotfile}\"\n"
        );
        if dotfile == &".zshrc" {
            wrapper.push_str(
                "# Ensure completion is active (ZDOTDIR trick can skip system compinit)\n\
                 if [[ -o interactive ]] && ! type compdef >/dev/null 2>&1; then\n\
                 \x20 autoload -Uz compinit && compinit -C\n\
                 fi\n",
            );
        }
        write_if_changed(&zdotdir.join(dotfile), &wrapper);
    }

    // Preserve original ZDOTDIR (may be unset, defaults to $HOME)
    if let Ok(original) = std::env::var("ZDOTDIR") {
        cmd.env("TUIC_ORIGINAL_ZDOTDIR", original);
    }
    cmd.env("ZDOTDIR", zdotdir_path_str(&zdotdir));
}

fn inject_bash(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, BASH_INTEGRATION) {
        // BASH_ENV is sourced for non-interactive bash; for interactive login
        // shells we rely on the user sourcing it or a future --init-file approach.
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

fn inject_fish(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Fish auto-sources scripts in conf.d/ directories under XDG_CONFIG_HOME.
    // For now, just point to the script via env var.
    let script_path = base.join("tuic-integration.fish");
    if write_if_changed(&script_path, FISH_INTEGRATION) {
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

/// Inject bash integration for WSL shells. The script files live on the
/// Windows filesystem but env vars reference them via `/mnt/` paths so
/// they're accessible inside the WSL Linux environment.
fn inject_bash_wsl(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, BASH_INTEGRATION) {
        let wsl_path = crate::pty::windows_to_wsl_path(&script_path_str(&script_path));
        cmd.env("TUIC_SHELL_INTEGRATION", wsl_path);
    }
}

fn zdotdir_path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn script_path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_wrapper_paths(shell: &str, script: &str) {
        let claude_inject = script
            .matches("--settings \"$TUIC_CLAUDE_SETTINGS\"")
            .count()
            + script.matches("--settings $TUIC_CLAUDE_SETTINGS").count();
        let codex_inject = script
            .matches("notify=[\\\"$TUIC_CODEX_NOTIFY\\\"]")
            .count();
        assert_eq!(claude_inject, 1, "{shell}: inject Claude settings once");
        assert_eq!(codex_inject, 1, "{shell}: inject Codex notify once");

        assert!(
            script.contains("--settings=*"),
            "{shell}: skip explicit Claude --settings=value"
        );
        assert!(
            script.contains("--settings"),
            "{shell}: skip explicit Claude --settings value"
        );
        assert!(
            script.contains("--bare"),
            "{shell}: skip Claude injection in bare mode"
        );
        assert!(
            script.contains("notify=*"),
            "{shell}: skip explicit Codex notify override"
        );

        assert!(
            script.contains("else\n      command claude") || script.contains("else command claude"),
            "{shell}: Claude setting-off passthrough"
        );
        assert!(
            script.contains("else\n      command codex") || script.contains("else command codex"),
            "{shell}: Codex setting-off passthrough"
        );
    }

    #[test]
    fn bash_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("bash", BASH_INTEGRATION);
    }

    #[test]
    fn zsh_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("zsh", ZSH_INTEGRATION);
    }

    #[test]
    fn fish_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths("fish", FISH_INTEGRATION);
    }

    /// Launch the wrappers in a real shell and read back the command line they
    /// build, for each of inject / skip-when-user-passed / setting-off.
    ///
    /// `assert_wrapper_paths` greps the script text, which is why it cannot
    /// stand in for this: change one wrapper's argument loop from `"$@"` to
    /// `"$1"` and every literal it looks for survives, yet
    /// `claude --model opus --settings /user/settings.json` comes out carrying
    /// a second `--settings` (measured by hand, 2026-09-13). Only a launch
    /// sees that. The two layers are complements — the grep runs on Windows
    /// too, where this module does not exist.
    #[cfg(unix)]
    mod launch {
        // Named, not a glob: these constants live two modules up, and a glob of
        // the parent's own glob is easy to break by accident.
        use super::super::{BASH_INTEGRATION, FISH_INTEGRATION, ZSH_INTEGRATION};
        use std::path::{Path, PathBuf};
        use std::process::Command;

        /// Stand-in for the agent binary: prints the argument list the wrapper
        /// built, and nothing else.
        ///
        /// It is installed as a **symlink to `/bin/echo`**, never as a freshly
        /// written script. On macOS the first exec of a new executable blocks
        /// on an exec-time code scan — AGENTS.md, "A freshly written executable
        /// is not a cheap thing to run" — and this matrix execs one per
        /// assertion. A symlink resolves to an inode the OS has already vetted:
        /// measured at ~3ms per exec here against seconds for a new file. Do
        /// not turn it back into a script to make it print more.
        const REAL_ECHO: &str = "/bin/echo";

        /// The launch matrix. Every one of these must be installed wherever
        /// these tests run — there is no presence probe and nothing is skipped.
        ///
        /// An earlier version gated zsh and fish on the interpreter being
        /// found. fish is on neither the macOS base install nor the CI images,
        /// so its half of the matrix had never executed anywhere while still
        /// reporting as passed: nothing in the output said fish was not tried.
        /// A skipped test that reports as passed is worse than a missing one.
        /// `scripts/install-launch-shells.sh` installs and verifies all three
        /// and is the record of what the matrix requires; the CI workflow runs
        /// it, and it is the one command to run on a new machine.
        const LAUNCH_SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

        /// A `PATH` prefix holding `claude` and `codex` stand-ins.
        fn agent_bin_dir() -> PathBuf {
            assert!(
                Path::new(REAL_ECHO).exists(),
                "{REAL_ECHO} is missing; the wrapper launch harness needs it"
            );
            // Under `target/`, so it is gitignored and survives between runs.
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-bin");
            std::fs::create_dir_all(&dir).expect("create fake agent bin dir");

            for agent in ["claude", "codex"] {
                let link = dir.join(agent);
                if std::fs::read_link(&link).is_ok_and(|target| target == Path::new(REAL_ECHO)) {
                    continue;
                }
                // Stage under a process-unique name and rename over the target,
                // so tests running in parallel never observe a missing link.
                let staging = dir.join(format!("{agent}.{}", std::process::id()));
                let _ = std::fs::remove_file(&staging);
                std::os::unix::fs::symlink(REAL_ECHO, &staging).expect("stage agent symlink");
                std::fs::rename(&staging, &link).expect("install agent symlink");
            }
            dir
        }

        /// Write the shell's integration constant where the shell can source it.
        ///
        /// Rewritten on every call on purpose: the file is sourced, never
        /// executed, so there is no scan to amortise, and a stale copy of the
        /// constant would make every assertion below a lie.
        fn integration_script(shell: &str) -> PathBuf {
            let (name, body) = match shell {
                "bash" => ("tuic-integration.bash", BASH_INTEGRATION),
                "zsh" => ("tuic-integration.zsh", ZSH_INTEGRATION),
                "fish" => ("tuic-integration.fish", FISH_INTEGRATION),
                other => panic!("no integration script for {other}"),
            };
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shell-integration-tests");
            std::fs::create_dir_all(&dir).expect("create integration script dir");
            let path = dir.join(name);
            let staging = dir.join(format!("{name}.{}", std::process::id()));
            std::fs::write(&staging, body).expect("write integration script");
            std::fs::rename(&staging, &path).expect("install integration script");
            path
        }

        /// Flags that stop `shell` from reading the machine's dotfiles.
        ///
        /// Without them the assertions below would depend on whoever runs them:
        /// zsh reads `~/.zshenv` even for `-c`, and fish reads `config.fish`, so
        /// a dotfile that greets on stderr, or that defines its own `claude`
        /// alias, would decide the result. The subject here is the integration
        /// script and nothing else.
        fn rc_free_flags(shell: &str) -> &'static [&'static str] {
            match shell {
                "bash" => &["--noprofile", "--norc"],
                "zsh" => &["-f"],
                "fish" => &["--no-config"],
                other => panic!("no rc-free flags for {other}"),
            }
        }

        /// Fail, naming `shell` and how to install it, unless it can be
        /// launched rc-free on this machine.
        ///
        /// The probe is the rc-free launch itself, not `which`, because the
        /// trap this guards against is a shell that IS installed and still
        /// cannot be used: `rc_free_flags` passes `--no-config` to fish, which
        /// only exists in fish >= 3.3. Under the old presence gate an older
        /// fish failed that probe, dropped out of the matrix, and produced a
        /// run log byte-identical to a green one.
        fn require_shell(shell: &str) {
            assert!(
                LAUNCH_SHELLS.contains(&shell),
                "{shell} is not in the launch matrix"
            );
            let probe = Command::new(shell)
                .args(rc_free_flags(shell))
                .arg("-c")
                .arg("exit 0")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            assert!(
                probe.is_ok_and(|status| status.success()),
                "{shell} cannot be launched with {:?}, so the launch matrix is \
                 incomplete. Install it (macOS: `brew install {shell}`; \
                 Debian/Ubuntu: `sudo apt-get install -y {shell}`), or run \
                 `scripts/install-launch-shells.sh`. fish must be >= 3.3, which \
                 is when `--no-config` was added.",
                rc_free_flags(shell)
            );
        }

        /// Source the integration script in `shell`, run `invocation`, and
        /// return one entry per command line the wrappers handed to an agent.
        ///
        /// Every invocation must parse in bash, zsh **and** fish — that is what
        /// lets one case description drive the whole matrix.
        fn wrapper_command_lines(
            shell: &str,
            tuic_env: &[(&str, &str)],
            invocation: &str,
        ) -> Vec<String> {
            let script = integration_script(shell);
            let bin_dir = agent_bin_dir();
            let path = match std::env::var_os("PATH") {
                Some(existing) => format!("{}:{}", bin_dir.display(), existing.to_string_lossy()),
                None => bin_dir.display().to_string(),
            };

            let mut cmd = Command::new(shell);
            cmd.args(rc_free_flags(shell))
                .arg("-c")
                .arg(format!("source '{}'\n{invocation}", script.display()))
                .env("PATH", path)
                // The wrappers are defined only inside a TUIC session.
                .env("TUIC_SESSION", "wrapper-launch-test")
                // Start from setting-off, so a case that wants injection has to
                // ask for it and the off case cannot pass on an inherited value.
                .env_remove("TUIC_CLAUDE_SETTINGS")
                .env_remove("TUIC_CODEX_NOTIFY");
            for (key, value) in tuic_env {
                cmd.env(key, value);
            }

            let out = cmd
                .output()
                .unwrap_or_else(|err| panic!("launch {shell}: {err}"));
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                out.status.success(),
                "{shell} exited {:?}: {stderr}",
                out.status.code()
            );
            // A syntax error in the integration script can still leave the exit
            // status at zero; the empty stderr is what rules that out.
            assert!(stderr.is_empty(), "{shell} wrote to stderr: {stderr}");

            String::from_utf8(out.stdout)
                .expect("shell output is utf-8")
                .lines()
                .map(str::to_owned)
                .collect()
        }

        /// Run one case in one shell and compare against `expected`.
        fn assert_in_shell(
            shell: &str,
            case: &str,
            tuic_env: &[(&str, &str)],
            invocation: &str,
            expected: &[&str],
        ) {
            require_shell(shell);
            let actual = wrapper_command_lines(shell, tuic_env, invocation);
            assert_eq!(actual, expected, "{shell}: {case}");
        }

        /// Emit one `#[test]` per case per shell, so the shell that ran is in
        /// the test name and the run log names every shell that was launched.
        ///
        /// One test per case looping over the matrix would hide the shell: a
        /// log line reading `setting_on_appends_launch_scoped_status_flags`
        /// says nothing about whether fish was among the shells it tried,
        /// which is exactly how the fish half stayed unexecuted.
        macro_rules! launch_matrix {
            ($($case:ident),+ $(,)?) => {
                $(mod $case {
                    #[test]
                    fn in_bash() { super::$case("bash") }
                    #[test]
                    fn in_zsh() { super::$case("zsh") }
                    #[test]
                    fn in_fish() { super::$case("fish") }
                })+
            };
        }

        launch_matrix!(
            setting_on_appends_launch_scoped_status_flags,
            an_explicit_user_flag_suppresses_injection,
            setting_off_leaves_the_command_line_untouched,
        );

        const CLAUDE_SETTINGS: &str = "/tuic/agent-hooks/claude.json";
        const CODEX_NOTIFY: &str = "/tuic/agent-hooks/codex-notify.sh";

        fn signals_on() -> [(&'static str, &'static str); 2] {
            [
                ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                ("TUIC_CODEX_NOTIFY", CODEX_NOTIFY),
            ]
        }

        fn setting_on_appends_launch_scoped_status_flags(shell: &str) {
            let claude = format!("--model opus --settings {CLAUDE_SETTINGS}");
            assert_in_shell(
                shell,
                "Claude gets the TUIC settings file appended",
                &signals_on(),
                "claude --model opus",
                &[claude.as_str()],
            );
            let codex = format!("exec --full-auto -c notify=[\"{CODEX_NOTIFY}\"]");
            assert_in_shell(
                shell,
                "Codex gets the TUIC notify script appended",
                &signals_on(),
                "codex exec --full-auto",
                &[codex.as_str()],
            );
        }

        fn an_explicit_user_flag_suppresses_injection(shell: &str) {
            assert_in_shell(
                shell,
                "Claude leaves the user's own --settings alone",
                &signals_on(),
                "claude --settings /user/settings.json\n\
                 claude --settings=/user/settings.json\n\
                 claude --bare\n\
                 claude --model opus --settings /user/settings.json",
                &[
                    "--settings /user/settings.json",
                    "--settings=/user/settings.json",
                    "--bare",
                    // The flag is not the first argument. This is the case a
                    // grep over the script text cannot see.
                    "--model opus --settings /user/settings.json",
                ],
            );
            assert_in_shell(
                shell,
                "Codex leaves the user's own notify override alone",
                &signals_on(),
                "codex -c 'notify=[\"/user/notify.sh\"]' exec\n\
                 codex -cnotify='[\"/user/notify.sh\"]' exec\n\
                 codex '--config=notify=[\"/user/notify.sh\"]' exec",
                &[
                    "-c notify=[\"/user/notify.sh\"] exec",
                    "-cnotify=[\"/user/notify.sh\"] exec",
                    "--config=notify=[\"/user/notify.sh\"] exec",
                ],
            );
        }

        fn setting_off_leaves_the_command_line_untouched(shell: &str) {
            assert_in_shell(
                shell,
                "Claude is launched exactly as typed",
                &[],
                "claude --model opus --dangerously-skip-permissions",
                &["--model opus --dangerously-skip-permissions"],
            );
            assert_in_shell(
                shell,
                "Codex is launched exactly as typed",
                &[],
                "codex exec --full-auto",
                &["exec --full-auto"],
            );
        }
    }
}
