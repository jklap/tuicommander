# Project Progress

Progress answers one question: **what changed in this project since I last
looked?**

Progress records outcomes, not work. It groups them by project and workstream:
capabilities, decisions, discoveries, blockers, and completed objectives. The
start or the end of an agent task is not an outcome.

## Workstream compared with task

A **task** is what an agent does: an edit, a command, a test run, a commit.
Tasks are frequent and TUICommander does not record them.

A **workstream** is an objective that the project wants. It can continue for
hours or days, and more than one agent can work on it. One task can cause no
event or several events. Several tasks can cause one event.

A workstream starts when an agent reports it. You do not create workstreams in
advance. Agents group their reports with the same workstream name. TUICommander
compares these names after it removes extra spaces and changes the letters to
lower case. There is no approximate matching.

When you rename a workstream, TUICommander keeps the old name as an alias.
An agent that still uses the old name therefore reports into the renamed
workstream.

## How an agent reports

The MCP `progress` tool records the event and shows its toast in one call. It
accepts three fields:

| Field | Required | Limit | Meaning |
|---|---|---|---|
| `type` | yes | `started`, `milestone`, `blocked`, `done` | The kind of outcome |
| `summary` | yes | 500 characters | One standalone sentence about the outcome |
| `workstream` | no | 80 characters | The objective that the event belongs to |

```json
{"type":"milestone","summary":"OpenRouter applications can now be identified.","workstream":"Shadow AI Detection"}
```

TUICommander adds the identity, the time, and the available provenance. The
agent does not supply them.

| Type | Effect on the workstream |
|---|---|
| `started` | The workstream starts or reopens. An active blocker stays active. |
| `milestone` | The workstream is progressing, unless it is blocked or done. |
| `blocked` | An active blocker is added. Blocked has precedence. |
| `done` | The workstream is complete. Its active blockers close. |

The receipt reports `recorded`, `duplicate`, or `paused`. An identical report
from the same reporter within 60 seconds gets the first receipt again. This is
a retry rule, not semantic deduplication.

The agent must not retry a `paused` receipt, and must not resume collection.

Reporting needs no lookup, no history read, and no periodic work. The default
tool description is short. The full prompt below is optional.

## Optional agent instructions

Copy this block into your agent instructions when you want the fuller reporting
policy. It is not part of the TUICommander payload. Use the live MCP schema for
the argument names.

```text
Report project changes worth remembering tomorrow through TUICommander progress.

Types:
- started: a meaningful objective or workstream begins or reopens.
- milestone: a capability, decision, discovery, or validated outcome is achieved.
- blocked: an objective cannot continue until a dependency or decision is resolved.
- done: the whole objective or workstream is complete.

Write one concise, standalone outcome sentence. Name the workstream if clear;
reuse a known name. Otherwise omit it. Preserve uncertainty and verification scope.

Report:
- OpenRouter applications can now be identified.
- MCP configuration discovery works across macOS and Windows.
- The enforcement architecture has been agreed.
- Provider attribution changed approach after a discovered limitation.
- Windows packaging is working.

Skip edits, commands, test runs, commits, refactors, compilation fixes, routine
task lifecycle, and recoverable worker coordination. A local worker blockage is
not a project blocker when the workstream can continue. A task can yield zero or
several events; several tasks can yield one milestone.

Report when the change is known, using the context already available. Do not scan
history, poll, maintain a taxonomy, or repeat a result already reported. Prefer
one report from the agent that establishes the outcome; handoffs do not repeat it.

Progress already persists and shows a toast. Do not also call ui.toast for the
same event. Use ui.toast for immediate operational messages when needed.

Respect paused collection; do not retry, resume, or backfill automatically.
Delete, clear, reorganize, resolve blockers, or export only when the user asks.
Never mark done beyond the scope that was verified.
```

The measured effect of this block is in
[Progress reporting evaluation](../evaluations/progress-reporting.md).

## The Progress panel

Open the panel from the command palette with **Open Project Progress**. The
notification bell adds the unread count of all projects and opens the panel.
The panel also works in browser and PWA mode.

Select **All projects** or one project. Then select a view:

- **Since last visit** — the events that arrived after your last acknowledgement.
- **Today** — the events of the current day.
- **Blockers** — the active blockers.
- **Completed** — the completed objectives.
- **History** — all events, most recent first.

**Mark viewed** acknowledges the snapshot that you see. Later events stay
unread. Acknowledgement in one project does not acknowledge another project.

