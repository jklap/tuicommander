---
id: 1511-4ea3
title: Prevent Git Bash from converting Windows CMake debug flags into source paths
status: pending
priority: P1
type: fix
created: "2026-10-04T12:53:45.990Z"
updated: "2026-10-04T15:13:57.279Z"
dependencies: []
started_at: "2026-10-04T12:54:18.303Z"
---

# Prevent Git Bash from converting Windows CMake debug flags into source paths

## Problem Statement

CI run 37200663419 on bad67737b passes Windows Clippy but Tests fails the whisper-rs-sys CMake compiler probe: cl receives C:/Program Files/Git/Z7 as a source file. Tests invokes native Cargo through Git Bash with CMAKE_C_FLAGS and CMAKE_CXX_FLAGS=/Z7; MSYS converts the slash-prefixed environment value. Release and nightly also set /Z7. Native Windows validation remains CI-owned.

## Acceptance Criteria

- [x] Use MSYS-safe -Z7 for both CMake flag variables in CI, release and nightly while preserving other platform flags
- [x] Document why Windows CMake flags use -Z7
- [x] Parse all changed workflow YAML, pass fmt-changed main...HEAD and git diff --check; report that no native Windows build ran

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

### 2026-10-04T12:54:18.533Z - Contract: native MSVC receives the embedded-debug-info option unchanged when launched through Git Bash or the default Windows shell. State comes from CMAKE_C_FLAGS/CMAKE_CXX_FLAGS in the checked-in workflow steps. CI log 37200663419 records the converted C:/Program Files/Git/Z7 compiler argument; Microsoft documents slash/dash equivalence, and MSYS2 documents automatic environment path conversion. Use -Z7 consistently in CI/release/nightly; preserve macOS/Linux flags. Static YAML parsing and formatting/whitespace checks are the authorized validation; no native Windows run or new runtime test.

### 2026-10-04T12:54:46.968Z - Implemented on fix/ci6-windows-z7: -Z7 in both CMake environment variables for CI Clippy/Tests and release/nightly. Updated docs/guides/ci.md. YAML safe_load passed for all three changed workflows; fmt-changed main...HEAD and git diff --check main...HEAD passed. Instruction link check passed (2 pairs); the reported prior violation is absent in this checkout. No Cargo build, runtime test or native Windows execution ran. Coordinator owns CI validation and closure after landing. Risk low: configuration spelling only, documented MSVC-equivalent option; no runtime or shared-state change.

### 2026-10-04T15:13:54.460Z - Status reset to pending by the coordinator on 2026-10-04: no live agent; last state: Implemented on fix/ci6-windows-z7: -Z7 in both CMake environment variables for CI Clippy/Tests and release/nightly.

