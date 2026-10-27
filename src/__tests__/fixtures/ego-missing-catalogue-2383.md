# Recorded AI Chat catalogue refusal

`ego-missing-catalogue-2383.json` is the exact `data_json` of the daily TUIC log entry 104139, read with the MCP debug log tool on 2026-10-27. The entry timestamp is 1793106098126 ms, source `ai-chat`, message `listing conversations failed`.

The operation is `list`, not `load` or `prompt`. The session id is null. The backend `acp/connection.rs` maps the agent's JSON-RPC error into `agent_error` and preserves the operation. This fixture records the observed failure; it does not prove which durable record ego could not read.

No live connection, configuration, or session was changed to capture this error. The adjacent 2348 fixture records the distinct missing-session load refusal.