Provenance expands on demand. It contains only the reporter identifier, the
reporter name, the session identifier, and the workspace path. Data that is not
available stays absent.

A live report shows one toast. Progress events are not copied into Messages.

### Empty, paused, and unavailable

A view with no events shows **No progress matches this view.** This is not an
error. Change the view or the scope to see more.

A project whose collection is paused shows **Paused** in its state card, and the
**Pause** button becomes **Resume**. The history stays visible while collection
is paused.

A project that the backend cannot read shows a red card with the project name
and the reason above the list. The panel does not hide such a project and does
not show it as a project without progress. Other projects in the same scope
continue to show their events.

### Narrow windows

Below 720 pixels of window width the panel fills the area to the right of the
sidebar, its minimum width no longer applies, and the scope row and the export
row wrap to more than one line. Hide the sidebar to give the panel the full
width. On a phone, use the mobile interface, where Progress is a tab and the
panel fills the screen. The mobile interface does not load the repository list
yet, so its Progress tab shows the views and the controls but no events.

## Controls

The panel, the MCP `repo` actions, the Tauri commands, and the HTTP routes give
the same operations. Every operation names one project.

| Operation | Behavior |
|---|---|
| Pause / Resume | Stop or start collection for one project. Pause keeps the history. Pause does not stop agents. Resume does not backfill. |
| Delete selected | Remove the events that you name. |
| Clear project | Remove the history, the workstreams, and the read state, and pause collection in the same transaction. Supply the current revision. |
| Corrections | Edit a summary, move an event, rename or merge workstreams, merge events, resolve a blocker, or set a workstream state. |
| Mark viewed | Acknowledge the exact snapshot that you saw. |
| Export | Preview or write `progress.md`. |

Clear does not delete an existing `progress.md`. That file is a separate export.

One limit applies to the transports. The Tauri `progress_export` command is
built only into the desktop application. A browser client, a remote client, and
the headless `tuic-remote` daemon use the `POST /progress/export` route instead.
The daemon serves all the Progress routes and protects them with its usual
authentication.

## Storage

The authoritative store is `<owning-project-root>/.tuic/progress.sqlite3`.
Managed workspaces report to the store of the project that owns them, so the
removal of a temporary workspace does not remove the project history. An
inherited copy of the database in a workspace is not authoritative.

A caller with no registered project gets `project_required`. TUICommander never
uses the focused repository instead.

The database and its SQLite sidecars are local runtime state. TUICommander adds
them to `.git/info/exclude`, not to the tracked `.gitignore`. A clone therefore
carries no history. Use the Markdown export to move the history.

A project root that is not in a Git repository records progress normally. The
excludes are housekeeping for a repository, so TUICommander writes none and
continues.

If SQLite reports corruption, TUICommander keeps the database and the WAL data,
and keeps any existing SHM file, under unique `.corrupt-<uuid>` names. It then
creates an empty replacement and validates it. The operation that found the
corruption fails and names the kept files. You must do that operation again. An
empty replacement never looks like a successful operation.

## Markdown export

Export is manual. The target is always `<owning-project-root>/progress.md`, and
not the working directory of the agent.

1. Select one project and, if you want it, **include provenance**.
2. Preview. The preview contains the full Markdown, the snapshot identifier, the
   snapshot revision, and the snapshot time.
3. Write. The write keeps the same snapshot. If the snapshot moved, TUICommander
   refuses the write and asks for a new preview.

The Markdown contains the project name, the snapshot revision, the UTC snapshot
time, the collection state, the workstreams with their active blockers, the
active blockers, and the dated history. The output is deterministic for the
same snapshot and the same options.

Source metadata is off by default. Transcripts, commands, credentials, and
tokens are never exported.

To replace an existing file, you must confirm the replacement and supply the
exact content that the preview returned. A file that a human changed after the
preview, a file that was removed, a symlink, or a directory is refused. The
write uses a temporary file in the same directory and an atomic replacement, so
a failure keeps the original file.

Export does not pause collection, does not clear history, does not change the
read state, and does not stage, commit, or push.

Changes that you make in `progress.md` do not go back into the database.

## Not in version 1

These items stay outside this version on purpose:

- Periodic inference, transcript scanning, and background LLM summaries.
- Approximate or automatic workstream discovery. Grouping is exact.
- Integration with external issue trackers, and remote synchronization.
- Scheduled exports and Markdown import.
- Estimates, percentages, and productivity scores.
