# Wiz Kanban

Wiz Kanban is a plugin that visualizes the **Wiz framework** workflow — stories, plans, and
reviews tracked as plain Markdown files in your repo — as a kanban board inside a TUICommander
panel. It reads and (for stories) rewrites those files directly; there's no separate database or
server.

This plugin doesn't invent the stories/plans/reviews convention itself — it's a *viewer and
light editor* for files you (or an agent) author by hand or with your own tooling. Everything
below documents exactly what the plugin expects those files to look like, since getting the
filename or frontmatter slightly wrong makes a file silently disappear from the board rather than
show an error.

## Installing it

Wiz Kanban isn't bundled with the app by default. Install it from the Community Registry:

1. Open **Settings** (`Cmd+,`) → **Plugins** → **Browse**
2. Find **Wiz Kanban** (author `sstraus`) and click **Install**

See [Plugins](./plugins.md) for other install methods (ZIP, deep link, manual copy).

## Opening the board

Right-click inside any terminal pane and choose **Wiz Kanban** from the context menu. The board
opens in a new panel tab.

The board always follows **the currently active repository** — there's no per-board repo picker.
If no repo is active when you open it, nothing happens (no board, no error). Switching repos or
branches while the board is open automatically re-points it at the new repo; you never need to
reopen it by hand.

The panel has three views, switched with the buttons at the top: **Stories**, **Plans**, **Reviews**.

## Stories

The Stories view is the only one with drag-and-drop; Plans and Reviews are read-only boards (see
below).

### Where stories live

Story files must sit directly inside a `stories/` directory at the root of the active repository
(`<repo>/stories/*.md`) — no subdirectories are scanned, except `stories/archive/` (see
**Archiving**, below), which the board itself never lists.

### Filename convention

Every story file's name must match exactly:

```
{seq}-{hash}-{status}-{priority}-{title-slug}.md
```

For example: `12-a1b2c3-ready-P2-improve-error-messages.md`

| Segment | Format | Notes |
|---|---|---|
| `seq` | one or more digits | Used only to sort the board / your file listing; doesn't need to be zero-padded, just needs to sort the way you want. |
| `hash` | lowercase hex (`a-f0-9`) | Any length ≥ 1. A short unique-ish id you assign — the plugin never generates or checks these for collisions. |
| `status` | one of `pending`, `ready`, `in_progress`, `blocked`, `complete`, `wontfix` | Becomes the card's column. Rewritten automatically by drag-and-drop (see below) — you don't need to rename files by hand once a story exists. |
| `priority` | `P0`–`P3` | Captured but **not currently shown anywhere on the card** — treat it as metadata for your own tooling/search, not something this board surfaces. |
| `title-slug` | anything | Free text; only used as a fallback display title if frontmatter has no `title:`. |

**If a filename doesn't match this pattern exactly — wrong hash characters, a missing priority
segment, extra dashes in the wrong place — the file is silently skipped.** It won't appear on the
board and no error or log entry is produced. If a story you just created isn't showing up, check
the filename first.

### Frontmatter

Every story file also needs a YAML frontmatter block at the very top:

```markdown
---
id: 12-a1b2c3
title: Improve error messages
status: ready
priority: P2
created: "2026-09-20T10:00:00Z"
updated: "2026-09-28T14:30:00Z"
dependencies: ["7-9f0e1d"]
---

## Work Log

### 2026-09-28 — Started investigation
...
```

**A file with no frontmatter block at all (no leading `---` ... `---`) is also silently
skipped**, same as a bad filename.

| Field | Required? | Behavior |
|---|---|---|
| `id` | No | Falls back to `{seq}-{hash}` from the filename. This is what drag-and-drop actions refer to internally. |
| `title` | No | Falls back to the filename's title-slug with dashes turned into spaces. |
| `status` | No, but see below | Falls back to the filename's status segment. **Must be one of the 6 known statuses** (see the filename table) — anything else and the card doesn't render in *any* column, even though the file was successfully parsed. |
| `priority` | No | Falls back to the filename's priority segment. Parsed but unused by the UI today. |
| `created` | No | Only consulted as a fallback for archiving (see below) when `updated` is absent. |
| `updated` | No, but see below | Drives archive eligibility, and is what drag-and-drop rewrites on every status change. |
| `dependencies` | No | An array of story ids (e.g. `["7-9f0e1d"]`). The only effect is a small 🔗 badge on the card when non-empty — there's no dependency graph, validation, or lagging-dependency warning here (that's a `md-kanban`-plugin feature, not this one). |

**`status:` and `updated:` must already exist as literal lines in the frontmatter for
drag-and-drop to keep the file in sync.** When you drag a card to a new column, the plugin
renames the file's status segment *and* tries to update the frontmatter's `status:` and
`updated:` lines in place. If either line doesn't already exist in the file, that particular
line-based rewrite silently does nothing (no error) — you'll end up with a filename that says
one status while the frontmatter still says another. Always include both lines up front, even
with placeholder values, when creating a new story by hand.

