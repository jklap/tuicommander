# Plugin review 1756: isolated-runtime prerequisite

## Result

The required loaded-user-plugin E2Es remain blocked by the permitted runtime, before account login is relevant. Do not close story `1756-4d42` from existing module tests or its checked candidate 6/7/8 boxes.

## Branch reconciliation

Starting branch: `chore/1756-live-checks`, HEAD `de1729a4c8`.
Compared against main `5f1b86840bc325f691b66b6595986845acb1f394`.

Main contains the production cancellation restoration, async lifetime ownership, schema-backed project defaults and content identity. Its cancellation socket test also has later EOF/reset and complete-header corrections. After merging main at `874680040`, the branch has no production behavior delta from that main. Only retained append-only documentation differs. The isolated submodule checkout now matches main's recorded pin `56659df3cf53edb1e1fc04a70b19f88d1a6c9b06`; Boss's different main-checkout submodule was not changed.

The older branch restoration is therefore not independently needed for production code. No implementation was discarded or rewritten, and no plugin commit, dependency change, catalogue regeneration or notice regeneration was needed.

## Decisive runtime evidence

- `src/plugins/pluginLoader.ts:363`: `loadUserPlugins()` returns immediately when `isTauri()` is false, before discovery or startup. It logs `User plugin loading skipped in browser mode`.
- `src/plugins/pluginLoader.ts:196-209`: module URLs are `plugin://<id>/~v<n>/<main>` on macOS/Linux and `http://plugin.localhost/...` on Windows. Neither is a browser HTTP module-asset transport to a private headless daemon.
- `src-tauri/src/plugins.rs:506-515`: the custom module protocol is compiled only with `feature = "desktop"` and registered on the Tauri builder.
- The existing `docs/user-guide/plugins.md` post-login probe explicitly requires an isolated desktop instance, not a Vite/headless runtime. It also explicitly does not prove the actual Tasks registry's lifecycle.

A private port, native headless build, different Vite port or agent-browser session does not change these branches. Injecting `isTauri`, replacing the loader or manually activating imported plugin objects would bypass the production discovery/import path and is not claimed as the required E2E. No desktop instance, headless daemon, Vite server or browser was started for a path which inspection proves cannot load these user plugins.

## Required decision

The owner must approve a supported browser user-plugin loading change (module-asset HTTP transport, production loader URLs, discovery/hot reload and lifetime integration), or provide another permitted isolated runtime that really runs the production user-plugin loader. This changes the product's current desktop-only loading contract, rather than merely choosing test ports. An agent must not start a second desktop app or restart Boss's app to avoid this decision.

Once that prerequisite exists, the remaining non-account checks are agent-actionable. They are not classified as hardware or human-only checks.

## Exact remaining checks

| Candidate | Required real-runtime check | Additional operator prerequisite |
|---|---|---|
| 3 | Reload GitHub Work, GitLab and Jira twice; count one provider registration per configured account; trigger rejected async startup and inspect its error. | Configure authorized provider accounts in the isolated runtime. Do not copy credential stores. |
| 4 | Install the catalogue payload, change installed `main.js` without a version change, rediscover, refresh Browse and observe Update; restore payload and check badge clears. | Supported runtime and published catalogue payload/digest must match; no vendor login. |
| 6 | Switch repositories while a real scan is in flight; verify old completion cannot replace current panels/ticker and MDKB refreshes. | Disposable repositories and installed required CLI tools; no vendor login. |
| 7 | Change the real composer between retry/wakeup read and submission; verify no obsolete input is sent. | Disposable agent session; preserve the actual backend conditional-send path. |
| 8 | Open same-name CSV, valid DOCX and XLSX files in two disposable repositories; verify independent panels and Edit targets. | Supported runtime; no vendor login. |
| 9 | Save a Jira project ID or GitLab project through plugin Settings, reload and use it; verify an account-specific project takes precedence. | Boss supplies/login-authorizes an accessible test project and account through supported UI, not credentials in chat. |
| 10 | Resolve an accessible Jira issue URL; cancel a live pending detail request and verify no late result and request retirement. | Boss authenticates an isolated Jira account and names an accessible issue. Never fabricate vendor success or add synthetic credentials. |
| 11 | Install an actual plugin with an incompatible minimum version and observe clear refusal in the real loader. | Supported runtime; no vendor login. |
| 12 | Invoke account-bound review through core and verify the loaded GitHub plugin path; keep writes out of scope. | Boss completes GitHub device-flow login with `repo read:project` consent in the isolated runtime and names an accessible PR. A write action requires separate explicit authorization. |

## Validation and limits

MDKB search preceded source investigation. The latest `Closure audit 2026-10-19` was read through `stories-cli.js show 1756-4d42` from the main store. Instruction-link validation passed with two valid pairs in this worktree.

No new behavioral fix, RED test or GREEN run is claimed. No Rust build/test, Vitest, tsc, live E2E, credential access, vendor request, desktop interaction, process launch, landing or push was performed. The source change only records the deferred runtime prerequisite beside the browser guard. `to-test.md` corrects historical claims that project settings, cancellation and content identity are still unimplemented. No acceptance criterion is newly proved, so none is checked.
