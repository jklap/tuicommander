---
name: to-test-cycle
description: >
  Work through `to-test.md` items end-to-end: pick the top item, delegate the
  actual verification work to a forked subagent so the primary session's
  context stays clean, research and fix (or triage-and-defer) any issue
  found, remove the entry once verified, and commit cleanly — fixup commits
  when the originating work is still in-branch, never squashed. Use when
  asked to "test to-test.md", "go through the to-test list", "verify the
  to-test items", or similarly to work through pending manual-verification
  entries.
keywords:
  - to-test.md
  - manual verification
  - TUIC_APP_INSTANCE
  - fixup commit
  - red green testing
  - live verification
  - triage
  - fork subagent
---

# to-test.md Cycle

`to-test.md` is this repo's queue of features/fixes awaiting manual live verification (see root
`AGENTS.md`'s "Tests" section). This skill is the disciplined loop for burning that queue down:
verify → fix-if-needed → remove-the-entry → commit-cleanly → repeat, without either skipping real
verification or running the full test suite on every single item (that's what makes the loop
fast — see step 7's "scoped tests during the loop").

**Terminology note:** if the user says "to-do.md," they almost always mean `to-test.md` — this
repo has no `to-do.md` (only `todo.md`, a general backlog, and `to-test.md`, the structured
pass/fail verification queue this skill is built around). Confirm which file if genuinely
ambiguous, but default to `to-test.md` for anything shaped like "test the next item," "verify
X," or "go through the queue."

## 0. Kickoff — ask once, at the start of the session

Ask the user (`AskUserQuestion`) how much autonomy to run with:

- **Confirm each item** — ask before starting each new `to-test.md` entry, and offer (ask) before
  applying even a small fix.
- **Run N items unattended** — get a number (offer a couple of presets like 3/5/10, plus a custom
  "Other" value). While the counter has remaining budget: pick the next item, narrate what you're
  about to do in one line, then proceed through the whole verify→fix→remove→commit cycle without
  stopping for approval (still stop for a genuinely large/ambiguous issue — see step 7).

Track the remaining count across the session. Re-ask only if the user explicitly asks to change
mode, or the counter hits zero and they want to continue.

**This session's own request is standing authorization to commit** fixups/removals that pass
their tests, scoped exactly to what this skill does (steps 8–9) — don't re-ask "should I commit
this?" per item. Still narrate every commit made, and never force-push, rewrite published
history, or run `git rebase --autosquash`/`-i --autosquash` (step 8 covers this — the user
squashes fixups themselves).

## 1. Delegate verification work to a forked subagent — keep this session clean

This whole loop is tool-call-heavy in exactly the way that bloats a long session for no
benefit: curl output, screenshot bytes, compiler/test-runner logs, grep sweeps across the
codebase. None of that needs to live in *this* conversation once a verdict has been reached. Use
Claude Code's own **Agent tool with `subagent_type: "fork"`** for the actual verification legwork
(steps 5 and 6 below, and the re-verification tail of step 7) — a fork inherits this session's
full context for free (so you never have to re-explain the item, the app instance, or repo
conventions in the prompt), runs the noisy work in its own transcript, and hands back only the
report you asked for.

**Disambiguation — this is NOT the same thing as TUICommander's own `agent action=spawn` MCP
tool.** The MCP server's own instructions warn about this exact confusion in both directions:
Claude Code's built-in Agent/Task tool and TUICommander's `mcp__tuicommander__agent`/`session`
tools are two unrelated things that happen to share the word "agent." This skill means the
former — an in-process subagent, invoked via the `Agent` tool, never a TUICommander-managed PTY
tab. If a verification fork needs to reach TUICommander's own MCP surface as part of testing an
item (e.g. probing `mcp__tuicommander__debug`/`session` against the running instance), that's
fine — but it must never itself call `mcp__tuicommander__agent action=spawn` to create *another*
layer of managed peer, and per root `AGENTS.md`'s "TUIC Protocol Markers" section, a
subagent/fork must never emit `ack`, `intent:`, or `suggest:` even if it's talking to
TUICommander's MCP tools directly — those markers are for this top-level session only.

**Writing the fork's prompt** — since it inherits context, keep it a directive (what to verify
and what to report), not a re-explanation of background it already has:

