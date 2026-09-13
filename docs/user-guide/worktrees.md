# Git Worktrees

TUICommander uses git worktrees to give each branch an isolated working directory.

## What Are Worktrees?

Git worktrees let you check out multiple branches simultaneously, each in its own directory. Instead of stashing or committing before switching branches, each branch has its own complete copy of the files.

## How TUICommander Uses Them

When you click a non-main branch in the sidebar:

1. TUICommander creates a git worktree for that branch
2. A terminal opens in the worktree directory
3. You work independently without affecting other branches

Main branches (main, master, develop) use the original repository directory — no worktree is created.

## Worktree Storage Strategies

Configure where worktrees are stored (Settings → Git & GitHub → Worktree Defaults → Storage):

| Strategy | Location | Use case |
|----------|----------|----------|
| **Sibling** (default) | `{repo_parent}/{repo_name}__wt/` | Keeps worktrees near the repo |
| **App directory** | `~/Library/Application Support/tuicommander/worktrees/{repo_name}/` | Centralised storage |
| **Inside repo** | `{repo_path}/.worktrees/` | Self-contained, add to `.gitignore` |
| **Claude Code default** | `{repo_path}/.claude/worktrees/` | Compatible with Claude Code's native `EnterWorktree` |

Override per-repo in Settings → Repository → Worktree.

## Two Mechanisms: Worktree or Clone

A workspace is built one of two ways. Both land in the same directory (see
*Worktree Storage Strategies* above); what differs is the isolation you get.

| | **Linked worktree** | **Copy-on-write clone** |
|---|---|---|
| What it is | A second checkout sharing the repo's `.git` | An independent repository, block-shared with the original |
| Two workspaces on one branch | Not possible — git refuses | Possible; this is the reason the clone exists |
| Build output (`node_modules`, `target`) | Empty — you rebuild | Arrives warm, at near-zero disk cost |
| Parent's uncommitted work | Stays in the parent | Carried over by default (configurable) |
| Where commits live | In the parent repo, shared | **Only in the clone**, until you publish |
| Removing it | Deletes a checkout; commits survive in the parent | Deletes a repository; unpublished commits are gone |

A clone is not a full copy: the filesystem shares the blocks until something
rewrites them. Measured on a 12 GB repository: **19 MB of real disk and 26
seconds**.

In the sidebar, a copy-on-write clone uses a cow-head icon while a linked
worktree uses the fork icon. The Worktree Manager also labels clone rows with a
`clone` badge. Both surfaces use the same backend lifecycle snapshot: `Dirty`,
`N unpublished`, `Published`, `Merged`, or `Unknown`. `Published` means the tip
exists in a parent or remote ref; it does not mean the default branch contains
it. Large tracked-line counts are compacted in the sidebar (`9.9k`), with the
exact count in the tooltip.

Copy-on-write needs filesystem support (APFS, Btrfs, XFS with reflink…) and both
directories on the same volume. TUICommander never trusts the filesystem *name*
for this — it makes a real copy-on-write copy of one file and looks at whether it
worked, trying macOS `clonefile` and then a reflink copy. Where neither works,
**Auto** gives you a linked worktree.

### When a clone is refused

These stop a clone with the reason stated; `Auto` then falls back to a linked
worktree, `Clone` reports the error:

- the destination sits inside the source repository (the copy would walk into
  itself — this is what the **Inside repo** and **Claude Code default** storage
  strategies do)
- the source is itself a linked worktree, not a primary checkout
- the source is a bare repository
- a rebase, merge, cherry-pick, revert, or bisect is in progress
- a lock is held by a live git process

A *stale* lock is not a refusal: it is left out of the copy instead.

## Creating Worktrees

### From the `+` Button (with prompt)

Click `+` next to a repository name. A dialog opens where you can:
- Type a new branch name (creates branch + worktree)
- Select an existing branch from the list
- Choose a "Start from" base ref (default branch, or any local branch)
- Generate a random sci-fi name
- Pick the **Mechanism** and, for a clone, what happens to the **Parent's
  changes** (see below)

#### Mechanism

