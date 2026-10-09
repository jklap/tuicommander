#!/usr/bin/env bash
# TEMPORARY debug instrumentation — logs the full stdin payload Claude Code
# delivers to a given hook event, so we can see ground truth for whether a
# hook actually fired (and with what data) independent of tuic-hook's own
# OSC output. Safe to delete this file, hook-debug.log, and the "hooks" key
# in settings.local.json once done investigating.
#
# ONE log for every worktree, on purpose: Claude Code resolves project
# settings for a worktree back to the shared repo (confirmed empirically
# 2026-08-30), so the hook config is invoked from every worktree's sessions,
# and a log fragmented per worktree is less useful for cross-session
# troubleshooting. The log therefore lives in the MAIN checkout's `.claude/`,
# found from this script's own location via `git rev-parse --git-common-dir`
# (works from a linked worktree too) — no personal absolute path. Override
# with HOOK_DEBUG_LOG=/path/to/log; if git cannot answer, the log goes next to
# `$CLAUDE_PROJECT_DIR/.claude/` (else this script's own checkout). The
# hook-log-analysis skill's hooklog.py resolves the same path the same way.
set -euo pipefail

event="${1:-unknown}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
default_logfile() {
  local common
  if common="$(git -C "${script_dir}" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" \
    && [[ -n "${common}" ]]; then
    printf '%s/.claude/hook-debug.log\n' "$(dirname "${common}")"
  else
    printf '%s/.claude/hook-debug.log\n' "${CLAUDE_PROJECT_DIR:-$(dirname "$(dirname "${script_dir}")")}"
  fi
}
logfile="${HOOK_DEBUG_LOG:-$(default_logfile)}"
retention_days=7
payload="$(cat)"

# Daily rotation: if the live log was last written on an earlier day, archive
# it as hook-debug.log.YYYY-MM-DD (the day it was last written) and prune
# archives older than ${retention_days} days. Pruning only runs on rotation.
# Errors are swallowed (a concurrent hook may have rotated first) so a
# rotation hiccup never breaks the hook or loses the current entry.
if [[ -f "${logfile}" ]]; then
  log_day="$(stat -f '%Sm' -t '%Y-%m-%d' "${logfile}" 2>/dev/null || true)"
  today="$(date '+%Y-%m-%d')"
  if [[ -n "${log_day}" && "${log_day}" != "${today}" ]]; then
    mv -n "${logfile}" "${logfile}.${log_day}" 2>/dev/null || true
    find "$(dirname "${logfile}")" -maxdepth 1 -name "$(basename "${logfile}").*" \
      -mtime "+${retention_days}" -delete 2>/dev/null || true
  fi
fi

{
  printf '\n===== %s | %s | %s | tuic_session=%s =====\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$event" "${CLAUDE_PROJECT_DIR:-unknown}" "${TUIC_SESSION:-none}"
  printf '%s\n' "$payload"
  # Hook-process env (CLAUDE_*/TUIC_*/TMUX*) to find any parent/team linkage the
  # payload lacks. Credential-looking names are excluded so they never hit the log.
  printf '%s\n' "--- env ---"
  env | grep -E '^(CLAUDE|TUIC|TMUX)' | grep -viE '(TOKEN|KEY|SECRET|PASSWORD|CREDENTIAL)' | sort || true
} >> "${logfile}"

exit 0
