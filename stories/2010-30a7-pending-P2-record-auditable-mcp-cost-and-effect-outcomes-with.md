---
id: 2010-30a7
title: Record auditable MCP cost and effect outcomes without response bodies
status: pending
priority: P2
type: feature
created: "2026-10-17T16:09:53.975Z"
updated: "2026-10-22T09:04:04.812Z"
dependencies: []
started_at: "2026-10-22T09:03:05.518Z"
---

# Record auditable MCP cost and effect outcomes without response bodies

## Problem Statement

Candidate 8. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, sections 1, 3 and 4. Measured evidence includes a historical is_error:true wrapper around {"state":"rendering","error":null,...}; an apparent "41-hop single send" is explicitly unsafe because batch targets and intent differ. Runtime Codex custom-call counts cannot be derived by evaluating transcript JavaScript. Existing 1550-7415 records inbox caller/owner/message ids only; ego 495-e668 is parked OTel export, not this local operation/effect record. Extend authoritative call instrumentation rather than infer effects from wire error flags.

## Acceptance Criteria

- [ ] RED: mcp_outcome_record_separates_transport_application_and_effect - catches: a rendering-success payload being marked as application failure or a lost reply being recorded as no effect
- [ ] Record call id, operation, target, response text bytes, transport error, application error and effect certainty separately, without secrets or full bodies by default.
- [ ] Carry a parent intent or operation identifier so distinct recipients in one batch are not classified as retries or one long send chain.
- [ ] A captured fixture with permission denial, application refusal, rendering success and lost reply yields distinct verifiable outcomes; unknown effect remains unknown.
- [ ] Expose a bounded redacted audit projection that reports actual runtime Codex custom calls without evaluating transcript JavaScript, and document byte measurement and retry grouping rules.
- [ ] Coordinate client correlation with ego without implementing or unparking optional OTel export; preserve existing inbox audit records and keep response bodies excluded.

## Related

- 1550-7415
- ego:495-e668

## Work Log

### 2026-10-22T09:03:39.162Z - REALITY: effect=Capture four genuine MCP outcomes without response bodies; state=actual client observations and isolated server audit projection; assumptions=agent-test can render speech, checked before build; radius=fixture and provenance only. Preflight found agent_test::enable rejects the dictation feature, so rendering-success prerequisite is being traced before starting expensive native build.

### 2026-10-22T09:04:04.589Z - BLOCKED: Required rendering success cannot occur in the authorized agent-test instance. src-tauri/src/agent_test.rs:30-32 rejects any dictation-enabled build; mcp_transport_ancillary.rs:936-949 selects the no-audio handler without that feature. Existing mdkb report 7837 already records genuine client permission denial separately from server ingress. No client run, desktop launch, build or tests started; no response fabricated. Need authorization for an isolated audio-capable fixture capture design, not daily-instance access.
