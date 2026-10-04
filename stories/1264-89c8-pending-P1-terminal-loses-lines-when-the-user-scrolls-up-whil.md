---
id: 1264-89c8
title: Terminal loses lines when the user scrolls up while an agent is writing
status: pending
priority: P1
type: fix
created: "2026-09-29T17:34:15.098Z"
updated: "2026-10-04T15:13:15.582Z"
dependencies: []
started_at: "2026-09-30T11:31:18.877Z"
---

# Terminal loses lines when the user scrolls up while an agent is writing

## Problem Statement

Boss, 2026-09-29 ~19:40, COORDINATOR Claude Code session in the running make dev app (backend older than main 6696321d0): while the terminal was printing, scrolling up a little showed lines missing from the output. Boss believed it already fixed; no open or completed story matches (834-1878 fixed DL/SU manufacturing EXTRA scrollback rows, the opposite symptom). Unknown: whether rows are lost in the backend grid/scrollback, in the frontend viewport render while not pinned to the bottom, or at the scrollback cap trim.

## Acceptance Criteria

- [x] RED: a test that writes N lines while the viewport is scrolled up (frontend or backend layer, whichever is proven) - catches: rows skipped or overwritten while the user is not at the bottom
- [x] Root cause layer named with evidence (backend grid, scrollback trim, or frontend viewport)
- [x] GREEN: every written line is present in order after scrolling back, including at the scrollback limit
- [ ] [VISUAL] Boss scrolls up during agent output in the running app and sees no gap
- [x] RED: CanvasTerminal-scroll-during-output.test.tsx 'keeps the lines under the user when output lands mid-gesture' - catches: gesture offset not rebased when output grows history, so the second wheel-up sent offset 4 instead of >7 (verified failing on bdbd763c1)
- [x] GREEN: same test plus 6 followHistory attack tests in canvasTerminalScroll.test.ts pass (18/18) at 249b2769e; backend test output_while_scrolled_back_keeps_the_viewport_lines passes on the box
- [x] RED: CanvasTerminal-scroll-during-output.test.tsx 'shows the committed lines, not the live-region rows cached before they were committed' - catches: chunk cached while lines were live keeps stale rows after the redraw commits them (failed with V102,V103 painted)
- [x] GREEN: same test + 6 canvasTerminalScroll commitLiveRows/cacheFetchedRows tests pass; src/components/Terminal + CanvasTerminal suites 293/293 at 96d446c7b
- [x] GREEN: tuic-terminal history_rows_never_change_while_scrolled_up_over_real_captures, history_rows_survive_the_scrollback_cap_over_real_captures, history_content_survives_a_resize_during_redraw_over_real_captures, alt_screen_round_trip_leaves_history_untouched pass on the rb box (16/16 with the history_ filter, 06fd6f669 tree): real .tcap bytes, viewport held k=0,1,3,rows/2,rows,rows+7 rows up, no committed history row ever changes, none missing, cap and shrink/grow resize and alt-screen included. Backend is correct; no backend RED exists
## Proof

- [x] [completeness] Completeness (Wheel gesture path （render, flush, settle） rebased; seed uses rebased offset. 18/18 vitest; 5 manual mutants killed.)
- [x] [feature-availability] Feature availability (Change is in the always-on CanvasTerminal onFrame path; no flag.)
- [x] [robustness] Robustness (Attack tests: offset 0 follows tail, cap clamp, stale settle target, unsent pending offset, backend already agreeing, backend missing output.)
- [ ] [resilience] Resilience (In-flight offset race under sustained output can still hand off k lines forward via the 400ms settle fallback; needs a backend history-anchored scroll command.)
- [~] [security] Security (UI scroll state only; no input crosses a trust boundary.)
- [~] [defense-in-depth] Defense in depth (Single-layer viewport fix; backend already keeps the viewport stable.)
- [x] [input-validation] Input validation (grownLines<=0 and null/zero position are no-ops; result clamped to historySize; resize/alt-screen frames pass 0 growth.)
- [~] [thread-safety] Thread safety (Single-threaded webview code; backend untouched.)
- [~] [configurability] Configurability (Terminal contract behaviour, not a preference.)

## Work Log

### 2026-09-29T17:45:16.709Z - Root cause: frontend viewport (smooth-scroll gesture). Backend grid is faithful: alacritty raises display_offset by each line of output while scrolled back (patches/alacritty_terminal/src/grid/mod.rs:344); proven by terminal_grid test output_while_scrolled_back_keeps_the_viewport_lines (box, 1 passed). The gesture kept scroll.position as distance-from-bottom, so renderSmooth painted hist-position (slid forward) and flushScroll sent the stale offset (backend scrolled forward over the new output). Fix 249b2769e: canvasTerminalScroll.followHistory rebases position/pendingOffset/settleTarget by the all-time history growth of each accepted frame (offset 0 keeps following; clamp to historySize). Criterion 3 left unchecked: the tests prove the top line never jumps forward and the cap clamp at unit level, not a literal every-line-in-order sweep at the cap. Residual: an offset in flight when output lands arrives k lines low; followHistory resends on the next frame, but under sustained output the 400ms settle fallback can hand off k lines forward. Scrollbar drag has the same stale-offset class (scrollDragStartOffset) - follow-up story.


### 2026-09-29T17:45:17.411Z - Proof completeness set PROVEN: Wheel gesture path (render, flush, settle) rebased; seed uses rebased offset. 18/18 vitest; 5 manual mutants killed.

### 2026-09-29T17:45:18.017Z - Proof feature-availability set PROVEN: Change is in the always-on CanvasTerminal onFrame path; no flag.