```
Agent({
  subagent_type: "fork",
  name: "verify-<short-slug>",
  description: "Live-verify to-test.md: <item heading>",
  prompt: "Verify every bullet under to-test.md's '<item heading>' section against
    <the running instance: standalone at 127.0.0.1:<port>, TUIC_APP_INSTANCE=<id> |
    OR the orchestrator, confirmed rebuilt with this fix>. Follow root AGENTS.md's
    [HUMAN]-is-a-last-resort escalation ladder (code inspection -> test execution ->
    CLI probing -> MCP maccontrol -> MCP invoke/JS) -- only fall back to recommending
    a real human check if none of those can capture it, and say exactly why. For each
    bullet report PASS with the concrete evidence (file:line, command output, or
    screenshot description) or FAIL with the exact reproduction and suspected root
    cause location. End with one verdict line: ALL PASS, or N of M failed (name them).
    Do not edit to-test.md, do not commit anything, do not touch Boss's real sessions
    or repos -- create and clean up your own scratch sessions/repos if you need one.
    Report only; do not fix anything yourself."
})
```

Do **not** pass `isolation: "worktree"` for a verification fork — it needs the exact same working
tree (including any uncommitted fix you're mid-testing) and the exact same running app instance
process the primary just started; an isolated worktree copy would be testing different code
against nothing.

**Treat a fork's report as evidence, not as ground truth.** A subagent's summary describes what
it intended to do and believes it found, not a guarantee — this has bitten this exact kind of
workflow before (memory `feedback_verify_review_suggestions_before_applying`,
`feedback_code_review_pattern_findings_undercounted`). Before designing a fix off a FAIL verdict,
at minimum re-read the file:line it cites yourself. **After every fork call, check `git status
--porcelain` / `git diff --stat` before proceeding** — a fork has full tool access and has been
known to wander outside a narrow ask (memory `feedback_fork_scope_overrun_on_narrow_ask`,
`feedback_code_review_subagent_ran_git_checkout`); catch it immediately, not at the end of the
session.

**Not everything belongs in a fork.** Deciding what an item means, classifying a fix as
small-vs-large, designing the actual fix, and writing the regression test are understanding-heavy
work this session should keep doing itself — delegating "based on the findings, fix the bug" to a
subagent produces shallower work and is explicitly the wrong use of a fork. What gets forked is
the *execution* of a known checklist (the escalation ladder, running a specific test, re-verifying
specific bullets), not the judgment calls around it. A single, long-running, non-interactive
command with nothing to interpret mid-flight (the full check-gate in step 11) doesn't need a fork
at all — `run_in_background` plus its completion notification is the simpler clean-context tool
for that shape; save forking for work that needs several tool calls and a live decision ladder.

## 2. Start an isolated app instance

Default to a **standalone test instance**, not the orchestrator you're running inside — see root
`AGENTS.md`'s "Test instance vs orchestrator instance" section, and use the
`live-verify-debug-instance` skill for the full mechanics (HTTP API on 9877/9878, PID tracking,
click discipline, cleanup). This section covers the one thing that skill doesn't: making sure
your instance doesn't collide with anyone else's.

**Isolation is automatic by directory name, not by process.** `make dev`/`make test` derive
`TUIC_APP_INSTANCE` from the checkout's own directory name (see the Makefile's own comment above
that line), so two *different* checkouts/worktrees never collide without you doing anything. The
case that DOES collide: running this skill from the **same checkout** (e.g. the main checkout)
that Boss, or another agent, might already have a `make dev` running in — that second launch
would derive the identical auto instance id and either fail the port bind or silently share the
first one's config/session state.

Before starting anything:

