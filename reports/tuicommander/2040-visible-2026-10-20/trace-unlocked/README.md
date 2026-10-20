# Screen-unlock follow-up — 2026-10-20

Story `2040-16b2`, peer `desk-2040e`. The coordinator sent a follow-up at 12:49 local stating that Boss had unlocked the Mac and authorizing a repeat of lock-affected captures. This mail was consumed after the first result had been committed as `bee0288f7`.

## Result

**The authorized repeat still cannot supply visible canvas timing.** `loaded.json` records native `visible=false`, document `visibility=hidden`, document/native focus false, window position `(1630, 1092)` and physical size `1280 × 840`. The document remains hidden throughout the recorded visibility observations. Unlocking the screen did not establish the observer's window-visibility precondition in this run; the reason for the hidden native window is unknown. No show/focus workaround or production app interaction was attempted.

The observer acquired its trace and frame recording, then exited 1 at the explicit guard: `Trace run was not visible; do not claim paint timing`. This is a failed visibility precondition, not a latency pass. No additional measurement is silently substituted for it.

The earlier [viewport-omission verdict](../trace/README.md) remains unchanged. In this additional trace, every intake-correlated glyph is either present in the first eligible serialized snapshot (③, ⑥), or already above the viewport before that snapshot (⑧, ⑨, ⑩, ⑬, ⑭). Several other dispatched keys have no recorded PTY intake and are not classified as snapshot omissions. The recorded floor/gate counts are acquisition observations, not an attribution of the original visible latency.

## Lifecycle and source

Command:

```sh
TUIC_2040_TRACE_DIR=trace-unlocked python3 reports/tuicommander/2040-visible-2026-10-20/run_trace.py
```

It ran once through `tuic-bg.sh`, using the same isolated `at-2040c` / `19891` instance and Unix socket. The launcher first encountered a killed sidecar (137); its existing self-heal/retry path then started the one recorded acquisition. No extra application acquisition was launched manually. Native make agent-test uses its build-slot/mbx integration; no rb test run is claimed.

`run_trace.py` now accepts an evidence subdirectory through `TUIC_2040_TRACE_DIR`, defaulting to the original `trace`. The repeat observer uses the same variable so it cannot overwrite the completed traces. The exact temporary Rust observer is retained in `observer.patch`; `src-tauri/src/pty.rs` was restored to HEAD after acquisition and is not changed by this commit.

`cleanup.json` records zero remaining sessions, exact-instance stop exit 0 and no worktree app processes. The make supervisor exits 2 after the intentional child termination. A final exact-instance stop reports no running supervisor. No separate Vite or browser session was started.

## Validation and remaining criterion

- Regenerated `summary.json` and `timeline.json` from losslessly compressed `native.log.gz`. Raw frame arrivals, backend observation and measurement data remain alongside them. The analysis and coordinate conventions are documented in `../trace/README.md`.
- Python AST and JSON parsing, deterministic analyzer regeneration, instruction-link check, final `fmt-changed.sh` and `git diff --check` pass. Formatting has zero applicable code files because reports are excluded.
- No unit suite, RED/GREEN run, mutation, production correction, `.tcap`, native trusted keyboard timing or compositor observation is claimed.
- `story-close.sh --check <main checkout> 2040-16b2` refuses original criterion 3: `/help ... within 2x ... and keystroke echo stays under one frame`. It remains unchecked. This assignment prohibits an unproved production fix; evidence cannot honestly check that criterion. Original native REALITY also remains unchecked. The coordinator must retain the story as open rather than weaken these requirements.

The committed result remains evidence-only and low risk. No push or landing was performed.
