# Project Progress

Progress answers one question: **what happened in this terminal while I was not
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

TUICommander adds the project, the time, the agent's name and the source PTY
when it can identify one. The agent does not supply them.

The receipt is `{"id": <n>}` and nothing more. Each call appends one entry: the
journal does not deduplicate agent reports, because an agent that reported the
same step twice did the work twice, and only you can decide what that means.
Intents are the exception. TUICommander reads them off the screen, and a repaint
of the same `intent:` line is not a new intent, so a repeat of the PTY's
newest intent — same text, same agent — returns that entry instead of adding one.

`step` is a free-text label, not a registered object. Nothing has to be created
in advance, and two agents writing the same label are simply two entries with
the same label.

## Reading it

Open the Progress dialog from the command palette (`progress`) or from the
toolbar bell, which shows how many entries arrived since you last opened it.

The dialog opens on the active PTY, newest first. The selector switches to
another PTY in the project or **All repo**, which combines their histories.
Closed PTYs with saved entries remain selectable. A PTY with no entries has an
empty view; it does not inherit another PTY's work. Entries recorded before
terminal identity was stored, and direct local reports with no PTY binding,
appear in **All repo** as **Terminal unknown**. They are never assigned by guess.

Each PTY and **All repo** has its own last-visit divider. The divider stays
fixed while you read a view; switching views loads that view's mark. Closing
the dialog marks every view you visited during that opening.

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
directory, with the project and nullable PTY ID as columns. **Nothing is written inside your
repositories** — no `.tuic` directory, nothing for Git to ignore, nothing for a
file watcher to react to.

A worktree's entries are filed under its parent project, so switching between a
repository and its worktrees shows one history rather than several.

An agent running in a directory that belongs to no registered repository writes
nothing. There is no project to file the entry under, and the repository you
happen to be looking at is not the answer.
