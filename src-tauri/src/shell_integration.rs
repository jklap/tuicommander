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
//! own `.zprofile`/`.zshrc` run — the very first prompt still needs its `A`/`B`
//! markers, and nothing in a user's startup scripts plausibly probes for those
//! names, so there's no shadowing risk to defer any of them for. (An earlier
//! version also deferred the three OSC 7770 helpers; a user calling one from
//! their own `.zshrc` then hit "command not found", so they stay eager.)
//!
//! The deferred half (the `claude`/`codex`/`grok`/`opencode`/`goose`
//! auto-injection wrappers) used to load from `.zshenv` too — a real bug
//! (2026-09-24): defining `claude` as a shell function before the user's
//! `.zshrc`/`.zshrc.d/*` run breaks any `command -v claude` "is this a real
//! binary" guard there (`command -v` on a function returns the bare name, so
//! `[[ -x $(command -v claude) ]]` silently goes false and a whole guarded
//! block of the user's exports never runs). `.zshenv` now registers a one-shot
//! `precmd_functions` bootstrap that sources the deferred half from *inside*
//! the first precmd call, i.e. strictly after `.zprofile`/`.zshrc` finished.
//! Two things this is NOT:
//!
//! - It does NOT keep `$ZDOTDIR` pointed at the wrapper directory through
//!   `.zprofile`/`.zshrc`: `${ZDOTDIR:-$HOME}` is load-bearing for many real
//!   setups (oh-my-zsh's `compinit` dump path among them). `.zshenv` still
//!   resets `ZDOTDIR` immediately; only the function definitions move.
//! - It does NOT append a *second* `precmd_functions` entry: verified in a real
//!   PTY (`-c` never runs `precmd_functions`) that an entry appended during
//!   precmd only runs on the NEXT prompt. The deferred half is sourced as an
//!   ordinary statement inside the bootstrap's own body instead.
//!
//! **A user's own function of the same name is never wrapped without
//! consent.** Each deferred wrapper checks `(( $+functions[name] ))`. For
//! `claude`/`codex`/`goose` the decision lives in
//! `AgentSettings::wrap_user_function` + `wrap_user_function_hash`
//! (`config.rs`), threaded in by `inject_zsh` as
//! `TUIC_WRAP_USER_FN_<AGENT>` (`wrap`/`skip`/`ask`) and
//! `TUIC_WRAP_USER_FN_<AGENT>_HASH` (the fingerprint — `cksum` of the
//! function body — the decision was made for):
//!
//! - `wrap` applies ONLY when the current function's fingerprint equals the
//!   recorded one; the user's function is copied to `__tuic_user_<agent>` and
//!   `<agent>()` calls it through the same flag-injecting helper. A changed
//!   (or unfingerprintable) function is asked about again, never wrapped.
//! - `skip` leaves the function alone (a fingerprint-less `skip`, set from
//!   Settings, applies to any function).
//! - `ask` (undecided) leaves the function alone AND emits OSC 7770
//!   `userwrap=<agent>:<fingerprint>` (handled in `pty.rs`, which validates
//!   both halves and calls `agent_wrap_prompt::request`) so the user is asked
//!   once; the answer is persisted together with that fingerprint.
//! - unset (the script sourced by hand outside TUIC) behaves as `skip`.
//!
//! `grok`/`opencode` have no consent flow: a user's own function is always
//! left alone. Bash and fish keep eager wrappers (no deferred half), where a
//! later user redefinition simply wins.

use std::path::Path;

/// Zsh shell integration script — the eager half, sourced immediately from
/// `.zshenv`. See this module's doc comment for why the agent wrappers live in
/// `ZSH_DEFERRED_INTEGRATION` instead, not here.
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
# to call; no shadowing risk, so they stay eager (see the module doc comment).
tuic_state()   { printf '\e]7770;state=%s\a' "$1"; }
tuic_suggest() { printf '\e]7770;suggest=%s\a' "$*"; }
tuic_intent()  { printf '\e]7770;intent=%s\a' "$*"; }
"#;