### 2026-09-29T17:45:18.324Z - Proof robustness set PROVEN: Attack tests: offset 0 follows tail, cap clamp, stale settle target, unsent pending offset, backend already agreeing, backend missing output.

### 2026-09-29T17:45:18.840Z - Proof resilience set UNPROVEN: In-flight offset race under sustained output can still hand off k lines forward via the 400ms settle fallback; needs a backend history-anchored scroll command.

### 2026-09-29T17:45:19.134Z - Proof security set NOT_APPLICABLE: UI scroll state only; no input crosses a trust boundary.

### 2026-09-29T17:45:19.739Z - Proof defense-in-depth set NOT_APPLICABLE: Single-layer viewport fix; backend already keeps the viewport stable.

### 2026-09-29T17:45:20.074Z - Proof input-validation set PROVEN: grownLines<=0 and null/zero position are no-ops; result clamped to historySize; resize/alt-screen frames pass 0 growth.

### 2026-09-29T17:45:20.431Z - Proof thread-safety set NOT_APPLICABLE: Single-threaded webview code; backend untouched.

### 2026-09-29T17:45:21.010Z - Proof configurability set NOT_APPLICABLE: Terminal contract behaviour, not a preference.

### 2026-09-30T10:12:19.238Z - Criterion 3 evidence: tuic-terminal terminal_grid::tests::scrolled_back_output_leaves_every_retained_line_in_order_at_the_cap passes (cap 10, 60 lines, output while scrolled back, sweep top row per offset = l47..l59 in order, no dedup). Branch fix/1264-scroll-lines 1b70d313b. Criterion 4 [VISUAL] and proof resilience remain open (desktop restart / backend history-anchored scroll, see 1265).

### 2026-09-30T10:12:56.367Z - Test landed; criterion 4 [VISUAL] is to-test after the TUIC restart; resilience proof waits on 1265.

### 2026-09-30T11:31:18.605Z - 2026-09-30 13:3x Boss: lines lost again while scrolled up, in the running app started 21:20 on 29/09 (after merge 691ca3a05 20:24), so the 249b2769e fix is present and insufficient. Verified gap vs the Claude Code transcript 40c2444d: between the backgrounded 'Bash(mail-watch.sh)' row and the '- D1:' list, the app shows 2 blank lines; missing are 3 MCP tool rows (session close, ui tab, progress) and the first text line 'Boss, ti ho aperto il tab ...'. Screenshot in the coordinator conversation.

### 2026-09-30T11:45:49.696Z - 2026-09-30 reopen: root cause frontend row cache. fetchChunk caches whole 64-row chunks incl. live screen rows and marks the chunk requested forever; when the agent's redraw commits those rows to history the cache keeps the pre-commit content (stale/blank) - matches Boss's gap (blank rows + missing tool rows). Fix 96d446c7b on branch fix/1264-live-region-scroll: commitLiveRows drops committed rows + releases chunks; cacheFetchedRows discards in-flight stale rows. Not replayed from a real .tcap; reproduced with a model of live rows -> committed rows through the real CanvasTerminal.

### 2026-09-30T13:16:39.972Z - Live 2026-09-30: backend check on scratch session (400 numbered lines + cursor-up spinner redraws every 5th, API scroll deltas mid-output): 400/400 lines present, in order, DONE marker present, 563 total lines. Canvas render (VISUAL criterion) NOT observed; left unchecked.

### 2026-09-30T13:28:35.178Z - Live 2026-09-30 second pass: could not reach the canvas render in web mode within budget (no scratch tab created); VISUAL criterion still unchecked.

### 2026-09-30T14:22:24.989Z - Backend corpus test landed on fix/1264-inplace-rows (06fd6f669): all committed .tcap fixtures replayed with viewport held k rows up; history rows never change, none missing, in order; cap (10) over one concatenated session, rows-only resize (grow keeps content, shrink keeps history prefix), alt-screen round trip. rb box 16/16 green. Backend correct, so no RED against pre-fix backend. Notes: agents erase scrollback (ESC[3J: base 34 history 0 seen in codex-0.157.1-approval-cancel.tcap), shrinking drops non-blank rows below the cursor by design. Ignored full-corpus run (TUIC_HISTORY_CORPUS, operator captures) not run: policy limits to one rb run before commit and the corpus is not on the box.

### 2026-09-30T14:23:18.997Z - 2026-09-30 16:23 new live occurrence in the COORDINATOR session (Claude Code, main instance): a 16-line block vanished from history while Claude kept writing below it. Lost: the lines from the 'Alternative' heading's items A, B and the start of C; the tail of C ('dura decine di secondi, non minuti.') and items D, E survived right after the heading. Screenshot: /private/tmp/claude-501/-Users-stefano-straus-Gits-personal-orchestrator/40c2444d-b9bb-4344-9469-acc603523a70/images/48.png. Pattern: a contiguous block missing in the middle, not at the edges.

### 2026-10-02T00:26:29.748Z - 2026-10-02 landed on main a5fa16217 (resize shrink keeps every row; reprint dedupe after 3 critic rounds, CLEAN). Box batch at a5fa16217: vitest 7997/7997, --lib 4443/4443, dictation+git+terminal+alacritty 1912/1912, tuic-remote check ok. Open: [VISUAL] live scroll check, resilience criterion.

### 2026-10-03T08:11:35.289Z - Blitz: remaining AC4 explicitly requires Boss visual check in the running app. Backend/corpus evidence already present; no new test can substitute for the creator VISUAL criterion. No desktop instance launched.

### 2026-10-04T15:13:15.021Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Blitz: remaining AC4 explicitly requires Boss visual check in the running app.

