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

- [x] Persist typed automation definitions in automations.json through ConfigFile<T>, with per-id locked mutations that preserve unrelated definitions and global settings.
- [x] Store name, literal prompt, run config, repository, workspace mode/base branch, cron and timezone, enabled, grace seconds, overlap skip, max duration and optional precheck; global concurrency defaults to 2.
- [x] Reject blank required fields, zero limits, duplicate or missing ids, malformed or unsupported documents without overwriting recovery data; independent writers retain both edits.
- [x] Document the definition schema and add a post-restart verification item; cron/IANA parsing and local-zone default belong to Step 2.
- [x] REALITY: effect=typed durable definitions preserve unrelated writer edits; state=real isolated automations.json through ConfigFile locks; assumptions=OS advisory locks and atomic persistence, no external agent fixtures; radius=definitions and future scheduler/API consumers.
- [x] SYMPTOM: automation definitions cannot be loaded before implementation — replayed by: absent_file_defaults_to_two_without_creating_a_definition_document at 44126e55394fc0c569743478a5a060dad53881ac.
- [x] RED: absent_file_defaults_to_two_without_creating_a_definition_document — catches: missing definition storage/defaults; pre-implementation 44126e55394fc0c569743478a5a060dad53881ac, 1 executed/1 failed, exit 100; load returned Automation definition storage is not implemented.
- [x] GREEN: scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E test(automations::tests) via mbx/build-slot; 9 passed, exit 0; green.log.
## Proof

- [ ] [completeness] Completeness
- [ ] [robustness] Robustness
- [ ] [security] Security

## QA

Nine native macOS targeted storage tests pass. Files/schema, concurrent duplicate claim, fresh per-id edits, no-op bytes, corrupt recovery and invalid wire inputs inspected. Cron parser, scheduler, IPC/HTTP, UI, full suites and native desktop launch are intentionally outside Step 1; coordinator performs critic/integration before landing.

## Work Log

### 2026-10-09T16:15:32.180Z - Contract: consumer reads typed definitions and max_concurrent_runs=2 from an absent file; real state is an isolated automations.json through ConfigFile locks. Per-id edits read fresh disk state; corrupt and unsupported files must not be overwritten. No external agents or cron parsing in this story. REALITY: effect=durable per-id definitions without lost unrelated edits; state=real JSON plus ConfigFile process/file locks; assumptions=OS advisory file locks and persist_atomic; radius=definition storage, later scheduler/API consumers.

### 2026-10-09T16:29:03.905Z - RED verified: 44126e55394fc0c569743478a5a060dad53881ac; targeted absent_file_defaults_to_two_without_creating_a_definition_document executed and failed because load returned Err Automation definition storage is not implemented, exit 100 (red-runtime.log). Initial missing-import compile failure at 4d251ec2c was not behavioral RED. GREEN: scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E test(automations::tests), via mbx/build-slot/tuic-bg with Gits test temp root: 9 passed, exit 0, build 2m16s/test 0.076s. Tested source based on HEAD 44126e553 with 10 dirty paths (storage plus docs/plan/story); automations source SHA256 c15161ee91dfac26eba7e07ef0745403d92c24ca1920a7d2b6924bfc296231fc. Subsequent lib.rs change is rustfmt-only module order/attribute formatting. No frontend or tsc, desktop launch, full suite or cross-platform build. Definitions are storage only; Step 2 owns cron/IANA/local default. Risk medium (durable shared state); coordinator owns critic and landing.

