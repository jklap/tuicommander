---
id: 1148-c25f
title: Repeated fresh MCP initialize accumulates protocol sessions while backend memory grows to 33 GB
status: pending
priority: P1
type: fix
created: "2026-09-28T13:23:48.360Z"
updated: "2026-10-04T15:13:14.269Z"
dependencies: []
started_at: "2026-09-30T17:02:15.899Z"
---

# A reconnect-looping tuic-bridge creates thousands of MCP sessions and grows the backend to 33 GB

## Problem Statement

2026-09-28 12:51-13:03 UTC: one tuic-bridge (tuic_session e1f00d96, the AI Chat ego peer id for Fleet__wt/poc-01158-nu-registry in config.json ai_chat_peer_ids) sent 13,409 fresh MCP initialize requests (up to 1,736 per minute). The diagnostics report showed mcp.sessions=1958, malloc_bytes_in_use 27.3 GB against 141 MB accounted, footprint 6.3 -> 32.9 GB in 2 minutes, CPU SPIKE 138-217%, frontend unresponsive 38 s and grid frame gates stuck on every session. After the storm stopped the footprint stayed at 4987 MiB (2162 MiB an hour earlier). Log: ~/Library/Application Support/com.tuic.commander/logs/tuic.log.2026-09-28.

## Acceptance Criteria

- [ ] Root cause: why the bridge re-initialized in a loop (bridge side, server response, or ego ACP respawn) is proven from logs or a reproduction
- [x] RED: repeated fresh initialize from one tuic_session does not grow mcp.sessions without bound (the stale binding is replaced, not accumulated) — catches: one looping client exhausting memory
- [x] RED: the bridge backs off on repeated initialize failure instead of retrying at full speed — catches: a tight reconnect loop
- [x] The memory held per MCP session is identified and released when a session is replaced or reaped
- [x] GREEN: targeted nextest for mcp_transport initialize and bridge reconnect pass
- [x] RED: reaping_fresh_mcp_sessions_releases_per_session_state - catches: expired protocol sessions retain routing maps and broadcast senders
- [x] GREEN: targeted Linux reaper tests release protocol routes and channels, preserve addressable peers, and promote live siblings
## Proof

- [ ] [completeness] Completeness (Historical 13,409-call trigger and 27.3 GB malloc allocation source remain unidentified)
- [x] [feature-availability] Feature availability (Linux rb nextest test（reap） 19/19; new 12-session cleanup test passes)
- [x] [robustness] Robustness (Expired metadata, routes, reverse routes, and channels reach zero; other session peers stay intact)
- [x] [resilience] Resilience (Reaper promotes live sibling and preserves addressable PTY identity in targeted Linux tests)
- [x] [security] Security (Reaping one session leaves another session peer alone; targeted reaper test passed)
- [~] [defense-in-depth] Defense in depth (No additional security boundary added in this cleanup)
- [~] [input-validation] Input validation (Reaper receives internal session IDs, not new external input)
- [ ] [thread-safety] Thread safety (Shared identity lock used, but refresh-versus-reap concurrency has no deterministic test)
- [~] [configurability] Configurability (No new configuration)

## QA

- [ ] Reproduce or trace the historical ego ACP request that initiated 13,409 bridge initializes; collect bridge PID and ACP method.
- [ ] Capture a bounded allocation profile or equivalent evidence for the unaccounted 27.3 GB malloc source; compare before and after MCP cleanup.

## Work Log

### 2026-09-28T13:33:09.158Z - Reality contract: mcp_post initialize with x-tuic-session creates McpSessionMeta in AppState.mcp.sessions; 2026-09-28 log shows repeated fresh tuic-bridge initializes for one identity and mcp.sessions rising. The bridge starts with server_initialize; ego stdio operations spawn a fresh duplex, discover, then settle by closing stdin. The server 1h sweep is the only cleanup for protocol metadata unless DELETE /mcp is sent. New tests drive mcp_post and bridge dispatch_loop. Assumptions open: the 27 GB malloc growth is not yet attributed to per-session state; ego operation frequency source needs reproduction or its own logs. Blast radius: peer routing, live sibling bridges, SSE teardown and reconnect.

### 2026-09-28T14:02:54.738Z - Observed 13,409 fresh initialize lines for one TUIC identity, but the log does not identify bridge PID or operation. Ego source opens a new stdio duplex and runs discover for each MCP operation (ego-mcp operation.rs:986-1002), then settles it; TUIC bridge previously exited on stdin EOF without DELETE. RED tests reproduced unbounded session metadata (12 retained) and full-speed failed initialize (5 attempts), plus missing DELETE on EOF. Fix sends DELETE on clean EOF, reaps stale metadata/routes/channels at next initialize for the same identity, and backs off failed reconnects per process (1 to 8 seconds). The 27.3 GB malloc in use is not attributable to McpSessionMeta: AppState accounted 141 MB at the same point. The trigger for 13,409 ego operations and the unaccounted allocation remain unresolved. Story stays in progress; do not claim full memory fix.