| Option | What it does |
|--------|--------------|
| **Auto** (default) | A copy-on-write clone where the filesystem and the repository allow it, a linked worktree otherwise — telling you why it degraded |
| **Clone** | A copy-on-write clone, or an error naming the check that refused. Use it when you specifically need the isolation |
| **Worktree** | A linked worktree even where a clone is available |

#### Parent's changes (clone only)

What the clone does with work you have not committed in the parent repository.
The picker is hidden when Mechanism is **Worktree**, because a linked worktree
is a fresh checkout and carries nothing over.

| Option | What it does | Cost |
|--------|--------------|------|
| **Keep** (default) | The parent's modified and untracked files are simply there | Free — nothing is written |
| **Drop untracked** | `git clean -fd`: removes untracked files but **keeps** ignored build output | Metadata only |
| **Reset** | `git reset --hard --recurse-submodules` then the same clean — a pristine tree | The only option that costs real disk: every rewritten block stops being shared (measured 15 MB → 113 MB) |

### From the `+` Button (instant mode)

When "Prompt on create" is off (Settings → Git & GitHub → Worktree Defaults), clicking `+` instantly creates a worktree with an auto-generated name based on the default branch. Skipping the dialog takes its defaults — Mechanism **Auto**, Parent's changes **Keep** — not a different set.

### From Branch Right-Click (quick-clone)

Right-click any non-main branch without a worktree → **Create Worktree**. This creates a new branch named `{source}--{random-name}` based on the selected branch, with a worktree directory.

### What a path that does not choose gets

**Auto**, with **Keep** — the backend's own defaults, which is why the dialog
preselects them. That applies to the quick-clone above, to the instant `+`, and
to the auto-fix worktree created from a GitHub issue: each one asks for a
workspace without naming a mechanism, so each gets a copy-on-write clone where
the repository and filesystem allow it and a linked worktree otherwise.

## Worktree Settings

Global defaults apply to all repos. Per-repo overrides take precedence when set.

### Global Defaults (Settings → Git & GitHub → Worktree Defaults)

| Setting | Options | Default |
|---------|---------|---------|
| **Storage** | Sibling / App directory / Inside repo / Claude Code default | Sibling |
| **Prompt on create** | On / Off | On |
| **Delete branch on remove** | On / Off | On |
| **Auto-archive merged** | On / Off | Off |
| **Orphan cleanup** | Ask before removing / Auto-remove / Keep | Ask |
| **PR merge strategy** | Merge / Squash / Rebase | Merge |
| **After merge** | Archive / Delete / Ask | Archive |

### Per-Repository Overrides (Settings → Repository → Worktree)

Each setting can use the global default or be overridden for a specific repository.

## Merge & Archive

Right-click a worktree branch → **Merge & Archive** to:

1. Merge the branch into the main branch
2. Handle the worktree based on the "After merge" setting:
   - **Archive**: Moves the worktree directory to `__archived/` — the whole directory, uncommitted changes included (accessible but removed from sidebar)
   - **Delete**: Removes the worktree and branch entirely. Anything not committed is gone
   - **Ask**: Merge succeeds, then you choose what to do

The merge uses `--no-edit` for a clean fast-forward or merge commit. If conflicts are detected, TUICommander attempts `git merge --abort` and leaves the worktree intact. If that abort fails, the error message tells you the repository may still be conflicted and includes the manual abort command.

### Uncommitted work in the worktree

Both **Archive** and **Delete** remove the worktree, so TUICommander asks first whenever the worktree is not known to be clean — whether or not the branch carries commits, and whether the cleanup was started by hand or by **Auto-archive merged**. The confirmation names what happens to the work: archived files travel to `__archived/`, deleted files do not come back. If the check itself cannot run, that counts as "not clean" and the cleanup still stops.

The automatic sweep never asks — it keeps a dirty worktree and reports it in the status line (`kept N with uncommitted work`).

When using **Ask** mode, the cleanup dialog detects uncommitted changes and auto-stashes them during the branch switch. An "Unstash after switch" checkbox lets you restore changes on the target branch. That stash covers the **base repository**; the warning under the worktree step is about the branch's own directory, which is a different place.

### Archive Script

A per-repo lifecycle hook that runs **before** a worktree is archived or deleted. Configure it in Settings → Repository → Scripts tab, or via `.tuic.json` (`archive_script` field).

