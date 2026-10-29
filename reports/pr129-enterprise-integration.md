# PR 129: Claude enterprise usage integration

## Contract and source audit

The Extra Usage card must distinguish an unknown percentage from a measured zero. It must retain available credit totals when utilization is null. Production state comes from `get_claude_usage_api`; the dashboard reads `extra_usage.utilization`. The backend already preserves missing utilization as `None`. No live Anthropic account was queried for this integration. The contributor's recorded enterprise response remains the external-schema evidence.

Before integration, source at `b028a0f340` used `extra()?.utilization ?? 0`, then rendered either `0%` or `0.0% used`. This is decisive evidence of the false-zero defect. The imported component regressions cover null utilization with and without credit details. Two additional cases protect measured zero in those same branches. Existing tests cover populated enterprise usage, hidden rate-bucket precedence, priority thresholds and disabled extra usage.

The worst case is the two existing rendering branches: null with credit totals must show totals and an unavailable-percentage label; null without totals must show Unavailable. A real zero must remain zero in both branches. No polling, cache, transport or account-selection behavior changes.

## Integration decisions

- Work starts from local main `1e111aa5cd6da0a73b63d90579dfda296a30c341`, plus its managed-worktree start commit `b028a0f340`.
- Fetch used the SSH origin and `pull/129/head`, yielding `bc7d12c02dd71ffc65c92f985781c8ff00fd2918`.
- Merge the PR head, rather than duplicate its commits. This preserves Jason Klapste's original authorship and commit IDs. Existing upstream commit metadata is retained, not rewritten.
- Local Git found no textual conflicts, despite the earlier GitHub CONFLICTING status. Main already contains the first enterprise fallback commit `f234d92f99` and its Rust parser tests. The net production change is the remaining dashboard null handling.
- Preserve current main's session-scoped requests, stale-profile response guards, quota-window formatting and existing tests.
- The automatic merge placed two PR changelog entries inside the beta.4 contributor list. Remove those misplaced additions and put the remaining fix, with contributor credit, under Unreleased / Fixed. Extend the existing enterprise paragraph in the user guide.

## Old Windows CI failure

Read the failed-job log of run `40368348339` with `gh run view --log-failed`; retained local copy: `~/Gits/.tmp/pr129-old-ci.log`.

The log does not show a failure caused by enterprise null rendering or its new parser tests. It contains multiple existing Windows runtime/fixture failures:

- Lines 7042-7051 and 7074-7083: the two `claude_usage` warm-cache tests receive project-directory `Modify(Any)` events, trigger rescans, and stat unchanged transcripts. The warm test records 2 metadata reads versus the expected 1. This is the directory-metadata invalidation defect documented and repaired under 2310, not a null-utilization failure.
- Lines 5126-5127: cleanup's internal `ucrt64/bin/git.exe` exits with access violation `0xC0000005` (`-1073741819`), leaving the remote proof ref. `reports/2310-windows-ci.md` records the controlled same-install launcher differential and native five-owner GREEN in run `40474953148`.
- Lines 8876-8877: repeat worktree listing takes 13.9344626 seconds. The same report attributes the repaired path to unnecessary recursive submodule scans.
- Lines 9807-9810: unchanged-draft publication returns 500 with `workflow does not belong to project`, expected 422. Other failures cover Windows path spelling, resume quoting, monitoring cache and native-shell fixtures. `reports/2310-windows-ci-round-g.md` classifies those owners and their repairs.

Current base includes the subsequent 2310 integration `79d7f968ce`. This integration does not claim a new native Windows run or that Linux testing proves Windows behavior.

## Validation

User-authorized targeted commands only:

- Local, through `build-slot.sh scripts/with-test-tmp.sh`: `pnpm vitest run src/__tests__/claudeUsage.test.ts src/__tests__/components/ClaudeUsageDashboard.test.tsx`: **39 passed** across two files, 2.17 seconds.
- Local, through `build-slot.sh`: `pnpm exec tsc --noEmit`: **passed**.
- rb Linux, through `build-slot.sh --remote scripts/with-test-tmp.sh`: `cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(claude_usage)'`: **44 passed**, test execution 0.106 seconds, incremental build 4m17s, `RB_BOX_DIRTY=no`. The first attempt lost SSH with exit 69 and did not produce a test result; the single retry passed. No native Mac fallback was used.
- `pnpm exec biome check` on the component and component-test files: **passed**.
- Instruction-link validation and `git diff --check`: **passed**.

Logs: `~/Gits/.tmp/results/tuic-pr129-validation.log` (local checks and interrupted rb attempt), `tuic-pr129-retry.log` (successful rb run).

A temporary Vite fixture mounted the production dashboard with an intercepted invoke boundary. Headless Chrome captured `~/Gits/.tmp/results/tuic-pr129-null-utilization.png`; retained DOM is `tuic-pr129-visual.html`. DOM verification confirms `Usage percentage unavailable` and `162,709` credit totals. This is a component rendering fixture, not a live account or desktop test. The first harness attempts failed to load/intercept the fixture correctly; using a real TSX entry and a pre-resolution mock fixed the harness without changing production code. Vite emits a PostCSS `from`-option warning during this standalone fixture.

No full suite, native Windows run, live credential access, mutation run or separate critic was performed. No new Rust source or tests differ from main, so no Rust restart or new-test Clippy gate is required by this integration. The regression tests have not been executed against pre-fix source; the source audit above establishes the defect, and after-the-fact RED remains coordinator verification.

Risk: low. This changes local rendering only, with no shared state, persistence or protocol changes. No push, merge into main, GitHub comment or daily-app restart is authorized or performed.
