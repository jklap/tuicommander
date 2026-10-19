# GitHub toolbar and request-volume investigation

Date: 2026-10-18. Stories: 1994-9862 and 1995-279a.

## Scope

No installed desktop process, production sessions, account data or live GitHub
writes were used. Browser fixtures mount production Solid components. The Rust
experiment runs the production poller and HTTP client against loopback TLS with
a fake token and controlled protocol replies. Neither fixture is live-account
E2E evidence. The historical blocked tab remains unidentified.

## 1994: current toolbar candidates

At a 1400 by 900 viewport, trusted browser clicks showed:

| Production surface | First action | Second action | Observed result |
| --- | --- | --- | --- |
| `GitHubPanel` | Ops Dashboard | Changelog | Dashboard creates a `github-ops` tab; Changelog mounts `ChangelogModal` and calls `generate_changelog` once |
| `PrDiffTab` | Side-by-side | Inline | The production UI store switches between split and unified diff rendering |

The Changelog success fixture renders markdown. Its error variant renders
`No ego executable is configured` and disables Copy/Save. Modal and panel Close
work. Copy/Save were clicked but clipboard contents and download bytes were not
independently checked. No live ego generation or authenticated Git action was
verified.

Changelog was restored for ego in `d28773245b`; PR Files has later review-thread
and pinned-head changes (`bb0aba563f`, `307643451e`). `GitPanel` is a separate
maintained component, not a destination of either tested second action.
Therefore these current candidates do not establish an abandoned editor or a
plugin replacement. No deletion or speculative fix is justified. The original
report needs its already-retained screenshot/recording to identify the target;
that evidence was requested from the coordinator. Story 1994 remains open.

The first browser attempt used an inherited 500-pixel viewport, placing a
fixed desktop control offscreen. It is a fixture setup failure, not a product
reproduction. Corrected screenshots and transcripts are retained below.

## 1995: frontend boundary

The real `PrDetailContent`, `githubStore`, invoke wrapper, command mapper and
browser HTTP transport sent requests to a Vite fixture receiver:

| Scenario | Received frontend CI requests |
| --- | ---: |
| Idle, 60.001493 seconds | 0 |
| Open one PR, then 60.002819 seconds mounted | 1 |
| Ten identical store snapshots | 0 additional |
| Close/reopen once | 1 additional |

Both receives are GET `/repo/ci`, at `2026-10-18T18:30:03.299Z` and
`2026-10-18T18:31:06.430Z`. These are frontend HTTP requests, not GitHub REST
requests. Failed text-selector and reused-closed-browser-session attempts are
retained separately and are not passing measurements. The successful runner
uses a new named browser session and closes only its own browser and Vite.

## 1995: production Rust receiver boundary

`github_poller::volume_measurement_1995::measure_github_client_fixture_1995`
runs one hot repository, one account, visible cadence and no extra user actions.
A real temporary Git repository supplies its origin. Loopback receives retain
production URLs, query construction, parsing, caching and circuit breakers.
The poller must publish one parsed PR, and the CI consumer must receive one
parsed check. The open/reopen workload replays the two calls observed above;
it is not a simultaneous browser/backend run.

The completed commit-pinned experiment recorded:

| Phase | Elapsed observation | GraphQL receives | REST receives |
| --- | ---: | ---: | ---: |
| Cold start | setup | 2: viewer lookup and batch | 0 |
| Idle | 60.201528 seconds | 1 additional batch | 0 |
| Open one PR | 60.201448 seconds | 2 additional: checks and periodic batch | 0 |
| Reopen | one action | 1 additional checks query | 0 |

For each of exhausted 403 and 429, on each of REST and GraphQL, the receiver
observed the rate response, zero immediate retry sends, and one send after
1.1 seconds with `retry-after: 1`. No real server limit was triggered.

The selected test passed in 124.939 seconds, with 4 minutes 12 seconds of
compilation. The rb result records
`source_ref=0b41b53ecd8cf8f0f202e83f16904ff40d81eff7`, `exit=0`, `stale=no`
and `box_dirty=no`. This is Linux remote evidence, not a native macOS run.

- Command: `RB_REF=0b41b53ecd8cf8f0f202e83f16904ff40d81eff7 build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(measure_github_client_fixture_1995)' --run-ignored all --success-output immediate`.
- Result: `/Users/stefano.straus/Gits/.tmp/rb/20261018T193153Z-63695/result.txt`.
- Log: `/Users/stefano.straus/Gits/.tmp/tb-github-1995-rust-client-volume-final.log`.
- Receiver JSON, NDJSON and metadata: retained evidence `backend/final/`.

