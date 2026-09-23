# Design Mode

Design Mode lets you point at an element in a local web app and send its context to the agent terminal you chose. TUICommander opens a separate Chrome window, highlights elements on hover, and captures the element you click. It inserts a compact description and an optional screenshot reference into the agent's input without pressing Enter. Add your instruction after the grab, then submit it yourself.

## Start and use it

1. Start your development server. In the repository's **Repo Settings → Scripts** tab, set **Dev Server URL** to its address. This setting is local to your TUICommander installation; it is not read from `.tuic.json`.
2. Open an agent terminal for that repository. Choose **Start Design Mode** from its tab context menu or the Command Palette. The action is available for agent terminals, not plain shells.
3. In the Chrome window, hover to see Chrome's element highlight, then click the element. The click selects the element for Design Mode instead of activating the page's own click handler.
4. Return to the bound agent terminal. The draft includes a selector, DOM path, nearby text, an HTML snippet, selected computed styles, its rectangle, source location when available, and an `[image: …]` line when the screenshot was saved. You can click more elements; each grab is appended to the draft. Add your request and press Enter when ready.

If you leave the URL empty, Chrome opens `about:blank`; navigate to the page yourself. Starting from another agent terminal in the same repository rebinds the existing Design Mode window to that terminal. The status indicator shows which tab is bound. Closing the browser or the bound terminal stops inspection. TUICommander also closes the Chrome windows it started when it quits.

Source locations depend on development build metadata. React 19.1+ and Svelte 5 can provide file and line; Vue provides the component file. Production builds or unrecognised scripts may omit the source location. Cross-origin iframe elements may not be inspectable from the main page.

## Attach browser automation to the same Chrome

Chrome writes its CDP port to `DevToolsActivePort` in its dedicated Design Mode profile directory under TUICommander's configuration directory (`design-mode/<repo-key>/DevToolsActivePort`). The repo key is the first 16 lowercase hex digits of SHA-256 of the canonical main repository path. Read the first line of that file, then pass the port to an automation client:

```sh
agent-browser --session design-mode --cdp <port> snapshot
```

For example, replace `<port>` with `9222` if the file's first line is `9222`. This attaches to the existing Chrome instead of launching a second browser. The port is bound to loopback on the host running TUICommander. Browser/PWA clients can request Design Mode, but its Chrome window opens on that host's screen.

The selected page is untrusted input. TUICommander limits the grab text, drops event-handler attributes and unsafe URLs, redacts recognised secrets, and removes terminal control characters before inserting the draft. Screenshot capture can fail or exceed its size limit; in that case the text is still inserted without an image line.
