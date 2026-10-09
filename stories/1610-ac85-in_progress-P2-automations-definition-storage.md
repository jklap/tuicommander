---
id: 1610-ac85
title: "Automations: Definition storage"
status: in_progress
priority: P2
type: feature
created: "2026-10-09T16:14:58.201Z"
updated: "2026-10-09T16:15:32.384Z"
dependencies: []
plan: plans/automations-scheduler.md
plan_step: Step 1
started_at: "2026-10-09T16:15:32.383Z"
---

# Automations: Definition storage

## Problem Statement

Implement definition storage for the approved Automations scheduler. See the linked plan for the fixed contract and exclusions.

## Acceptance Criteria

- [ ] Persist typed automation definitions in automations.json through ConfigFile<T>, with per-id locked mutations that preserve unrelated definitions and global settings.
- [ ] Store name, literal prompt, run config, repository, workspace mode/base branch, cron and timezone, enabled, grace seconds, overlap skip, max duration and optional precheck; global concurrency defaults to 2.
- [ ] Reject blank required fields, zero limits, duplicate or missing ids, malformed or unsupported documents without overwriting recovery data; independent writers retain both edits.
- [ ] Document the definition schema and add a post-restart verification item; cron/IANA parsing and local-zone default belong to Step 2.

## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## Work Log

### 2026-10-09T16:15:32.180Z - Contract: consumer reads typed definitions and max_concurrent_runs=2 from an absent file; real state is an isolated automations.json through ConfigFile locks. Per-id edits read fresh disk state; corrupt and unsupported files must not be overwritten. No external agents or cron parsing in this story. REALITY: effect=durable per-id definitions without lost unrelated edits; state=real JSON plus ConfigFile process/file locks; assumptions=OS advisory file locks and persist_atomic; radius=definition storage, later scheduler/API consumers.