An earlier experiment passed in 124.921 seconds but its rb wrapper exited 75
because the coordinator began a local merge after sync. That earlier result
(`20261018T191803Z-68903`) remains qualified as synced-snapshot evidence. The
clean commit-pinned run above supersedes that qualification only for its exact
source commit; it does not remove the whole-app and historical-report limits.

Earlier failures were setup-only compilation against an unavailable `git2`
crate, then a 120-second harness timeout. The latter could not contain two
60.2-second windows plus setup/backoff. No failing test was removed or disabled:
the explicitly ignored measurement now has its own 360-second Nextest outer
bound above its 300-second internal hang bound; routine limits are unchanged.

## Interpretation and limits

- Repositories are batched by account. A cold viewer lookup adds a request;
  successful viewer lookup is cached.
- Visible polling starts at 60 seconds, hidden at 120 seconds, with a 300-second
  maximum, account-budget throttling, repository tiering and event debounce.
- Truncated review-thread walks, authentication fallback, account count, user
  actions and reconnect/resync can add requests. The table is not a worst case.
- `get_ci_checks_impl` performs a GraphQL checks query when prerequisites hold.
  Leaving the tested PR component mounted did not itself repeat its CI request.
- Debug logs are not send counters: missing tokens can overcount, authentication
  fallback can undercount, and REST builder logs can precede breaker rejection.
- Request counts do not establish GraphQL point cost, secondary-limit safety or
  a whole-app upper bound. The controlled backoff cases do not prove every
  proactive-budget boundary or every rate-limit response shape.
- A previous read-only `/rate_limit` capture is a budget snapshot, not volume
  evidence. No new authenticated traffic was needed for these experiments.
- No retained trace identifies Boss's reported blocked tab or connects its wait
  to GitHub. Story 1995 remains open; no safe-rate guarantee is claimed.

## Follow-up audit: 2026-10-19

Story 1995 was reassigned on branch `fix/1995-github-request-volume`, based on
`6fc80f59c18f6f2958fd1a4a40b83e113a4ef76f`. The existing measurement commit
`9d91d0189f` is already an ancestor. Do not recreate or merge that work again.

Source inspection confirms the following existing behavior:

- `src-tauri/src/github.rs:436-444` reads remaining, reset and retry headers
  before classifying HTTP 429. Lines 469-487 classify exhausted and secondary
  HTTP 403 responses.
- `src-tauri/src/github.rs:629` checks the account breaker before GraphQL
  requests. Lines 663-667 record the rate-limit wait for a named account;
  lines 695-699 do the same for the ambient account.
- `src-tauri/src/github.rs:1355-1371` checks the REST breaker before
  `request.send()` and classifies rate-limit headers. Lines 1399-1402 record
  the wait. `rate_limit_wait_secs` at line 510 gives `retry-after` precedence.
- `src-tauri/src/github_poller.rs:284-301` implements budget-dependent cadence.
- `src-tauri/src/github_volume_measurement_1995.rs:91-252` already exercises
  the production client, publishes parsed consumer results and asserts no
  immediate retry send plus a resumed send for 403/429 on both transports.

The retained `backend/final/receiver-summary.json` and rb `result.txt` were
read again, not regenerated. They agree with the counts and commit-pinned
Linux result above. These are historical results for `0b41b53ecd`, not a new
run on this branch. No build, test, browser or live-account request was run
for this documentation-only audit. There is no new failing regression or
proven production defect, so no speculative fix or artificial RED criterion
was added.

All three original acceptance criteria remain unchecked. The existing evidence
covers a controlled client workload, not simultaneous whole-app traffic or an
account-wide safe upper bound. The exact historical blocked tab, its timestamp
and a corresponding request/wait trace are still missing. The next action
requires that report evidence from Boss or the coordinator; another synthetic
measurement cannot recover it. No production behavior, UI or timeout changes
are justified by this audit.

## Retained evidence

Local evidence root:
`/Users/stefano.straus/Gits/.tmp/tb-github-evidence-rhmyaupv/`.

- `frontend/`: toolbar/changelog/dashboard and split/unified screenshots,
  trusted-click transcripts, fixture sources, `volume-summary.json`,
  `client-trace.ndjson`, `volume-transcript.txt`, and failed-attempt traces.
- `backend/`: `receiver-summary.json`, `receiver-trace.ndjson`, selected source
  and fixture configuration, and full receiver-run log.

The checked-in explicitly ignored Rust experiment and its fixture README provide
an independent rerun path. Browser artifacts prove fixture routing/rendering,
not a live GitHub account or the original historical report. Integration checks
for 1993 are separate and recorded in its worklog.
