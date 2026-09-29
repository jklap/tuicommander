# Claude AskUserQuestion dismissed with Esc

Claude Code 2.1.280 (haiku) in the disposable `ui-review-sandbox` session,
captured by the running TUICommander `/diagnostics/capture` tap on 2026-09-29.
The fixture preserves framed records 0–195, including the recorded 40×220
geometry, original input/output chunks, directions and monotonic timestamps.
The source capture is `3a3b033d-00f9-4293-94aa-d076ee813a73.tcap` (SHA-256
`06bf7bcd0f4c6d5c5bd6a1e13d637e63d9dc0a7f1ff29e80344a6ceb4250fba3`).
The fixture SHA-256 is
`3ef4c9b9e81ed0fd6ca6db60ee732844500a2a43d9881642250599ffaacd778a`.

Record 184 carries `Claude needs your permission`; record 185 is the single
Esc input byte. Record 188 renders `User declined to answer questions` and
`Worked for 4s · done` above the ready composer. A later prompt begins at
record 196 and is excluded. The regression test replays the source bytes
through the PTY chunk processor and the event-bus session-state accumulator.
