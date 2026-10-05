---
id: 1477-40ab
title: "Nightly update-notes fails: release body over 125000 characters"
status: pending
priority: P3
type: fix
created: "2026-10-04T05:34:35.155Z"
updated: "2026-10-04T15:13:32.487Z"
dependencies: []
started_at: "2026-10-04T09:28:35.209Z"
---

# Nightly update-notes fails: release body over 125000 characters

## Problem Statement

Nightly run 37177771178 (73036c886, 2026-10-04) job update-notes: gh release edit nightly returns HTTP 422 body is too long (maximum is 125000 characters). The notes list every commit since the last stable tag v1.7.6 (no v1.7.7 tag exists), about 1970 commits.

## Acceptance Criteria

- [x] The nightly notes stay under the GitHub 125000-character limit for any commit count (truncate with a link to the compare view)
- [ ] update-notes passes on the next nightly run

## Proof

- [ ] [completeness] Completeness
- [ ] [feature-availability] Feature availability
- [ ] [robustness] Robustness
- [ ] [resilience] Resilience
- [ ] [security] Security
- [ ] [defense-in-depth] Defense in depth
- [ ] [input-validation] Input validation
- [ ] [thread-safety] Thread safety
- [ ] [configurability] Configurability

## Work Log

### 2026-10-04T09:28:34.223Z - Contract: nightly readers see grouped recent commit notes and, when they exceed the body budget, a complete compare-view link. The real state is git history since LAST_STABLE, rendered by nightly.yml:286-320. Both criteria are currently unmet: there is no body bound before gh release edit at line 322; next-nightly success requires landing and a live workflow run, which this peer cannot claim. Preserve generation and cap its final UTF-8 notes file at 120000 bytes, reserving the truncation footer and cutting only at complete line boundaries. This is conservative against the documented 125000-character rejection in run 37177771178; no fixture or mock of GitHub is needed. release.yml uses a version-specific changelog section, not the same notes step. Target CLI regression cases: short notes remain identical, oversized history and multibyte text stay below the limit with a compare link, an oversized single line cannot escape the bound.

### 2026-10-04T09:32:13.260Z - Implemented in 757d072832cf2935e5a4ceb1555be70f0f640767 on fix/wave1-release-scripts. scripts/test-release-notes-limit.sh passed in the single final targeted lane run (exit 0): unchanged empty/short/exact-budget bodies, long commit history, complete-line retention, compare-link preservation, idempotency, UTF-8 subjects, oversized single line. fmt-changed.sh main...HEAD and git diff --check passed. Criterion 2 remains unchecked: a next-nightly success requires landing and the real GitHub Actions run; no push or release edit was performed. release.yml does not share this notes generator. Pre-fix commit: d08a9d2ca192fa876dda8d35769e85e2663793d7.

### 2026-10-04T09:40:03.618Z - Critic contract: a failed gh release edit must fail the actual update-notes shell step. Execute the workflow run block with real local git history and an injected gh function that exits 42; assert the shell exit, not the stub output. Existing cap CLI cases do not exercise the workflow error boundary. No production seam or GitHub/token access is needed.

### 2026-10-04T13:10:05.019Z - Cleanup audit: Leave open as explicitly instructed until the next nightly. Run 37200663339 on bad67737b is already green including update-notes, but no later nightly evidence was requested/obtained.

### 2026-10-04T15:13:30.775Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Cleanup audit: Leave open as explicitly instructed until the next nightly.

