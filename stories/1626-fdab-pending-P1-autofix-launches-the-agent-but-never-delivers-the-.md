---
id: "1626-fdab"
title: "Autofix launches the agent but never delivers the seed prompt"
status: pending
priority: P1
type: fix
created: 2026-10-13T06:44:05.661Z
updated: 2026-10-13T06:44:05.661Z
dependencies: []
risk: medium
---

# Autofix launches the agent but never delivers the seed prompt

## Problem Statement

SYMPTOM (Boss, 2026-10-13): Autofix on issue 125 created worktree autofix/issue-125, opened a terminal and started Claude; Claude sat idle with an empty composer and the branch had 0 commits. Log tuic.log.2026-10-13, session d027b162: launch command sent (busy user-submit 06:11:10), agent idle 06:11:15, then NO further write to the PTY and no error. LAYER: frontend start-work delivery (src/hooks/git/startWork.ts). createStartWork finds the agent terminal by comparing terminal.agentLaunchCommand with the configured launch command, but useAgentPolling rewrites agentLaunchCommand to the discovered real command once the agent runs (and the lookup runs after an awaited getDiffStats), so the lookup returns undefined and deliverWhenReady is never called: the brief is dropped silently. Second defect: typing into a composer after a heuristic ready check can answer a startup dialog with Enter. FIX: look the terminal up by the id setupNewWorktree created; pass the brief as a launch argument where the CLI supports an interactive initial prompt (submit delivery), keep typing as the fallback.

## Acceptance Criteria

- [REJECTED] RED: start_work_delivers_brief_after_agent_launch_command_is_rewritten - catches: terminal looked up by mutable agentLaunchCommand, brief silently dropped (Replace proposed name with the executed regression name.)
- [REJECTED] RED: per-agent initial-prompt argv builder tests (quoting, newline, length limit falls back to typing) - catches: unescaped prompt reaches the shell, over-long prompt breaks the command line (Replace aggregate placeholder with concrete quoting regression and recorded live boundary probe.)
- [REJECTED] GREEN: submit delivery launches the agent with the brief as argv for claude, codex, gemini, pi, grok and types nothing; draft delivery and unsupported agents keep typing (pi has no option terminator support and correctly uses typing; replace with executed command evidence.)
- [x] Docs and sync-matrix updated if required
- [x] RED: separates_the_initial_prompt_from_a_trailing_variadic_add_dir — catches: Claude --add-dir consumes the initial task brief as a directory
- [x] RED: resume_does_not_resubmit_the_initial_brief_preserved_by_discovery — catches: Claude discovery persists the seed brief and resume resubmits it
- [x] TEST-CHANGE: src/__tests__/hooks/startWork.test.ts::autofix_submits_the_confirmed_prompt — old expectation wrong because submit delivery now seeds the initial message via launch argv rather than a later PTY Enter
- [x] TEST-CHANGE: src/__tests__/hooks/startWork.test.ts::brief_is_not_typed_when_the_shell_is_in_the_foreground — old expectation wrong because Codex submit now uses argv; an unsupported agent exercises the remaining typed fallback
- [x] brief_is_delivered_when_discovery_rewrites_the_launch_command_during_setup passes in the full package run; terminal identity survives discovery rewriting the launch command.
- [x] Prompt quoting and newline cases pass for claude, codex, gemini and grok; the recorded live probe demonstrates 16384-character argv and 16385-character typing fallback.
- [x] GREEN: build-slot.sh scripts/with-test-tmp.sh pnpm vitest run — 743 files, 9318 tests passed; submit uses argv for claude, codex, gemini and grok, while pi, draft and unsupported agents use typing. rb cargo nextest run -p tuicommander — 5446 passed, 21 intentional skips; cargo clippy -p tuicommander --all-targets -- -D warnings — exit 0.
- [x] RED: resume_drops_a_seed_brief_containing_a_shell_escaped_apostrophe — catches: escaped apostrophe breaks tokenization and resume resubmits the seed brief (9054e77250; expected prompt-free command, received entire brief).
- [x] REALITY: effect=Claude receives the brief as a prompt instead of an add-dir value ; state=run-config argv and shell command produced by launchCommandWithPrompt ; assumptions=installed Claude help lists variadic add-dir, round-1 CLI probe recorded ; radius=Start Work launch commands. RED: separates_the_initial_prompt_from_a_trailing_variadic_add_dir
- [x] REALITY: effect=resume does not submit the seed task again ; state=Claude discovery reconstructs argv and polling persists agentLaunchCommand ; assumptions=POSIX shell quoting and installed Claude resume flag, recorded live probe ; radius=restored agent sessions. RED: resume_does_not_resubmit_the_initial_brief_preserved_by_discovery
- [x] REALITY: effect=resume remains prompt-free for a work item containing an apostrophe ; state=quote_if_needed and escapePosixShellArg produce the escaped apostrophe spelling used by the regression ; assumptions=POSIX quoting, apostrophes already verified by the recorded real PTY probe ; radius=resume tokenization and original env/flags. RED: resume_drops_a_seed_brief_containing_a_shell_escaped_apostrophe
- [x] SYMPTOM: Claude treats the initial task as another directory — replayed by: separates_the_initial_prompt_from_a_trailing_variadic_add_dir
- [x] SYMPTOM: restored Claude submits the initial task again — replayed by: resume_does_not_resubmit_the_initial_brief_preserved_by_discovery
- [x] SYMPTOM: a brief containing an apostrophe remains appended to the resume command — replayed by: resume_drops_a_seed_brief_containing_a_shell_escaped_apostrophe
## Work Log

