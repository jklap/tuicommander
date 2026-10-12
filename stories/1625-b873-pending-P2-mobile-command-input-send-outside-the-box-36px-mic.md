---
id: 1625-b873
title: "Mobile command input: send outside the box (36px), mic inside, same wrap width"
status: pending
priority: P2
type: feature
created: "2026-10-12T16:36:07.194Z"
updated: "2026-10-12T16:39:43.610Z"
dependencies: []
---

# Mobile command input: larger send button, mic/send moved left, same wrap width

## Problem Statement

Send button is 24px with a 24px hit area; Boss wants it larger and both buttons further left without changing the 286px text wrap width at 390px (mock variant A).

## Acceptance Criteria

- [x] GREEN: TMPDIR=$HOME/Gits/.tmp/ pnpm vitest run src/mobile passes
- [x] RED: CommandInputLayout.test.ts — catches: send hit area under 44px tall or overlapping the mic, or right-side budget (paddings + mic + send) above 62px (stale after variant C (d1b2666d8): budget is 70px; replaced by the next RED criterion)
- [ ] [VISUAL] Boss checks it on the iPhone
- [x] GREEN: build-slot.sh scripts/with-test-tmp.sh pnpm vitest run (full suite), 734 files / 9227 tests passed
- [x] REALITY: effect=send 36x36 outside the composer with 42x44 hit area, mic 22x38 inside; text wrap width 286px at 390px ; state=measured in Chrome (agent-browser) at 390x844, 820x1180, 1024x1366 via worktree Vite ; assumptions=iOS Safari lays out flex/padding as Chrome (Boss [VISUAL] check pending) ; radius=mobile CommandInput only, Queue menu position unchanged (right:12px of form)
- [x] TEST-CHANGE: src/mobile/__tests__/MobileVoiceControl.test.tsx::appends interim and final speech without focusing, duplicating hypotheses or auto-submitting — old expectation wrong because send moved out of the composer, so it is no longer the mic's next sibling
- [x] RED: CommandInputLayout.test.ts — catches: send hit area under 44px tall or overlapping the mic, right-side budget above 70px (variant C), or a fixed textarea width
## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-12T17:02:27.643Z - critic-1625 PASS round 1: real layout via agent-browser 390/820/1024 (textarea 292/722/926, fluid), queue menu clear of send, full vitest 9227 pass. Findings F1 (criterion text) fixed; F2 (budget test omits attach/padding), N1 (mic hit 22 wide), N2 (focus ring) left as improvements.

