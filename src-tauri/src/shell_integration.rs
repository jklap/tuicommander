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
//!
//! **Zsh's integration is split into an eager half (`ZSH_INTEGRATION`) and a
//! deferred half (`ZSH_DEFERRED_INTEGRATION`), loaded at two different points
//! in shell startup — this split is load-bearing, not stylistic.**
//!
//! The eager half (precmd/preexec + the OSC 133 markers, plus the general-
//! purpose `tuic_state`/`tuic_suggest`/`tuic_intent` OSC 7770 helpers a user's
//! own scripts can call directly) sources from `.zshenv`, before the user's
//! own `.zprofile`/`.zshrc` run, same as always — the very first prompt still
//! needs its `A`/`B` markers, and nothing in a user's own startup scripts
//! plausibly probes for `__tuic_precmd`/`__tuic_preexec`/`tuic_state`/
//! `tuic_suggest`/`tuic_intent` by name, so there's no shadowing risk to defer
//! any of them for.
//!
//! The deferred half (`claude`/`codex`/`goose` auto-injection only) used to
//! load from `.zshenv` too, eagerly, alongside the eager half above — and
//! that was a real bug (found 2026-09-24). Defining `claude` as a shell
//! function before the user's own `.zshrc`/`.zshrc.d/*` have run breaks any
//! `command -v claude`/`type claude` "is this a real binary" guard those
//! scripts rely on: `command -v` on a function returns the bare function
//! name, not a filesystem path, so `[[ -x $(command -v claude) ]]` silently
//! goes false and an entire guarded block of exports (a user's own
//! `CLAUDE_CODE_*` env var configuration, in the reported case) never runs —
//! with no error, no missing-file, nothing to grep for. (An earlier version
//! of this fix also deferred `tuic_state`/`tuic_suggest`/`tuic_intent`
//! alongside claude/codex/goose, purely by bundling them into the same
//! script — a code-review pass caught that this broke the opposite case: a
//! user calling one of them from their own `.zshrc`/`.zshrc.d` now hit
//! "command not found," a real regression from the previous eager-loading
//! behavior. They have no shadowing conflict of their own, so they moved
//! back to the eager half above.)
//!
//! Fixed by deferring the second half: `.zshenv` registers a one-shot
//! `precmd_functions` bootstrap that sources `ZSH_DEFERRED_INTEGRATION` from
//! *inside* the very first precmd call, i.e. strictly after `.zprofile`/
//! `.zshrc` (and everything they source) have fully run — right before the
//! first prompt is drawn, never sooner. Two things this is NOT:
//!
//! - It does NOT keep `$ZDOTDIR` pointed at the fake wrapper directory through
//!   `.zprofile`/`.zshrc` to get there — that was the first design considered
//!   and rejected: `${ZDOTDIR:-$HOME}` is a load-bearing convention for many
//!   real zsh setups (oh-my-zsh's `compinit` dump-file path among them), and
//!   redirecting it for the whole `.zshrc` execution window would silently
//!   relocate a user's completion cache into TUIC's app-data directory for as
//!   long as any TUIC tab is open. `.zshenv` still resets `ZDOTDIR` back to
//!   the real value immediately, exactly as before; only the *function
//!   definitions* move, via the precmd bootstrap, not the directory itself.
//! - It does NOT register `claude`/`codex`/`goose` by appending a *second*
//!   `precmd_functions` entry and hoping it also fires within the same first
//!   cycle as the bootstrap — verified empirically (real PTY, not `-c`, since
//!   `-c` never invokes `precmd_functions` at all) that a newly *appended*
//!   entry does NOT run until the *next* prompt cycle, which would silently
//!   lose the correctness of the very first prompt for anything registered
//!   that way. The deferred half is instead sourced as an ordinary statement
//!   directly inside the bootstrap's own body — plain function definitions,
//!   not a queued hook — which does take effect within the same cycle,
//!   confirmed the same way.
//!
//! Each deferred wrapper checks `(( $+functions[name] ))` — if the user's own
//! `.zshrc` (already run by this point) defined its own `claude`/`codex`/
//! `goose`, TUIC branches three ways on `AgentSettings::wrap_user_function`
//! (`config.rs`), threaded in as `TUIC_WRAP_USER_FN_CLAUDE`/`_CODEX`/`_GOOSE`
//! (`wrap` / `skip` / `ask`) by `inject_zsh` below:
//!
//! - `skip` (or the env var unset — the default for a decision that hasn't
//!   been made yet the very first time, and for any OTHER shell/script that
//!   sources this by hand): leave the user's function alone entirely. This
//!   matches what already happened by accident under the old eager-loading
//!   order (TUIC's wrapper loaded first and got silently overwritten by a
//!   later user redefinition) — same end state, reached on purpose instead
//!   of by accident.
//! - `wrap`: capture the user's function (`functions[__tuic_user_<agent>]=
//!   $functions[<agent>]`) and redefine `<agent>()` to call it through the
//!   same arg-scan-and-inject helper the no-user-function path already uses
//!   — so the user's own customization AND TUIC's launch flag both apply.
//! - `ask`: emit a `userwrap=<agent>` OSC 7770 verb (handled in `pty.rs`,
//!   which calls `agent_wrap_prompt::request`) so the user can be asked,
//!   once, whether to opt in — see that module's doc comment for the full
//!   consent flow and why a dismiss there must NOT collapse to the same
//!   outcome as an explicit "leave it alone".

