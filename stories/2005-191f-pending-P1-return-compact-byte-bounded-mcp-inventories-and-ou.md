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

- [ ] RED: oversized_mcp_item_respects_byte_budget_and_continues - catches: one large inbox or terminal item bypassing the response byte bound or losing retained text
- [ ] Inventory supports scoped recipient and session lookup plus a compact projection with identity, state, activity age and explicit omitted fields; unknown state and failed inventory are not empty success.
- [ ] Inbox and terminal output accept a documented byte budget with lossless continuation or reference; define whether envelope bytes count and enforce that definition even for one oversized item and multibyte UTF-8.
- [ ] Captured large-response fixtures meet the selected budget; concatenating continuations reconstructs retained text exactly without changing inbox cursor semantics.
- [ ] Cover empty results, expired retention, concurrent new output and invalid continuation; document explicit omissions rather than silently truncating.

## Related

- 1947-3d17
- 554-9df0

## Work Log
