# Agent-work triage — 2026-10-20

## Scope

Reconcile the three items in section 1 of the orchestrator's 2026-10-19 Boss-wait triage report: GitHub traffic (1995-279a), ego resize width (573-bd86), and keyboard-to-paint latency (2040-16b2). Do not restart Boss's app, capture its desktop pixels, start another desktop instance, or close criteria that still need native evidence.

This is an evidence report, not a production change. Story worklogs were updated through the Wiz CLI in each main checkout. The assigned worktrees use the coordinator-approved branch `chore/triage-3`.

## 1995: already measured and complete

The main-store story is already complete. The committed report [GitHub toolbar and request volume](../../reports/tuicommander/2026-10-18-github-toolbar-and-request-volume.md#joined-live-account-observation-2026-10-19) records the joined production browser App, HTTP router, poller and reqwest client observation. This is stronger evidence than inferring counts from configured intervals.

| Observed phase | Window | REST | GraphQL | GraphQL per minute |
| --- | ---: | ---: | ---: | ---: |
| Visible idle app, no PR panel | 60.002623 s | 0 | 1 batch | 0.99996 |
| Open GitHub panel, expand one PR, then leave mounted | 63.321365 s | 0 | 2: PR action state and CI checks | 1.89509 |

The recorded workload used one account and the public `tauri-apps/tauri` repository. Three cold-start queries (two concurrent viewer lookups and one batch) preceded the idle window. Two other batch arrivals occurred outside the numeric windows. All eight recorded upstream replies were HTTP 200.

The source still defines a 60-second visible base interval and 120-second hidden interval. After 15 minutes without changes, hot repositories use every fifth tick; dormant repositories use every tenth tick with jitter. Budget pressure and failure backoff can increase cadence intervals. Panel actions and additional pagination can create more requests: these intervals are not an account-wide request ceiling.

Source: `src-tauri/src/github_poller.rs` constants and `tiered_paths` / `current_interval`. Existing fixture evidence covers 403 and 429 pre-send backoff for REST and GraphQL. It does not prove that other applications cannot exhaust the same account budget, or that every secondary-limit condition is impossible.

No new GitHub request or credential access was necessary. This peer re-read the committed report, story worklog and source. The report's original active-worktree artifact path is no longer present at the tested path; this peer does not claim to have revalidated its raw receiver trace. The committed report and detailed original worklog remain the evidence cited here.

`story-close.sh --check` passed for 1995-279a without writing anything. No duplicate close was performed.

## 573: landed regression and viewport replay pass

The ego story is already `wontfix` by the coordinator's reversible not-reproducible decision. Its regression commit `8b2f28ec` is an ancestor of master `d6927ba517c271644a9150489db30e20deb89549`.

A fresh rb run of `live_width::tool_rows_follow_real_pty_growth_and_shrink` passed: one selected test, 4.76 seconds, exit 0, `stale=no`, `box_dirty=no`. It launches a real PTY and checks completed tool rows at 80, 200 and 60 columns. Log: `~/Gits/.tmp/results/triage-3-resize-regression.log`.

The companion ego report is `docs/internals/2026-10-20-resize-width-triage.md` on ego branch `chore/triage-3`. A native worktree binary built through build-slot/mbx in 1 minute 28 seconds. Real PTYs started at 80 columns, then changed to 47 and 173; completed tool headers used exactly 47 and 173 cells. A named stealth-wrapper browser session replayed those actual captured headers through the production grid renderer at 390×844 and 1400×900. The retained JSON records matching viewport/column/header widths, and matching PNG screenshots exist under `~/Gits/.tmp/triage-3-width/`. Controlled metrics are 8-pixel cells with 12 pixels of horizontal allowance. Own Vite, native children, loopback stub and named browser resources were stopped. A renderer replay cannot prove the normal full-app resize-dispatch path or native WebKit presentation; those distinctions remain explicit.

## 2040: prerequisites landed; native paint observation still missing

Git ancestry confirms that ego 571 (`e30e0e38`) and 574 (`b1d07ab2`) are already on master. Waiting for these source fixes is obsolete.

Existing measurements are not keyboard-to-paint measurements:

- Story 574: 371.629042 ms from TUIC submission to the final help row in session output; 100.912203 ms for draft echo during long output in its real-PTY test.
- Story 2040: direct cursor-position response median 0.01215 ms and application echo p95 0.12029 ms in TUIC.
- The landed discovery regression measures kernel PTY echo and independent frame consumption during an 800 ms filesystem delay. It does not observe presentation of a glyph.

A valid native measurement needs a trusted input timestamp, identification of the matching glyph update, and a native WebKit presentation observation. The current assignment forbids desktop pixels/interaction on Boss's instance and a second desktop instance. A worktree Vite browser replay can measure the browser renderer, not settle that native criterion. No synthetic draw timing is substituted for native keyboard-to-paint.

The latest existing story worklog already records a post-restart runtime and absence of discovery in a desktop main-thread sample. Therefore another restart is not demonstrated to be the remaining prerequisite. The remaining gap is the authorized native input/presentation observation itself. The original native GREEN and REALITY stay unchecked, and 2040 remains open.

## Story-close reconciliation

Initial preflight results:

- 1995-279a: pass.
- 573-bd86: refuses the original unchecked RED; the bug did not reproduce.
- 2040-16b2: refuses the original unchecked generic RED, despite later checked named regressions. Native GREEN and REALITY also remain unproved.

The coordinator authorized rejection of 573's original unreproduced RED and addition of a narrow verification GREEN; the `wontfix` status stays unchanged. The coordinator also authorized rejection of 2040's generic RED as superseded by its checked named renderer and discovery regressions. Its native GREEN and REALITY stay unchecked and the story stays blocked.

The named owner/prerequisite for the remaining 2040 work is an isolated `make agent-test` after story 2086 lands, or Boss's authorized native observation. This peer will not fabricate a RED, relabel unproved native criteria as passed, or change a creator's criterion to human-only merely to make a close gate pass.

## Validation scope and operational limits

This peer ran one targeted Rust regression on rb and one explicitly authorized native ego binary build for exploration, not a broad test suite. Native PTY and browser replay drivers both exited 0 after the initial retained probe-readiness failure was corrected. No paid model request, GitHub request, token read, production frontend change, new committed capture, installed-binary replacement, main/master source edit, merge or push occurred.

A read-only request for worktree warm status through the desktop HTTP API returned 401 Unauthorized; no credentials were read and no bypass attempted. The evidence-only Vite replay used the already available worktree dependencies. The authorized story GREEN addition also encountered the terminal-story CLI refusal described in the ego report; no criteria success is inferred from a failed command.

Final coordinator disposition: do not reopen terminal story 573 to add criteria; retain its current criteria and record the successful verification in its worklog only. Final preflight passes 1995, refuses 573 original broad GREEN, and refuses 2040 native GREEN. These refusals are explicit limitations, not test failures to suppress. Story states remain complete / wontfix / blocked respectively.