use std::path::Path;

/// Zsh shell integration script — the eager half, sourced immediately from
/// `.zshenv`. See this module's doc comment for why `claude`/`codex`/`goose`
/// live in `ZSH_DEFERRED_INTEGRATION` instead, not here.
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
# OSC 7770 TUIC protocol helpers — general-purpose, for a user's own scripts
# to call. No shadowing risk (unlike claude/codex/goose below), so these stay
# eager: a user calling them from their own .zshrc/.zshrc.d must not hit
# "command not found" just because that script ran before the deferred half
# loads. See this module's doc comment for why claude/codex/goose can't join
# them here.
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
"#;

/// Zsh shell integration script — the deferred half, sourced lazily from a
/// one-shot precmd bootstrap (see `ZDOTDIR_ZSHENV`) so it never shadows
/// `claude`/`codex`/`goose` while the user's own rc files are still deciding
/// whether those names resolve to a real binary. See this module's doc
/// comment for the full rationale.
const ZSH_DEFERRED_INTEGRATION: &str = r#"# TUIC Shell Integration — agent auto-injection wrappers
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  __tuic_inject_claude() {
    local callee=$1; shift; local a
    for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) "$callee" "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then "$callee" "$@" --settings "$TUIC_CLAUDE_SETTINGS"; else "$callee" "$@"; fi
  }
  if (( $+functions[claude] )); then
    case "$TUIC_WRAP_USER_FN_CLAUDE" in
      wrap)
        # Guard against copying an unloaded autoload stub — copying the stub
        # itself (rather than the real function body) would make zsh look
        # for a file literally named __tuic_user_claude.
        [[ $functions[claude] == *'builtin autoload -X'* ]] && autoload +X claude
        functions[__tuic_user_claude]=$functions[claude]
        claude() { __tuic_inject_claude __tuic_user_claude "$@"; }
        ;;
      ask)
        [[ -n "$TUIC_CLAUDE_SETTINGS" ]] && printf '\e]7770;userwrap=claude\a'
        ;;
      *) ;;  # skip (or unset): leave the user's function alone, exactly as before
    esac
  else
    __tuic_real_claude() { command claude "$@"; }
    claude() { __tuic_inject_claude __tuic_real_claude "$@"; }
  fi

  __tuic_inject_codex() {
    local callee=$1; shift; local a prev
    for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then "$callee" "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then "$callee" "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else "$callee" "$@"; fi
  }
  if (( $+functions[codex] )); then
    case "$TUIC_WRAP_USER_FN_CODEX" in
      wrap)
        [[ $functions[codex] == *'builtin autoload -X'* ]] && autoload +X codex
        functions[__tuic_user_codex]=$functions[codex]
        codex() { __tuic_inject_codex __tuic_user_codex "$@"; }
        ;;
      ask)
        [[ -n "$TUIC_CODEX_NOTIFY" ]] && printf '\e]7770;userwrap=codex\a'
        ;;
      *) ;;
    esac
  else
    __tuic_real_codex() { command codex "$@"; }
    codex() { __tuic_inject_codex __tuic_real_codex "$@"; }
  fi

  __tuic_inject_goose() {
    local callee=$1; shift; local a
    for a in "$@"; do
      case "$a" in --name|-n|--resume|-r) "$callee" "$@"; return;; esac
    done
    case "$1" in
      session|run) "$callee" "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) "$callee" "$@";;
    esac
  }
  if (( $+functions[goose] )); then
    case "$TUIC_WRAP_USER_FN_GOOSE" in
      wrap)
        [[ $functions[goose] == *'builtin autoload -X'* ]] && autoload +X goose
        functions[__tuic_user_goose]=$functions[goose]
        goose() { __tuic_inject_goose __tuic_user_goose "$@"; }
        ;;
      ask)
        # Goose's --name injection isn't gated behind a separate hook-settings
        # flag the way claude/codex's is, so this always fires (unlike the
        # two above, which only ask when their own settings env var is set).
        printf '\e]7770;userwrap=goose\a'
        ;;
      *) ;;
    esac
  else
    __tuic_real_goose() { command goose "$@"; }
    goose() { __tuic_inject_goose __tuic_real_goose "$@"; }
  fi
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