```bash
ps aux | grep -E "target/debug/tuicommander|tauri.js dev" | grep -v grep
```

- **Nothing running from this checkout** → just `make dev` (or `make test`), no manual
  `TUIC_APP_INSTANCE` needed. The auto-derived isolation is already correct.
- **Something IS already running from this checkout** → don't touch it (it may be Boss's own
  session). Launch your own with an explicit, unique override instead:
  ```bash
  TUIC_APP_INSTANCE="totest-$(date +%s)" make dev
  ```
  Record that instance id — you'll need it again if you restart the same instance later in the
  loop (a fresh random id would spin up a THIRD instance instead of restarting yours).

Either way, confirm the actual bound port before testing (`9876` → `9877` → `9878` retry) —
`lsof -iTCP -sTCP:LISTEN -Pn | grep tuicommander` — and check that instance's own `config.json`
has `services.server.enabled: true` if HTTP calls come back empty (see the debug-instance skill's
note on this — a fresh named instance defaults the HTTP server off).

This one-time-per-session startup is short enough (a handful of commands, one poll loop) that it
doesn't need its own fork — run it directly, but keep only the facts you'll need again
(`TUIC_APP_INSTANCE` id, bound port, the PIDs to kill later) in mind; you won't need the raw
`make dev` build log again after confirming the port is up.

