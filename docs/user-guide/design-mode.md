# Design Mode

Design Mode collects elements and comments in a browser sidebar before sending feedback to an agent. Ordinary page interaction remains available until you explicitly start picking.

## Start and install the pilot

1. Open an agent terminal in your repository. Choose **Start Design Mode** from its context menu or the Command Palette. **Choose Design Mode browser** offers Edge or Chrome; the palette also offers a command for each browser. Safari is unsupported.
2. TUIC opens a dedicated profile for this repository and browser. On first use, follow the setup page: open `chrome://extensions` or `edge://extensions`, enable Developer mode, choose **Load unpacked**, and select the extension folder shown there. This is the pilot distribution method.
3. Click the TUICommander browser extension icon to open the sidebar. It lists configured and running dev servers. Choose one, enter a URL in the address bar, or use **Start dev server** if you have configured a **Run Script** in Repository Settings. Refresh rescans servers and agents.
4. Choose **Pick element**, then click an element. A card appears and normal browsing resumes. **Multi-pick** continues collection until **Stop**, Escape or the two-minute timeout. Each card has an independent comment and Delete button. Drafts survive worker suspension; closing their browser tab deletes them. A tab holds at most 32 picks.
5. Add comments, choose the receiving agent, and click **Send to agent**. The terminal that launched Design Mode is selected initially. Other live local agents in the same Git repository, including linked worktrees, are available. If your selection exits, the sidebar asks you to choose another; it never silently switches recipients.
6. Feedback is delivered immediately when the agent is idle, or queued for its next idle window. The sidebar reports which happened. Failed sends retain your cards; retries to the same recipient reuse the exact batch and delivery key. Choosing a different recipient rebuilds the batch from retained commented picks with a new delivery key. Only commented picks are sent.

The D badge and Stop action remain on the launching terminal. Browsers open on the backend host, including when TUIC is controlled from a browser/PWA client. No debugging port or automation banner is requested by the v2 launcher.

## Connection and evidence

The native host pins the extension identity, repository and exact TUIC instance. It relays a bounded set of operations over that instance’s local socket or named pipe. Reopen Design Mode after restarting TUIC to refresh its binding. Connection failures show the endpoint and reason. An older running backend can list agents but returns an explicit restart error for the new feedback/discovery route.

Cards retain selectors, DOM paths, snippets, selected styles, CSS tokens and available framework source hints. Rust bounds and sanitises page evidence, redacts recognised secrets, strips terminal controls and fences it as untrusted content. Captured PNG crops are saved in `design-grabs` and sent as readable `[image: path]` references. Screenshot capture is optional: permission errors and oversized crops retain DOM evidence. Cross-origin frame picks currently retain DOM evidence without frame screenshots.

Discovery orders the configured **Dev Server URL** first, then live listeners attributed by process directory or an absolute command path to this repository’s worktrees. Fresh PTY URLs are accepted only when their port has a live owned listener. Multiple candidates are listed instead of automatically choosing one. Windows native host registration currently targets Edge and awaits Chrome/multi-profile validation. Windows discovery offers configured URLs and the address bar; listener attribution is not claimed there. Repo-local `.tuic.json` cannot silently supply executable scripts.

The pilot has browser evidence from Chrome for Testing. Native Edge Load unpacked persistence, toolbar-granted screenshot permissions and sidebar restoration still need an Edge check. Automatic post-agent reload and everyday-profile installation are later work; this pilot uses the dev server’s own HMR or the browser’s Reload button.

### Start from a link or coordinator

Right-click a terminal HTTP(S) link and choose **Start in Design Mode**, then **Edge** or **Chrome**. This always starts or rebinds to that terminal agent and opens the exact URL, even if another agent is already connected. The D badge follows the bound session.

For layout work, coordinators use MCP `design_mode` with `action: "start"`, `session_id` naming the live terminal agent, and `url` naming the HTTP(S) page. Optional `browser` selects `chrome` or `edge`. Use `action: "status"` for bindings and `action: "stop"` with the returned `repoPath` as `repo_path`. Browser launch happens on the host; an ACP coordinator does not become the terminal agent.

If the sidebar icon is hidden, open the Extensions (puzzle) menu and pin **TUICommander Design Mode**. You can also press **Option+Shift+D** on macOS or **Alt+Shift+D** on Windows/Linux. Browser choice currently lists Edge and Chrome; launch reports if the chosen browser is not installed.
