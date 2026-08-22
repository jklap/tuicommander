# TODO — deferred improvements & ideas

Items surfaced during work that are larger than an in-line fix, need discussion,
or carry non-trivial risk. Immediate low-risk improvements are done in place, not
listed here.

## Open

### `get_github_remote_url` uses a loose `contains("github.com")` substring filter
- **Problem/opportunity:** `github.rs:2030` decides "is this a github.com repo?" via
  `url.contains("github.com")`. That is a substring check, so a remote like
  `https://github.company-internal.example/...` or any URL that merely contains the
  literal `github.com` anywhere passes. The security impact for the four parse sites is
  already mitigated (story 119-3150 added an `is_cloud()` gate in `parse_remote_url`, so a
  spoofed host now yields `None`), but the upstream filter itself is still imprecise.
- **Proposed solution:** Replace the substring test with host-aware parsing —
  reuse `github_account::parse_remote_url(url)` and check `host.is_cloud()` — so the
  filter and the parser share one host definition (single source of truth).
- **Expected benefits:** Removes the last loose host check; eliminates a class of
  false-positive "github.com" matches; one canonical host rule across the module.
- **Trade-offs:** `get_github_remote_url` becomes slightly heavier (full parse vs
  substring). Negligible — it already parses the URL immediately after.
- **Estimated complexity:** S (single function + a couple of unit tests).
- **Recommended priority:** P3 (correctness/DX polish; not a live vulnerability given the
  119 gate).

### Command blocks: three items explicitly deferred during the turn-level rewrite, never filed
- **Problem/opportunity:** The session that shipped `9ceae216` (turn-level command
  blocks from the idle↔busy edge) explicitly named three follow-ups as "deferred, not
  part of this fix" and never filed them anywhere in-repo — they exist only in an
  ephemeral local `~/.claude/plans/` file:
  1. Narrowing `claude_hook_map()`'s `Notification` matcher (`agent_hook.rs`) — currently
     an unscoped, generic match; needs Claude Code's regex dialect confirmed first, and
     risks silently breaking a shared global `settings.json` if done carelessly.
  2. A WS/remote OSC 133 broadcast path — risks reintroducing the double-dispatch bug an
     earlier fix (F3, pre-`BASE`) removed, unless gated by `isTauri()`.
  3. Rewriting the `⏺`-heuristic pattern (`is_cc_tool_call_header` in `pty.rs`) — no
     reliable syntactic marker currently exists for it; open-ended.
  Separately: Gemini and Grok still fire a "busy" hook on every tool call
  (`gemini_hook_map()`'s `("BeforeTool", "", ...)`, `grok_hook_map()`'s
  `("PreToolUse", "", ...)`) — the exact green-tick-scrollbar-pollution pattern
  `9ceae216` deliberately removed for Claude via `claude_map_has_no_broad_pretooluse_busy_entry`,
  never extended to the other two agents.
- **Proposed solution:** Each item needs its own design pass; for the Gemini/Grok
  pollution specifically, the fix is likely structurally identical to what `9ceae216`
  already did for Claude, pending verification that Gemini/Grok's busy/idle semantics
  don't depend on the broad per-tool-call fire for some other reason.
- **Expected benefits:** Closes a known cosmetic bug (Gemini/Grok green-tick pollution)
  and unblocks three named improvements that currently exist only in a file no future
  contributor would find.
- **Trade-offs:** None of the three deferred items are urgent; the Gemini/Grok fix is
  small but unverified against those agents' actual hook payload shapes.
- **Estimated complexity:** S (Gemini/Grok fix) to M (the other three).
- **Recommended priority:** P3 (Notification/WS-broadcast/heuristic), P2 (Gemini/Grok
  green-tick pollution — user-visible, if minor).

### Whether Claude Code's alt-screen use inside TUICommander is screen-reader- or fullscreen-gated is unresolved
- **Problem/opportunity:** During the command-blocks investigation, it was observed
  that Claude Code runs in alt-screen mode both normally and when forcing
  `{"tui":"scrolling"}` — so it's unknown whether the gate into alt-screen is
  screen-reader-mode-based or fullscreen-rendering-based. This matters: if it's
  fullscreen-gated, nudging Claude Code out of fullscreen would give TUICommander real
  per-turn boundary markers for every session, which would beat the `⏺`-heuristic
  fallback outright and could retire a whole class of heuristic-detection bugs.
- **Proposed solution:** Investigate CC's alt-screen entry conditions directly (env
  vars, TTY capability probing) before any further block-detection work; this was
  flagged as "the one finding that could still change the plan's shape."
- **Expected benefits:** Could eliminate the `⏺`-heuristic fallback entirely for
  Claude Code sessions, closing several existing heuristic-detection edge cases at once.
- **Trade-offs:** Investigation-only until resolved; no code change implied yet.
- **Estimated complexity:** S (investigation) — unknown for the follow-up work.
- **Recommended priority:** P2 (could reshape the block-detection plan).
