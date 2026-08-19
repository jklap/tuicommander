# fx — UI layout and detection

Agent: `fx` ([Vercel Labs fx](https://fx.sh/try)).
Version audited: **0.0.3** · Date: **2026-08-19**.

All UI values below were captured from the official macOS arm64 release in a
real PTY with an isolated `HOME` and a loopback model gateway. They were not
inferred from source.

## Identity and rendering

| Property | Value |
|----------|-------|
| Binary | `fx` (native Mach-O arm64) |
| Interactive launch | `fx` |
| Headless launch | `fx ask --no-save "{prompt}"` |
| Resume | `fx --resume <session-id>` |
| Config directory | `~/.fx/` under the launched process's `HOME` |
| Sessions | `~/.fx/sessions/<id>/session.json` |
| MCP config | `~/.fx/mcp.json` |

The audited release was the official `fx-macos-aarch64.tar.gz` asset from tag
`v0.0.3`: 3,970,300 bytes with SHA-256
`87c4939621b0c028e4506f18416b8722ded0dbace6358380011400c8ed127a8b`.

fx rejects a positional prompt. TUICommander therefore starts the persistent
TUI without a prompt argument and reuses the deferred prefill path to submit the
initial prompt after the screen is ready. The one-shot headless path was also
verified: `fx ask --no-save` exited successfully, printed only the model reply,
and did not create a session directory.

## Bottom zone

The ready screen includes these stable markers at 100 columns; the same state
contract was replayed at 62 columns:

```text
𝒇x v0.0.3 · Run /help for commands
┃
auto · sonnet 4.5
```

The empty `┃` composer row is the readiness marker. The model label and header
identify the screen but do not determine activity. fx also emits the OSC title
`fx · anthropic/claude-sonnet-4.5`.

## Working state

During a model turn, fx keeps the surrounding UI visible and renders a status
line such as:

```text
• Thinking (2s)
```

During a tool call it additionally renders:

```text
● 1 tool call · 1 command
└ Running sleep 3; printf FX_TOOL_DONE
```

The composer can remain visible while work is active, so its mere presence is
not safe evidence of readiness. `detect_fx_screen_activity` consequently scans
the bottom zone for `• Thinking` first; only an exactly empty `┃` composer with
no thinking marker means `Ready`. Any other screen is `Unknown`. This ordering
prevents a visible composer from suspending a live turn.

## Input

| Property | Value |
|----------|-------|
| Submit | `Enter` (verified live: text plus `\r`) |
| Initial orchestrated prompt | Deferred bracketed paste plus `Enter` |
| Persistent follow-up | Bare interactive TUI |

The lifecycle capture submitted two turns to one persistent process. The second
turn invoked a real tool path through the loopback gateway, remained busy while
the command ran, then restored the empty composer.

## Awaiting input

With `FX_PERMISSION_MODE=ask`, a command requiring approval renders a framed
numbered choice with the captured footer:

```text
Permission needed · Choose one
Would you like to run the following command?
❯ 1. Yes
  2. Yes, and don't ask again for this exact command
  3. No
1–3 Choose now    ↑↓ Options    Tab Amend    Enter Confirm    Esc Cancel
```

The screen adapter deliberately returns `Unknown` for this overlay. The shared
choice-prompt parser recognizes it only when both the exact fx footer and the
permission header are present, then publishes a confident awaiting-input state.
Its choice payload records that a numeric selection requires confirmation, so
awaiting state remains set until `Enter`; `Escape` and `Tab` retain the shared
dismiss/amend behavior.

## Sessions

The audited manifest used schema version 3 and storage format `event_log_v1`.
TUICommander discovers only recent manifests whose directory ID, manifest ID,
and normalized `workspace_root` agree. It rejects unsafe IDs, symlinked session
directories, oversized or malformed manifests, workspace mismatches, and
already-claimed sessions.

Launching the official binary with the captured session ID restored both prior
turns and returned to the ready composer without issuing a new chat request.
The launch process's `HOME` is part of the restore contract; run-configuration
environment must therefore be forwarded to both discovery and verification.
The configured command is retained on restore, but launch-only arguments are
not appended: the official binary exits with usage code 1 for
`fx --resume <id> --model <model>`, and the saved session already owns its model.

## MCP

fx owns a native local-server entry in `~/.fx/mcp.json`:

```json
{
  "mcp": {
    "tuicommander": {
      "type": "local",
      "command": ["/path/to/tuic-bridge"],
      "enabled": true,
      "required": false
    }
  }
}
```

The official v0.0.3 binary launched the real `tuic-bridge` from this entry. Its
native discovery request identified the MCP client as `fx` version `0.0.3`;
TUICommander maps that exact client name to the canonical `fx` agent type.
fx first probes modern `2026-07-28` `server/discover`, then follows its legacy
fallback after the deliberate method-not-found response. TUICommander's legacy
initialize boundary therefore negotiates `2025-11-25`; the bridge projects the
request revision into `MCP-Protocol-Version` while retaining compatibility with
older requests that omit it.

The official binary also completed the lazy-tool path end to end. It searched
the TUICommander catalog, selected `mcp_tuicommander_session`, and called that
dynamic model-facing name with `{"action":"list"}`. fx resolved the selected
alias internally and emitted a standard MCP `tools/call` whose raw tool name was
`session`; the real bridge and isolated TUICommander instance returned
`isError: false`. This distinction is intentional: the prefixed name belongs to
fx's model-tool namespace, while the MCP server continues to own the unprefixed
tool identifier.

## Not yet observed

Authentication screens and provider-specific interactive errors were not
triggered during this audit. They remain conservatively `Unknown` until a
capture proves a safe transition marker.
