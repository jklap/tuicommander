# TUICommander — Frontend (`src/`)

Repo-wide rules (doc sync, git workflow, architecture split, TUIC protocol markers,
IPC/HTTP parity contract, accepted security decisions) live in the root
`AGENTS.md` — read that first. This file covers what's specific to the SolidJS
frontend: reactivity gotchas, focus/modal handling, panel data-refresh conventions,
and frontend-only utilities.

Directory-specific rules that don't belong here:
`src/components/SettingsPanel/AGENTS.md`, `src/components/Terminal/AGENTS.md`.

## Visual

- All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).
- **Plugin dashboards MUST follow [`docs/plugins-style.md`](docs/plugins-style.md)** — use the shared `.dashboard`/`.dash-*` classes from `PLUGIN_BASE_CSS`, never hand-roll inline layout CSS. The built-in Claude Usage dashboard is the reference.
- Icons: monochrome inline SVGs with `fill="currentColor"` — never emoji.
- Take a screenshot after EVERY visual/CSS/layout change to verify rendering.
- **The canvas-terminal frame protocol's bit layout** (`terminal_grid.rs`'s `serialize_dirty_rows`, mirrored in `canvasTerminalUtils.ts::decodeBinaryFrame` and `docs/frontend/canvas-terminal-audit.md`) — see `src/components/Terminal/AGENTS.md` for the three-way-sync rule and why it broke three times.

## Global Focus/Keyboard Handlers Must Check `anyModalOpen()`