### 2026-09-28T14:04:56.022Z - Partial mitigation committed as 6d01e9608 (no closing keyword; story remains in_progress). RED: server test held 12 sessions vs expected 1; failed bridge initialize produced 5 attempts vs expected 1; EOF sent 0 DELETE vs expected 1. GREEN: server focused 4/4 plus strengthened cleanup 1/1, bridge focused 4/4 and cross-cutting bridge binary 14/14. rustfmt, git diff --check and instruction symlink check passed. No release build, live dev restart, end-to-end memory profile, mutation or full app suite. The old live ego process is gone, no per-operation logs identify why it made 13,409 stdio operations, and 27.3 GB malloc remains outside AppState accounting. Need investigation of ego operation trigger and a memory allocation profile before this P1 story is complete.

### 2026-09-29T16:14:27.200Z - 2026-09-29 follow-up RED: 12 fresh initialize requests under one peer, then simulated per-session reap, leave to_session routes allocated after McpSessionMeta is removed. Targeted Mac test executed 1 and failed on reaped routes retained: ~/Gits/.tmp/tuic-cli/red-1148-reap.log. This identifies a bounded routing/channel retention defect, not the 27.3 GB malloc source. Coordinator directed subsequent Rust runs to rb due Mac load; remote GREEN pending.

### 2026-09-29T16:20:45.512Z - Follow-up 2bbf1a544: RED Mac test executed 1/1 and failed on retained to_session after 12 fresh initializes and simulated reaps. GREEN Linux rb targeted nextest test(reap) 19/19, including new 12-session cleanup, live-sibling promotion, addressable PTY retention, and prior reaper consumers (~/Gits/.tmp/tuic-cli/green-1148-remote.log). The fixed state is McpSessionMeta plus per-session to_session, session_to_mcp, and messaging_channels; all reach zero in the test after reap. This is a bounded metadata/channel leak, not an attribution of 27.3 GB malloc. Ego story 198-6423 reproduces repeated failed ACP session/load causing repeated stdio admission; its historical durable session has ten confinement events and no admitted run/tool event, so the exact 13,409-call trigger remains unproven. 1148 criteria 1 and 4 stay open.


### 2026-09-29T16:20:59.787Z - Proof completeness set UNPROVEN: Historical 13,409-call trigger and 27.3 GB malloc allocation source remain unidentified

### 2026-09-29T16:21:00.094Z - Proof feature-availability set PROVEN: Linux rb nextest test(reap) 19/19; new 12-session cleanup test passes

### 2026-09-29T16:21:00.422Z - Proof robustness set PROVEN: Expired metadata, routes, reverse routes, and channels reach zero; other session peers stay intact

### 2026-09-29T16:21:00.766Z - Proof resilience set PROVEN: Reaper promotes live sibling and preserves addressable PTY identity in targeted Linux tests

### 2026-09-29T16:21:01.150Z - Proof security set PROVEN: Reaping one session leaves another session peer alone; targeted reaper test passed

### 2026-09-29T16:21:01.463Z - Proof defense-in-depth set NOT_APPLICABLE: No additional security boundary added in this cleanup

### 2026-09-29T16:21:01.824Z - Proof input-validation set NOT_APPLICABLE: Reaper receives internal session IDs, not new external input

### 2026-09-29T16:21:02.147Z - Proof thread-safety set UNPROVEN: Shared identity lock used, but refresh-versus-reap concurrency has no deterministic test

### 2026-09-29T16:21:02.447Z - Proof configurability set NOT_APPLICABLE: No new configuration

### 2026-09-29T16:22:10.724Z - Diagnosis handoff: tuic.log.2026-09-28 confirms 13,409 fresh tuic-bridge initializes for e1f00d96 at 12:51-13:03 UTC, without bridge PID or ACP method. Ego story 198-6423 shows matching durable session had only ten confinement observations and no admitted tool/run; its repeated failed ACP session/load test reproduces repeated stdio admission, but cannot assign the historical storm to that method. Current ego ACP integration now uses host MCP rather than the old stdio bridge, so replaying the exact incident needs historical request instrumentation or a preserved client. 27.3 GB malloc remains outside AppState accounting (141 MB); current fix releases only known protocol routing/channel state. Separate refresh-versus-reap snapshot race registered as 1259-62e3 from decisive code evidence; not changed here.

