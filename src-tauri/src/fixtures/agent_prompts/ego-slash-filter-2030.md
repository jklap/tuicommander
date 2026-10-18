# Ego slash menu filtering capture

- Source: installed `/Users/stefano.straus/.local/bin/ego`, startup reports ego 0.1.0.
- Recorded on 2026-10-18 by fix-2030 through a disposable native Python PTY.
- Geometry: 41 rows, 128 columns, xterm-256color.
- Format: TUICCAP2, actual output read chunks and input writes with monotonic microsecond timestamps.
- SHA-256: `5a36ff273707ede6d3ebcabd938e2d6d97a7801518d0dd0b6c7ed6ae8bf6355a`.
- Input: `/`, `g`, `o`, `a`, `l`, four Backspaces, then `zzz`.
- Cursor position queries receive `ESC[1;1R`; these terminal replies are input records, not composer edits.
- No prompt was submitted and no model turn was run in this capture.

The broad `/` menu becomes exactly one `/goal` row at `/g`, `/go`, and `/goa`.
At the exact `/goal` spelling, ego closes completion and returns to its normal
composer. Later Backspaces and unmatched text do not leave menu rows on screen.
This proves that filtering already works in ego while idle. The capture does
not establish busy-turn or browser behavior.

Independent evidence: a disposable session created through TUIC's session MCP
surface also painted only `/goal` after raw `/` then `go` writes. Browser
verification could not proceed: the real HTTP backend showed its login form.
No credential store was read.