- The script runs in the worktree directory that is about to be removed
- If the script exits with a non-zero code, the archive/delete operation is **blocked** and an error is shown
- Use cases: backing up local data, cleaning up resources, notifying external systems
- The script is invoked via the platform shell (`sh -c` on macOS/Linux, `cmd /C` on Windows)

## Moving Terminals Between Worktrees

Right-click a terminal tab → **Move to Worktree** to move it to a different worktree. The terminal will `cd` into the target worktree path, and the tab automatically reassigns to the new branch in the sidebar.

Also available via **Command Palette** — type "move to worktree" to see available targets for the active terminal.

## Publishing a Clone

A clone is an independent repository, so its commits exist **only there** until
they are published. Running `git merge <branch>` in the parent does not find
them — and if the parent has a branch with the same name, it silently merges
that stale ref instead.

**Publish** appears on clone rows in the Worktree Manager (and nowhere else: a
linked worktree shares its refs with the parent already). It does two things,
reported separately:

1. **Into the parent** — the workspace tip is staged under
   `refs/tuic/published/<id>` in the parent repository, then
   `refs/heads/<branch>` is fast-forwarded to it. Fast-forward only: a parent
   branch that has diverged, or that the parent currently has checked out, is
   refused with the reason rather than forced.
2. **Out to origin** — a push, run **from the parent repository** and only after
   step 1 succeeded. An unreachable origin is reported on its own, so it never
   reads as "the parent did not get it"; a parent that *refused* the update,
   however, stops the push, because origin must never move ahead of the parent
   that vetoed the move. What reaches origin is the ref the parent accepted, not
   the clone's live branch — the clone may have moved on since.

The clone cannot reach origin by itself: its `origin` remote is given the same
unpushable push URL as its `parent` remote, so "publish is the only transfer
path" is an invariant rather than an instruction.

Because the objects are transferred before the ref moves, retrying a refused
publish costs no transfer.

## Removing Worktrees

- **Sidebar `×` button** on a non-main branch — Removes worktree and branch entry
- **Right-click → Delete Worktree** — Context menu option
- Both prompt for confirmation

Removing a worktree:
1. Closes all terminals associated with that branch
2. Runs `git worktree remove` to clean up
3. Removes the branch entry from the sidebar
4. If branch deletion was requested but `git branch -d` keeps the branch because it is not safely merged, shows a status message that the worktree was removed and the branch was kept

### Removing a clone

Deleting a clone deletes a repository, so anything it holds and has not
published is gone. Immediately before removal, TUICommander refreshes one
backend verdict for the exact workspace id. The confirmation separately names
working-tree dirtiness, unpublished commits, and whether `HEAD` is published or
merged. Dirty files or unpublished clone commits require an explicit
destructive confirmation; `Unknown` blocks removal. The backend repeats its
guards during deletion, so a workspace that changes after the dialog is kept
visible and reports the refusal.

A clone whose directory no longer looks like a repository is also refused, so a
stale record cannot authorise deleting whatever now sits at that path. A
workspace that is already gone is dropped silently.

## Worktree Manager Panel

Open the Worktree Manager with `Cmd+Shift+W` (or via the Command Palette → "Worktree Manager"). It shows a unified view of all worktrees across your repositories.

### What It Shows

Each worktree row displays:
- **Branch name** and **repository badge**
- **`clone` badge** — the workspace is a copy-on-write clone, not a linked worktree. The directory does not say which, and publish and remove behave differently
- **Current branch name** — clone rows read the branch from the clone itself, so a branch renamed inside the clone is reflected after refresh
- **Lifecycle badges** — dirty working-tree state and exact commit reachability (`N unpublished`, `Published`, `Merged`, or `Unknown`), from the same progressive refresh used by the sidebar
- **Dirty status** — file additions/deletions, or "clean"
- **PR state** — open (with PR number), merged, or closed
- **Last commit timestamp** — relative time since last activity
- **Main badge** — marks the main branch (actions disabled)

Orphan worktrees (detached HEAD or deleted branch) appear at the bottom with a warning badge and a **Prune** button to clean them up.

### Filtering

- **Repo pills** — Click a repository name to filter by repo (appears when you have multiple repos)
- **Text search** — Type in the search field to filter branches by name
- Filters compose: selecting a repo and typing text shows only matching branches in that repo

