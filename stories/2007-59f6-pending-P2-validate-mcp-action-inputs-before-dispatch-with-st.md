---
id: "2007-59f6"
title: "Validate MCP action inputs before dispatch with structured repair errors"
status: pending
priority: P2
type: feature
created: 2026-10-17T16:09:53.204Z
updated: 2026-10-17T16:09:53.204Z
dependencies: []
---

# Validate MCP action inputs before dispatch with structured repair errors

## Problem Statement

Candidate 4. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, section 3. Measured evidence: "33 progress length failures", "24 missing text", "18 missing title", "11 missing repository path". The directly verified progress retry sequence rejected "582", then "543", then "502" characters before success. These are historical observations, not a claim that every current action fails. Action-specific contracts and structured repair details can prevent repeated trial-and-error; client-side static validation must use the same advertised contract.

## Acceptance Criteria

- [ ] RED: action_validation_rejects_observed_bad_inputs_before_effects - catches: an oversized progress message or missing action field reaching dispatch and producing only an opaque retry error
- [ ] Action schemas encode legal actions and conditional required fields; progress advertises its 500-character limit and all static violations are rejected before side effects.
- [ ] Repair errors identify field or JSON path, limit or allowed values, retryability, and effect or outcome certainty; permission and safety refusals remain refusals, not auto-retry advice.
- [ ] Fixtures cover progress lengths 582, 543 and 502, the valid boundary and Unicode counting rules, missing tab title, invented agent list action and missing worktree path.
- [ ] An ego client contract fixture detects static violations locally without a server round trip; coordinate that consumer change explicitly rather than weakening server validation.
- [ ] Document the error contract and verify consistent validation across MCP and corresponding HTTP or IPC entry points.

## Work Log