**Rust changes never hot-reload** (`make dev --no-watch`, by design — see root `AGENTS.md`'s "Dev
Hot Reload"). Any time you land a Rust fix mid-loop (step 7), you MUST kill and restart this exact
instance (same `TUIC_APP_INSTANCE`) before re-verifying — testing against the old binary and
reporting success is a false pass.

**Exception — the orchestrator itself was just rebuilt with the fix.** If the user's own message
says the app was "fully rebuilt" and is "running with all of the latest code changes" (this
happened once already, 2026-09-30), the orchestrator instance you're embedded in already has the
fix, and `mcp__tuicommander__*` tools test it directly — no standalone instance needed for that
session. **Verify this before trusting it**: the MCP server instructions' version banner
(`TUICommander vX.Y.Z-nightly.<date>.<short-sha>`) should match the commit that made the fix.
Don't assume it's still true in a later session — the orchestrator is a separately-launched
release build that does NOT rebuild itself just because your worktree's code changed.

**MCP-tool-shaped features are the one thing you often can't test against a standalone
instance.** `agent action=spawn` and friends are bound to *this* Claude Code session's own MCP
connection to whichever instance it's attached to — a standalone `make dev` instance's HTTP-side
MCP surface exists but isn't wired into your tool list. For a `to-test.md` item shaped like this,
either use the orchestrator-was-just-rebuilt exception above, or fall back to the escalation
ladder's other rungs (code inspection, unit/integration tests, a raw MCP client script against
the standalone instance's HTTP MCP endpoint) rather than assuming it's untestable. Because this
case is bound to *this* session's own MCP connection, it cannot be delegated to a fork the way
step 6's HTTP/CLI verification can — a fork has its own, separate MCP connection/tool list, not
this session's — so this one rung of testing has to run in the primary session regardless.

**A "raw MCP client script against the standalone instance" is harder than it sounds for a
named instance — `tuic-bridge` has no named-instance awareness at all.** Found 2026-09-30
live-verifying the deferred-prompt-delivery deadlock fix (`stuck_on_pre_first_turn_session_start`,
`pty.rs`): a named instance's real MCP Unix socket lives at a SHA256-hashed path under
`$TMPDIR` (`mcp_http::mod::named_socket_path`, needed because macOS's ~104-byte `SUN_LEN` limit
rules out `<config_dir>/instances/<id>/mcp.sock`) — but `tuic-bridge`'s own `connect_ipc()`
(`crates/tuic-bridge/src/main.rs`) only ever checks the `TUIC_SOCKET` env override or the
*default* instance's `<config_dir>/mcp.sock` (+ `mcp-*.sock` siblings in that same directory).
It has zero knowledge of `TUIC_APP_INSTANCE`, so a spawned agent in a named instance can NEVER
discover the socket automatically — this is why named instances skip MCP-bridge auto-install in
the first place (see this file's own `TUIC_APP_INSTANCE` section), not an oversight. To actually
exercise a real MCP handshake against a named standalone instance:
1. Read the real socket path from that instance's own startup log (`Unix socket listening ...
   path=...`) or `lsof -U | grep tuicommander`.
2. Write a **project-local** `.mcp.json` in a throwaway scratch cwd (never the real repo root —
   it's uncommitted scaffolding) pointing `command` at that checkout's
   `target/debug/tuic-bridge` with `"env": {"TUIC_SOCKET": "<that exact path>"}`. This never
   touches `~/.claude.json` or any global config.
3. **A freshly-trusted project MCP server still shows an interactive "Use this MCP server?"
   dialog before it accepts any tool call** — not present in production, where the default
   instance's bridge is pre-installed/pre-trusted. Send Up, Up, Enter (`POST
   /sessions/{id}/write` with `{"data": "\u001b[A\u001b[A\r"}`) to pick "Use this MCP server"
   (the default-selected option is the *last* one, "Continue without using this MCP server").
4. **This manual approval step races the deferred-delivery mechanism you're often trying to
   test in the first place** — if the automatic forced-write fires before your approval
   keystrokes land, its content can be swallowed by the trust dialog instead of the real
   composer, muddying the very outcome you're trying to observe. Send the approval keystrokes
   immediately after spawn returns, before polling status, to minimize the window — or accept
   that the deepest "does the agent visibly complete" layer may need the orchestrator (once
   confirmed rebuilt) instead of a clean standalone rig.

Running `target/debug/tuicommander` directly (skipping `make dev`/`pnpm tauri dev`) to avoid a
slow rebuild panics immediately (`wry`'s WebView init `unwrap()`s on a `None` — it's missing
bundle/dev-harness context Tauri's own dev wrapper provides). Always relaunch through `make dev`,
even just to flip a config flag and restart.

## 3. Pick the top item

```bash
grep -n '^## ' to-test.md
```

An "item" is one whole `##` section — its heading plus every `- [ ]`/`- [x] [HUMAN]` bullet under
it, including the trailing "Delete this section once verified" bullet the file's own convention
adds. The "top" item is the first section from the top of the file (ignore the `tweak-comments`
HTML block at the very top of the file if present — that's Imark annotation syntax, not a
to-test item). Read the whole section before doing anything else — some sections cross-reference
which exact functions/tests already give unit coverage (worth checking before assuming you need
to write one from scratch).

Note whether the heading says "Rust, needs a `make dev` restart" — if so, step 2's rebuild note
applies to the *initial* verification too, not just a mid-loop fix: make sure the instance you're
about to test against was started (or restarted) AFTER the commit that added this item landed.

## 4. Confirm before testing (one-by-one mode only)

In unattended-batch mode, skip the ask — just state in one line which item you're starting and
proceed.

## 5. Before changing any code, audit test coverage first

This applies both to a normal verification pass and to fixing an issue found in step 7 — do this
BEFORE writing or editing implementation code, not after. The grep/read sweep here is a good fit
for a fork too (it's pure reconnaissance, no judgment call) — have it report back a plain list of
"covered" vs. "no test found" for the function(s) named plus their callers, and make the "is this
gap worth closing now" call yourself:

1. Existing tests covering the function(s) you're about to touch, AND their callers — not just
   the one function the `to-test.md` item names.
2. If a caller or a closely-related function has no coverage, that's a gap to close as part of
   this pass, not a separate future task — write a characterizing test for its CURRENT behavior
   first (see memory `feedback_coordinator_tests_before_change` / this skill's red/green
   convention in step 7), so a regression there would actually be caught.
3. Only once coverage is accounted for, move on to step 6/7.

## 6. Do the live verification

Dispatch this to a verification fork per step 1's template — the escalation ladder itself is the
fork's job; you're deciding what the *result* means, not clicking through it yourself. Root
`AGENTS.md`'s `[HUMAN]` escalation ladder, for reference (an item marked `[HUMAN]` in the file is
not an instruction to stop and ask a person; it's a last resort after these, in order):

1. **Code inspection** — read the source, confirm the logic exists at file:line.
2. **Test execution** — run the specific test(s) this item's own text names, if it names any.
3. **CLI probing** — `curl` HTTP endpoints, `grep` for patterns.
4. **MCP maccontrol** — screenshots, click UI elements, verify visual state.
5. **MCP invoke/JS** — call commands, inspect store state, trigger actions programmatically.

The fork should only fall back to recommending an actual human check when the item genuinely
needs real hardware, real multi-app interaction, or timing-sensitive observation none of the
above can capture — and it should say so explicitly in its report rather than silently marking it
done. **Never test against Boss's live sessions or real repos** — the fork's prompt should say
this explicitly (throwaway sessions/scratch repos only, cleaned up when done — see the
debug-instance skill's cleanup section).

Read the fork's verdict; spot-check anything surprising per step 1's "evidence, not ground truth"
rule. If everything verifies clean: skip to step 9.

## 7. If verification surfaces an issue

**Research before proposing anything.** Read the actual source at the point of failure yourself
(don't just trust the fork's citation); reproduce it directly rather than guessing from a
plausible-sounding mechanism (see memory `feedback_verify_rootcause_against_reported_symptom` — a
plausible cause found in code may not be the real one until you've actually reproduced the
reported symptom against it). This root-causing step is understanding-heavy — do it yourself,
per step 1; a fork's earlier report is a lead, not a diagnosis.

**Write the regression test before the fix, and confirm it's red.** This is not optional per the
user's own standing instruction for this workflow: a test that characterizes the bug, run once to
confirm it actually fails against the current code, THEN the fix, THEN confirm the same test goes
green. A test added only after the fix already compiles/passes proves much less. You author the
test and the fix yourself (understanding-heavy); a fork is a fine way to just *run* the red check,
the green check, and the scoped test filter afterward and report the pass/fail + output, if you'd
rather keep the compiler/test-runner noise out of this session too.

Then classify the fix:

- **Small** (a self-contained change, clear root cause, no new data model/architecture, roughly
  the size of "add a function and touch one call site"): propose it in one or two sentences (what's
  broken, what you'll change), apply it, make the red test green, and — critically — check whether
  the fix reveals a SIMILAR gap in a sibling code path before moving on (a fix scoped to "the
  thing I'm looking at" routinely misses a sibling with the identical shape in this codebase; see
  memory `feedback_code_review_pattern_findings_undercounted`). In one-by-one mode, ask before
  applying; in unattended-batch mode, apply directly and report it in the same narration as the
  rest of the item.
- **Large** (needs new state/data-model plumbing across multiple files, a new mechanism, a
  genuinely uncertain design tradeoff, or realistically more than about an hour of focused work):
  do **not** implement it here. Write a plan document instead — `plans/<slug>.md` — covering: the
  bug/goal, root cause (with file:line citations), the proposed approach, risks, and rejected
  alternatives (matches this repo's own `mdkb memory_write` convention — see root `AGENTS.md`'s
  "Implementation Memory" section for the shape). Tell the user this item needs a dedicated
  agent/session, and hand off with that plan file. **Do not remove the `to-test.md` entry** in
  this case — instead annotate it in place with a short note ("Investigated <date>: root cause is
  X, plan at plans/<slug>.md, needs dedicated implementation") so the next pass doesn't
  re-investigate from scratch. Note: `plans/` is gitignored in this repo (memory
  `project_plans_dir_gitignored`) — a plan written in a worktree won't be visible from the main
  checkout or another worktree; write it wherever the agent picking it up will actually look.

Once a small fix is green: run only the **scoped** tests for the area you touched (e.g. `cargo
nextest run -p tuicommander <module>::` or a targeted `vitest run <file>`), not the full suite —
that's what step 11 is for. Re-verify live per step 6 (restarting the app instance first if the
fix touched Rust — step 2), again via a fork if you want the re-verification noise out of this
session, checking `git status`/`git diff --stat` afterward per step 1.

## 8. Commit cleanly

For each thing you're about to commit — a code fix, or the `to-test.md` entry removal in step
9 — decide fixup vs. plain commit. This step is low-noise (a handful of `git` commands) and stays
in the primary session:

```bash
# Is the commit that introduced this feature/entry still unique to this branch,
# i.e. not already an ancestor of the branch's upstream/base?
git log --oneline <base>..HEAD | grep <short-sha-or-subject>
# or, if you have the exact sha:
git merge-base --is-ancestor <sha> <base-branch> && echo "already on base — plain commit" || echo "in-branch — fixup"
```

- **In-branch** (the commit that added this feature/to-test-entry is one of this branch's own,
  not yet on the shared base): `git commit --fixup=<that-sha> -- <files>`. This is what keeps the
  history squashable later without the user having to hand-sort commits.
- **Not in-branch** (predates this branch, or the feature already landed on the base and this is
  a follow-up fix): a normal commit with a real message.

**Never squash, autosquash, or rebase these fixups yourself** — no `git rebase -i`, no
`--autosquash`, no `-i` anything. The user does that pass themselves, on their own schedule; your
job is only to leave clean, correctly-targeted fixup commits behind.

## 9. Remove the entry once fully verified

Delete the whole `##` section (heading through its final bullet) from `to-test.md` — don't leave
checked boxes behind; the file's own convention is "delete once verified," not "mark done." Commit
this removal per step 8's fixup rule (the commit that ADDED the section to `to-test.md` is what
you're checking against — often, but not always, the same commit that implemented the feature).

## 10. Offer the next item

- **One-by-one mode**: summarize what happened (verified clean / fixed+verified / deferred with a
  plan), then ask whether to continue to the next item.
- **Unattended-batch mode**: decrement the counter, narrate the outcome in one line, and move
  straight to the next item if budget remains.

## 11. Wrap-up — when the batch is exhausted or the user says stop

Run the **full** check-gate (the `check-gate` skill — `./scripts/check-gate.sh`), not a scoped
filter. This is a single, long, non-interactive command — per step 1, prefer `run_in_background`
plus its completion notification over a fork here; there's nothing to interpret mid-flight, so a
fork buys nothing a background task doesn't already give you. This is the one point in the loop
where the full suite matters: a repo-wide consistency test (IPC/HTTP parity, etc.) never runs
under a scoped filter, and several small in-branch fixups can drift out of `cargo fmt`'s canonical
formatting even though each passed its own scoped check (memory
`feedback_full_check_before_declaring_done` / `project_fixup_reorder_squash_20260827`-adjacent
gotchas).

If it surfaces anything new: same triage as step 7 (small → fix+test+commit, large → plan doc) —
a fork is a good fit again for investigating a specific failing test's output if it's noisy, per
step 1's rule of thumb — then re-run the full gate until it's clean. Only then give the final
summary: which items were verified/fixed/deferred, links to any plan docs written, and the exact
`git log` of commits made this session (so the user can review before squashing).

## 12. Keep this skill current

If this run taught you something a future run of this skill would need — a startup gotcha, a
new way an app instance can silently fail to isolate, a build step that turned out necessary, a
case where forking a subagent turned out to be the wrong (or an especially good) call, an
assumption in this file that turned out wrong — **edit this file before finishing the session**,
in the relevant section (or add a dated note here if it doesn't fit cleanly elsewhere). Don't let
it evaporate into a one-off memory that only this session benefits from; this file is the durable
place for it, the same way root `AGENTS.md` accumulates its own hard-won gotchas.
