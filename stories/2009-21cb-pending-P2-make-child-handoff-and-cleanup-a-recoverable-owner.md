---
id: "2009-21cb"
title: "Make child handoff and cleanup a recoverable ownership transaction"
status: pending
priority: P2
type: feature
created: 2026-10-17T16:09:53.712Z
updated: 2026-10-17T16:09:53.712Z
dependencies: []
---

# Make child handoff and cleanup a recoverable ownership transaction

## Problem Statement

Candidate 7. Source: ~/Gits/personal/orchestrator/reports/tuicommander/2026-10-17-mcp-usage-audit.md, section 5. Decisive repository evidence: child-close.sh and child_state.py "Persist successor disposition before close, lock/atomically retire ownership, transfer secondary worktrees, retain tables when close fails". Commit 635a151b prevents cleanup closing the calling tab. Existing 1209-cc47 and 1319-a52a already protect keep-open/BLOCKED children from idle-close; this story must not duplicate those protections. The missing composed transaction must retain recoverable ownership across close and cleanup failure; coordinator staffing and merge/test policy remain local.

## Acceptance Criteria

- [ ] RED: interrupted_child_handoff_preserves_successor_ownership - catches: closing a child before durable disposition or cleanup retry deleting a working successor checkout
- [ ] Durable disposition and successor ownership precede closing; failures retain recoverable ownership and return a resumable transaction handle, including secondary worktrees.
- [ ] Caller protection resolves stable and PTY identity correctly so a restored coordinator cannot close itself.
- [ ] Dirty, unmerged and foreign-owned checkouts remain protected; retry after partial close or removal never force-deletes or closes a working successor.
- [ ] Fault-injection fixtures cover failure before ownership persistence, after close and after one worktree cleanup; retries are idempotent and report completed versus pending steps.
- [ ] Keep staffing decisions, task selection, repository merge policy and integration validation outside this transaction; preserve existing idle-close and hold-open contracts.

## Related

- 1209-cc47
- 1319-a52a
- 1894-7683

## Work Log
