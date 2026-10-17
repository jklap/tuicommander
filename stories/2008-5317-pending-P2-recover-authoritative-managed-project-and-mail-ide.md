---
id: "2008-5317"
title: "Recover authoritative managed project and mail identity after reconnect"
status: pending
priority: P2
type: feature
created: 2026-10-17T16:09:53.464Z
updated: 2026-10-17T16:09:53.464Z
dependencies: []
---

# Recover authoritative managed project and mail identity after reconnect

## Problem Statement

Candidate 6. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, sections 1 and 3. Measured historical evidence: "118 project-required" and "114 not-registered failures"; two inbox calls in ego 01a12ae9 both returned "You are not registered". Rewording a request cannot repair connection identity. Existing TUIC 554-9df0 already supports explicit replaces with migration/stranded-mail receipts; ego 346-5a2e fixes per-call stdio lifetime; TUIC 1819-b44b fixes ego spawn auto-binding. This story covers remaining authoritative project/restart recovery and bounded diagnostics, not reimplementation of those fixes. Reproduce on the current build before changing it.

## Acceptance Criteria

- [ ] RED: managed_reconnect_restores_project_without_identity_guessing - catches: a restarted managed caller losing its authoritative project/mail binding and entering repeated register or inbox repair
- [ ] A managed caller resolves its authoritative live session and project without model-driven register/list/register repair; headerless external callers receive an explicit missing-context result rather than a guessed project.
- [ ] Reconnect with identity replacement preserves existing migration versus stranded-mail reporting; no cross-project or foreign-session rebinding is allowed.
- [ ] Fixtures cover restart, stable tab versus PTY identity, external headerless caller and missing project, and preserve the completed 554-9df0 and ego 346-5a2e contracts.
- [ ] One failed inbox read returns an actionable bounded diagnostic and cannot trigger an unbounded identical retry loop; verify ego consumer behavior with the agreed identity contract.
- [ ] Record current-build reproduction or existing test evidence for each historical symptom before implementation and narrow the remaining change to proven gaps.

## Related

- 554-9df0
- 1819-b44b
- 1673-da49
- ego:346-5a2e

## Work Log
