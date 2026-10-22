# MCP upstream outage investigation — 2126-8000

## Outcome

**The proposed cause is not reproduced. No production fix is justified yet.**

On unchanged source `2c90bcf60a53b17eb6094e56c80094292b54d43e`, a real
`tuic-bridge` connected to an isolated desktop instance retained native tools
while an owned fake HTTP upstream stopped, opened its circuit and recovered.
Only `fake2126__echo` left the list. The bridge also forwarded
`notifications/tools/list_changed` during these transitions.

This does **not** prove that Claude Code retains its client-side catalogue.
The original report of all TUIC tools disappearing still needs a timestamped
client trace, bridge stderr or tools/list response from the failing connection.

## Contract and scope

- Consumers must retain native discovery and calls during upstream outages.
- Native definitions come from `filtered_native_tools`; upstream definitions
  come from the registry's independent ready-entry snapshot.
- The owned fake implements the server contract for this experiment. It is not
  a fixture of Slack, Quill or another external service.
- Native macOS evidence only. No rb test, RED test, GREEN suite, installed binary
  replacement, daily-instance mutation or production edit was performed.

## Live experiment

Launch, from this linked worktree:

```sh
make agent-test AGENT_TEST_INSTANCE=agent-test-2126 AGENT_TEST_PORT=19889 AGENT_TEST_DRY_RUN=1
make agent-test AGENT_TEST_INSTANCE=agent-test-2126 AGENT_TEST_PORT=19889
```

The target builds through build-slot and the mbx Cargo shim. `mbx doctor`
reported zero failures and warnings. The instance has its own configuration,
binary and Unix socket. The probe used HTTP over **that socket**, not the daily
instance or an unauthenticated TCP request.

The probe started an owned Python HTTP server on `127.0.0.1:64834`, added it via
`PUT /mcp/upstreams`, and connected the built bridge with both
`--instance agent-test-2126` and an explicit `TUIC_SOCKET` pointing at the
isolated socket. It initialized as `claude-code`, then read and called tools
through the real bridge stdio protocol.

| UTC timestamp | Observation |
| --- | --- |
| 10:46:23.493352 | Healthy tools/list: nine native tools plus `fake2126__echo` |
| 10:46:23.671329 | Owned fake listener stopped and socket closed |
| 10:46:23.672142–10:46:23.673490 | Three calls to the fake returned `isError: true`, with `Upstream 'fake2126' request failed` |
| 10:46:23.674508 | `/mcp/upstream-status`: `circuit_open`, three calls and three errors |
| 10:46:23.678546 | tools/list: the same nine native tools, no fake tool |
| 10:46:23.679008 | Bridge stdout: `notifications/tools/list_changed` |
| 10:46:23.682520 | Owned fake listener restarted on the same port |
| 10:46:23.744170 | After explicit upstream reconnect, tools/list again included the fake tool |

The nine native names were `agent`, `automations`, `plugin_dev_guide`, `progress`,
`repo`, `secret`, `session`, `ui` and `workflow`. The probe checked that `agent`,
`ui`, `session`, `repo` and `progress` stayed listed at all three snapshots.
It also successfully called `agent list_peers`, `session list` and `repo list`
in each phase. It did not call `ui` or `progress`.

This experiment induced the circuit through failed tool calls, not the periodic
health-check task. Recovery used the reconnect endpoint, not a timed automatic
health probe. The client was a stdio JSON-RPC probe, not Claude Code itself.
Collapsed discovery and simultaneous outages of multiple upstreams were not
exercised. The snapshots and notifications are in `observations.json`.

## Source findings

- `src-tauri/src/mcp_proxy/registry.rs:478`: aggregation skips non-ready entries;
  it does not own the native definitions.
- `src-tauri/src/mcp_proxy/registry.rs:1274`: periodic health checks update the
  affected entry and publish the shared tools-changed signal.
- `src-tauri/src/mcp_http/mcp_transport_catalogue.rs:1014`: expanded discovery
  starts with filtered native definitions and extends them with upstream tools.
  Collapsed discovery takes a separate meta-tool branch.
- `src-tauri/src/mcp_http/mcp_transport.rs:1132`: tools/list performs a bounded
  initial settle, then serializes the merged catalogue without awaiting an
  upstream network request.
- `src-tauri/src/mcp_http/mcp_transport.rs:1430`: upstream changes cause a search
  index refresh and list_changed notification, not an explicit disconnect.
- `src-tauri/crates/tuic-bridge/src/main.rs:985`: bridge disconnect detection
  uses failed local MCP pings, not upstream health status. The offline response
  at line 665 contains an empty tool list; this is a separate possible loss layer.

## Daily-log correlation is not causation

Read-only source: `logs/tuic.log.2026-10-22` in the daily application's config
folder. The following observations narrow, but do not establish, the cause:

| UTC timestamp | Daily-log evidence |
| --- | --- |
| 08:42:36.635031 | `Sleep-wake detected`, `gap_secs=285` |
| 08:42:36.658548–08:42:36.658585 | Circuits open for openrouter-key-manager, quill and gh-metrics |
| 08:59:24.822617 | `Sleep-wake detected`, `gap_secs=1000` |
| 08:59:26.298034–08:59:27.978555 | Circuits open for spinach, slack, outlook-mail, outlook-calendar and publishwith-ai-mcp |
| 09:00:08.809789–09:01:09.223084 | Recorded upstream recoveries |
| 09:17:08.634471–09:17:08.651192 | Another circuit cluster, with continuous repo-watcher and standby logging before and after it |
| 09:18:10.575515–09:18:11.475007 | outlook-calendar, slack, spinach and outlook-mail recover |

The first two clusters coincide with long process timer gaps. The third does
not have the same local evidence. A sleep-only explanation would overstate the
record. None of these log lines records the downstream tools/list result or
Claude Code's tool registry.

## Setup errors and cleanup

The first bridge probe omitted the isolated socket override. It used the system
temp directory, could not connect, and returned an empty offline list **before
any upstream outage**. That is invalid reproduction evidence, not a defect in
upstream aggregation. The corrected run used the explicit isolated socket.

The initial background command wrapped the non-exiting desktop supervisor.
It could not emit BG DONE until stopped, which delayed continuation. The second
run used a finite supervisor/probe/cleanup wrapper. Its exit code was zero.
The second wrapper issued a redundant stop after the probe had already stopped
the instance; this printed `no running agent-test supervisor`, not a leaked
process or application failure.

The bridge exited, the fake server closed, the saved fake-upstream configuration
was removed, and `make agent-test-stop` stopped the isolated desktop instance.
Its PID file was absent at handoff. The daily instance was not restarted.

Full local evidence is under `~/Gits/.tmp/results/`:
`tuic-2126-probe.jsonl`, `tuic-2126-bridge.stderr`,
`tuic-2126-agent-test.log`, `tuic-2126-agent-test-relaunch.log` and
`tuic-2126-live.log`. The diagnostic scripts are retained there as
`tuic-2126-probe.py` and `tuic-2126-run-live.py`.

## Required next evidence

Capture the affected Claude Code connection's bridge stderr and raw tools/list
responses across a reported disappearance. Distinguish a bridge-generated empty
offline list, an RPC error/timeout, and client-side catalogue refresh behavior.
Do not change registry filtering or suppress list_changed based on correlation
alone. Story acceptance criteria remain open because the reported native-tool
loss and a corresponding RED test have not been reproduced.