### 2026-10-13T06:54:25.541Z - Critic round 1 contract: effect=the agent receives the brief once as its first message; resume never repeats it. state=run-config args feed launchCommandWithPrompt; Claude discovery compose_launch_command retains every positional argv and useAgentPolling overwrites agentLaunchCommand. assumptions=installed claude --help declares --add-dir <directories...>; installed codex -c consumes one key=value, not variadic; Gemini, pi, grok help confirms interactive positionals. radius=start-work launch and restored Claude sessions. Independent regressions added before execution: trailing --add-dir requires an option terminator; discovered wrapped brief must be absent from resume. No production changes. Duplicate draft test removed because start_work_uses_repo_run_config already asserts the identical launch command.

### 2026-10-13T07:01:39.946Z - Critic round 1: REQUEST CHANGES. Full local package run via build-slot.sh scripts/with-test-tmp.sh pnpm vitest run: 9317 passed, exactly two critic tests failed on f935ae658a production; 743 files, 245.94s. Variadic failure: Expected claude --add-dir /repo/shared -- followed by quoted brief; received no --. Resume failure: received the entire wrapped brief after claude --resume UUID --model opus. Three unrelated async leaks in DictationToast and two existing useAppInit critic tests are reported to the coordinator. Smallest fixes: terminate options before positional brief; ensure Claude discovery cannot persist/replay the generated start-work brief. No new mechanism is needed. No production edits in critic round 1.

### 2026-10-13T07:45:24.943Z - Long-prompt live probe (Boss request): actual installed Claude 2.1.286 and Codex 0.162.1, real PTYs running zsh -f -i in scratch git directories under ~/Gits/.tmp/critic-1626-probe. Launch lines came from the actual TypeScript launchCommandWithPrompt source; actual sendCommand generated the bracketed-paste and Enter bytes. Brief: 12057 UTF-16 code units, 13032 UTF-8 bytes, multiline, apostrophes, dollar substitutions, backticks and Unicode; SHA256 b7c7df71f44540087cf2074be1b1154d2603d59b043c1ba878e93f6a5e3377ea. Both real CLIs displayed the final sentinel and submitted the complete brief as their first user message to a local HTTP model stub. Claude request contains an exact matching user text block; Codex persisted exact matching first user text in rollout response_item/event_msg lines 7/8. No paid API or real repository was used. Fresh Claude onboarding/trust/API-key-selection was isolated in scratch config; initial unsuccessful probes were startup-dialog/flag issues, not truncation. Actual helper boundary for both agents: 16383/16384 argv, 16385 typing fallback. No fragility observed at ~12k under zsh ZLE with bracketed paste. A brief file plus quoted cat substitution would shorten shell input while retaining the TTY, but would still produce the same argv/ARG_MAX and discovery/resume issues; it adds backend file creation, same-machine paths and cleanup. Command substitution strips trailing newlines (current wrapped brief has none). No file-based implementation is justified by this evidence. Native Windows execution was not available; the windows-native helper fallback passed in the package run.

### 2026-10-13T08:23:50.122Z - Critic round 2: F1 and N1 resolved. F2 apostrophe boundary reproduced on 9054e77250: resume_drops_a_seed_brief_containing_a_shell_escaped_apostrophe received the entire brief instead of a prompt-free resume command; RED log ~/Gits/.tmp/critic-1626b-red.log. Minimal tokenizer fix handles an unquoted escaped character without toggling quote state. Full local Vitest: 743 files, 9318 passed, one unrelated DictationToast timer leak; full rb Rust package: 5446 passed, 21 intentional skips; clippy all-targets -D warnings passed. Final tested tree: d64925b03 plus only src/utils/agentSession.ts production change. No excess machinery found.

