# Claude awaiting exit before input (1985-45bd)

- Source: real Claude Code 2.1.308 in isolated `make agent-test AGENT_TEST_INSTANCE=agent-test-1986 AGENT_TEST_PORT=19897`, captured on 2026-10-23 through the instance diagnostics API.
- Backend baseline: `11679d5f1415844d93f1c209b27cd9f8d8994abc`.
- Geometry: 14 rows, 37 columns, recorded in TCAP v2.
- Capture: `claude-await-exit-before-input-1985.tcap`.
- SHA-256: `8248ed40efacf0c981564fd717cc3f868585372b830aea28135999b2a3ecd645`.
- Session: `02d9104a-f476-46e9-85d2-00f5975bff6b`.

Claude opened a real AskUserQuestion asking for a test color, with Red and Blue choices. SIGTERM was sent only to the verified Claude child started for this check. No shell input followed its exit. The isolated API reported no agent identity and an idle shell, but retained a confident question and awaiting state.

Capture was enabled before session creation. Terminal bytes and input records are unmodified. TCAP does not contain process observations: the replay applies the shared foreground-observation seam after the captured output, using the confirmed shell return from the live API/OS evidence. It also checks that an unknown helper observation does not withdraw the question.

Live evidence is retained in the orchestrator report directory `reports/tuicommander/img/2026-10-23-1985/`: `refix-await-before.json`, `refix-await-after.json`, `refix-await-agent.txt`, and `refix-exit-logs.json`. The `refix-await-after.json` snapshot precedes all new shell input.
