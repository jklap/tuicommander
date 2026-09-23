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
payload="$(cat)"

{
  printf '\n===== %s | %s | %s =====\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$event" "${CLAUDE_PROJECT_DIR:-unknown}"
  printf '%s\n' "$payload"
} >> "$logfile"

exit 0