`useKeyboardRedirect.ts` (forwards printable keys to the active terminal
whenever focus isn't in an `INPUT`/`TEXTAREA`/`SELECT`) had no
`anyModalOpen()` check at all — a dialog control that isn't a plain input
(e.g. `CreateWorktreeDialog`'s "Start from" `<button>` trigger, Tab-focused)
was fair game, so typing while it had focus both leaked the keystroke into
the terminal underneath **and** yanked focus there via `activeTerminal.ref
.focus()`. Fixed by adding an early `if (anyModalOpen()) return;` (from
`stores/modalStack`) — every overlay in this codebase already calls
`registerModal()` (confirmed: both current `role="dialog"` components do), so
this single check covers all of them; it deliberately does NOT also add an
ARIA-role fallback, since that would be unexercised code standing in for an
invariant ("every dialog calls registerModal") that's enforced by convention,
not by this hook. Any new document-level keydown/focus-stealing handler that
only special-cases `INPUT`/`TEXTAREA`/`SELECT` has the same gap for every
non-input dialog control — check `anyModalOpen()` before assuming a
plain-input allowlist is enough, and make sure any new overlay actually calls
`registerModal()` rather than relying on a hook to guess it's a dialog.

The inverse failure also exists: a panel that reclaims focus/keyboard input
for *itself* whenever it loses it (e.g. `ComposePanel`'s `focusout` handler,
which used to unconditionally refocus its own CodeMirror instance one frame
after ANY blur) traps focus away from a dialog opened over it, even though
the dialog is strictly higher z-index and otherwise fully interactive. The
fix there was symmetric: only reclaim when focus demonstrably fell through to
`<body>`/nowhere (the actual "keystrokes would vanish" case) or a modal is
registered — never when focus visibly moved to a specific other element.
Any panel with this "reclaim focus on blur" pattern needs the same
`document.activeElement !== document.body` / `anyModalOpen()` guard, not a
z-index fix — the two panels were never actually z-index-conflicting.


## Panel Refresh

Panels with repo-dependent data MUST use `repositoriesStore.getRevision(repoPath)` in `createEffect` — not file watchers or polling. `repo_watcher` emits `"repo-changed"` → `bumpRevision()`.

**A panel that renders ONLY committed history** (commit log, file history, stashes) uses `getGitRevision(repoPath)` instead, so a plain file save no longer re-runs its git processes. The two counters are nested, not parallel: `bumpGitRevision` bumps **both**, and `getRevision` still moves on every event. `getRevision` is therefore always the safe default — a panel left on it cannot go stale, while a panel wrongly moved to `getGitRevision` silently misses working-tree changes. Move a panel only after checking every command it calls ignores uncommitted state.

**Panel visibility gates must check the specific scope they care about.** A gate bundled in from cross-repo/global state (e.g. `globalWorkspaceStore.isActive()`) must check the scope (`MANUAL_SCOPE`) it actually cares about, not just `isActive()` — a per-repo auto-consolidated workspace is a different activation with a single well-defined repo, and a bare `isActive()` check will wrongly suppress panels for it. The same "boolean/flag check too coarse for a growing state space" shape has recurred more than once (File Browser/Git Panel suppression, sidebar branch-icon color falling through to the wrong case) — when a state space grows a new case, re-check every existing boolean gate against it rather than assuming the old check still covers the new state correctly.


## `branch.terminals` Membership Must Never Be Pruned On Terminal Exit

`repositoriesStore`'s `branch.terminals: string[]` means "every terminal ever
attached to this branch/worktree, whether or not it's still alive" — NOT "every
currently-live terminal." Multiple consumers depend on the former, broader
meaning: `closeTerminalsForBranch` (`createWorktreeWorkflowCoordinator.ts`,
`createWorktreeRemovalCoordinator.ts`) iterates `branch.terminals` to close
every terminal — including an already-**exited** one — before a worktree is
merged/archived/removed, so its tab doesn't survive pointing at a deleted
directory; `useWorktreeSwitchPrompt.ts`'s `pruneRemovedWorktree` uses
`branch.terminals.length` to decide whether cleanup is even needed; and
`RepoSection.tsx`'s `getBranchTabsAvailable`/`BranchTabList` render the sidebar's
own expandable tab list straight from `branch.terminals`, so a still-open (if
exited) tab stays reachable there.

A 2026-09-10 fix for a real bug (a ghost-attached terminal — one whose process
exited on its own rather than via the tab's close button — inflating the
worktree-removal "N terminal(s) attached" dialog and keeping the sidebar dot
green) initially tried removing the id from `branch.terminals` the moment the
terminal exited (`Terminal.tsx`'s `hadAgent` PTY-exit branch,
`useAppInit.ts`'s `"session-closed"` listener). An independent code-review pass
caught that this broke all three consumers above: an exited-but-lingering tab
became invisible to `closeTerminalsForBranch` (so it was never closed,
surviving worktree teardown as a dangling tab pointing at a deleted directory),
could make `pruneRemovedWorktree` skip cleanup entirely (`branch.terminals`
going empty), and vanished from the sidebar's own tab-list even though the tab
itself was still open — directly contradicting the intent that the tab
"lingers so the user can still see it."

**The correct fix does not touch array membership at all.** `branch.terminals`
keeps every id, live or exited, until the terminal's tab is actually closed (or
the branch/worktree itself goes away). Instead, everything that only cares
about *live* activity — the removal-confirmation gate and the sidebar dot —
filters by `terminalsStore.get(id)?.shellState !== "exited"` at read time:
`branchActivitySummary`'s `isBusy` (`activitySnapshot.ts`) and
`hasLiveTerminals()` (`RepoSection.tsx`). An id missing from `terminalsStore`
entirely still counts as live/busy by design (conservative — see
`activitySnapshot.ts`'s doc comment on `branchActivitySummary`). The
confusing `"—"` label a stale/exited entry showed in the removal dialog (the
shared `effectiveActivityState`/`terminalStatusLabel` has no `"exited"` case,
so it falls through to `"unknown"`) is likewise handled locally in
`branchActivitySummary`, not by adding an `"exited"` case to the shared
dashboard label function — the Activity Dashboard has its own existing,
tested contract for how it renders an exited session (`terminalStatusLabel`'s
`"—"` fallback), which this fix deliberately leaves alone.

**Do not reintroduce eager pruning of `branch.terminals` on terminal exit** —
if a future ghost-attachment bug shows up again, fix it the same way: filter
by live `shellState` in the specific consumer that cares about liveness,
never by mutating branch membership.


## SolidJS `<For>` Index Staleness

`<For>`'s mapping callback is invoked once per distinct item **reference**, not once per render — it does not re-run just because filtering/sorting shifted that same item to a new position. `<For>` hands the callback an `index` **accessor** (a function) specifically so consumers can read the item's current position later; calling it immediately (`i()`) and stashing the plain number in a closure throws that liveness away. Any handler built from that captured number (a click/hover callback that indexes back into the filtered array) goes stale the instant the array's composition changes without that item's own identity changing — the callback still runs, but against a now-wrong (sometimes out-of-bounds) slot, so it silently no-ops instead of throwing.

This bit `BaseRefDropdown` in `CreateWorktreeDialog/CreateWorktreeDialog.tsx`: typing a query that filtered out an earlier match shifted a later option to a new index, and clicking or hovering it used a captured stale pre-filter index, so the click silently did nothing. **The fix is to never capture the number at all** — `renderItem(option, index: () => number)` keeps `index` as a lazy accessor called inline inside each handler (`index()`), matching the sibling branch list in the same file, which already did this correctly. (An earlier attempt at this fix computed the index at use-time via `filteredRefs().indexOf(option)` instead — don't do that either: it recomputes on every row on every keystroke, and the component's own comment now says so explicitly.) When a `<For>`-rendered row's event handler needs its position, call the index accessor inline at the moment the handler runs — never cache the number, and don't reach for `array.indexOf(item)` as a substitute.

## `<For>` Remounting a Row on Every Keystroke (whole-array-replace hazard)

A **reference-keyed** `<For>` over a list where an edit handler rebuilds the
**entire backing array** via `.map()` with a spread-copied element
(`arr.map(x => x.id === id ? { ...x, ...patch } : x)`, then a single
`setState("listField", newArray)`) disposes and recreates that one row's DOM
on every keystroke — Solid can't reconcile a plain-object replacement of the
whole array in place, so the edited index gets a fresh reference every call.
This destroys whatever was focused inside that row (an `<input>` mid-type)
and, if the row's own expand/collapse state is a **local** `createSignal`
(not hoisted to the parent), collapses the row back too. If the array was
previously *empty* (falling back to a shared default/built-in list) and the
edit is the *first* one, every element flips reference simultaneously,
remounting the whole list and resetting scroll position along with it — this
hit `SelectionTab.tsx`'s Smart Selection rule editor, fixed by converting
both the rule list and each rule's action list to `<Index>`, which keys by
position and updates via a per-slot signal instead of remounting.

**This is NOT a blanket "any store update near a `<For>`" problem** — check
how the specific setter is implemented before assuming a bug. A store setter
that targets a specific key with `setState("mapField", id, patch)` (a
**path-based** update into an object/record, e.g. `promptLibraryStore
.updatePrompt`'s `setState("prompts", id, {...existing, ...data})`) merges
onto the existing proxy *in place*, preserving that entry's reference
identity — `<For>` never sees a change and never remounts. The hazard is
specifically "whole-array replacement built from spread-copied plain
objects," not "any field of any list item changing." Verify empirically
(mount the real component against the real store, edit the field, assert the
same DOM node stays focused) before either claiming or ruling out this bug in
a sibling list.

## `<Show>` Never Remounts on a Truthy→Truthy Value Change

`<Show when={signal()}>{(value) => <Child target={value()} />}</Show>` only
re-invokes its render callback on a **falsy→truthy transition** — Solid's own
implementation memoizes the outer condition with an `equals` comparator that
treats any two truthy values as equal, so switching `signal()` from one
truthy value to a *different* truthy value does **not** recreate `<Child>`.
If `Child` reads its prop once at setup time (`const target = props.target`)
and seeds `createSignal`s from that snapshot, it silently keeps showing the
FIRST value forever — found in `RemoteServersTab.tsx` (code review
2026-09-23): a shared, non-modal inline editor mounted via `<Show
when={editorTarget()}>`, where only the "Add" button (not each row's own
"Edit" button) was gated behind `!editorTarget()`. Clicking Edit on a second
row while the editor was already open left the form showing the first row's
stale data, and Save would have written the second row's edits to the
first row's id.

**Fix: mount through a reference-keyed `<For>` instead of `<Show>`, when the
child must remount on every distinct target, not just null→value.**
`<For each={target() ? [target()] : []}>{(t) => <Child target={t} />}</For>`
works because `<For>` keys by array-item reference, and any code path that
calls `setSignal({...})` (a fresh object literal) produces a new reference
every time — so `<For>`'s reconciler sees the old and new single-element
arrays disagree at index 0 and unmounts+remounts the child, catching BOTH
null→value and value→different-value transitions. This is not a workaround
specific to that one component: any inline (non-modal) editor/panel gated by
a "target object" signal, where more than one entry point can retarget it
while it's already open, has the same latent bug if it's mounted via
`<Show>`.


## solid-js Signals Inside `vi.mock` Factories

When a mocked hook (e.g. `useAgentDetection`) needs to expose a signal a test can flip **after** the component has mounted (simulating async data resolving), do not create that signal via a plain top-level `import { createSignal } from "solid-js"` referenced inside the `vi.mock(...)` factory. It silently resolves to a *different* solid-js module instance than the one `<For>`/the component's own reactive tracking uses under this project's Vite/Vitest config — the signal's value updates fine, but `<For>` never re-renders, because its tracking context lives in the other instance's module-scope globals. Confirmed empirically while writing the headless-agent `<select>` regression tests (`ProvidersTab.test.tsx`, `SmartPromptsTab.headlessAgent.test.tsx`, 2026-08-28): a first attempt using a top-level import produced a signal that updated but triggered zero re-renders.

Fix: create the signal *inside* the `vi.mock` factory via a dynamic `await import("solid-js")` (the factory can be async), which resolves through the same module cache the rest of the app uses:

```ts
const detectionBox = vi.hoisted(() => ({
  availableAgentTypes: (): string[] => [],
  setAvailableAgentTypes: (_types: string[]) => {},
}));

vi.mock("../../hooks/useAgentDetection", async () => {
  const { createSignal } = await import("solid-js"); // NOT a top-level import
  const [availableAgentTypes, setAvailableAgentTypes] = createSignal<string[]>([]);
  detectionBox.availableAgentTypes = availableAgentTypes;
  detectionBox.setAvailableAgentTypes = setAvailableAgentTypes;
  return { useAgentDetection: () => ({ /* ... */ }) };
});
```

Use `vi.hoisted()` for the mutable box itself, not a plain module-scope `let` — `vi.mock` calls (and their factories' effects) are hoisted above other top-level statements in the file, so a plain `let` the factory assigns into hits a TDZ `ReferenceError`.


## A New Effect That Calls `fetch()` Needs `fetch` Stubbed In EVERY Test File That Renders The Component, Not Just Its Own

`RepoSection.tsx`'s `BranchItem` fires a real `fetch()` on mount (`pollWarmStatusOnce`,
added for the worktree-warming sidebar badge) whenever a row mounts with no live
`warmState` yet — a real, unmocked network call in a test environment either hangs or
rejects, and either way `vitest`'s leak detector marks the whole test FILE as failed for
a "leaking promise," even when every individual `it()` in that file passes. The new
`RepoSection.test.tsx` stubbed `global.fetch` in its own `beforeEach`/`afterEach` and was
fine — but `Sidebar.test.tsx`, a **sibling** test file that renders the full `Sidebar`
tree (and therefore every `BranchItem` row inside it) with no `fetch` stub of its own,
started leaking the moment this feature landed, even though nothing in `Sidebar.test.tsx`
itself changed. Fixed by adding the same `vi.stubGlobal("fetch", ...)` /
`vi.unstubAllGlobals()` pair to `Sidebar.test.tsx`'s global `beforeEach`/`afterEach`.

**The general lesson: adding a `fetch`/network call to a component's mount effect is not
safe to verify by checking only the test file you're adding assertions to.** Grep for
every OTHER test file that renders the same component (directly or via a parent like
`Sidebar.tsx`) before declaring the change complete — a sibling file with no reason to
have ever mocked `fetch` before will silently start leaking, and the failure surfaces as
a generic "Test Files N failed" with a stack trace pointing at your new code, not as an
assertion failure in the file you actually touched.


## `t()` Interpolation and `$`-Pattern Injection

`src/i18n/t.ts`'s `t(key, fallback, params)` interpolates via `str.replace(new RegExp(...), v)` — passing the raw dynamic value `v` as `String.replace`'s **replacement** argument, not as literal text. Per the JS spec, a replacement string containing `$&`, `$1`, `$$`, etc. is reinterpreted as a special pattern (matched substring, capture group, literal `$`) rather than inserted verbatim. Any call site that interpolates a value which can contain `$` and isn't fully controlled by us (a git branch name, a file path, free-form user text) can render garbled output — e.g. a branch named `foo$&bar` would have `$&` replaced by the matched `{placeholder}` text instead of appearing literally. Fixed (2026-09-10, found via code review while adding branch-name interpolation to the Create Worktree dialog's stale-setting warning) by escaping `$` → `$$` in each `v` before calling `.replace()`. This fix lives in the single shared helper, so it protects every existing and future `t(..., {...})` call site — no call site itself needs to change.


## PTY Command Injection

NEVER write text + `\r` directly to a PTY. Always use `sendCommand()` from `src/utils/sendCommand.ts` — it handles agent-specific Enter semantics (Ink raw mode needs split writes). This applies to dictation, command palette, suggested actions, and any other feature that sends input to a terminal.


## Logging

Use `appLogger` from `src/stores/appLogger.ts` — never `console.log/warn/error`. Check app logs via `GET http://localhost:9876/logs` (supports `?level=`, `?source=`, `?limit=` filters) before asking Boss for logs.


## Frontend performance instrumentation (`perfDebug`)

The frontend counterpart to backend Diagnostics. **One master flag gates ALL frontend perf/debug instrumentation** — `isPerfDebug()` from `src/utils/perfDebug.ts`.

- **Default = `import.meta.env.DEV`** → active in dev, **dormant in release**. We ship a quiet binary; we never distribute hyper-logging.
- **Runtime-toggleable, NOT tree-shaken** — a release build can be woken up to diagnose a field issue:
  ```js
  window.__TUIC__.setPerfDebug(true)   // persists to localStorage; starts the freeze detector
  window.__TUIC__.perfDebug()          // read current state
  ```
  (Run via the WebView devtools / MCP `debug invoke_js`. After toggling on in a build that started dormant, the freeze detector is (re)started automatically.)
- **Dormant cost:** a single boolean read at each entry point — negligible even per-frame.

**What it gates** (all in `src/utils/`): `markPerf`, `timeSync`, `timeBatch` (`perfTrace.ts`), `noteFrameRequest` (frame-burst detector), and `startFreezeDetector` (`freezeDetector.ts`). `frameTiming.ts` is a **heavy opt-in sub-harness subordinate to this master gate** — it has its own local enable (`__terminalFrameTiming.enable(true)`), but cannot record unless `perfDebug` is also on.

**RULE for all future perf/debug instrumentation:** gate it on `isPerfDebug()` (or route it through a `perfTrace` helper, which already does). **Never ship always-on perf logging or per-frame timing.** Do not invent a second on/off flag — extend this one.

**Reading the output** (only present when active): `appLogger.warn` lines like `SLOW <label>: <n>ms` (and `SLOW git.refreshBatch:<repo>: <n>ms (body Xms + flush Yms)` — body = our setState loop, flush = dependent effects/memos waking), plus `UI freeze: <n>ms main-thread block` carrying a `perfTrace` breadcrumb that names the culprit. Read via `GET http://localhost:9876/logs` (or `:9877` for a worktree build).


## Worktree Automation Script Context (`TUIC_*`) — Frontend Side

Backend mechanics (`ScriptContext::derive`, the setup-script/file-sync background
chain, the pollable `worktree_setup_status`): see `src-tauri/AGENTS.md`'s
"Worktree Automation Script Context" section. This is the frontend half — the
Smart Prompts variable registry and the worktree-creation ordering fix that
consumes that backend chain's completion event.

**Smart Prompts' `{var}` context-variable list is single-sourced in
`src/data/contextVariables.ts`** (name, description, group, `source` — who resolves it:
Rust, frontend, or one of three host surfaces — plus `repoControlled`/`script` flags).
It replaced four independently hand-maintained copies that had drifted (a stray
undocumented `branch_name` variable, 11 missing entries in one list, two near-identical
picker arrays). `src/__tests__/contextVariablesParity.test.ts` parses `prompt.rs`'s
`ALL_VARS`/`resolve_single_var` and `script_env.rs`'s `ScriptContext::pairs` straight out
of the Rust source and asserts the TS registry stays in sync with both — when you add a
new rust-sourced variable, add it to `contextVariables.ts` first; the parity test will
tell you if `prompt.rs` disagrees. A variable's `TUIC_*` script-env name is always
`TUIC_` + the registry name uppercased — no separate naming step.

**Smart Prompts variables must resolve against the *terminal's* tree, not just "the
active repo."** `executeSmartPrompt` used to resolve `{branch}`/`{diff}`/etc. against
`repositoriesStore.getActive()` (the last-focused repo) while the command it substitutes
into runs in the active *terminal's* cwd — with a worktree tab focused, this could
generate a commit message from the main checkout's diff and then commit it in the
worktree. Fixed via `resolvePromptTreeIn`/`resolvePromptTree`
(`src/utils/repoOwnership.ts`/`src/stores/repositories.ts`), which resolves the tree
(worktree or repo root) that owns a path — falls back to the active-repo behavior when
the terminal's cwd belongs to no registered repo. `resolveFrontendVars` still needs the
**repo root** specifically (never a worktree path): `repositoriesStore.get(...)` is
keyed by repo root, so passing a worktree path there silently drops every `pr_*`
variable — carry repo-root and tree-path as two separate values, don't conflate them.

**`GitPanel/ChangesTab.tsx`'s "Generate commit message" had the same shape of bug** — it
called `executeSmartPrompt(prompt)` with no target override, so it inherited whatever
`executeSmartPrompt` resolves against (the active terminal's tree) rather than
`props.repoPath` — the specific repo/worktree *that GitPanel instance* is showing, which
can differ from whichever repo is currently active/focused. This could generate a commit
message from one repo's diff and commit it (via `doCommit`, which always uses
`props.repoPath`) into another. Confirmed pre-existing via
`git diff main..HEAD -- src/components/GitPanel/ChangesTab.tsx` (that file's only prior
change on this branch was an unrelated indicators feature) before fixing it.

**Fixed by giving `executeSmartPrompt` an optional third `targetPath` parameter** —
when a caller supplies one (a real filesystem path, worktree or repo root, same shape as
the active terminal's cwd), it's what `resolvePromptTreeIn` resolves against instead of
the active terminal's cwd, for BOTH variable resolution and — since `executeHeadless`/
`executeShell` independently derived their own execution cwd from
`active?.cwd ?? repositoriesStore.getActive()?.path` — the actual directory a
shell/headless prompt runs in. `ChangesTab.tsx` now passes its own `props.repoPath`
(already the correct worktree-aware filesystem path — `GitPanel.tsx`'s `gitPath()`, not
its `storeRepoPath`). Omitting `targetPath` (every other existing caller) is
byte-for-byte the old behavior — nothing else needed to change.

**The identical shape recurred one component over: `SmartButtonStrip.tsx` also takes a
`repoPath` prop, used by five different callers (`GitHubPanel.tsx`, `PrSection.tsx`,
`PrDetailPopover.tsx`, `BranchesTab.tsx`, and `ChangesTab.tsx`'s own commit-button strip,
distinct from its direct `executeSmartPrompt` call above), and wasn't forwarding it
either — fixed the same way, in the one shared component, so all five callers got the fix
in a single change rather than five. If you add a sixth caller (or any new caller of
`executeSmartPrompt` that is itself bound to a specific repo/worktree independent of
terminal focus), pass its own path through `targetPath` rather than trusting the
active-terminal fallback — grep for existing `repoPath`-prop components that call
`executeSmartPrompt` before assuming this pattern is now fully closed out.

## Tab/Dialog Openers Bound To "The Active Repo" Must Resolve The Worktree First

Any handler that opens a tab, dialog, or file-picker default path for "the current
repo" and is reachable while a **worktree** tab is focused must resolve
`gitOps.activeWorktreePath() || repositoriesStore.state.activeRepoPath` — never
`repositoriesStore.state.activeRepoPath` alone. `activeRepoPath` is keyed by the main
repo checkout root (`repositoriesStore` is keyed by repo root, not by worktree), so a
handler that reads it directly always resolves to the main checkout even when a
worktree tab has focus.

`useAppShortcutHandlers.ts`'s `openFile`/`openFolder`/`newFile` already get this right.
`openSessionReview` and `toggleDiffScroll` (same file) didn't — Session Diff opened
against the worktree's terminal, then queried Claude Code session transcripts for the
**main repo's** path instead of the worktree's, so it correctly found zero sessions and
showed "No Claude sessions found" even though the worktree had live sessions the
sidebar could see fine (the sidebar resolves each terminal's own real `cwd` via a
different, correct mechanism — `useAgentPolling.ts`). Fixed 2026-09-18 by matching the
established `activeWorktreePath() || activeRepoPath` pattern; regression-tested in
`src/__tests__/hooks/useAppShortcutHandlers.test.ts`.

This is the third documented instance of "the active-repo/active-terminal fallback
resolves to the wrong tree when a worktree is focused" (see also `executeSmartPrompt`'s
`targetPath` and `SmartButtonStrip`'s `repoPath` forwarding above, a different
mechanism — `resolvePromptTreeIn`/`repoOwnership.ts` — for a different set of callers).
Before adding a new handler that opens something scoped to "the current repo," check
whether it needs `gitOps.activeWorktreePath()` first.

**The setup-script/run-script ordering fix threads a real wait, not just documentation.**
`createWorktreeCreationCoordinator.ts`'s `setupNewWorktree` used to `await
runSetupScript(...)` inline before creating the terminal — an implicit ordering guarantee
that Phase 7's background-chain refactor removed with no replacement, so the Run Script
could start concurrently with (or before) the Setup Script (e.g. `npm run dev` racing `npm
install`). Restored via `waitForSetupScriptCompletion` (same file): a `listen()` on
`worktree-setup-script-completed`, matched by `repoPath`+`branch`, gated on
`effective?.setupScript` being truthy, with a generous (900s) safety-net timeout so a lost
event (backend crash, SSE disconnect) can't hang worktree creation forever — it resolves
either way, never rejects.
