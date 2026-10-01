#!/usr/bin/env bash
# TEMPORARY debug instrumentation — logs the full stdin payload Claude Code
# delivers to a given hook event, so we can see ground truth for whether a
# hook actually fired (and with what data) independent of tuic-hook's own
# OSC output. Safe to delete this file, hook-debug.log, and the "hooks" key
# in settings.local.json once done investigating.
#
# Fixed absolute paths on purpose, NOT ${CLAUDE_PROJECT_DIR}-relative: this
# hook config applies repo-wide across every git worktree of tuicommander
# (confirmed empirically 2026-08-30 — Claude Code resolves project settings
# for a worktree back to the shared repo, not the per-worktree checkout), but
# this script only physically exists in the main checkout. A path built from
# $CLAUDE_PROJECT_DIR resolves correctly per-session (that part isn't broken)
# but then points at a copy of this script that doesn't exist in whichever
# worktree fired the hook, which is exactly what produced the "No such file
# or directory" errors surfacing in unrelated worktree sessions. Fixed paths
# also mean every worktree's hook activity lands in ONE log, which is more
# useful for cross-session troubleshooting than a log fragmented per worktree.
set -euo pipefail

event="${1:-unknown}"
logfile="/Users/jason.klapste/src/external/tuicommander/.claude/hook-debug.log"
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
} >> "$logfile"

exit 0
