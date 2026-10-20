# Story 2095: thirty-minute native observation

## Scope and command

On 2026-10-20, run the unchanged agent-test presentation policy at commit
`5a25a558e587a94032b22c0d6053141162db5911` in the linked worktree
`fix/2095-agent-test-presented`:

```sh
python3 -B scripts/agent-test-visible.py --instance at-2095 --port 19995 \
  --duration 1800 \
  --evidence-dir "$HOME/Gits/.tmp/results/fix-2095-long"
```

The launcher built the native macOS binary through build-slot and mbx. The
observer used only this instance's Unix socket and read-only window-server
queries. It did not activate, raise, reposition or resize any native window.
The terminal canvas capture contains only the observer's throwaway terminal.
No production presentation code changed. No remote Rust test ran.

## Result: no spontaneous hidden transition

The observation lasted **1,800,029 ms** and retained **344 samples**. All samples
reported `document.visibilityState=visible`, native `visible=true`, and
`applicationHidden=false`. The visibility event listener recorded only its initial
visible state: it recorded no hidden transition between samples either.

The final animation-frame counter was **100,665**. Its maximum inter-frame gap was
**1,073 ms**. This is not a clean latency run: the observer returned exit 1, not a
pass. It also saw foreground changes and window geometry changes.

This result does **not** establish the cause of the original hidden-window bug,
produce its causal RED, or constitute a post-fix GREEN. It narrows the evidence:
ordinary concurrent work did not hide this instance during this interval. The
system authentication prompt and competing agent-test instances remain
hypotheses, not proven causes.

## Other observed failures

- Before the build, `PlatformSSOUIAgent` was frontmost. At frontend readiness,
  the daily `tuicommander` application was frontmost. The observer therefore
  rejected the strict unchanged-foreground precondition.
- Foreground sample counts were: `tuicommander` 301, `Google Chrome for Testing`
  35, and `tuicommander-agent-test` 8. Of the latter, four samples reported our
  owned application PID 16786; the others reported different PIDs. At **every**
  sample, both `document.hasFocus()` and native `isFocused()` were false.
  The observer's `agent-test took focus` failure means its application became
  frontmost; the evidence does not show its window became key or explain who
  activated the application.
- The first cumulative frame gap above one second appeared at elapsed
  **601,098 ms**, while the document was visible and daily TUIC was frontmost.
  Its cause is unknown. Animation frames continued afterward.
- Initial physical window geometry was position `(1632, 1092)`, size
  `1280 × 840`. At elapsed **1,144,354 ms**, it was `(1448, -1085)`, size
  `938 × 475`. At **1,149,509 ms**, it was `(1280, -1142)`, size `1106 × 532`,
  and remained there. The owned application was frontmost during these changes,
  but native window focus remained false. The observer did not request these
  changes; their external cause is not established.
- The owned main window remained at native layer 5 and on-screen in the retained
  samples. These window-server fields are **not** `NSWindow.occlusionState`.

The complete transition list and first one-second frame-gap observation are in
`summary.json`. Do not convert these unrelated failures into a visibility RED.

## Retained evidence and cleanup

`presentation.json` retains every sample and the owned instance's window-server
rows. Rows for other applications' windows were removed; their foreground process
identity remains. The raw local evidence is in
`~/Gits/.tmp/results/fix-2095-long/`. The SHA-256 of its original
`presentation.json` is:

```text
0935227a5e1c943c09817113b4c6c8e313f19a3aa375b3ec111bf4b2f6cb979f
```

`observer-result.txt` preserves the command's failure output. `cleanup.json`
records successful `make agent-test-stop`, an empty session list, and no remaining
owned worktree processes. The make supervisor exited 2 after the intended child
termination (the desktop process received SIGTERM); this is not a cleanup leak.

No acceptance criterion about cause, a production fix, or a five-minute post-fix
GREEN is checked from this negative observation.
