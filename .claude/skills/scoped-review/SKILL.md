---
name: scoped-review
description: >
  Run a correctly-scoped code review or security review against only the
  current change — not the whole branch/repo. Use whenever a review is
  needed for uncommitted work, a feature branch with unrelated prior
  commits, or any time `/code-review` or `/security-review` would otherwise
  be asked to review "the change I just made." Also use if the user asks to
  "review this," "run a security review," or "run a code review" on
  in-progress work in this repo.
keywords:
  - code review
  - security review
  - diff scope
  - git diff
  - review scope
---

# Scoped Review

The built-in `/code-review` and `/security-review` skills default to computing their own diff
(often `git diff main...HEAD` or the full working-tree `git status`), not "the specific change
I want reviewed." On a long-lived branch or worktree with prior unrelated commits, this hands
the reviewing agent a diff many times larger than the real change (multi-MB, hundreds of files
observed repeatedly in this repo) — diluting or defeating the review entirely, and burning a
large amount of tokens/time on unrelated history.

**Passing a scope override in the invocation's own `args`/prompt text does NOT reliably fix
this** — confirmed across 9+ separate sessions in this repo. The skill's diff-collection step
runs independently of what the prompt asks for, and a same-turn correction or a follow-up
message to an already-running review agent is too late (the wrong diff may already be baked
into its context, or into sub-teammates it already spawned with that wrong scope).

## The pattern that actually works

1. **Compute and save the exact diff yourself first.** For uncommitted working-tree changes,
   including new untracked files:

   ```bash
   # Capture untracked new files into the diff too (round-trip, doesn't leave anything staged):
   git add -N <untracked files/dirs>
   git diff > /tmp/scoped-review-diff.txt
   git reset -- <untracked files/dirs>   # restores them to untracked, nothing left staged
   ```

   For a specific commit range instead: `git diff <base>..<head> -- <paths> > /tmp/scoped-review-diff.txt`.

2. **Do NOT invoke `Skill({skill: "code-review"})` or `Skill({skill: "security-review"})`** —
   route around them entirely. Use a plain `Agent()` call instead.

   **Prefer `subagent_type: "fork"` inside TUICommander.** A named-persona type (e.g.
   `"code-reviewer-pro"`, `"security-auditor"`) can materialize as a real TUIC-managed pane
   teammate whose tool-permission requests don't reliably route back to the spawning session —
   confirmed 2026-09-29: two review agents launched this way sat at `agent_state: working` in
   `session action=list` for 30+ minutes with zero progress, stuck on "Waiting for team lead
   approval" for a plain `Read` call. That state is indistinguishable from a genuinely long
   review via `ListAgents`/`session action=list` alone. A `fork` runs in-process and inherits
   the caller's own permission mode directly — no separate approval hop, no stall risk — and
   the reviewer persona itself doesn't matter much here since step 3's prompt already tells the
   agent exactly what kind of review to perform and what to look for.

   If you do use a named-persona type (e.g. because you want its specific system prompt) and a
   review shows `agent_state: working` with no change across 2+ status checks, use
   `session action=output` on its pane BEFORE assuming it's just slow — a stuck approval prompt
   means `TaskStop` it and relaunch as a `fork`; waiting longer will not resolve it.

3. **The agent's prompt must open with an explicit scope override**, naming the exact file path
   and forbidding it from computing its own scope. For example:

   > SCOPE OVERRIDE — do not compute your own diff, do not run `git diff`/`git status` against
   > HEAD or main, and do not review the whole branch/repo. The ONLY thing to review is the
   > exact diff already saved at: `/tmp/scoped-review-diff.txt`. Read that file directly and
   > review ONLY the changes it contains.

   Follow with real context: what the change is, what's already been reviewed/fixed if this is
   a follow-up pass, and what to look for specifically (don't make the agent re-derive intent
   from the diff alone).

4. Launch code-review and security-review **in parallel** (two `Agent()` calls in one message)
   when both are warranted — they're independent and both read the same saved diff file.

## When a security review is actually warranted

Not every change needs one. Reach for it when the diff touches: new HTTP/IPC endpoints or their
auth gating, anything parsing untrusted input (PTY/terminal escape sequences, file uploads,
external process output), shell script generation/injection, credential/token handling, or
persisted config that flows into a command line or file path. Skip it for pure UI/CSS,
refactors with no behavior change, or test-only changes.

## Don't re-litigate accepted design

Before reporting a finding, check whether it's already an accepted decision in `AGENTS.md`'s
"Accepted Security Decisions" section (permissive CSP, `dangerouslyDisableAssetCspModification`,
plugin capability model, the self-signed-HTTPS redirect, etc.) — these are known and intentional
for this local-first dev tool; don't flag them again without a genuinely new exploitation path.