/// Template for the ZDOTDIR `.zshenv` wrapper. At runtime `{eager_script}` and
/// `{deferred_script}` are replaced with the absolute paths to
/// `tuic-integration.zsh` and `tuic-integration-deferred.zsh`. See this
/// module's doc comment for why the deferred half is not sourced directly
/// here, but registered as a one-shot precmd bootstrap instead.
const ZDOTDIR_ZSHENV: &str = r#"# TUIC ZDOTDIR wrapper — sources the eager integration now, defers the rest,
# then restores real dotfiles.
source "{eager_script}"
__tuic_bootstrap() {
  precmd_functions=(${precmd_functions:#__tuic_bootstrap})
  source "{deferred_script}"
}
precmd_functions+=(__tuic_bootstrap)
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
    // Write the eager integration script (OSC 133 precmd/preexec markers)
    let script_path = base.join("tuic-integration.zsh");
    if !write_if_changed(&script_path, ZSH_INTEGRATION) {
        return;
    }

    // Write the deferred integration script (claude/codex/goose wrappers) —
    // sourced lazily; see this module's doc comment and ZDOTDIR_ZSHENV.
    let deferred_script_path = base.join("tuic-integration-deferred.zsh");
    if !write_if_changed(&deferred_script_path, ZSH_DEFERRED_INTEGRATION) {
        return;
    }

    // Create ZDOTDIR wrapper directory
    let zdotdir = base.join("zdotdir");
    if std::fs::create_dir_all(&zdotdir).is_err() {
        return;
    }

    // .zshenv — sources the eager integration, registers a precmd bootstrap
    // for the deferred integration, then restores real ZDOTDIR and sources
    // real .zshenv.
    let zshenv_content = ZDOTDIR_ZSHENV
        .replace("{eager_script}", &script_path.to_string_lossy())
        .replace("{deferred_script}", &deferred_script_path.to_string_lossy());
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

    // zsh only (see AgentSettings::wrap_user_function's doc comment) — thread
    // the persisted decision into the shell so ZSH_DEFERRED_INTEGRATION's
    // three-way branch can act on it without a round trip. An unset var (any
    // other shell, or this script sourced by hand) reads as "skip" in the
    // shell — today's actual behavior, so that default is safe, not new.
    cmd.env(
        "TUIC_WRAP_USER_FN_CLAUDE",
        wrap_user_function_env_value("claude"),
    );
    cmd.env(
        "TUIC_WRAP_USER_FN_CODEX",
        wrap_user_function_env_value("codex"),
    );
    cmd.env(
        "TUIC_WRAP_USER_FN_GOOSE",
        wrap_user_function_env_value("goose"),
    );
}

/// `wrap` / `skip` / `ask`, matching `ZSH_DEFERRED_INTEGRATION`'s `case`
/// arms — see `AgentSettings::wrap_user_function`'s doc comment for what
/// each persisted value means.
fn wrap_user_function_env_value(agent_type: &str) -> &'static str {
    match crate::agent_hook_launch::wrap_user_function(agent_type) {
        Some(true) => "wrap",
        Some(false) => "skip",
        None => "ask",
    }
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

    /// `off_passthrough` is the exact substring proving each agent's
    /// "no settings configured" path calls the real command with no flag
    /// appended — this differs for zsh, where the refactor that added
    /// wrap/skip/ask branching moved the plain-passthrough call behind a
    /// `$callee` variable instead of a literal `command claude`/`command
    /// codex` (bash/fish are untouched by that refactor and keep the
    /// original literal).
    fn assert_wrapper_paths(shell: &str, script: &str, off_passthrough: (&str, &str)) {
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
            script.contains(off_passthrough.0),
            "{shell}: Claude setting-off passthrough"
        );
        assert!(
            script.contains(off_passthrough.1),
            "{shell}: Codex setting-off passthrough"
        );
    }

    #[test]
    fn bash_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths(
            "bash",
            BASH_INTEGRATION,
            ("else command claude", "else command codex"),
        );
    }

    #[test]
    fn zsh_wrappers_cover_inject_user_override_skip_and_setting_off() {
        // The claude/codex wrappers live in the deferred half now — see the
        // module doc comment for why they moved out of ZSH_INTEGRATION.
        assert_wrapper_paths(
            "zsh",
            ZSH_DEFERRED_INTEGRATION,
            ("else \"$callee\" \"$@\"", "else \"$callee\" \"$@\""),
        );
    }

    #[test]
    fn fish_wrappers_cover_inject_user_override_skip_and_setting_off() {
        assert_wrapper_paths(
            "fish",
            FISH_INTEGRATION,
            ("else\n      command claude", "else\n      command codex"),
        );
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
        use super::super::{BASH_INTEGRATION, FISH_INTEGRATION, ZSH_DEFERRED_INTEGRATION};
        use std::path::{Path, PathBuf};
        use std::process::Command;

        /// Stand-in for the agent binary: prints the argument list the wrapper
        /// built, and nothing else.
        ///
        /// It is installed as a **symlink to `/bin/echo`**, never as a freshly
        /// written script. On macOS the first exec of a new executable blocks
        /// on an exec-time code scan — src-tauri/AGENTS.md, "A freshly written executable
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
                // The claude/codex/goose wrappers under test here live in the
                // deferred half in production — sourced directly (not through
                // the precmd bootstrap) since this harness only cares about
                // wrapper correctness, not the deferred-loading mechanism
                // itself (covered separately by `zsh_deferred_load_tests`).
                "zsh" => ("tuic-integration-deferred.zsh", ZSH_DEFERRED_INTEGRATION),
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
            ("zsh", ZSH_DEFERRED_INTEGRATION),
            // fish uses `fish --no-execute` (its own syntax-check flag, not -n);
            // handled in its own block below since it isn't a `-n`-style shell.
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

        // Installed via `apt-get install fish` on the ubuntu-22.04 "rust" CI job
        // (ci.yml) specifically so this actually runs there, not just on a
        // developer machine that happens to have fish. Still gracefully skipped
        // (not a failure) anywhere else fish isn't present — the string-content
        // assertions above (`fish_emits_all_four_markers`) still cover that case.
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

    /// Real-PTY regression tests for the eager/deferred split documented at
    /// the top of this module.
    ///
    /// `launch` above deliberately avoids real interactivity (`-c`, rc-free)
    /// to keep those tests cheap and focused on wrapper *correctness* — but a
    /// bare `-c` invocation never enters zsh's interactive read-eval loop, so
    /// it can't exercise `precmd_functions` at all: verified empirically that
    /// an entry appended to `precmd_functions` under `zsh -i -c '...'` (no
    /// real tty, but interactivity forced) never fires. Testing the deferred
    /// bootstrap itself needs a real PTY and a real `.zshrc`, which is what
    /// this module adds, calling the actual `inject_zsh` under test rather
    /// than a hand-copied template.
    #[cfg(unix)]
    mod zsh_deferred_load {
        use super::super::inject;
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::{Read, Write};
        use std::path::{Path, PathBuf};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        /// A real symlink to `/bin/echo`, never a freshly written script — see
        /// `launch::REAL_ECHO`'s doc comment for why (exec-time code-scan cost
        /// on a fresh inode). Kept as a separate constant/dir rather than
        /// reusing `launch::agent_bin_dir()` so this module has no dependency
        /// on `launch`'s internals.
        const REAL_ECHO: &str = "/bin/echo";

        fn fake_claude_bin_dir() -> PathBuf {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-claude-bin");
            std::fs::create_dir_all(&dir).expect("create fake claude bin dir");
            let link = dir.join("claude");
            if !std::fs::read_link(&link).is_ok_and(|target| target == Path::new(REAL_ECHO)) {
                let staging = dir.join(format!("claude.{}", std::process::id()));
                let _ = std::fs::remove_file(&staging);
                std::os::unix::fs::symlink(REAL_ECHO, &staging).expect("stage claude symlink");
                std::fs::rename(&staging, &link).expect("install claude symlink");
            }
            dir
        }

        /// Serializes temporary mutation of this *test process's own*
        /// ambient `$ZDOTDIR` — mirrors the `SSH_AUTH_SOCK_GUARD` pattern in
        /// `tunnels/agent.rs` for the identical hazard shape (a process-wide
        /// env var, unsafe to mutate concurrently, that a test needs a known
        /// value for).
        static ZDOTDIR_TEST_GUARD: Mutex<()> = Mutex::new(());

        /// Run `f` with this test process's own `$ZDOTDIR` cleared, restoring
        /// whatever it was afterward.
        ///
        /// `inject_zsh`'s "preserve the original ZDOTDIR" step
        /// (`if let Ok(original) = std::env::var("ZDOTDIR")`) reads the
        /// *calling* process's real environment directly — `cmd.env_clear()`
        /// on the `CommandBuilder` has no effect on that read, since it only
        /// governs what gets handed to the spawned child, not what this
        /// process's own `std::env::var` sees. This developer's environment
        /// happens to export `$ZDOTDIR` (from an unrelated tool's shell
        /// snapshot) — without this guard, `inject()` faithfully (and
        /// correctly, for real production use) passed that real value
        /// through as `TUIC_ORIGINAL_ZDOTDIR`, which made the wrapper's
        /// `.zshenv` reset `$ZDOTDIR` back to *this developer's real home*
        /// instead of the test's fake one, so the shell loaded this
        /// developer's real `~/.zshrc` (and none of the test's fake
        /// `.zshrc.d` PATH-forcing) instead of the test's. Confirmed nothing
        /// else in this crate reads `$ZDOTDIR` (grep), so this mutation can't
        /// race any other test — only these two, serialized against each
        /// other by this dedicated lock.
        fn without_ambient_zdotdir<T>(f: impl FnOnce() -> T) -> T {
            let _guard = ZDOTDIR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
            let original = std::env::var_os("ZDOTDIR");
            // SAFETY: serialized by `ZDOTDIR_TEST_GUARD` above; nothing else
            // in this crate reads `$ZDOTDIR`.
            unsafe { std::env::remove_var("ZDOTDIR") };
            let result = f();
            if let Some(value) = original {
                // SAFETY: same guard, same justification.
                unsafe { std::env::set_var("ZDOTDIR", value) };
            }
            result
        }

        /// Spawn `/bin/zsh -l` in a real PTY, with `inject()` — the actual
        /// public entry point `pty.rs`'s real PTY-spawn closure calls, not a
        /// hand-copied template — applied exactly as it is in production.
        /// Writes `typed` immediately (the pty's input queue holds it
        /// regardless of whether the shell has reached a prompt yet, so no
        /// separate "wait for prompt, then type" round trip is needed), then
        /// polls the accumulated output for `sentinel` up to `budget` before
        /// exiting the shell and returning everything it wrote.
        ///
        /// **Never blocks past `budget` plus a small grace window, even if
        /// the child never exits.** A real `-l` login shell on macOS runs
        /// `/etc/zprofile`'s `path_helper`, which rebuilds `$PATH` from
        /// `/etc/paths`/`/etc/paths.d/*` and can reorder anything the caller
        /// set via `cmd.env("PATH", ...)` — on this machine that pushed
        /// `/opt/homebrew/bin` ahead of a `cmd.env`-prepended fake-agent bin
        /// dir, so `command claude` inside the wrapper found the *real*,
        /// system-installed `claude` CLI instead of the test's stand-in, and
        /// it sat there waiting on the pty's stdin forever — an unbounded
        /// `child.wait()` at the end of an earlier version of this harness
        /// hung the whole test suite because of it. Fixed two ways: (1)
        /// `write_home_rc_files` force-prepends the fake bin dir from
        /// `.zshrc` itself, which always runs *after* `/etc/zprofile`, so it
        /// wins regardless of what `path_helper` did; (2) belt-and-suspenders,
        /// this function now polls `child.try_wait()` with its own timeout
        /// and force-kills rather than blocking forever if `exit` somehow
        /// still doesn't land — a test must never be able to hang the suite
        /// just because a child process didn't behave.
        /// `app_data_dir` is passed straight through to `inject()`, which
        /// derives `TUIC_CLAUDE_SETTINGS` from it (`app_data_dir.join("agent-hooks/claude.json")`)
        /// whenever `agent_hook_launch::enabled("claude")` says the hook is
        /// on — which needs a `set_config_dir_override` in effect at the call
        /// site (see the two `#[test]` fns below) so that check reads a
        /// known-clean, test-owned directory instead of whatever this
        /// machine's real on-disk config happens to say.
        fn run_real_zsh_session(
            home: &Path,
            app_data_dir: &Path,
            tuic_session: Option<&str>,
            typed: &str,
            sentinel: &str,
            budget: Duration,
        ) -> String {
            let mut cmd = CommandBuilder::new("/bin/zsh");
            cmd.env_clear();
            cmd.env("HOME", home);
            cmd.env("TERM", "xterm");
            let bin_dir = fake_claude_bin_dir();
            let path = std::env::var_os("PATH").map_or_else(
                || bin_dir.display().to_string(),
                |existing| format!("{}:{}", bin_dir.display(), existing.to_string_lossy()),
            );
            cmd.env("PATH", path);
            if let Some(session) = tuic_session {
                cmd.env("TUIC_SESSION", session);
            }
            without_ambient_zdotdir(|| inject(app_data_dir, "zsh", &mut cmd));
            cmd.arg("-l");

            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 24,
                    cols: 200,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .expect("open pty");
            let mut child = pair.slave.spawn_command(cmd).expect("spawn zsh");
            drop(pair.slave);

            let mut writer = pair.master.take_writer().expect("take writer");
            let mut reader = pair.master.try_clone_reader().expect("clone reader");
            let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
            let buf_reader = buf.clone();
            let reader_thread = std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf_reader.lock().unwrap().extend_from_slice(&chunk[..n]),
                    }
                }
            });

            if !typed.is_empty() {
                writer.write_all(typed.as_bytes()).ok();
            }

            let sentinel_bytes = sentinel.as_bytes();
            let deadline = Instant::now() + budget;
            loop {
                if buf
                    .lock()
                    .unwrap()
                    .windows(sentinel_bytes.len())
                    .any(|w| w == sentinel_bytes)
                {
                    break;
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }

            let _ = writer.write_all(b"\nexit\n");
            drop(writer);

            // Bounded wait, never `child.wait()` alone — see the doc comment
            // above for the exact hang this is guarding against.
            let kill_deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if Instant::now() >= kill_deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
            let _ = reader_thread.join();
            let bytes = Arc::try_unwrap(buf)
                .expect("no other Arc owner left")
                .into_inner()
                .expect("mutex not poisoned");
            String::from_utf8_lossy(&bytes).into_owned()
        }

        fn write_home_rc_files(home: &Path, zshrc_d_files: &[(&str, &str)]) {
            std::fs::create_dir_all(home.join(".zshrc.d")).expect("create .zshrc.d");
            std::fs::write(
                home.join(".zshrc"),
                format!(
                    // Force-prepend the fake-agent bin dir *after* whatever
                    // `/etc/zprofile`'s `path_helper` did to `$PATH` during
                    // login-shell startup — see `run_real_zsh_session`'s doc
                    // comment for why this can't just rely on `cmd.env`.
                    "export PATH=\"{}:$PATH\"\n\
                     for __tuic_test_rc in \"$HOME\"/.zshrc.d/*(N); do source \"$__tuic_test_rc\"; done\n",
                    fake_claude_bin_dir().display()
                ),
            )
            .expect("write .zshrc");
            for (name, content) in zshrc_d_files {
                std::fs::write(home.join(".zshrc.d").join(name), content)
                    .unwrap_or_else(|e| panic!("write .zshrc.d/{name}: {e}"));
            }
        }

        /// The bug this whole module exists to catch: a `~/.zshrc.d/*`-style
        /// script gating a block of exports on
        /// `if [[ -x $(command -v claude) ]]` must see the real `claude`
        /// binary — not TUIC's own wrapper function — because `command -v`
        /// on an existing function returns the bare function name, not a
        /// path, which fails an `-x` test on any ordinary file layout. This
        /// broke silently (no error, nothing to grep for) when the
        /// claude/codex/goose wrappers loaded eagerly, from `.zshenv`, before
        /// `.zshrc`/`.zshrc.d` ran. It is fixed by deferring them past the
        /// first precmd call — see this module's own doc comment and the
        /// module-level doc comment above `ZSH_INTEGRATION`.
        ///
        /// Also asserts, in the same real launch, that:
        /// - the deferred wrapper is genuinely still active afterward (the
        ///   fix must not simply disable the feature to dodge the bug): typed
        ///   `claude --model opus` must come out through TUIC's wrapper with
        ///   `--settings` appended, which only the wrapper — never the bare
        ///   binary — would add.
        /// - the very first prompt still gets its OSC 133 `A` marker: the
        ///   eager half's registration timing is unrelated to this fix and
        ///   must not regress (a real trap here: an early prototype of this
        ///   fix deferred the OSC133 hooks too, by mistake, and lost exactly
        ///   this).
        #[test]
        #[serial_test::serial]
        fn claude_wrapper_loads_after_zshrc_d_not_before() {
            let home = tempfile::tempdir().expect("tempdir");
            // Isolate `config_dir()` so `inject()`'s
            // `agent_hook_launch::enabled("claude")` check (which decides
            // whether to set `TUIC_CLAUDE_SETTINGS` at all) reads a known
            // directory with no `agents.json` in it — deterministically
            // `true` (the crate's own default), never this machine's real,
            // possibly-different on-disk config.
            let config_dir = tempfile::tempdir().expect("config tempdir");
            let _guard = crate::config::set_config_dir_override(config_dir.path().to_path_buf());

            write_home_rc_files(
                home.path(),
                &[(
                    "50-claude-guard.sh",
                    "if [[ -x $(command -v claude) ]]; then\n\
                     \x20 print -n 'GUARD:PASSED'\n\
                     else\n\
                     \x20 print -n 'GUARD:FAILED'\n\
                     fi\n",
                )],
            );

            // What `inject()` will set `TUIC_CLAUDE_SETTINGS` to, given
            // `app_data_dir = home.path()` below.
            let expected_settings = home.path().join("agent-hooks/claude.json");
            let settings_flag = format!("--settings {}", expected_settings.display());

            let out = run_real_zsh_session(
                home.path(),
                home.path(),
                Some("test-session"),
                "claude --model opus\n",
                // The fake `claude` is a plain `/bin/echo` symlink (see
                // `fake_claude_bin_dir`'s doc comment on why: not a freshly
                // written script) — this is the exact line it prints when
                // the wrapper is active, so it doubles as both the
                // wait-sentinel and (via the assertion below) the proof the
                // wrapper actually appended `--settings`.
                &format!("--model opus {settings_flag}"),
                Duration::from_secs(10),
            );

            assert!(
                out.contains("GUARD:PASSED"),
                "a `command -v claude`-style guard in .zshrc.d must see the \
                 real binary, not TUIC's wrapper function — got: {out:?}"
            );
            assert!(
                !out.contains("GUARD:FAILED"),
                "guard reported FAILED — TUIC's claude() must not exist yet \
                 while .zshrc.d is still running — got: {out:?}"
            );
            assert!(
                out.contains(&settings_flag),
                "the deferred wrapper must still be active for real use \
                 after startup finishes (this must not become the fix by \
                 simply never loading the wrapper) — got: {out:?}"
            );
            assert!(
                out.contains("\u{1b}]133;A\u{07}"),
                "the very first prompt must still emit its OSC 133 'A' \
                 marker — the eager half's registration timing is a \
                 separate concern from the deferred fix and must not \
                 regress — got: {out:?}"
            );
        }

        /// If the user's own `.zshrc.d` already defines `claude`/`codex`/
        /// `goose` themselves, that definition must win — TUIC's deferred
        /// wrapper only defines itself `(( $+functions[name] )) ||`. This
        /// preserves what already happened by accident under the old
        /// eager-loading order (TUIC's wrapper loaded first and silently got
        /// overwritten by a later user redefinition): same end state, now
        /// reached on purpose instead of by accident, and load-bearing now
        /// that TUIC's definition is what would otherwise land last.
        #[test]
        #[serial_test::serial]
        fn users_own_claude_redefinition_still_wins_over_tuic() {
            let home = tempfile::tempdir().expect("tempdir");
            let config_dir = tempfile::tempdir().expect("config tempdir");
            let _guard = crate::config::set_config_dir_override(config_dir.path().to_path_buf());

            write_home_rc_files(
                home.path(),
                &[(
                    "60-user-claude.sh",
                    "claude() { print -n \"USER_OVERRIDE:$*\" }\n",
                )],
            );

            let out = run_real_zsh_session(
                home.path(),
                home.path(),
                Some("test-session"),
                "claude --model opus\n",
                "USER_OVERRIDE:",
                Duration::from_secs(10),
            );

            assert!(
                out.contains("USER_OVERRIDE:--model opus"),
                "the user's own claude() must win over TUIC's deferred \
                 wrapper — got: {out:?}"
            );
            assert!(
                !out.contains("--settings"),
                "TUIC's wrapper must not have run at all here (no \
                 --settings injected) — got: {out:?}"
            );
        }
    }
}