### 2026-09-30T17:05:17.341Z - 2026-09-30 investigation (read-only, logs). (1) Storms are not one-off: tuic.log fresh-initialize counts per tuic_session: 09-28 e1f00d96 27,242 and 11c60fa3 34,626 (both client=tuic-bridge, both ai_chat_peer_ids of ego worktrees: Fleet poc-01158-nu-registry and ego poc-01199-ego-acp-perms; second storm 15:41-16:20 UTC, ~25 ms spacing, 1,973/min peak); 09-29/30 ad8b8e8a 324k initializes, client=tuic-cli (one per CLI call, no DELETE), steady ~37/min, ended 06:26 UTC 09-30. (2) Root cause location: client side = ego AI Chat admission of a stdio MCP server (tuic-bridge) per attempt, retried without backoff; ego commits 3ab4303 'back off failed MCP admissions' (09-28 16:42) and eaf2bfb 'share concurrent MCP admission outcome' (17:31) landed after storm 2 ended; storm 1 start (12:51:44) follows an ego child process at 30.9% CPU in HEALTH at 12:51:30. Bridge PID / ACP method still absent from TUIC logs, so attribution to the ego admission path is by correlation plus ego RED repro (story 198-6423), not a captured request. (3) Live state now: /diagnostics/memory footprint 620 MB, mcp.sessions=24, despite 324k tuic-cli initializes today: session count stays bounded with the reaper (grace 6 s => cap about rate x 6 s). (4) Per-session protocol state is tiny (broadcast channels of 4-8 slots, McpSessionMeta); 27.3 GB / ~3,500 initializes in 2 min is ~8 MB per initialize, so it is not McpSessionMeta. Running rb RSS-per-initialize measurement (scratch test, not committed) to decide.

### 2026-09-30T18:38:22.636Z - 2026-09-30 rb measurement (scratch test, reverted, log ~/Gits/.tmp/tuic-1148/scratch4.log): 2000 x (initialize + tools/list, 28,813-byte body) through mcp_post under one tuic_session: RSS 52,160 -> 57,288 kB = 2.56 kB per initialize, sessions=2000; after reap RSS 57,480 kB, sessions=0 (allocator keeps pages, no growth). So protocol session state is ~2.6 kB, not the ~8 MB/init implied by 27.3 GB. The 27 GB is not retained per-session state; likely transient in-flight load (concurrent 28 kB tools/list responses, SSE/stream buffers, slow readers) - unproven. Criterion 4 closed (per-session memory identified, released on reap: reaping_fresh_mcp_sessions_releases_per_session_state + this measurement). Criterion 1 stays open: client attribution only by correlation.

### 2026-09-30T21:21:17.585Z - 2026-09-30 contract (coordinator follow-up): consumer = operator reading tuic.log after the next storm, and the server reaper. Real state: MCP initialize log line gets client_pid from header x-tuic-client-pid sent by tuic-bridge (POST /mcp) and tuic-cli (every request); digits-only filter. tuic-cli McpClient::drop sends DELETE /mcp with mcp-session-id (existing server DELETE, used by bridge). Assumes: server DELETE handler exists (bridge uses it). ACP method is NOT observable: the bridge speaks stdio MCP and never sees the ACP method; only the pid is logged, correlate with ego logs by pid. Tests: client_pid_is_logged_only_when_it_is_a_plain_pid (forged/oversized header), mcp_deletes_its_protocol_session_after_the_call (leaked session per call), mcp_initialize_names_the_cli_process, the_bridge_names_its_process_to_the_server, every_cli_request_names_its_process; existing mcp_command stub updated for the extra DELETE request.

### 2026-09-30T21:30:07.826Z - Committed follow-up (pid in initialize log, tuic-cli DELETE). rb: nextest -p tuic-cli -p tuic-bridge 97/97 pass, lib client_pid_is_logged_only 1/1 (log ~/Gits/.tmp/tuic-1148/fix7.log). ACP method not observable from bridge.

### 2026-09-30T21:44:12.971Z - Landed: client_pid in the initialize log (tuic-bridge, tuic-cli), tuic-cli DELETE per call, end_mcp_session keeps reachable identities (critic-1148 defect fixed). Criterion 1 stays open until a storm is caught with the pid; needs TUIC restart and rebuilt bridge.

### 2026-10-02T08:11:49.636Z - Boss 2026-10-02 (open-asks): confirmed the coordinator's decision as implemented; 1148: yes, an agent reproduces the storm on the box with an allocation profile after the mutation gates.

### 2026-10-04T10:31:20.318Z - Wave-2 audit: bounded initialize cleanup, bridge backoff, CLI DELETE and client PID logging already exist with targeted regression evidence in the worklog. Remaining criterion is attribution of the historical storm; existing logs lack the ACP method and bridge PID. A current synthetic initialize allocation measurement cannot prove that historical trigger or the 27.3 GB source. Keep story open; no claim of full memory fix.

### 2026-10-04T13:10:02.616Z - Cleanup audit: Leave open: root-cause attribution of the historical storm remains unproven; mitigations and synthetic allocation tests do not identify the historical trigger. Existing Boss decision calls for rb reproduction after mutation gates.

### 2026-10-04T15:13:13.857Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Cleanup audit: Leave open: root-cause attribution of the historical storm remains unproven; mitigations and synthetic allocation tests do not identify the historical trigger.

