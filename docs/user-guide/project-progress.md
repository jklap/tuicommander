# Project Progress

Status: storage foundation implemented; the Progress tool, controls, panel, and
export described here are not available yet. This page distinguishes the shipped
project-local persistence layer from the remaining planned interfaces.

Progress answers: **What meaningfully changed since I last looked?**
It organizes outcomes by project and workstream: capabilities, decisions,
discoveries, blockers, and completed objectives. Agent task completion alone does
not create a milestone.

The planned `progress` tool accepts `type`, `summary`, and an optional `workstream`.
It persists the event and shows a toast in one call. Project and source identity
come from TUICommander. The default tool description is short; the following
prompt is optional for users who want more control.

## Optional agent instructions

Copy this block into agent instructions once the Progress feature is available.
Use the live MCP schema for argument names and available management actions.

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

## Planned controls and storage

- **Progress panel:** What changed, Today, Blockers, Completed, current project
  state, and timeline. Provenance is expandable. The existing notification bell
  keeps all its current sections and behavior; Progress adds an unread entry that
  opens the panel. Dismissing a toast leaves the event in history.
- **Stop / Resume:** pause or resume collection for one project. Stop preserves
  history and does not stop agents. Resume records future reports without backfill.
- **Delete selected:** remove specific events. Reading or dismissing is separate
  from deletion.
- **Clear project:** remove its history and workstreams and pause collection.
  Existing `progress.md` exports remain unchanged; they are separate files.
- **Corrections:** rename/merge workstreams or events, edit summaries, move events,
  resolve blockers, and explicitly complete or reopen a workstream.
- **Export:** preview and explicitly write `<project-root>/progress.md`. Replacing
  an existing file requires an explicit choice. Export does not commit or push.

The authoritative store will be `<project-root>/.tuic/progress.sqlite3`. Managed
workspaces report to their owning project's store, so removing a temporary
workspace does not remove project history. The database is local runtime state,
excluded from Git; Markdown is the portable, optionally versioned export.
Editing that export does not change the stored history. No background model is
required to collect, group, display, or export explicit reports.

If the database is corrupt, TUICommander preserves the database and WAL data and
retains any existing SQLite SHM sidecar under unique `.corrupt-<uuid>` names before
creating a validated empty replacement. The triggering operation reports the
recovery and must be retried; it never reports success against the replacement as
though the previous history were still present. The preserved artifacts remain
available for manual recovery.
