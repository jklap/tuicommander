# Mobile session menu verification — 2349-f6f8

## Scope and decision

The implementation is commit `b7932828d7`. The branch merges `main` before handoff.
Boss rejected backward compatibility for old phone clients and remote hubs on
2026-10-26. Critic finding F1 is closed by that decision; no fallback was added.

## Pre-fix replay

The replay used the production files from
`f7cb0fd31da0ccd555950688566c5d56e8b50f0c` with the committed regression tests.
The replay restored the fixed production files before the GREEN run.

- `build-slot.sh node scripts/check-mobile-session-menu.mjs <evidence>` failed
  with `menu extends below available session pane`. At 390×844, the containing
  pane ended at 810, but the menu ended at 864. Copy session ID ended at 815;
  Terminate session ended at 859. The fixture readiness condition waited for the
  final action instead of new quota text, so missing quota could not mask the
  layout failure.
- `build-slot.sh scripts/with-test-tmp.sh pnpm vitest run
  src/mobile/__tests__/SessionDetailScreen.usage.test.tsx -t 'shows structured quota'`
  failed because `Claude Max plan, 5-hour window: 2% used` was absent. The old
  component rendered `Model unavailable` instead.
- On rb, `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run
  --manifest-path src-tauri/Cargo.toml --lib -E
  'test(menu_deduplicates_absolute_paths_without_collapsing_worktree_files)'`
  failed with actual ids `first, duplicate, worktree` instead of `first, worktree`.

Evidence: `~/Gits/.tmp/2349b-red-green.log` and
`~/Gits/.tmp/2349-red-menu/geometry.json`.
These are expected assertion failures, not compile failures.

## GREEN checks

Native Mac checks ran through `build-slot.sh` and `scripts/with-test-tmp.sh`:

- `sessionUsageFormatting.test.ts`, `SessionDetailScreen.usage.test.tsx`,
  `OpenMarkdownTabs.test.tsx`, and `ClaudeUsageDashboard.test.tsx`: 26 tests passed,
  four files, 4.66 seconds.
- `CodexUsageDashboard.test.ts`: 16 tests passed, one file, 2.12 seconds.
  The first command used `.tsx` for this file and did not select it; the separate
  run corrects that omission. Its existing error-recovery test emitted the known
  `[network] Codex usage fetch failed` line on stderr.
- Instruction-link validation passed. `fmt-changed.sh <worktree> main...HEAD`
  passed with no fixes.

Logs: `~/Gits/.tmp/2349b-red-green.log` and
`~/Gits/.tmp/2349b-codex-green.log`.
The critic already recorded GREEN rb evidence for `session_usage` and
`open_markdown_tabs` in `~/Gits/.tmp/critic-2349-r1-checks.log`; these modules and
their dependencies did not change in the merges. No new full-suite run is claimed.

## Live isolated instance

`make agent-test AGENT_TEST_INSTANCE=agent-test-2349b AGENT_TEST_PORT=19876`
built the native Mac application and embedded frontend. The browser opened
that instance's mobile session route at 390×844. No daily TUIC endpoint or Vite
dev server was used. A throwaway repository and PTY supplied 16 Markdown entries.

At maximum scroll:

| Element | Top | Bottom |
| --- | ---: | ---: |
| Containing session pane | 0 | 844 |
| Session menu | 56 | 836 |
| Copy session ID | 743 | 787 |
| Terminate session | 787 | 831 |

The menu had `scrollHeight=1236`, `clientHeight=778`, and `scrollTop=458`.
Both final actions were fully inside the menu and containing pane.
Menu-only screenshots and machine-readable evidence:

- `~/Gits/.tmp/2349b-live/menu-top.png`
- `~/Gits/.tmp/2349b-live/menu-bottom.png`
- `~/Gits/.tmp/2349b-live/geometry.json`
- `~/Gits/.tmp/2349b-live/transcript.json`
- `~/Gits/.tmp/2349b-live/usage.json`

### Limits

The backend detected the real Claude process, but the account usage source
returned `session usage source unavailable`. The live menu correctly displayed
`Usage not available`, not a measured zero or raw timestamp. This check does
**not** prove live provider percentages or reset times. Provider/plan/window
wording and client-local reset formatting are covered by the focused tests and
the earlier isolated quota fixture, not authenticated provider access.
Screenshots were saved; this agent's text-only reader did not visually inspect
PNG pixels. Geometry and rendered text were checked through the browser DOM.

The browser tab and its named daemon were closed. The agent-test supervisor
exited with status 0 and closed its throwaway PTY. No agent-test process remains.
Rust changes still require Boss to rebuild/restart the daily application when
he is ready; agent-test does not update that running application.