### The Work Log gate

A story can't be dragged to **Complete** or **Won't Fix** until it has at least one Work Log
entry. The plugin checks for a literal `## Work Log` heading somewhere in the file, followed
later by at least one `### ` sub-heading:

```markdown
## Work Log

### 2026-09-28 — Fixed the thing
Describe what you actually did here.
```

Dragging a card without this to Complete/Won't Fix shows an error toast and the move is
rejected outright — no rename, no frontmatter change.

### Drag-and-drop

Drag a card between columns to change its status. This:

1. Rewrites the `status:` and `updated:` frontmatter lines in the file (see the caveat above).
2. Renames the file so its status segment matches.
3. Adds the change to a **pending changes** bar at the bottom of the board.

A plain click (no drag) on a card opens the underlying `.md` file instead of moving it.

### Pending changes / "Apply to Claude"

Every status change you make shows up in a bar at the bottom of the board listing each change
(`storyId: Old Status → New Status`) with an individual **✕ undo** button, plus:

- **Apply to Claude** — sends a summary of all pending changes as a message into whichever
  terminal session is currently active/focused (via the same mechanism as typing into it). If no
  terminal session is active, you get an error toast instead. Once sent, the pending list clears.
- **Discard all** — clears the pending list without sending anything. The file changes
  themselves are *not* undone — only the notification queued for "Apply to Claude" is discarded.

Undoing a single pending item removes it from the list, but — like Discard — does **not** revert
the file. The pending bar tracks *what to tell your agent*, not an edit history; if you want to
actually revert a status change, drag the card back or edit the file yourself.

### Archiving (`Archive >5d`)

The **Archive >5d** button moves every `complete`/`wontfix` story whose `updated` field (or
`created` if `updated` is missing) is more than 5 days old into `stories/archive/`.

**`stories/archive/` must already exist.** The plugin doesn't create it — if the directory is
missing, the move fails for every candidate file and you'll see an "Archived 0, N failed" toast.
Create the folder once (`mkdir stories/archive`, or just an empty file inside it) and archiving
works from then on.

A story with no parseable `updated`/`created` date is never archived, regardless of age.

## Plans

The Plans view lists every `.md` file directly under `<repo>/plans/`, grouped into 3 columns by
frontmatter `status:`:

| Frontmatter `status:` | Column |
|---|---|
| `draft`, `validated`, missing, or anything unrecognized | **Planning** |
| `in_progress` | **In Progress** |
| `parked`, `completed`, `rejected` | **Done** |

There's no filename convention for plans — any `.md` file in the directory is picked up. Cards
are **click-to-open only**; there's no drag-and-drop here, since a plan's status isn't something
this board can safely rewrite the way a story's filename-encoded status can.

## Reviews

The Reviews view lists every `.md` file directly under `<repo>/reviews/`, grouped into 2 columns
by frontmatter `status:`:

| Frontmatter `status:` | Column |
|---|---|
| `open`, missing, or anything unrecognized | **Open** |
| `triaged` | **Triaged** |

Also click-to-open only, no drag-and-drop. Sorted newest-filename-first (reverse alphabetical),
which works out to newest-first if your review filenames start with a date.

## Search

Each view has its own search box in the filter bar:

- **Stories** — matches against title or id.
- **Plans / Reviews** — matches against the filename (minus `.md`).

Switching views clears the search box.

## Multiple repos and worktrees

The board is not scoped by settings — it always tracks whichever repo/branch is currently active
in the app. If you work across several repos or worktrees, each has its own independent
`stories/`, `plans/`, `reviews/` directories, and switching your active repo/branch instantly
re-renders the board against the new one (re-fetching from disk, not a cached view).

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| A story file doesn't show up at all | Filename doesn't match the `{seq}-{hash}-{status}-{priority}-{slug}.md` pattern, or the file has no `---`-delimited frontmatter block. Check both carefully — there's no error log for this. |
| A story shows up in the file list but not on any board column | Its effective `status` (frontmatter, or filename if frontmatter omits it) isn't one of the 6 recognized values. |
| Dragging a card to Complete/Won't Fix does nothing but show an error | No `## Work Log` heading with at least one `### ` entry underneath it in the file yet. |
| A story's filename changed after a drag but the frontmatter still shows the old status | The file was missing a `status:` and/or `updated:` line in its frontmatter before the drag — add both lines (even with placeholder values) and future drags will stay in sync. |
| "Archived 0, N failed" toast | `stories/archive/` doesn't exist yet — create it. |
| "No active terminal session" when clicking Apply to Claude | No terminal tab is currently focused/active. Click into a terminal pane first. |
| The board doesn't open at all when you right-click | No repository is currently active — open or select a repo first. |

## See also

- [Plugins](./plugins.md) — installing, managing, and writing plugins in general.
- [Plugin Authoring Guide](../plugins.md) — the full `PluginHost` API, for anyone extending or
  forking this plugin.
