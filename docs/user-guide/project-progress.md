# Project Progress

Progress answers one question: **what happened in this project while I was not
watching?**

It is a journal. Entries are appended, never edited. You read it, and you delete
what you do not want to keep. There is nothing else to operate.

## What lands in the journal

Three kinds of entry.

| Kind | Written by | Meaning |
|---|---|---|
| `done` | the agent | The work an `intent:` announced is finished. |
| `blocked` | the agent | The agent cannot continue without you. |
| `intent` | TUICommander | The agent said what it was starting. Recorded from its `intent:` marker. |

`intent` is the reliability floor. An agent's obligation to report sits in the
`initialize` blob it read hours ago, but its `intent:` marker fires at the start
of every task — so even an agent that never calls the tool leaves a trail of
what it set out to do. An agent cannot write an `intent` entry itself; the
reporting tool refuses that kind.

## How an agent reports

The MCP `progress` tool takes three fields.

| Field | Required | Limit | Meaning |
|---|---|---|---|
| `type` | yes | `done` or `blocked` | The kind of entry |
| `text` | yes | 500 characters | One standalone sentence |
| `step` | no | 80 characters | What the entry belongs to |

```json
{"type": "done", "text": "OpenRouter applications can now be identified.", "step": "Shadow AI Detection"}
```

TUICommander adds the project, the time and the agent's name. The agent does not
supply them.

The receipt is `{"id": <n>}` and nothing more. Each call appends one entry: the
journal has no deduplication, because an agent that reported the same step twice
did the work twice, and only you can decide what that means.

`step` is a free-text label, not a registered object. Nothing has to be created
in advance, and two agents writing the same label are simply two entries with
the same label.

## Reading it

Open the Progress dialog from the command palette (`progress`) or from the
toolbar bell, which shows how many entries arrived since you last opened it.

The dialog shows one project — the active one — newest first, with a divider
marking where your last visit ended. The divider is frozen while the dialog is
open: it moves when you close it, never under the line you are reading.

Blocked entries are red. `intent` entries are muted, because they are what an
agent set out to do rather than a result. A checkbox narrows the list to blocked
entries only. A row can be deleted, and deletion is permanent.

There is no export, no pause, no clear and no correction. An entry is appended
once, and it is either there or deleted.

## Turning it off

**Settings → Agents → Collect project progress** turns collection off for every
agent. The `progress` tool then disappears from every agent's tool list, and no
`intent:` marker is recorded.

Each agent also has its own **Collect progress** toggle in its expanded settings
card. Off there keeps that one agent out of the journal while the rest keep
reporting. The global switch wins: off globally means off everywhere.

## Where it is stored

One SQLite database, `progress.sqlite3`, in TUICommander's configuration
directory, with the project as a column. **Nothing is written inside your
repositories** — no `.tuic` directory, nothing for Git to ignore, nothing for a
file watcher to react to.

A worktree's entries are filed under its parent project, so switching between a
repository and its worktrees shows one history rather than several.

An agent running in a directory that belongs to no registered repository writes
nothing. There is no project to file the entry under, and the repository you
happen to be looking at is not the answer.
