---
id: "2010-30a7"
title: "Record auditable MCP cost and effect outcomes without response bodies"
status: pending
priority: P2
type: feature
created: 2026-10-17T16:09:53.975Z
updated: 2026-10-17T16:09:53.975Z
dependencies: []
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
