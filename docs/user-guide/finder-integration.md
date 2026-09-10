# Finder Integration (macOS)

Right-click a folder — or a file inside one — in Finder and choose **New TUICommander Tab Here**
to open a terminal there, the way iTerm2's "New iTerm2 Tab Here" service works.

## Installing it

**From the app:** Settings > General > Finder Integration > Add Finder Integration

**First launch:** TUICommander offers to add it the first time you run the app on macOS. Decline
or accept — either way it only asks once; you can add or remove it later from Settings.

Installing copies a small Automator service bundle into `~/Library/Services/`. No admin password
is needed. Removing it (Settings > General > Finder Integration > Remove) deletes that bundle.

## Which repo does it open in?

TUICommander groups terminals by repository and branch in the sidebar, so opening a pane from
Finder has to decide which group the new pane belongs to. It works through this in order:

1. **The folder is inside a repo you've already added to TUICommander** (its root, or one of its
   linked worktrees) — the pane opens there, filed under that repo's branch.
2. **Otherwise, whichever repo you're currently working in** — the pane opens there instead.
3. **Otherwise, TUICommander asks you** — a dialog offers every repo you've registered, plus two
   fallbacks: add the clicked folder as a new repo, or just open a plain terminal with no repo
   attached.

In every case, the terminal's working directory is the **exact folder you clicked** — even if
that folder is nested deep inside a repo — not the repo's root.

If TUICommander isn't running, invoking the service launches it first, then opens the pane once
it's ready.

## Selecting multiple items

Selecting several folders (or files) and invoking the service at once opens one terminal pane per
item, up to 5 at a time. A larger selection only opens the first 5.

## Under the hood

Finder → the installed service bundle → the bundled `tuic` CLI's `tuic open-here <paths...>` (see
the [CLI guide](cli.md#opening-a-plain-terminal-at-a-path)) → a `tuic://open-terminal` link that
TUICommander's already-running deep-link handler picks up. No extra permissions or native macOS
integration code are involved beyond the one-time service install.
