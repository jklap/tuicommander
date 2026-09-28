# AI Chat

AI Chat is a conversation with **ego**, which TUICommander launches and speaks to
over ACP. TUICommander is the environment — terminals, repositories, the MCP
server — and ego is the intelligence. TUICommander keeps no provider, no API key,
no tool loop and no sandbox of its own.

## Before it can talk

1. Turn on **Experimental Features** in `Settings > General`. The panel is behind
   it and there is no separate AI Chat switch.
2. Set **ego executable** in `Settings > General` (**Select…** opens a file picker). While it is empty, ACP is not
   configured: the panel says so and launches nothing.
3. Optionally set **ego profile** in `Settings > General` to select a profile from ego's user configuration.
   An empty value leaves ego's usual profile selection in effect. TUICommander passes only the name at launch;
   it does not send profile rules in `session/new`.

## Opening it

`Cmd+Alt+A` (macOS) / `Ctrl+Alt+A` toggles the panel; so do the status-bar
button and the command palette. The detach control moves it into its own window,
and the main window shows the *Bring back* placeholder. Drag the left edge to
resize — the width applies for the session and is not persisted.
The conversation view loads when you first open it; terminal input is available
while it loads.

On the mobile PWA, **Chat** is the first tab. Choose a configured repository in
the header to open ego there. The conversation picker shows the titles of saved
sessions; choose one to load its history, or tap **New**. Messages, collapsed tool
activity and pending permission or form cards use the same ACP stream as desktop.
If the connection drops, the client resumes from its last received event. A
missing part of the journal is shown as a gap with a **Recover** action.

When ego requests permission or a form response, a subscribed phone receives
one push linking to that conversation in mobile Chat if the desktop is unfocused or idle. Answering
on desktop before the notice is delivered suppresses the alert. Repeated
requests in one conversation share a 30-second push limit. Activity updates
do not alert the phone.

## What it is bound to

**A repository and a session, never a terminal.** A turn ego runs outlives any
tab, may touch files no tab is showing, and is the same conversation for every
window looking at that repository. The header names the repository.

Switching repository opens a new conversation and leaves the previous one
running. Coming back to a repository picks its conversation up where it was —
nothing is relaunched, and the turn that was running kept running. **New** in the
control bar starts a second conversation on the same repository; the picker
beside it lists previous conversations by title, newest first. Selecting one
loads its history. The last selected conversation is restored after restarting
TUICommander.

The panel has chat tabs for parallel conversations on the same repository.
Click **+** or press `Cmd+T` (`Ctrl+T` on Windows/Linux) while the panel has
focus to start another ACP session; in browser mode use `Cmd/Ctrl+Alt+T` so the
browser keeps its own new-tab shortcut. Click a tab to switch, or close it to
remove it from the panel. Closing a chat tab does not delete ego's conversation:
the conversation picker can reopen it. Each tab keeps its own transcript and
unsent composer draft. Open tabs and the selected tab return when the panel is
hidden and shown or detached into its own window; the detached view replays
their histories from ego.

An untitled conversation appears with its first prompt or latest activity time;
its session ID is available in the option tooltip.

When ego updates the session title, the panel header and conversation picker
show the new title. During a turn, the footer shows context-window use as a
percentage and shows the cumulative cost when ego reports one.

ego reaches terminals and repositories the way any external agent does: by
calling TUICommander's own MCP server. The entry for it is built by
TUICommander, not by whoever opened the session, so a conversation can never be
pointed at some other endpoint. It also names the socket **this** copy of
TUICommander bound, so a second copy started with `TUIC_APP_INSTANCE=<id>` drives
its own terminals and repositories rather than the default install's.

## During a turn

Select text in user messages, answers, code blocks, and tool output and use
`Cmd/Ctrl+C` to copy it. **Copy** on a message or code block copies its raw
text through the same clipboard adapter as the terminal. Web links open in the
system browser; file links and plain source paths are resolved by the backend
and opened in TUICommander's file viewer or editor, as they are from a terminal.
With focus in the transcript, `Cmd/Ctrl+A` selects that transcript,
`Cmd/Ctrl+F` opens its search, and `Cmd/Ctrl+K` clears the visible history of
the current tab. Clearing the view does not delete ego's saved conversation.

Paste a PNG, JPEG, GIF or WebP image into the composer to preview it before
sending. Remove a preview with its close button if you change your mind. An
image can be sent without text. The composer refuses images when the connected
agent did not advertise image prompts, or when the pasted images exceed 10 MiB
in total. Text paste works as usual.

- **Streamed answer.** Text arrives a chunk at a time. Reasoning is folded into
  a *Thinking* disclosure, kept apart from the answer.
- **Turn markers.** AI Chat hides ego's TUICommander connection acknowledgement,
  shows `intent:` as a labelled status, and turns a `suggest: [ A | B | C ]`
  line into reply buttons. Choosing one sends that text as the next prompt.
  Mentions in ordinary prose and code examples remain in the answer.
- **Failed or empty turn.** An ACP prompt error appears in the conversation with
  the agent's diagnostic. A turn that finishes without an answer says so; the
  composer becomes available for another prompt.
- **Tool activity** appears as one collapsed line per turn, with a count,
  observed duration, status and the first two call titles. Expand it to see
  each call's title, kind and status; expand a call to see its output. The
  duration measures only time observed while this panel is open. A replayed
  conversation has no recorded timing data.
- **The plan** ego publishes is shown as a list and replaced whole each time it
  changes.
- **Stop** cancels the turn. **Pause** and **Resume** hold it where ego supports
  them; **Compact** shortens the conversation. Each button is drawn only when ego
  advertised that extension, so a build without it shows no button rather than a
  button that fails. Resume remains available when a paused turn ends at its
  boundary; select it to continue the conversation.
- **Queue** sends another message while a turn runs. TUICommander keeps it in
  order and sends it only after the running turn ends. If ego pauses at a turn
  boundary, the queue waits for Resume. The queued list appears
  in every connected view; either view can remove an item before ego receives
  it. Stop affects the running turn for every view. The conversation shows a
  queued message as sent only when it actually reaches ego.
- **Session settings** are published by the current conversation. The control bar
  shows the current model and mode; its settings button opens a dialog with
  labeled choices and descriptions for every select option ego offers, including
  reasoning effort and sandbox when available. Changes apply to this conversation
  and the displayed values follow ego's reply. A rejected change shows its error
  in the dialog. TUICommander keeps no separate model list.
  The model every *new* run starts from is ego's own default, editable in
  Settings → **AI Chat** (see [Settings](settings.md#ai-chat)).

## Questions ego asks back

- **Permission.** The buttons are the options ego published, answered with one of
  its own option ids. TUICommander never invents an Allow/Deny pair of its own.
- **A form.** An elicitation is drawn as a form built from the schema ego sent.
  A single choice field with one to three values appears as direct answer
  buttons plus **Cancel**; larger or more complex forms keep their fields.
  Only `form` mode is ever drawn; any other mode is declined before it reaches
  the panel.

A question raised while this window was not listening is still shown: the panel
fetches what is open when it attaches, rather than waiting for an announcement
that already happened.

When AI Chat is hidden, its status-bar button shows the number of unanswered
permissions and forms. The desktop sends one notification for each new question;
answering it removes the badge and closes its notification.

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
| Providers, models, slots, API keys, Ollama detection | Settings → **AI Chat** shows what ego is configured with and sets its default model (#786-4a6d). Slots, API keys and Ollama detection did not come back: no key is stored by TUICommander and no provider call is made from it |
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
