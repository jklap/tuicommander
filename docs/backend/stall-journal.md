# Persistent stall evidence

The stall journal retains selected observations after the application log ring wraps or the application restarts. It does not identify a cause or prove that the frontend and an independent HTTP request ran concurrently.

## Storage and limits

The journal is stored beside the backend logs in the instance config directory:

- `logs/stall-journal.jsonl`: current records, up to 2 MiB.
- `logs/stall-journal.previous.jsonl`: the previous file, up to 2 MiB.

Rotation replaces the previous file. The writer uses a lossy non-blocking queue of 128 records. Disk writes and rotation run on the existing tracing-appender worker mechanism, not on the calling thread. Graceful shutdown flushes the queue. A full queue, disk error, abrupt process termination, or rotation can lose evidence. This is bounded diagnostic retention, not an audit log.

Each JSON line has `schema_version: 1`, a backend `timestamp_ms`, a fixed `kind`, and numeric `metrics`. No log message, source string, repository path, branch, terminal ID, breadcrumb detail, or response body is stored. Unknown fields are discarded.

## Observations

| Kind | Producer | Metadata |
| --- | --- | --- |
| `git_refresh` | A slow `git.refreshBatch` | Total/body/flush milliseconds, active repository count, response worktree count, store worktree count, JSON response bytes |
| `git_stats` | A slow `git.statsBatch` | Total/body/flush milliseconds |
| `ui_freeze` | Existing frontend `UI freeze` report | Timer gap milliseconds |
| `heartbeat_gap` | Backend `Frontend unresponsive` report | Silence seconds, last heartbeat receipt wall timestamp, backend observation wall timestamp |

Frontend measurements retain the existing `perfDebug` gate. The heartbeat watchdog remains always on and desktop-only. Response bytes mean the UTF-8 encoding of the parsed structure response serialized as JSON, not transport framing or wire compression. Size calculation runs only after a slow structural batch, outside its timed body/flush region. This diagnostic calculation can add work when enabled.

The heartbeat receipt timestamp is taken in Rust when the heartbeat command runs. The watchdog's observation timestamp comes from its independent backend thread. Neither is an HTTP round-trip measurement, and neither distinguishes WebView blocking, delayed IPC delivery, and resource starvation on its own. Sleep rebaselining does not invent a received heartbeat timestamp.

The journal contains only events that the existing producers report. It does not change frontend sleep/background filtering or retroactively recover events lost before this build.

## Build an isolated replay input

Copy the journal files after graceful shutdown, or take a stable copy while the app is running. Do not point a workload harness at a live desktop instance.

```sh
node scripts/build-stall-replay.mjs replay.json stall-journal.previous.jsonl stall-journal.jsonl
```

Supply the previous file first; omit it when it does not exist. The builder rejects malformed/truncated JSON, unsupported versions, missing topology counts, oversized input, and more than 10,000 generated worktrees across the selected records. Select a smaller set of complete JSON lines if necessary.

The output contains sanitized observations and synthetic structure responses with deterministic checkout names. A refresh harness can use each `refreshes[].structure` as its `getRepoStructure` result and `repo_count` as the number of active repositories. `response_bytes` is the observed size, not the size of the generated response. The builder does not pad responses to that size or sleep for the observed timings.

**This is a metadata-derived synthetic topology, not the original workload.** Counts do not retain terminal state, branch status, string lengths, or the reactive graph. A count-based replay may fail to reproduce the stall. The original real-stall RED/GREEN criteria stay open until a captured workload reproduces the heartbeat gap and the same workload proves the fix. Do not turn the observed duration into an artificial busy loop and call it a reproduction.
