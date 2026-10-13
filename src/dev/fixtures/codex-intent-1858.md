# Recorded Codex intent frame

`codex-intent-1858.json` is a full Rust terminal wire frame from the existing,
unchanged `codex-streaming-intent-20260924.tcap` capture. The JSON records the
capture SHA-256 and the inclusive, zero-based record index. The legacy capture
has no geometry header; its committed replay oracle supplies 41 rows × 128 columns.

Generation used `pty_capture::decode_capture`, then `TerminalGrid::new(41, 128,
10000)`, and `TerminalGrid::process` on each output record through record 1072.
`TerminalGrid::serialize_full_frame` produced the base64 payload. The `rows` field
is `TerminalGrid::screen_text_rows` from that same state. No row or cell was edited.
The row `• intent: validating the streaming capture (Capture test)` is screen row 24.

Open `/codex-intent-harness.html` on this checkout's Vite server. The page mounts
the production `CanvasTerminal`, `TauriTransport`, decoder, renderer and overlay.
Its fixture-only IPC shim supplies the recorded frame; it never contacts a TUIC
instance or executes terminal input. This proves rendering of the recorded Rust
frame; it does not exercise live PTY delivery or resizing the replayed terminal.