/// Zsh shell integration script — the deferred half, sourced lazily from a
/// one-shot precmd bootstrap (see `ZDOTDIR_ZSHENV`) so the agent wrappers
/// never shadow `claude`/`codex`/`grok`/`opencode`/`goose` while the user's
/// own rc files are still deciding whether those names resolve to a real
/// binary. See this module's doc comment for the full rationale, and for the
/// consent rule that governs a user's OWN function of the same name.
const ZSH_DEFERRED_INTEGRATION: &str = r#"# TUIC Shell Integration — agent auto-injection wrappers (deferred)
# Auto-inject --name for Goose so tab↔session mapping is deterministic
if [[ -n "$TUIC_SESSION" ]]; then
  printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__' || TUIC_CLAUDE_HELP=__TUIC_RECORDED_CLAUDE_HELP__
  __tuic_screen_arg() {
    local flag="$1" skip="$2" a; shift 2
    [[ -n "$flag" && ( -z "$skip" || "$1" != "$skip" ) ]] || return 0
    for a in "$@"; do [[ "$a" == "$flag" ]] && return 0; done
    printf '%s' "$flag"
  }
  # Fingerprint of the user's own function $1 (cksum of its body: digits and
  # '-' only), in REPLY; empty when it can't be computed. An autoload stub is
  # resolved first so the fingerprint (and a later copy) is of the real body.
  __tuic_user_fn_fp() {
    REPLY=
    [[ $functions[$1] == *'builtin autoload -X'* ]] && { autoload +X "$1" 2>/dev/null || return 0; }
    local fp; fp=$(print -rn -- "$functions[$1]" | command cksum 2>/dev/null) || return 0
    fp=${(j:-:)${=fp}}
    [[ $fp =~ '^[0-9]+-[0-9]+$' ]] && REPLY=$fp
  }
  # Decide what to do about a user's own function: $1 = the stored decision
  # (wrap/skip/ask; empty outside TUIC), $2 = the fingerprint that decision
  # was made for, $3 = the current fingerprint. Wrapping needs consent for
  # THIS exact function body; anything else is asked about again.
  __tuic_user_fn_mode() {
    case "$1" in
      wrap) [[ -n "$3" && "$2" == "$3" ]] && REPLY=wrap || REPLY=ask;;
      skip) [[ -z "$2" || "$2" == "$3" ]] && REPLY=skip || REPLY=ask;;
      ask) REPLY=ask;;
      *) REPLY=skip;;
    esac
  }
  __tuic_inject_claude() {
    local callee=$1; shift
    case "$1" in
      ""|-*) ;;
      remote-control) "$callee" "$@"; return;;
      *) if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$1" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'; then "$callee" "$@"; return; fi;;
    esac
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) "$callee" "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then "$callee" --settings "$TUIC_CLAUDE_SETTINGS" "$@"; else "$callee" "$@"; fi
  }
  __tuic_inject_codex() {
    local callee=$1; shift
    local a prev screen
    screen=$(__tuic_screen_arg "$TUIC_CODEX_SCREEN_FLAG" "$TUIC_CODEX_SCREEN_SKIP" "$@")
    for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then "$callee" ${screen:+"$screen"} "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then "$callee" ${screen:+"$screen"} "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else "$callee" ${screen:+"$screen"} "$@"; fi
  }
  __tuic_inject_goose() {
    local callee=$1; shift
    local a; for a in "$@"; do
      case "$a" in --name|-n|--resume|-r) "$callee" "$@"; return;; esac
    done
    case "$1" in
      session|run) "$callee" "$1" --name "$TUIC_SESSION" "${@:2}";;
      *) "$callee" "$@";;
    esac
  }
  # claude/codex/goose: a user's own function is wrapped ONLY with recorded
  # consent for its exact fingerprint; otherwise it is left alone, and the
  # app is asked (OSC 7770 userwrap=<agent>:<fingerprint>) when undecided.
  # The agent names below are literals and the fingerprint is digits and '-'
  # only, so nothing user-controlled is interpolated into code or the OSC.
  if (( $+functions[claude] )); then
    __tuic_user_fn_fp claude; __tuic_fp=$REPLY
    __tuic_user_fn_mode "$TUIC_WRAP_USER_FN_CLAUDE" "$TUIC_WRAP_USER_FN_CLAUDE_HASH" "$__tuic_fp"
    case $REPLY in
      wrap)
        functions[__tuic_user_claude]=$functions[claude]
        claude() { __tuic_inject_claude __tuic_user_claude "$@"; }
        ;;
      ask) [[ -n "$TUIC_CLAUDE_SETTINGS" && -n "$__tuic_fp" ]] && printf '\e]7770;userwrap=claude:%s\a' "$__tuic_fp";;
    esac
  else
    __tuic_real_claude() { command claude "$@"; }
    claude() { __tuic_inject_claude __tuic_real_claude "$@"; }
  fi
  if (( $+functions[codex] )); then
    __tuic_user_fn_fp codex; __tuic_fp=$REPLY
    __tuic_user_fn_mode "$TUIC_WRAP_USER_FN_CODEX" "$TUIC_WRAP_USER_FN_CODEX_HASH" "$__tuic_fp"
    case $REPLY in
      wrap)
        functions[__tuic_user_codex]=$functions[codex]
        codex() { __tuic_inject_codex __tuic_user_codex "$@"; }
        ;;
      ask) [[ -n "$TUIC_CODEX_NOTIFY" && -n "$__tuic_fp" ]] && printf '\e]7770;userwrap=codex:%s\a' "$__tuic_fp";;
    esac
  else
    __tuic_real_codex() { command codex "$@"; }
    codex() { __tuic_inject_codex __tuic_real_codex "$@"; }
  fi
  if (( $+functions[goose] )); then
    __tuic_user_fn_fp goose; __tuic_fp=$REPLY
    __tuic_user_fn_mode "$TUIC_WRAP_USER_FN_GOOSE" "$TUIC_WRAP_USER_FN_GOOSE_HASH" "$__tuic_fp"
    case $REPLY in
      wrap)
        functions[__tuic_user_goose]=$functions[goose]
        goose() { __tuic_inject_goose __tuic_user_goose "$@"; }
        ;;
      # --name injection has no separate settings gate, so this always asks.
      ask) [[ -n "$__tuic_fp" ]] && printf '\e]7770;userwrap=goose:%s\a' "$__tuic_fp";;
    esac
  else
    __tuic_real_goose() { command goose "$@"; }
    goose() { __tuic_inject_goose __tuic_real_goose "$@"; }
  fi
  unset __tuic_fp
  # grok/opencode: no consent flow — a user's own function is always left alone.
  if (( ! $+functions[grok] )); then
    grok() {
      local screen=$(__tuic_screen_arg "$TUIC_GROK_SCREEN_FLAG" "$TUIC_GROK_SCREEN_SKIP" "$@")
      command grok ${screen:+"$screen"} "$@"
    }
  fi
  if (( ! $+functions[opencode] )); then
    opencode() {
      local screen=$(__tuic_screen_arg "$TUIC_OPENCODE_SCREEN_FLAG" "$TUIC_OPENCODE_SCREEN_SKIP" "$@")
      command opencode ${screen:+"$screen"} "$@"
    }
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
  printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__' || TUIC_CLAUDE_HELP=__TUIC_RECORDED_CLAUDE_HELP__
  __tuic_screen_arg() {
    local flag="$1" skip="$2" a; shift 2
    [[ -n "$flag" && ( -z "$skip" || "$1" != "$skip" ) ]] || return 0
    for a in "$@"; do [[ "$a" == "$flag" ]] && return 0; done
    printf '%s' "$flag"
  }
  claude() {
    case "$1" in
      ""|-*) ;;
      remote-control) command claude "$@"; return;;
      *) if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$1" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'; then command claude "$@"; return; fi;;
    esac
    local a; for a in "$@"; do
      case "$a" in --settings|--settings=*|--bare) command claude "$@"; return;; esac
    done
    if [[ -n "$TUIC_CLAUDE_SETTINGS" ]]; then command claude --settings "$TUIC_CLAUDE_SETTINGS" "$@"; else command claude "$@"; fi
  }
  codex() {
    local a prev screen
    screen=$(__tuic_screen_arg "$TUIC_CODEX_SCREEN_FLAG" "$TUIC_CODEX_SCREEN_SKIP" "$@")
    for a in "$@"; do
      if [[ "$prev" == "-c" && "$a" == notify=* ]] || [[ "$a" == -cnotify=* || "$a" == --config=notify=* ]]; then command codex ${screen:+"$screen"} "$@"; return; fi
      prev="$a"
    done
    if [[ -n "$TUIC_CODEX_NOTIFY" ]]; then command codex ${screen:+"$screen"} "$@" -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"; else command codex ${screen:+"$screen"} "$@"; fi
  }
  grok() {
    local screen=$(__tuic_screen_arg "$TUIC_GROK_SCREEN_FLAG" "$TUIC_GROK_SCREEN_SKIP" "$@")
    command grok ${screen:+"$screen"} "$@"
  }
  opencode() {
    local screen=$(__tuic_screen_arg "$TUIC_OPENCODE_SCREEN_FLAG" "$TUIC_OPENCODE_SCREEN_SKIP" "$@")
    command opencode ${screen:+"$screen"} "$@"
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
  if not printf '%s\n' "$TUIC_CLAUDE_HELP" | awk '__TUIC_CLAUDE_HELP_USABLE__'
    set -gx TUIC_CLAUDE_HELP __TUIC_RECORDED_CLAUDE_HELP__
  end
  function __tuic_screen_arg
    set -l flag $argv[1]
    set -l skip $argv[2]
    set -l args $argv[3..]
    if test -z "$flag"
      return
    end
    if test -n "$skip"; and test "$args[1]" = "$skip"
      return
    end
    for a in $args
      if test "$a" = "$flag"
        return
      end
    end
    printf '%s' "$flag"
  end
  function claude --wraps claude
    if test (count $argv) -gt 0
      switch $argv[1]
        case '-*'
        case remote-control
          command claude $argv; return
        case '*'
          if printf '%s\n' "$TUIC_CLAUDE_HELP" | awk -v verb="$argv[1]" '/^Commands:/ {commands=1; next} commands && /^  [^ ]/ {split($1, names, "|"); for (i in names) if (names[i] == verb) found=1} END {exit !found}'
            command claude $argv; return
          end
      end
    end
    for a in $argv
      switch $a
        case --settings '--settings=*' --bare
          command claude $argv; return
      end
    end
    if set -q TUIC_CLAUDE_SETTINGS
      command claude --settings $TUIC_CLAUDE_SETTINGS $argv
    else
      command claude $argv
    end
  end
  function codex --wraps codex
    set -l flag ''
    set -l skip ''
    if set -q TUIC_CODEX_SCREEN_FLAG; set flag $TUIC_CODEX_SCREEN_FLAG; end
    if set -q TUIC_CODEX_SCREEN_SKIP; set skip $TUIC_CODEX_SCREEN_SKIP; end
    set -l screen (__tuic_screen_arg "$flag" "$skip" $argv)
    set -l prev
    for a in $argv
      if test "$prev" = -c; and string match -q 'notify=*' -- $a
        command codex $screen $argv; return
      end
      if string match -q -- '-cnotify=*' $a; or string match -q -- '--config=notify=*' $a
        command codex $screen $argv; return
      end
      set prev $a
    end
    if set -q TUIC_CODEX_NOTIFY
      command codex $screen $argv -c "notify=[\"$TUIC_CODEX_NOTIFY\"]"
    else
      command codex $screen $argv
    end
  end
  function grok --wraps grok
    set -l flag ''
    if set -q TUIC_GROK_SCREEN_FLAG; set flag $TUIC_GROK_SCREEN_FLAG; end
    set -l screen (__tuic_screen_arg "$flag" '' $argv)
    command grok $screen $argv
  end
  function opencode --wraps opencode
    set -l flag ''
    set -l skip ''
    if set -q TUIC_OPENCODE_SCREEN_FLAG; set flag $TUIC_OPENCODE_SCREEN_FLAG; end
    if set -q TUIC_OPENCODE_SCREEN_SKIP; set skip $TUIC_OPENCODE_SCREEN_SKIP; end
    set -l screen (__tuic_screen_arg "$flag" "$skip" $argv)
    command opencode $screen $argv
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
/// `tuic-integration.zsh` and `tuic-integration-deferred.zsh`, single-quoted
/// for zsh (`zsh_single_quote`). See this module's doc comment for why the
/// deferred half is registered as a one-shot precmd bootstrap instead of being
/// sourced here.
const ZDOTDIR_ZSHENV: &str = r#"# TUIC ZDOTDIR wrapper — sources the eager integration now, defers the rest,
# then restores real dotfiles.
source {eager_script}
__tuic_bootstrap() {
  precmd_functions=(${precmd_functions:#__tuic_bootstrap})
  source {deferred_script}
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
        // Publish installed help, or the same recorded fallback used by managed launches.
        let help = crate::agent::cli_help("claude");
        cmd.env(
            "TUIC_CLAUDE_HELP",
            help.as_deref()
                .unwrap_or(crate::agent_hook_launch::RECORDED_CLAUDE_HELP),
        );
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
    let agents_config = crate::config::load_agents_config();
    for (agent, _, _) in crate::agent_hook_launch::SCREEN_POLICIES {
        inject_screen_policy(cmd, &agents_config, agent, agent);
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

fn inject_screen_policy(
    cmd: &mut portable_pty::CommandBuilder,
    config: &crate::config::AgentsConfig,
    agent_type: &str,
    binary_path: &str,
) {
    let prefix = format!(
        "TUIC_{}_SCREEN",
        agent_type.to_ascii_uppercase().replace('-', "_")
    );
    let flag_key = format!("{prefix}_FLAG");
    let skip_key = format!("{prefix}_SKIP");
    cmd.env_remove(&flag_key);
    cmd.env_remove(&skip_key);
    if !crate::agent_hook_launch::prevents_alt_screen_in(config, agent_type) {
        return;
    }
    let Some((flag, skipped_command)) = crate::agent_hook_launch::screen_policy(agent_type) else {
        return;
    };
    if !crate::agent::supports_no_alt_screen(agent_type, binary_path) {
        return;
    }
    cmd.env(flag_key, flag);
    if let Some(command) = skipped_command {
        cmd.env(skip_key, command);
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

/// Embed the recorded help as one shell-quoted value, shared by all wrappers.
fn render_integration(template: &str) -> String {
    let help = crate::agent_hook_launch::RECORDED_CLAUDE_HELP.replace('\'', "'\\''");
    template
        .replace("__TUIC_RECORDED_CLAUDE_HELP__", &format!("'{help}'"))
        .replace(
            "__TUIC_CLAUDE_HELP_USABLE__",
            "/^Commands:/ {commands=1; next} commands && /^  [^ ]/ && NF {found=1} END {exit !found}",
        )
}

fn inject_zsh(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Write the eager integration script (OSC 133 markers + OSC 7770 helpers)
    let script_path = base.join("tuic-integration.zsh");
    if !write_if_changed(&script_path, &render_integration(ZSH_INTEGRATION)) {
        return;
    }

    // Write the deferred integration script (agent wrappers) — sourced from the
    // first precmd; see this module's doc comment and ZDOTDIR_ZSHENV.
    let deferred_script_path = base.join("tuic-integration-deferred.zsh");
    if !write_if_changed(
        &deferred_script_path,
        &render_integration(ZSH_DEFERRED_INTEGRATION),
    ) {
        return;
    }

    // Create ZDOTDIR wrapper directory
    let zdotdir = base.join("zdotdir");
    if std::fs::create_dir_all(&zdotdir).is_err() {
        return;
    }

    // .zshenv — sources the eager integration, registers a precmd bootstrap for
    // the deferred one, then restores real ZDOTDIR and sources real .zshenv.
    let zshenv_content = ZDOTDIR_ZSHENV
        .replace(
            "{eager_script}",
            &zsh_single_quote(&script_path.to_string_lossy()),
        )
        .replace(
            "{deferred_script}",
            &zsh_single_quote(&deferred_script_path.to_string_lossy()),
        );
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

    // zsh only — thread each persisted wrap decision (and the fingerprint it
    // was made for) into the shell, so ZSH_DEFERRED_INTEGRATION can act on it
    // without a round trip. Always set (never inherited from a parent TUIC
    // shell): the values are fixed words and `[0-9]+-[0-9]+` fingerprints.
    let agents_config = crate::config::load_agents_config();
    for agent in WRAP_USER_FUNCTION_AGENTS {
        let (mode, hash) = wrap_user_function_env(&agents_config, agent);
        let key = format!("TUIC_WRAP_USER_FN_{}", agent.to_ascii_uppercase());
        cmd.env(&key, mode);
        cmd.env(format!("{key}_HASH"), hash);
    }
}

/// Agents whose zsh wrapper may wrap a user's own same-named function after
/// explicit consent (`agent_wrap_prompt`). Also the allow-list every entry
/// point (OSC verb, IPC/HTTP setter, prompt resolver) validates against.
pub(crate) const WRAP_USER_FUNCTION_AGENTS: [&str; 3] = ["claude", "codex", "goose"];

/// `(mode, fingerprint)` for `TUIC_WRAP_USER_FN_<AGENT>[_HASH]`: `wrap` /
/// `skip` / `ask` matching `__tuic_user_fn_mode`'s arms, and the recorded
/// fingerprint (empty when none). A stored fingerprint that is not well-formed
/// is dropped, so a hand-edited config can never inject shell text — and a
/// `wrap` without a valid fingerprint then reads as "ask" in the shell.
fn wrap_user_function_env(
    config: &crate::config::AgentsConfig,
    agent: &str,
) -> (&'static str, String) {
    let settings = config.agents.get(agent);
    let mode = match settings.and_then(|s| s.wrap_user_function) {
        Some(true) => "wrap",
        Some(false) => "skip",
        None => "ask",
    };
    let hash = settings
        .and_then(|s| s.wrap_user_function_hash.as_deref())
        .filter(|h| is_user_function_fingerprint(h))
        .unwrap_or_default()
        .to_string();
    (mode, hash)
}

/// A user-function fingerprint as the zsh integration computes it: `cksum`'s
/// CRC and byte count joined by `-` (digits only on both sides).
pub(crate) fn is_user_function_fingerprint(value: &str) -> bool {
    let Some((crc, len)) = value.split_once('-') else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit());
    digits(crc) && digits(len)
}

/// Single-quote `value` for zsh source text (`'` -> `'\''`), so a path with
/// spaces, `$`, backticks or quotes is taken literally.
fn zsh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn inject_bash(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, &render_integration(BASH_INTEGRATION)) {
        // BASH_ENV is sourced for non-interactive bash; for interactive login
        // shells we rely on the user sourcing it or a future --init-file approach.
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

fn inject_fish(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    // Fish auto-sources scripts in conf.d/ directories under XDG_CONFIG_HOME.
    // For now, just point to the script via env var.
    let script_path = base.join("tuic-integration.fish");
    if write_if_changed(&script_path, &render_integration(FISH_INTEGRATION)) {
        cmd.env("TUIC_SHELL_INTEGRATION", script_path_str(&script_path));
    }
}

/// Inject bash integration for WSL shells. The script files live on the
/// Windows filesystem but env vars reference them via `/mnt/` paths so
/// they're accessible inside the WSL Linux environment.
fn inject_bash_wsl(base: &Path, cmd: &mut portable_pty::CommandBuilder) {
    let script_path = base.join("tuic-integration.bash");
    if write_if_changed(&script_path, &render_integration(BASH_INTEGRATION)) {
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

    #[test]
    fn injected_shell_uses_agent_settings_and_clears_stale_screen_environment() {
        use std::ffi::OsStr;
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let mut config = crate::config::AgentsConfig::default();
        for agent in ["codex", "grok", "opencode"] {
            config.agents.insert(
                agent.into(),
                crate::config::AgentSettings {
                    prevent_alt_screen: Some(false),
                    ..Default::default()
                },
            );
        }
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
        let mut cmd = portable_pty::CommandBuilder::new("bash");
        cmd.env("TUIC_CODEX_SCREEN_FLAG", "stale");
        inject(dir.path(), "bash", &mut cmd);
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_GROK_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"), None);
        assert!(cmd.get_env("TUIC_CLAUDE_SETTINGS").is_some());
        assert!(cmd.get_env("TUIC_CODEX_NOTIFY").is_some());
        assert_eq!(
            cmd.get_env("TUIC_SHELL_INTEGRATION"),
            Some(OsStr::new(
                &dir.path()
                    .join("shell-integration")
                    .join("tuic-integration.bash")
            ))
        );
    }

    #[cfg(unix)]
    #[test]
    fn shell_environment_exports_only_supported_enabled_screen_policies() {
        use std::ffi::OsStr;
        let supported = crate::test_support::fake_ssh_script(
            "shell-screen-supported",
            "printf '%s\\n' '--no-alt-screen --mini'",
            "echo --no-alt-screen --mini",
        );
        let unsupported = crate::test_support::fake_ssh_script(
            "shell-screen-unsupported",
            "printf '%s\\n' 'old help'",
            "echo old help",
        );
        let mut config = crate::config::AgentsConfig::default();
        let mut cmd = portable_pty::CommandBuilder::new("bash");
        inject_screen_policy(&mut cmd, &config, "codex", &supported.to_string_lossy());
        assert_eq!(
            cmd.get_env("TUIC_CODEX_SCREEN_FLAG"),
            Some(OsStr::new("--no-alt-screen"))
        );
        assert_eq!(
            cmd.get_env("TUIC_CODEX_SCREEN_SKIP"),
            Some(OsStr::new("exec"))
        );
        inject_screen_policy(&mut cmd, &config, "opencode", &supported.to_string_lossy());
        assert_eq!(
            cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"),
            Some(OsStr::new("--mini"))
        );
        assert_eq!(
            cmd.get_env("TUIC_OPENCODE_SCREEN_SKIP"),
            Some(OsStr::new("run"))
        );

        inject_screen_policy(&mut cmd, &config, "codex", &unsupported.to_string_lossy());
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_CODEX_SCREEN_SKIP"), None);
        config.agents.insert(
            "opencode".into(),
            crate::config::AgentSettings {
                prevent_alt_screen: Some(false),
                ..Default::default()
            },
        );
        inject_screen_policy(&mut cmd, &config, "opencode", &supported.to_string_lossy());
        assert_eq!(cmd.get_env("TUIC_OPENCODE_SCREEN_FLAG"), None);
        assert_eq!(cmd.get_env("TUIC_OPENCODE_SCREEN_SKIP"), None);
    }

    /// `off_passthrough` is the exact substring proving each agent's
    /// "no settings configured" path calls the real command with no flag
    /// added — this differs for zsh, whose deferred wrappers call through a
    /// `$callee` (the real binary, or a consented user function) instead of a
    /// literal `command claude`/`command codex`.
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
        // The agent wrappers live in the deferred half — see the module doc.
        assert_wrapper_paths(
            "zsh",
            ZSH_DEFERRED_INTEGRATION,
            (
                "else \"$callee\" \"$@\"",
                "else \"$callee\" ${screen:+\"$screen\"} \"$@\"",
            ),
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
        use super::super::{
            BASH_INTEGRATION, FISH_INTEGRATION, ZSH_DEFERRED_INTEGRATION, render_integration,
        };
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

        /// A staging path next to `dir/name` that no other test thread uses.
        ///
        /// The process id alone is not enough: the launch tests run as threads
        /// of one process, and two of them staging the same `name` under the
        /// same pid raced — one renamed the file away while the other was
        /// still about to rename it, and the second failed with `NotFound`.
        fn staging_path(dir: &Path, name: &str) -> PathBuf {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            dir.join(format!("{name}.{}.{seq}", std::process::id()))
        }

        /// A `PATH` prefix holding `claude` and `codex` stand-ins.
        fn agent_bin_dir() -> PathBuf {
            assert!(
                Path::new(REAL_ECHO).exists(),
                "{REAL_ECHO} is missing; the wrapper launch harness needs it"
            );
            // Under `target/`, so it is gitignored and survives between runs.
            let dir = std::env::var_os("TUIC_SHELL_TEST_BIN_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-agent-bin")
                });
            std::fs::create_dir_all(&dir).expect("create fake agent bin dir");

            for agent in ["claude", "codex", "grok", "opencode"] {
                let link = dir.join(agent);
                if std::fs::read_link(&link).is_ok_and(|target| target == Path::new(REAL_ECHO)) {
                    continue;
                }
                // Stage under a unique name and rename over the target, so
                // tests running in parallel never observe a missing link.
                let staging = staging_path(&dir, agent);
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
                // The wrappers under test live in the deferred half in
                // production — sourced directly here (not through the precmd
                // bootstrap): this harness is about wrapper correctness; the
                // deferred loading itself is covered by `zsh_deferred_load`.
                "zsh" => ("tuic-integration-deferred.zsh", ZSH_DEFERRED_INTEGRATION),
                "fish" => ("tuic-integration.fish", FISH_INTEGRATION),
                other => panic!("no integration script for {other}"),
            };
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shell-integration-tests");
            std::fs::create_dir_all(&dir).expect("create integration script dir");
            let path = dir.join(name);
            let staging = staging_path(&dir, name);
            std::fs::write(&staging, render_integration(body)).expect("write integration script");
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
                .env(
                    "TUIC_CLAUDE_HELP",
                    include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt"),
                )
                // Start from setting-off, so a case that wants injection has to
                // ask for it and the off case cannot pass on an inherited value.
                .env_remove("TUIC_CLAUDE_SETTINGS")
                .env_remove("TUIC_CODEX_NOTIFY")
                .env_remove("TUIC_CODEX_SCREEN_FLAG")
                .env_remove("TUIC_GROK_SCREEN_FLAG")
                .env_remove("TUIC_OPENCODE_SCREEN_FLAG");
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
        /// log line reading `setting_on_prepends_launch_scoped_status_flags`
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

        // Catches: a nonempty cache without command rows prefixes auth with prompt settings.
        fn corrupt_cached_help_preserves_auth_subcommand(shell: &str) {
            require_shell(shell);
            let recorded = crate::agent_hook_launch::RECORDED_CLAUDE_HELP;
            let truncated = recorded.split("Commands:").next().expect("help prefix");
            let actual: Vec<String> = [" \n\t", truncated]
                .into_iter()
                .flat_map(|help| {
                    wrapper_command_lines(
                        shell,
                        &[
                            ("TUIC_CLAUDE_SETTINGS", "/tuic/claude.json"),
                            ("TUIC_CLAUDE_HELP", help),
                        ],
                        "claude auth status",
                    )
                })
                .collect();
            assert_eq!(
                actual,
                ["auth status", "auth status"],
                "{shell}: whitespace and truncated help must retain recorded auth argv"
            );
        }

        launch_matrix!(corrupt_cached_help_preserves_auth_subcommand);

        launch_matrix!(
            setting_on_prepends_launch_scoped_status_flags,
            an_explicit_user_flag_suppresses_injection,
            setting_off_leaves_the_command_line_untouched,
            screen_flags_follow_manual_agent_commands,
            hyphenated_prompts_keep_settings_in_every_shell,
            unavailable_help_keeps_verbs_and_prompts_without_launch_probe,
            unavailable_help_preserves_recorded_auth_subcommand,
        );

        fn screen_flags_follow_manual_agent_commands(shell: &str) {
            let flags = [
                ("TUIC_CODEX_SCREEN_FLAG", "--no-alt-screen"),
                ("TUIC_CODEX_SCREEN_SKIP", "exec"),
                ("TUIC_GROK_SCREEN_FLAG", "--no-alt-screen"),
                ("TUIC_OPENCODE_SCREEN_FLAG", "--mini"),
                ("TUIC_OPENCODE_SCREEN_SKIP", "run"),
            ];
            assert_in_shell(
                shell,
                "manual agents use the exported screen policy without changing explicit choices",
                &flags,
                "codex --model o3\n\
                 codex exec --full-auto\n\
                 codex --no-alt-screen --model o3\n\
                 grok --model fast\n\
                 grok --fullscreen --model fast\n\
                 opencode --model fast\n\
                 opencode run task\n\
                 opencode --mini --model fast\n\
                 command codex --model o3",
                &[
                    "--no-alt-screen --model o3",
                    "exec --full-auto",
                    "--no-alt-screen --model o3",
                    "--no-alt-screen --model fast",
                    "--no-alt-screen --fullscreen --model fast",
                    "--mini --model fast",
                    "run task",
                    "--mini --model fast",
                    "--model o3",
                ],
            );
        }

        const CLAUDE_SETTINGS: &str = "/tuic/agent-hooks/claude.json";
        const CODEX_NOTIFY: &str = "/tuic/agent-hooks/codex-notify.sh";

        fn signals_on() -> [(&'static str, &'static str); 2] {
            [
                ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                ("TUIC_CODEX_NOTIFY", CODEX_NOTIFY),
            ]
        }

        fn setting_on_prepends_launch_scoped_status_flags(shell: &str) {
            // Catches: root settings injected into a subcommand instead of a prompt.
            let claude = format!("--settings {CLAUDE_SETTINGS} --model opus");
            let prompt = format!("--settings {CLAUDE_SETTINGS} prompt");
            let resume = format!("--settings {CLAUDE_SETTINGS} --resume x");
            assert_in_shell(
                shell,
                "Claude gets the TUIC settings file before its own arguments",
                &signals_on(),
                "claude --model opus\n\
                 claude doctor\n\
                 claude remote-control --resume x\n\
                 claude prompt\n\
                 claude --resume x\n\
                 claude plugins",
                &[
                    claude.as_str(),
                    "doctor",
                    "remote-control --resume x",
                    prompt.as_str(),
                    resume.as_str(),
                    "plugins",
                ],
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

        /// Catches: hyphenated prompt text is mistaken for a hidden command.
        fn hyphenated_prompts_keep_settings_in_every_shell(shell: &str) {
            let short = format!("--settings {CLAUDE_SETTINGS} fix-bug");
            let sentence =
                format!("--settings {CLAUDE_SETTINGS} Explain the remote-control failure");
            assert_in_shell(
                shell,
                "ordinary prompts keep status hooks",
                &signals_on(),
                "claude fix-bug\nclaude 'Explain the remote-control failure'",
                &[short.as_str(), sentence.as_str()],
            );
        }

        /// Catches: missing cached help runs an extra CLI or injects settings into a known verb.
        fn unavailable_help_keeps_verbs_and_prompts_without_launch_probe(shell: &str) {
            let prompt = format!("--settings {CLAUDE_SETTINGS} fix-bug");
            assert_in_shell(
                shell,
                "unavailable help uses recorded verbs without a shell probe",
                &[
                    ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                    ("TUIC_CLAUDE_HELP", ""),
                ],
                "claude doctor\nclaude mcp list\nclaude plugins\nclaude upgrade\nclaude fix-bug",
                &["doctor", "mcp list", "plugins", "upgrade", prompt.as_str()],
            );
        }

        /// Catches: the shell fallback injects settings into recorded auth when help is absent.
        fn unavailable_help_preserves_recorded_auth_subcommand(shell: &str) {
            assert_in_shell(
                shell,
                "recorded auth is still a subcommand without cached help",
                &[
                    ("TUIC_CLAUDE_SETTINGS", CLAUDE_SETTINGS),
                    ("TUIC_CLAUDE_HELP", ""),
                ],
                "claude auth status",
                &["auth status"],
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
            assert_in_shell(
                shell,
                "Unsupported or disabled screen policies leave agent arguments unchanged",
                &[],
                "codex --model o3\n\
                 grok --model fast\n\
                 opencode --model fast",
                &["--model o3", "--model fast", "--model fast"],
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
            ("zsh", ZDOTDIR_ZSHENV),
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

        // Installed by `scripts/install-launch-shells.sh` on the "rust" CI job
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
        /// module-level doc comment at the top of this file.
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
                // TUIC puts `--settings` BEFORE the user's own arguments.
                &format!("{settings_flag} --model opus"),
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

        /// If the user's own `.zshrc.d` already defines `claude` and nothing
        /// has been consented to (a fresh config: "ask"), that definition
        /// must win untouched — TUIC's deferred wrapper never wraps a user's
        /// function without a recorded "yes" for its exact fingerprint. It
        /// does ask: the `userwrap=claude:<fingerprint>` OSC must be emitted.
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
            assert!(
                out.contains("\u{1b}]7770;userwrap=claude:"),
                "an undecided user function must raise the consent prompt — got: {out:?}"
            );
        }
    }

    /// The consent rule for a user's OWN `claude`/`codex`/`goose` function,
    /// exercised by sourcing the real deferred script in `zsh -f -c` with the
    /// exact env `inject_zsh` exports. No PTY needed: the decision logic runs
    /// at source time.
    #[cfg(unix)]
    mod zsh_consent {
        use super::super::{ZSH_DEFERRED_INTEGRATION, render_integration};
        use std::path::Path;
        use std::process::Command;

        const SETTINGS: &str = "/tmp/tuic-consent-test/claude.json";
        /// A user function that reports exactly what it was called with.
        const USER_FN: &str = "claude() { print -r -- \"USER:$*\"; }";

        fn script() -> std::path::PathBuf {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shell-integration-tests");
            std::fs::create_dir_all(&dir).expect("create integration script dir");
            let path = dir.join(format!(
                "consent-{}-{}.zsh",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&path, render_integration(ZSH_DEFERRED_INTEGRATION))
                .expect("write deferred script");
            path
        }

        /// Define `user_fn`, source the deferred script with the given wrap
        /// env, then run `claude --model opus`. Returns stdout.
        fn run(user_fn: &str, wrap_env: &[(&str, &str)]) -> String {
            let path = script();
            let mut cmd = Command::new("zsh");
            cmd.arg("-f")
                .arg("-c")
                .arg(format!(
                    "{user_fn}\nsource '{}'\nclaude --model opus",
                    path.display()
                ))
                .env("TUIC_SESSION", "consent-test")
                .env("TUIC_CLAUDE_SETTINGS", SETTINGS)
                .env(
                    "TUIC_CLAUDE_HELP",
                    include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt"),
                )
                .env_remove("TUIC_WRAP_USER_FN_CLAUDE")
                .env_remove("TUIC_WRAP_USER_FN_CLAUDE_HASH");
            for (k, v) in wrap_env {
                cmd.env(k, v);
            }
            let out = cmd
                .output()
                .expect("zsh must be installed (scripts/install-launch-shells.sh)");
            let _ = std::fs::remove_file(&path);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                out.status.success() && stderr.is_empty(),
                "zsh failed: {stderr}"
            );
            String::from_utf8(out.stdout).expect("utf-8")
        }

        /// The fingerprint the shell reported in its `userwrap` OSC, if any.
        fn asked_fingerprint(out: &str) -> Option<String> {
            let start =
                out.find("\u{1b}]7770;userwrap=claude:")? + "\u{1b}]7770;userwrap=claude:".len();
            let end = out[start..].find('\u{7}')? + start;
            Some(out[start..end].to_string())
        }

        fn ask_env() -> [(&'static str, &'static str); 1] {
            [("TUIC_WRAP_USER_FN_CLAUDE", "ask")]
        }

        #[test]
        fn undecided_leaves_the_function_alone_and_asks_with_a_fingerprint() {
            let out = run(USER_FN, &ask_env());
            assert!(
                out.contains("USER:--model opus\n"),
                "not wrapped without consent: {out:?}"
            );
            assert!(!out.contains(SETTINGS), "no flag without consent: {out:?}");
            let fp = asked_fingerprint(&out).expect("must ask");
            assert!(
                crate::shell_integration::is_user_function_fingerprint(&fp),
                "fingerprint must be digits-dash-digits: {fp:?}"
            );
        }

        #[test]
        fn wrap_applies_only_to_the_consented_fingerprint() {
            let fp = asked_fingerprint(&run(USER_FN, &ask_env())).expect("must ask");

            let out = run(
                USER_FN,
                &[
                    ("TUIC_WRAP_USER_FN_CLAUDE", "wrap"),
                    ("TUIC_WRAP_USER_FN_CLAUDE_HASH", &fp),
                ],
            );
            assert!(
                out.contains(&format!("USER:--settings {SETTINGS} --model opus\n")),
                "consented function must be wrapped: {out:?}"
            );
            assert!(
                asked_fingerprint(&out).is_none(),
                "no prompt once consented: {out:?}"
            );

            // The user edits the function: the old "yes" must not carry over.
            let changed = "claude() { print -r -- \"USER:$*\"; : changed; }";
            let out = run(
                changed,
                &[
                    ("TUIC_WRAP_USER_FN_CLAUDE", "wrap"),
                    ("TUIC_WRAP_USER_FN_CLAUDE_HASH", &fp),
                ],
            );
            assert!(
                out.contains("USER:--model opus\n"),
                "changed function not wrapped: {out:?}"
            );
            let new_fp = asked_fingerprint(&out).expect("changed function must be asked about");
            assert_ne!(new_fp, fp);
        }

        #[test]
        fn wrap_without_a_fingerprint_asks_instead_of_wrapping() {
            let out = run(USER_FN, &[("TUIC_WRAP_USER_FN_CLAUDE", "wrap")]);
            assert!(out.contains("USER:--model opus\n"), "{out:?}");
            assert!(asked_fingerprint(&out).is_some(), "{out:?}");
        }

        #[test]
        fn skip_and_outside_tuic_never_wrap_or_ask() {
            for env in [&[("TUIC_WRAP_USER_FN_CLAUDE", "skip")][..], &[][..]] {
                let out = run(USER_FN, env);
                assert_eq!(out, "USER:--model opus\n", "env {env:?}");
            }
        }

        #[test]
        fn a_hostile_function_body_is_fingerprinted_not_executed() {
            let marker =
                std::env::temp_dir().join(format!("tuic-consent-pwned-{}", std::process::id()));
            let _ = std::fs::remove_file(&marker);
            let hostile = format!(
                // Quotes, `$(...)`, `;` and a BEL inside the body: none of it
                // may reach the OSC or run while the shell fingerprints it.
                "claude() {{ print -r -- \"USER:$*\"; : '$(touch {m})' ';\\a'; }}",
                m = marker.display()
            );
            let out = run(&hostile, &ask_env());
            assert!(out.contains("USER:--model opus\n"), "{out:?}");
            let fp = asked_fingerprint(&out).expect("must ask");
            assert!(
                crate::shell_integration::is_user_function_fingerprint(&fp),
                "{fp:?}"
            );
            assert!(
                !marker.exists(),
                "fingerprinting must never run the function body"
            );
        }
    }
}
