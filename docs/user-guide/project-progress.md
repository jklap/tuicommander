# Project Progress

Status: project-local storage, reporting, controls, panel, and manual Markdown
export are implemented.

Progress answers: **What meaningfully changed since I last looked?**
It organizes outcomes by project and workstream: capabilities, decisions,
discoveries, blockers, and completed objectives. Agent task completion alone does
not create a milestone.

The `progress` tool accepts `type`, `summary`, and an optional `workstream`.
It persists the event and shows a toast in one call. Project and source identity
come from TUICommander. The default tool description is short; the following
prompt is optional for users who want more control.

## Optional agent instructions

Copy this block into agent instructions when you want the fuller reporting policy.
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

## Controls, export, and storage

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
- **Export:** choose one project, optionally include source metadata, and preview
  the complete `<owning-project-root>/progress.md` before writing. The preview
  identifies its database revision and snapshot time. Writing succeeds only if
  that same snapshot is still current. Replacing an existing regular file
  requires explicit confirmation and the exact content returned by the preview;
  a human edit, removal, symlink, or directory at the target is refused.

The authoritative store will be `<project-root>/.tuic/progress.sqlite3`. Managed
workspaces report to their owning project's store, so removing a temporary
workspace does not remove project history. The database is local runtime state,
excluded from Git; Markdown is the portable, optionally versioned export.
Editing that export does not change the stored history. No background model is
required to collect, group, display, or export explicit reports.

The Markdown is deterministic English for its snapshot and options. It includes
the project, UTC snapshot revision/time, collection and workstream states, active
blockers, and dated history. Source metadata is off by default; when enabled it
is limited to available reporter, workspace, and session identifiers. Transcript
text, commands, credentials, and tokens are never exported. Writing uses a
same-directory temporary file and atomic replacement, preserving the original on
failure. Export does not pause or clear collection, change read state, stage,
commit, push, import Markdown edits, or schedule later exports.

Controls are project-scoped across MCP `repo` actions, Tauri IPC, and HTTP.
Delete always names event IDs; clear names one project and supplies its current
revision. Corrections are typed operations rather than arbitrary database writes.
Opening a view captures `snapshotCursor`; acknowledging that exact value leaves
later reports, and reports in every other project, unread.

If the database is corrupt, TUICommander preserves the database and WAL data and
retains any existing SQLite SHM sidecar under unique `.corrupt-<uuid>` names before
creating a validated empty replacement. The triggering operation reports the
recovery and must be retried; it never reports success against the replacement as
though the previous history were still present. The preserved artifacts remain
available for manual recovery.
