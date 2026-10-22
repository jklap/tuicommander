# Story 2120-9f3f: WebView URL evidence

## Status

Evidence collection is incomplete. No production code was changed. The historical
panic message and its calling frame were not found in the inspected sources.
Do not identify the recovery thread as the historical caller from this report.

## Read-only evidence

Inspected `~/Library/Logs/DiagnosticReports`, including `Retired`, for TUIC reports
from 2026-10-22. The 25 matching reports are for `tuic-aarch64-apple-darwin`.
Every report has the same termination classification and zero stack frames.
For example:

`~/Library/Logs/DiagnosticReports/tuic-aarch64-apple-darwin-2026-10-22-074822.ips`

```text
"captureTime" : "2026-10-22 07:48:22.5460 +0200"
"procName" : "tuic-aarch64-apple-darwin"
"exception" : {"codes":"0x0000000000000000, 0x0000000000000000","rawCodes":[0,0],"type":"EXC_CRASH","signal":"SIGKILL (Code Signature Invalid)"}
"termination" : {"flags":66,"code":1,"namespace":"CODESIGNING","indicator":"Taskgated Invalid Signature"}
```

The triggered thread has `"frames":[]`. These reports do not contain a Rust
panic message or a backtrace. They do not prove that Wry caused the reported
desktop process death.

Inspected the persisted TUIC log:

`~/Library/Application Support/com.tuic.commander/logs/tuic.log.2026-10-22`

A case-insensitive search for `panic`, `wry`, `unwrap`, and `backtrace` finds no
panic entry. The only `wry` substring match is the newly created worktree path
`fix-2120-wry-url-panic`. Rust stderr is not established as an input to this log.
No files under the application support directory were changed.

## Proven source defect and caller audit

The worktree lockfile selects Wry 0.57.0 and tauri-runtime-wry 2.12.1.
The installed source is:

`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wry-0.57.0/src/wkwebview/mod.rs:1369`

```rust
pub fn url_from_webview(webview: &WKWebView) -> Result<String> {
  let url_obj = unsafe { webview.URL().unwrap() };
  let absolute_url = url_obj.absoluteString().unwrap();
```

Thus, a nil URL or absolute string panics before a `Result` can be returned.
This establishes a possible failure path, not a historical calling frame.

A source-wide search for `.url(` in `src-tauri/src` finds these native desktop
getters:

| Caller | Current handling | Audit result |
| --- | --- | --- |
| `webview_recovery.rs:156`, `desktop::lost_document` | `window.url().ok()?` | `ok()` does not catch the native getter's panic. |
| `background_mode_windows.rs:64`, `DesktopWindows::destroy` | `window.url().map_err(...)` | The detached-panel snapshot has the same exposure on macOS. The file is a desktop adapter, not Windows-only. |

The `payload.url()` calls in `lib.rs` read an existing page-load payload. They
are not native URL lookups. The async `candidate.url()` in
`design_mode/manager.rs` reads a CDP page. Other `.url()` matches are HTTP
requests, redirect attempts, or test-server addresses, not WKWebView getters.

A prospective fix can use `with_webview` to read the nullable native `URL` and
`absoluteString` values on the event-loop thread without `unwrap`. Both native
getter callers need to use that path. The native callback must return only
owned Rust data across threads. Catching an unwind in the poller is not a
suitable first choice: the Wry getter executes on the event-loop thread.

## Black screen

The evidence above does not establish why the initial window was black, whether
WebKit returned nil, or whether the black screen and reported desktop crash
share a cause. The code-signature reports are not evidence for those claims.
The coordinator must decide whether a separate investigation story is needed;
this managed peer did not create an out-of-scope story.

## Live-check prerequisite conflict

Ran only the non-mutating prerequisite:

```sh
make agent-test AGENT_TEST_DRY_RUN=1 AGENT_TEST_INSTANCE=agent-test-2120 AGENT_TEST_PORT=19880
```

It reports:

```text
config_dir: /Users/stefano.straus/Library/Application Support/com.tuic.commander/instances/agent-test-2120
listener: 127.0.0.1:19880 (no port fallback)
```

This is under the assignment's explicitly read-only application support root.
A real launch would write there. No live instance was launched, and no request
was sent to Boss's app or port 9876. Permission for this exact isolated instance
directory, or an approved Gits-only configuration arrangement, is needed before
that required verification can run.

## Validation and next input

- Instruction-link checker: passed, two instruction pairs.
- `git diff --check`: required before commit.
- RED/GREEN, Rust builds, mutation, and desktop execution: not run.
- Needed evidence: the original panic stderr or an exact log path that contains
  it; if no backtrace exists, the historical caller must remain unknown.
- Needed decision: whether to proceed as a preventive fix of both proven source
  exposures when the historical caller cannot be identified.
- Needed authority: permit writes to the exact isolated instance directory above,
  without changing permission for the daily instance.
