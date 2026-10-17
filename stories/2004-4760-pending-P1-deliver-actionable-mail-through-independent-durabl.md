---
id: "2004-4760"
title: "Deliver actionable mail through independent durable subscriptions"
status: pending
priority: P1
type: feature
created: 2026-10-17T16:09:52.452Z
updated: 2026-10-17T16:09:52.452Z
dependencies: []
---

# Deliver actionable mail through independent durable subscriptions

## Problem Statement

Candidate 1. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, sections 4-5. Measured evidence: "1,695 (38.5%)" of 4,403 message deliveries were lifecycle state_change messages; "72 same-operation read loops", 268 calls, longest 15. Deliveries are not unique transitions and useful waits are not waste. Pump and watcher wrappers own cursor locks, noise suppression and duplicate handling. Existing 554-9df0 provides one server cursor and 1123-7b07 coalesces lifecycle notices; neither is an independent durable filtered subscription.

## Acceptance Criteria

- [ ] RED: actionable_subscription_isolates_cursors_and_wakes_once - catches: lifecycle-only traffic waking an actionable subscriber or another reader consuming its RESULT
- [ ] A subscriber can exclude routine lifecycle and idle events without consuming another subscriber unread mail; exit and failure events remain selectable.
- [ ] Bounded batches report cursor, overflow or missed count, and continuation; reconnect resumes without duplicate result delivery or lost retained BLOCKED and RESULT messages.
- [ ] Replay captured lifecycle-only traffic without an actionable wake, then a RESULT with exactly one actionable wake; exercise two independent subscribers and interrupted resume without a model-driven wait loop.
- [ ] Document retention and acknowledgement semantics, preserve legacy cursor behavior, and cover all exposed MCP and HTTP notification transports.

## Related

- 554-9df0
- 1123-7b07

## Work Log
