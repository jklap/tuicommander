//! OSC 133 shell integration scripts for command block detection.
//!
//! Injects shell hooks (precmd/preexec for zsh, PROMPT_COMMAND/DEBUG trap for bash)
//! that emit FinalTerm/iTerm2-compatible OSC 133 markers:
//!   A = prompt start, B = command start, C = pre-execution, D = command
//!   finished (with exit code)
//!
//! `B` fires immediately after `A`, inside the same precmd hook — not
//! embedded in the prompt string itself the way some shell-integration
//! implementations place it (right where the visible prompt decoration ends
//! and the input area begins). That would require rewriting the user's
//! PROMPT/PS1, which risks breaking their existing prompt theme
//! (powerlevel10k, starship, …) — a worse regression than the approximation
//! here. Since nothing is printed between A and B, they land on the same
//! buffer line; for a typical single-line command that's also the line C
//! and (usually) D land on too, so `CommandBlock.commandLine` /
//! `.executionLine` end up equal — which `getBufferLines`'s inclusive-
//! inclusive range handles correctly (reads exactly that one line). A
//! genuinely multi-line typed command (rare) would have `commandLine` point
//! at the prompt line rather than where typing began; accepted.
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
  printf '\e]133;B\a'
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
  printf '\e]133;B\a'
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
  printf '\e]133;B\a'
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

    // This file had zero tests before this: the A/C/D emissions had never been
    // asserted, in either shell-syntax-validity or byte-content terms, which is
    // exactly why B could be added here with real confidence that A/C/D still
    // work — the baseline below is asserted first, on the same script text.

    #[test]
    fn zsh_emits_all_four_markers() {
        assert!(ZSH_INTEGRATION.contains(r"printf '\e]133;A\a'"));
        assert!(ZSH_INTEGRATION.contains(r"printf '\e]133;B\a'"));
        assert!(ZSH_INTEGRATION.contains(r"printf '\e]133;C\a'"));
        assert!(ZSH_INTEGRATION.contains(r#"printf '\e]133;D;%d\a' "$ec""#));
        // B must come after A within precmd, not before — it marks the start of
        // input, which can't precede the prompt that introduces it.
        let a_pos = ZSH_INTEGRATION.find(r"printf '\e]133;A\a'").unwrap();
        let b_pos = ZSH_INTEGRATION.find(r"printf '\e]133;B\a'").unwrap();
        assert!(b_pos > a_pos, "B must be emitted after A");
    }

    #[test]
    fn bash_emits_all_four_markers() {
        assert!(BASH_INTEGRATION.contains(r"printf '\e]133;A\a'"));
        assert!(BASH_INTEGRATION.contains(r"printf '\e]133;B\a'"));
        assert!(BASH_INTEGRATION.contains(r"printf '\e]133;C\a'"));
        assert!(BASH_INTEGRATION.contains(r#"printf '\e]133;D;%d\a' "$ec""#));
        let a_pos = BASH_INTEGRATION.find(r"printf '\e]133;A\a'").unwrap();
        let b_pos = BASH_INTEGRATION.find(r"printf '\e]133;B\a'").unwrap();
        assert!(b_pos > a_pos, "B must be emitted after A");
    }

    #[test]
    fn fish_emits_all_four_markers() {
        assert!(FISH_INTEGRATION.contains(r"printf '\e]133;A\a'"));
        assert!(FISH_INTEGRATION.contains(r"printf '\e]133;B\a'"));
        assert!(FISH_INTEGRATION.contains(r"printf '\e]133;C\a'"));
        assert!(FISH_INTEGRATION.contains(r"printf '\e]133;D;%d\a' $ec"));
        let a_pos = FISH_INTEGRATION.find(r"printf '\e]133;A\a'").unwrap();
        let b_pos = FISH_INTEGRATION.find(r"printf '\e]133;B\a'").unwrap();
        assert!(b_pos > a_pos, "B must be emitted after A");
    }

    #[test]
    fn every_shell_script_is_syntactically_valid() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        for (shell, script) in [
            ("bash", BASH_INTEGRATION),
            ("zsh", ZSH_INTEGRATION),
            // fish uses `fish --no-execute` (its own syntax-check flag, not -n);
            // skipped below if the binary isn't installed (not on CI's Ubuntu
            // runners) rather than silently passing or failing the whole test.
        ] {
            let Ok(mut child) = Command::new(shell)
                .arg("-n")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            else {
                continue; // shell not installed on this machine — not a syntax failure
            };
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{shell} -n rejected the integration script: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        if let Ok(mut child) = Command::new("fish")
            .arg("--no-execute")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(FISH_INTEGRATION.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "fish --no-execute rejected the integration script: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    /// Real behavioral confidence for one shell (bash — present on the CI
    /// runners this crate actually tests on), rather than string content
    /// alone: sources the script, then drives a real precmd → preexec →
    /// precmd cycle exactly as an interactive session would, and asserts the
    /// real printf byte sequence, including the exit code plumbed through D.
    #[test]
    fn bash_precmd_preexec_cycle_emits_the_real_byte_sequence() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let script = format!(
            "{BASH_INTEGRATION}\n\
             __tuic_precmd\n\
             __tuic_preexec_trap\n\
             ( exit 7 )\n\
             __tuic_precmd\n"
        );
        let mut child = Command::new("bash")
            .arg("--noprofile")
            .arg("--norc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn bash");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("bash exits");
        assert!(out.status.success());
        let got = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            got,
            "\u{1b}]133;A\u{07}\u{1b}]133;B\u{07}\u{1b}]133;C\u{07}\u{1b}]133;D;7\u{07}\u{1b}]133;A\u{07}\u{1b}]133;B\u{07}",
            "first precmd: A,B (no prior command); preexec: C; second precmd: D;7 (real exit code), then A,B for the next prompt"
        );
    }
}
