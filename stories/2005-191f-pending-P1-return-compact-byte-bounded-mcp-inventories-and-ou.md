---
id: "2005-191f"
title: "Return compact byte-bounded MCP inventories and output continuations"
status: pending
priority: P1
type: feature
created: 2026-10-17T16:09:52.689Z
updated: 2026-10-17T16:09:52.689Z
dependencies: []
---

# Return compact byte-bounded MCP inventories and output continuations

## Problem Statement

Candidate 2. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, sections 4-5. Measured maxima: "49,609" bytes for agent inbox, "49,162" for session output, "9,195" for list_peers. Explicit since=0 and limit=400 requests caused the largest examples; these are not default-response claims. A scoped lookup and bounded response avoid loading a full roster or terminal text when only identity and state are needed. Existing instruction-budget story 1947-3d17 addresses prompt prose, not response payloads.

## Acceptance Criteria

- [x] RED: oversized_mcp_item_respects_byte_budget_and_continues - catches: one large inbox or terminal item bypassing the response byte bound or losing retained text
- [x] Inventory supports scoped recipient and session lookup plus a compact projection with identity, state, activity age and explicit omitted fields; unknown state and failed inventory are not empty success.
- [x] Inbox and terminal output accept a documented byte budget with lossless continuation or reference; define whether envelope bytes count and enforce that definition even for one oversized item and multibyte UTF-8.
- [ ] Captured large-response fixtures meet the selected budget; concatenating continuations reconstructs retained text exactly without changing inbox cursor semantics.
- [ ] Cover empty results, expired retention, concurrent new output and invalid continuation; document explicit omissions rather than silently truncating.
- [ ] GREEN: batch integrator runs oversized_mcp_item_respects_byte_budget_and_continues, captured_mcp_response_reconstructs_inside_byte_budget, response_continuations_fail_closed_and_expire, empty_snapshot_acknowledges_once_and_bad_budgets_do_not_capture, compact_inventory_keeps_unknown_state_and_scoped_recipient_errors, budgeted_terminal_output_snapshot_survives_concurrent_output, session_output_raw_pages_report_eviction_and_exhausted_offsets and remote_mcp_output_preserves_clean_raw_and_tombstone_contracts; record emitted before/after bytes and page calls with --success-output immediate.
## Related

- 1947-3d17
- 554-9df0

## Work Log

### 2026-10-17T16:14:00.728Z - Contract: byte_budget opts inbox/output into immutable JSON-text chunks. The budget counts compact JSON response UTF-8 bytes, including the page envelope, excluding MCP JSON-RPC/content framing. Continuations are owner/action-bound, replayable for 5 minutes, and fail explicitly after expiry or cache eviction. Inbox advances only on the final chunk; concurrent mail/output stays outside the snapshot. Compact inventories expose identity, state (unknown if absent), activity_age_ms (null if absent), and omitted_fields. Existing unbudgeted cursor and response contracts remain unchanged. Worst case: retained snapshot cache is capped at 32 MiB; a single inbox may contain 100 x 64 KiB messages. No external-system behavior is assumed.

### 2026-10-17T16:49:39.119Z - RED confirmed on a529c83e39eb70ecebd8bf19cbdd0810b66d0550: oversized_mcp_item_respects_byte_budget_and_continues failed remotely because the response was 63150 bytes against byte_budget=4096. One test executed; nextest exit 100. Command: build-slot.sh --remote bash -lc scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -E test(oversized_mcp_item_respects_byte_budget_and_continues). Log: ~/Gits/.tmp/2005-red.log. Compilation took 10m06s; test failed in 0.054s.

### 2026-10-17T17:50:26.575Z - Implementation contract and evidence (impl-2005-191f): branch feat/2005-191f-mcp, base a529c83e39eb70ecebd8bf19cbdd0810b66d0550. byte_budget=1024..65536 counts compact JSON page-envelope UTF-8 bytes, excluding MCP/HTTP framing. Immutable owner/operation-bound snapshots expire after 300s or bounded-cache eviction (128 entries / 32 MiB serialized text); cursors and delivery acknowledgements advance only on final inbox chunk. Unknown inventory state/age remain explicit; scoped lookups and remote failures are not empty success. RED remote oversized_mcp_item_respects_byte_budget_and_continues failed at 63150 > 4096 bytes on the base commit (~/Gits/.tmp/2005-red.log, exit 100, 1 executed test). Captured historical fixtures are 49162-byte terminal output and 49609-byte inbox replies cited by the audit. Two remote cargo check --tests runs succeeded; final production source check log ~/Gits/.tmp/2005-final-check.log, 3m24s, no warnings. Subsequent change adds only the second captured fixture and its existing generic pager test input. No GREEN runtime run per managed batch policy; after-size/page-call measurements are not yet observed. Integration must run the named regressions with success output enabled to capture measured before/after bytes and calls. No native Mac build or desktop instance was started; restart check added to to-test.md. Existing retention regression: session_output_raw_pages_report_eviction_and_exhausted_offsets.

### 2026-10-17T18:26:10.443Z - Review-fix contract: overlapping byte snapshots must acknowledge a monotonic eviction watermark, never subtract an already-acknowledged historical count; fresh losses remain observable. A remote daemon that ignores the requested byte-page capability must fail explicitly within the budget, not pass through its full body. Compact projections derive age from backend last_activity_ms, while unknown time stays null. Exact registered slash-containing recipient names take precedence over connection syntax. Byte-budget wait is explicitly unsupported before consumption: wait currently acknowledges delivery before its response, and inventing a second deferred-wait ack model would conflict with the pending 2004 shared subscription/ack reconciliation. HTTP/IPC output tokens intentionally share local-user continuation scope (not authorization). RED tests added on dee4e152e3b8963a77f8a2b3198940df5caa983d for the five behavioral findings; one filtered remote run follows.
