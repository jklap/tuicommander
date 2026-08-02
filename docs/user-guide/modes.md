# TUICommander modes

TUICommander has one backend and several ways to connect to it. The available features depend on the client mode.

| Mode | How it is used | Best for | Main limitations |
|---|---|---|---|
| **Desktop app** | Launch TUICommander normally | Full local development workflow | None of the client-side limitations below |
| **Browser mode** | Open the local HTTP server in a browser | Remote control from a laptop or another desktop | Native dialogs, Command Palette, global hotkeys, updater, dictation, detached windows, and some file/clipboard integrations are desktop-only |
| **Mobile PWA** | Open the mobile endpoint from a phone/tablet | Monitoring agents and answering prompts | Deliberately reduced UI; not a replacement for the desktop workspace |
| **Remote daemon** | Run `tuic-remote` and connect through the remote-access flow | Hosting the backend on another machine | Requires separate remote configuration and network/security setup |

## Desktop app

The desktop app runs the Tauri shell and exposes the complete feature set: native file dialogs, IDE launchers, dictation, global hotkeys, detached panels, the Command Palette, and the native updater.

For local development, use `make dev` or `pnpm tauri dev`. Frontend files use Vite HMR; Rust changes require restarting the development process.

## Browser mode

The browser client uses the same backend over HTTP, WebSocket, and SSE. It is not a mock UI: sessions, terminal output, Git operations, settings, and most panels use the same backend as the desktop app.

Some desktop integrations cannot be reproduced in a browser. In particular, browser mode does not provide the native Command Palette, native file pickers, global hotkeys, detached OS windows, dictation, IDE launchers, auto-updater, or user-plugin installation from local files.

See [Remote Access](remote-access.md) for setup and [Troubleshooting](troubleshooting.md) if the browser cannot connect.

## Mobile PWA

The mobile interface is optimized for observation and quick actions: inspect sessions, follow output, answer questions, and monitor agent activity. Use the desktop or full browser workspace for editing, worktree management, and complex Git operations.

## Remote daemon

`tuic-remote` provides a standalone backend process for remote access. It is useful when the machine running the repositories is different from the machine displaying the UI.

See [Remote Access](remote-access.md) for connection methods, TLS/relay options, and configuration details.
