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

### Smart Prompts `icon` field is a display bug, and has no UI to set it
- **Problem/opportunity:** Every built-in Smart Prompt carries an `icon: string` (e.g.
  `"git-commit"`, `"sparkle"`, `"shield"` — 24 unique names across 27 prompts, set as a
  required positional argument to the `builtin()` helper in
  `src/data/smartPromptsBuiltIn.ts`), but nothing in the codebase ever interprets those
  names as icons. The only place `.icon` is read anywhere is
  `src/components/SettingsPanel/tabs/SmartPromptsTab.tsx:590`, which prints the raw string
  as text inside a 20px muted-gray box (`SmartPromptsTab.module.css:71-77`, sized for one
  glyph) — the surrounding comment literally calls it a "placeholder." So today it reads
  the word "sparkle" instead of a sparkle glyph. Separately, neither prompt editor
  (`SmartPromptsTab`'s `PromptEditor` nor `PromptDrawer`'s `PromptEditor`) exposes an icon
  field, and `promptLibraryStore.createPrompt()` never defaults one — so every custom
  prompt has `icon: undefined` permanently, with no way for a user to ever set one.
- **Proposed solution:** Build a small icon-name → glyph lookup (à la `AgentIcon.tsx`'s
  `AGENT_PATHS` map, but keyed by the free-form icon-name strings instead of a closed
  union) and render through it at the one call site instead of the raw string. Two open
  decisions before implementing, deliberately left open rather than pre-decided:
  1. **Scope** — fix the built-in display only (small, self-contained), or also add a
     picker to both prompt editors so custom prompts stop being permanently icon-less —
     modeled on `ColorSwatchPicker`/`colorPresets.ts` (`src/components/shared/`), the
     existing "pick one of N fixed presets, active-state highlight, click to select"
     pattern already used for accent color and per-repo sidebar color. *Leaning toward
     doing both* — fixing only the built-ins would make the asymmetry more visible, not
     less: built-ins get real icons while every custom prompt shows a blank gap next to
     them.
  2. **Glyph style** — the render context (`.promptIcon`) is explicitly
     `color: var(--fg-muted)` at `font-size: var(--font-xs)`, which only affects
     monochrome/text-presentation Unicode glyphs, not full-color emoji (emoji ignore CSS
     `color`). Monochrome-only (e.g. `●` `↑` `✎` `✦` `◎` `✓` `▶` `⚙` `⚠`) stays visually
     consistent with the row's other muted chrome and can reuse two symbols already
     established elsewhere in the app (Toolbar's `▶` for "running", `⚠` for PR conflicts).
     Mixing in a few colorful emoji where no decent monochrome symbol exists (🔍 magnifier,
     👀 review, 🛡 shield) is more instantly recognizable at the cost of a few icons not
     matching the row's muted styling. *Leaning toward monochrome-only* for visual
     consistency with the badges/text already in that row.
- **Expected benefits:** Fixes a visibly broken settings screen (raw words instead of
  icons); if the picker is included, closes a real feature gap (custom prompts can never
  be visually distinguished today) and removes the future asymmetry of "built-ins look
  nice, customs look blank."
- **Trade-offs:** The glyph map is a values judgment with no objectively correct answer —
  reversible, but worth a quick look before landing. Adding the picker touches two editor
  components (`SmartPromptsTab.tsx`'s and `PromptDrawer.tsx`'s `PromptEditor`) plus
  `createPrompt`/`updatePrompt` call sites, roughly doubling the change size versus the
  display-only fix.
- **Estimated complexity:** S (display fix only) or M (display fix + picker in both
  editors).
- **Recommended priority:** P3 (cosmetic; not a regression, the field was always
  unwired).

### tuic-hook's DERIVATIONS lookup is not scoped per agent
- **Problem/opportunity:** `tuic-hook`'s `hook_event_name`-based derivation
  (`src-tauri/crates/tuic-hook/src/main.rs`'s `DERIVATIONS` table) matches purely on
  the event-name string, with no awareness of which agent (Claude/Gemini/Grok/Codex)
  invoked it. Gemini's own event names "Notification" and "SessionEnd" are spelled
  identically to two Claude `DERIVATIONS` entries. Currently harmless in practice
  because Gemini's `agent_hook.rs` map still passes an explicit `--state` (which wins
  over derivation), but if a future Gemini payload shape turns out to include a
  `hook_event_name` field — unverified either way today — "Notification" would also
  start emitting a `notify` scrape Gemini's map never asked for, silently contradicting
  `main.rs`'s own doc comment that non-Claude agents "fall back to flags exactly as
  before derivation existed." Locked down as a regression test
  (`agent_hook.rs::golden_wire_output::gemini_notification_name_collision_with_claude_derivations_currently_leaks_a_scrape`)
  documenting current behavior, not a fix.
- **Proposed solution:** Either (a) scope `DERIVATIONS` lookups by a per-agent prefix/
  namespace passed via a new flag, or (b) confirm Gemini/Grok/Codex hooks never send
  `hook_event_name` and assert that in the same test, converting the "currently leaks"
  framing into "structurally can't happen."
- **Expected benefits:** Removes a latent cross-agent behavior leak before any agent
  other than Claude is confirmed to send `hook_event_name`.
- **Trade-offs:** Low urgency while no non-Claude agent has been confirmed to send this
  field; (a) adds a small amount of plumbing for a currently-hypothetical case.
- **Estimated complexity:** S–M.
- **Recommended priority:** P3 (documented, tested, no live impact today).