### Single-Row Actions

Each worktree row has action buttons (visible on the right):
- **`>_`** — Open a terminal in the worktree directory
- **Publish** — Clone rows only: fetch this workspace's commits into the parent repository and push them to origin
- **`✔`** — Merge the branch into main and archive (disabled for main branches)
- **`✕`** — Delete the worktree and branch (disabled for main branches)

### Batch Operations

Select multiple worktrees using the checkboxes (shown when more than one selectable worktree exists). A batch bar appears with:
- **Merge & Archive (N)** — Merges and archives all selected branches
- **Delete (N)** — Deletes all selected worktrees

Use the **Select All** checkbox in the toolbar to toggle all non-main worktrees.

## MCP Worktree Creation (AI Agents)

AI agents connected via MCP can create worktrees using `repo action=worktree_create`.

It takes the same two choices the dialog offers: `mode` (`auto` | `cow` |
`worktree`, default `auto`) and `dirty` (`inherit` | `clean_untracked` |
`clean`, default `inherit`). The response says which mechanism it got and, if
it degraded, why.

Because nothing enforces how an agent treats a workspace, the response also
carries instructions: how many paths of the parent's work in progress came
along (and that repairing them is not the task), which build directories
arrived warm and how large they are (so the agent does not run an install or a
full build to "set up"), and — for a clone — that its commits exist only there,
that the parent cannot see the branch, and that `git merge` in the parent would
silently take a stale same-named ref.

When TUICommander receives a worktree creation event while the active terminal is running an agent, the confirmation offers **Open Worktree**. Accepting selects an existing terminal in the new worktree or creates one when needed. The running agent stays in its original terminal, branch, and working directory; TUICommander does not relabel or interrupt it.

### Claude Code — Agent Bridge

Claude Code cannot change its working directory mid-session. When CC creates a worktree via MCP, the response includes a `cc_agent_hint` field with:

- `worktree_path` — Absolute path to the worktree directory
- `suggested_prompt` — Instructions for spawning a subagent that works in the worktree using absolute paths

CC should spawn a subagent (Agent tool) with the suggested prompt. The subagent uses Read, Edit, Glob, Grep with absolute file paths and `cd <path> && ...` for shell commands.

### Other MCP Clients

Non-Claude Code MCP clients receive the standard `{worktree_path, branch}` response without the `cc_agent_hint` field. These clients can change into the worktree directory directly.

### Publishing and Checking Unpublished Commits (AI Agents)

A copy-on-write clone is an independent repository: commits made inside it exist
only there until something moves them back. `repo action=worktree_unpublished`
(requires `path`, the `workspace_id` from `worktree_list`) reports how many
commits are reachable from the clone's `HEAD` and nowhere else — not on any
remote, not on the parent's mirrored ref for that branch. It is always `0` for
a linked worktree, whose objects already live in the parent.

`repo action=worktree_publish` (same two required fields) lands those commits
in the parent repo and pushes them to origin, using the exact same safe
staging-ref mechanism as the **Publish** button in the Worktree Manager and the
HTTP route — one implementation behind all three surfaces. It only ever
fast-forwards the parent branch: publishing is refused, not forced, if that
branch is checked out in the parent (or one of its other linked worktrees) or
if the parent has commits the clone does not, so an agent can never overwrite
someone else's work by merging a stale same-named ref. The parent update and
the origin push are reported independently, so a missing or unreachable origin
does not undo a parent update that already landed. Publishing a linked worktree
is a no-op — its refs are already shared with the parent.

Both actions run on TUICommander's blocking pool, and the local MCP bridge
grants them the same generous response window as `worktree_create` and
`worktree_remove`, since publishing also pushes over the network.

## External Worktree Detection

TUICommander monitors `.git/worktrees/` for changes. Worktrees created outside the app (via CLI or other tools) are detected and appear in the sidebar after the next refresh.

## Branch Switching

Switching branches in TUICommander does not change the working directory of existing terminals. Each branch's terminals stay in their worktree path.

When you switch branches:
- Previous branch's terminals are hidden (but remain alive)
- New branch's terminals are shown
- If the new branch has no terminals, a fresh one is created
