# Isolated MCP audit verification (#2010-30a7)

## Scope and provenance

On 2026-10-20 at 07:58 UTC, the isolated macOS desktop instance `at-2010`
reported version `2.0.0`, commit `f736a3b52f`. It was built and started from
this linked worktree with:

```sh
export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
make agent-test AGENT_TEST_INSTANCE=at-2010 AGENT_TEST_PORT=19881
```

The launcher ran the frontend and native Cargo build through build-slot and
mbx. `mbx doctor` reported zero failures and zero warnings. No test suite ran.

The initial TCP attempt returned HTTP 401. No token store or credentials were
read. The coordinator then explicitly authorized the instance's own Unix
socket. Calls used `curl --unix-socket` against:

```text
~/Gits/.tmp/tuic-at/08aa274d6b781ddb/tuic-mcp-08aa274d6b781ddb.sock
```

The recording contains actual runtime audit rows, not manufactured server
responses. Response text was parsed only in recorder memory to measure UTF-8
bytes and extract `isError` and explicit effect labels. The checked-in JSON
contains no response bodies, error messages, tool arguments or private titles.
Its SHA-256 is:

```text
d2c77acdacfb4a4f691a2554b9ab3218088007a9e114fd9010973d632d5507ee
```

See [the body-free recording](2010-agent-test-audit.json).

## Observed outcomes

| Actual operation | Client text bytes | Application error | Effect | Transport observation |
| --- | ---: | --- | --- | --- |
| `session.list` | 2 | false | unknown | reply constructed; delivery not asserted |
| `session.status`, missing target A | 51 | true | unknown | reply constructed; delivery not asserted |
| `session.status`, missing target B | 51 | true | unknown | reply constructed; delivery not asserted |
| `session`, invalid action | 558 | true | not_started | reply constructed; delivery not asserted |
| `agent.inbox`, no registered caller | 565 | true | not_started | reply constructed; delivery not asserted |
| `voice.status`, audio unavailable | 64 | true | unknown | reply constructed; delivery not asserted |
| `ui.confirm`, client disconnect | null | null | unknown | cancelled_before_reply |

All seven server call IDs were distinct. Each row retained the supplied client
UUID and the common parent-operation UUID. The two missing session targets had
different SHA-256 digests. A shared parent does not turn those targets into
retries or one long send chain.

For every completed call, the audit byte count matched the sum of received
MCP text-block UTF-8 lengths. Application error matched the actual formatted
MCP `isError`. Explicit `not_started` survived, while calls without an explicit
effect remained `unknown`.

Curl disconnected from the pending `ui.confirm` after 300 ms with exit 28 and
no reply bytes. Its audit row retained null bytes and application outcome,
`cancelled_before_reply`, and unknown effect. This proves client disconnect
handling, not loss of a reply that had already been constructed, delivery,
rollback, or a human permission refusal.

The projection contained exactly the documented eleven row fields. Private
request markers and the invalid action text were absent. It reported seven
retained rows, zero evictions, capacity 1024, projection limit 100, scope
`runtime_mcp_tools_call_ingress`, and `delivery_observed=false`. This small run
did not exercise eviction or the projection limit.

## Remaining criterion and limits

The four-outcome captured-fixture criterion remains incomplete:

- A real client permission denial was subsequently captured, as described
  below. It occurred before server ingress, so there is no corresponding audit
  row. An unregistered `agent.inbox` refusal and a cancelled UI confirmation
  remain separate observations.
- Voice rendering is **NEEDS-BOSS**. Agent-test has no dictation/audio support.
  Its error response is not rendering success or an `error:null` success case.
- This recording supplies UUIDs explicitly from the recorder. It is not new
  evidence of ego-selected call correlation or all Codex custom calls.

No production code changed, so no new RED or GREEN test run is claimed. The
historical regression and prior client-correlation evidence remain separate.

## Genuine client-side permission denial

At 08:20 UTC, after the coordinator authorized normal client-managed use of
existing authentication, installed Claude Code `2.1.304` ran inside a throwaway
session in `at-2010`. No credentials were inspected, copied or printed, and
no new login occurred. Its model was Sonnet 5.5 with medium effort. The CLI
used `--permission-mode manual`, never bypass, and a strict MCP configuration
pointing only to this instance's adjacent `tuic-bridge`. Hooks were disabled.
An explicit `mcp__audit_test__*` ask rule required tool confirmation even if
existing profile rules would otherwise allow the read-only tool.

The native prompt displayed:

```text
audit_test — Session Tool: (MCP)
Permission rule mcp__audit_test__* requires confirmation for this tool.
Do you want to proceed?
1. Yes
2. Yes, and don't ask again for audit_test — Session commands ...
3. No
```

The recorder sent `3` plus Enter through the instance socket to
`POST /sessions/{session_id}/write`. The dialog departed; the visible tool
status changed to `Called audit_test (ctrl+o to expand)`. The initial recorder
also found denial words in retained user-prompt history, so those words are
**not** treated as independent evidence of a new assistant acknowledgement.
The explicit native dialog, selected No key and departed controls are the
client-side observation.

Both audit projections contained zero rows, zero retained records and zero
evictions. Bridge initialization was visible in the instance log, but no
`tools/call` reached the audit boundary. The JSON therefore retains the client
observation separately, with `server_ingress_rows=0` and `effect_record=null`.
No server permission-denial row, byte charge or application effect is invented.
The earlier seven real rows remain the non-empty body-free allowlist evidence.

An earlier bounded attempt did not confirm a dialog and sent no keys. It is
not permission-denial evidence. The follow-up above captured and answered the
actual native confirmation. The recorder deleted its own Claude session,
then stopped the instance and waited for supervisor exit. A repeated stop
reported no supervisor; the final process check found no instance process.
Local evidence is under
`~/Gits/.tmp/results/desk-2010-denial-v2-20261020/`, including
`permission-dialog.json`, `input-actions.json`, both audit projections,
`launch.json`, `session-cleanup.json`, `stop.txt` and `cleanup.json`.

## Cleanup and local evidence

The recorder stopped the instance immediately after the calls with:

```sh
make agent-test-stop AGENT_TEST_INSTANCE=at-2010 AGENT_TEST_PORT=19881
```

The stop command reported `Stop requested for this agent-test instance only`.
The recorder waited for the supervisor to exit. A second stop before handoff
reported no running supervisor. A process-list check for this worktree's
`.tmp/agent-test` path found no remaining instance process. No browser, Vite
server or headless backend was started, and the daily app was not accessed.

Raw local evidence is retained under
`~/Gits/.tmp/results/desk-2010-unix-live-20261020/`: `agent-test.log`,
`client-observations-body-free.json`, `final-audit.json`, `assertions.json`,
`stop.txt`, and `cleanup.json`. The recorder assertions passed for all seven
calls. The background command exited zero. The supervisor's make exit 2
reflects deliberate SIGTERM shutdown of its desktop child, not a failed probe.
