# AI Chat

AI Chat is a conversation with **ego**, which TUICommander launches and speaks to
over ACP. TUICommander is the environment — terminals, repositories, the MCP
server — and ego is the intelligence. TUICommander keeps no provider, no API key,
no tool loop and no sandbox of its own.

## Before it can talk

1. Turn on **Experimental features** in `Settings > General`. The panel is behind
   it and there is no separate AI Chat switch.
2. Set **ego executable** in the same tab. While it is empty, ACP is not
   configured: the panel says so and launches nothing.

## Opening it

`Cmd+Alt+A` (macOS) / `Ctrl+Alt+A` toggles the panel; so do the status-bar
button and the command palette. The detach control moves it into its own window,
and the main window shows the *Bring back* placeholder. Drag the left edge to
resize — the width applies for the session and is not persisted.

## What it is bound to

**A repository and a session, never a terminal.** A turn ego runs outlives any
tab, may touch files no tab is showing, and is the same conversation for every
window looking at that repository. The header names the repository.

Switching repository opens a new conversation and leaves the previous one
running. Coming back to a repository picks its conversation up where it was —
nothing is relaunched, and the turn that was running kept running. **New** in the
control bar starts a second conversation on the same repository; the picker
beside it moves between them.

ego reaches terminals and repositories the way any external agent does: by
calling TUICommander's own MCP server. The entry for it is built by
TUICommander, not by whoever opened the session, so a conversation can never be
pointed at some other endpoint. It also names the socket **this** copy of
TUICommander bound, so a second copy started with `TUIC_APP_INSTANCE=<id>` drives
its own terminals and repositories rather than the default install's.

## During a turn

- **Streamed answer.** Text arrives a chunk at a time. Reasoning is folded into
  a *Thinking* disclosure, kept apart from the answer.
- **Tool calls** appear as one card per call, updated in place — not one card per
  status change.
- **The plan** ego publishes is shown as a list and replaced whole each time it
  changes.
- **Stop** cancels the turn. **Pause** and **Resume** hold it where ego supports
  them; **Compact** shortens the conversation. Each button is drawn only when ego
  advertised that extension, so a build without it shows no button rather than a
  button that fails.
- **Model, reasoning effort and mode** come from the options the session
  publishes. There is no list of models in TUICommander: the session is asked,
  and the answer is what the control bar draws. This changes one conversation.
  The model every *new* run starts from is ego's own default, editable in
  Settings → **AI Providers** (see [Settings](settings.md#ai-providers-tab)).

## Questions ego asks back

- **Permission.** The buttons are the options ego published, answered with one of
  its own option ids. TUICommander never invents an Allow/Deny pair of its own.
- **A form.** An elicitation is drawn as a form built from the schema ego sent.
  Only `form` mode is ever drawn; any other mode is declined before it reaches
  the panel.

A question raised while this window was not listening is still shown: the panel
fetches what is open when it attaches, rather than waiting for an announcement
that already happened.

## When it misses something

- **"Missed part of this conversation"** means the journal no longer holds the
  point the panel had read up to — a gap, not a dropped connection. **Recover**
  starts a fresh ego process and replays the conversation into it.
- **"Not receiving updates"** means the connection is up but nothing is reading
  its journal any more. The conversation on screen is intact; it has stopped
  moving.

## From a terminal

Right-click a terminal: **Explain with AI** and **Fix this error** put a question
about the selection — or about the last 50 lines when nothing is selected — into
the panel's composer and open it. They write the question; you send it.

## The session knowledge store

Command outcomes are still recorded per terminal — exit codes from OSC 133 where
the shell supports it, an `Inferred` outcome from the PTY silence timer where it
does not — and persist to `<config_dir>/ai-sessions/<session_id>.json`. Nothing
reads them today. They are kept because the recording has nothing to do with a
model and re-deriving the history later is impossible.

## Not here

| Feature | Where it went |
|---------|---------------|
| Providers, models, slots, API keys, Ollama detection | Settings → **AI Providers** shows what ego is configured with and sets its default model (#786-4a6d). Slots, API keys and Ollama detection did not come back: no key is stored by TUICommander and no provider call is made from it |
| Agent mode (ReAct loop), the safety checker, the file sandbox | nothing. ego runs its own tool loop and reaches terminals through TUICommander's MCP server |
| Terminal watchers (autonomous rules) | not scheduled |
| PR AI review, changelog generation, improvement scan | 795-320b |
| Smart Prompts `api` execution mode | One unattended ego turn (#787-ee50), with no interactive tools and the final text routed to the prompt's configured output |

## See also

- [`docs/backend/acp.md`](../backend/acp.md) — the ACP client, its journal and
  the authority a session is given.
- [`docs/backend/mcp-http.md`](../backend/mcp-http.md) — how an external agent
  drives TUICommander terminals, which is the path ego uses.
- [`docs/backend/pty.md`](../backend/pty.md) — PTY lifecycle, OSC 133, TUI
  detection, silence-based idle.
