# Development Setup

## Prerequisites

The macOS desktop app supports macOS 12 Monterey or later and requires system WebKit updated to Safari 17.4 or later for Mermaid diagrams. The bundle declares macOS 12.0 as its minimum; this does not guarantee that Safari updates are installed. `src-tauri/.cargo/config.toml` forces `MACOSX_DEPLOYMENT_TARGET=12.0` for Rust and native dependencies, including WebRTC and whisper.cpp, overriding inherited shell defaults. The Cargo config also forces `CMAKE_OSX_DEPLOYMENT_TARGET=12.0` so whisper.cpp updates existing CMake caches instead of retaining an older target. Make does not set a separate default. `scripts/test-macos-deployment-target.sh` checks both values against the bundle minimum and runs with system Python without extra dependencies. The installed Xcode SDK's libc++ `__configuration/availability.h` warns for macOS deployment targets below 11.0, so 12.0 needs no increase. Windows uses the Evergreen WebView2 runtime; Linux builds use WebKitGTK 4.1 API packages.

- **Node.js 24+** (the repository version is pinned in [`.nvmrc`](https://github.com/sstraus/tuicommander/blob/main/.nvmrc))
- **Rust** (stable toolchain via rustup)
- **Tauri CLI** (`cargo install tauri-cli`)
- **git** for git features. `gh` (GitHub CLI) is only a release tool; the app does not run it.

> **Windows users:** See the [Windows-specific prerequisites](#windows-prerequisites) section below before proceeding.

## Install Dependencies

```bash
pnpm install
```

### TypeScript target

The app uses `ES2024` for the TypeScript target and standard library, with
`DOM` and `DOM.Iterable` types. Vite keeps its existing `esnext` build target;
it does not lower ES2024 syntax to ES2021. This configuration does not add
runtime polyfills.

### Paired Tauri dependencies

Update each Tauri Rust plugin and its npm counterpart together. Notification stays
on `~2.4.0` and updater on `~2.11.0` in both manifests until their next minor
versions are validated together. The direct `rfd` dependency must match the
version and features used by `tauri-plugin-dialog`; dialog 2.8.1 uses `rfd 0.16`.

### Temporary dependency audit exception

`pnpm-workspace.yaml` configures `auditConfig.ignoreGhsas` for
[GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) only.
Boss approved this exception on 2026-10-04; it expires on **2026-11-04**.
At approval time, `braces` 3.0.3 had no patched version on npm (`3.0.4`
returned 404). Its only dependency path is build-only:
`vite-plugin-purgecss → purgecss → fast-glob → micromatch → braces`.
The advisory concerns stack exhaustion from deeply nested patterns. This path
processes repository build inputs, rather than runtime terminal or Markdown
content. Remove the exception when a patch becomes available, and review it
by the expiry date; pnpm does not enforce the date automatically.
No other advisory is ignored. `pnpm-workspace.yaml` pins DOMPurify 3.4.16
for the application and Mermaid, and the development-only `qs` to 6.16.0.
Rechecked on 2026-10-09: npm still publishes 3.0.3 as latest, and the advisory
still lists no patch. The dependency tree still has only the build path above;
retain the existing exception and its expiry. The frontend audit now requires
Seroval >=1.6.3 and source-map-js >=1.2.2. KaTeX 0.16.47 remains under
Mermaid 11; its patched >=0.18.2 line requires the separate Mermaid upgrade
and browser compatibility review. No KaTeX advisory is ignored.
pnpm 11 ignores the legacy `pnpm.auditConfig` field in `package.json`.

## Development

### Native Tauri App

```bash
make dev
```

Starts the Vite dev server and Tauri app. Frontend files use Vite HMR; Rust changes require restarting the development process.
On macOS, `make dev` requires Python 3 and passes a [Cargo target runner](https://doc.rust-lang.org/cargo/reference/config.html#targetcfgrunner) to Tauri's `cargo run`. The runner copies the desktop executable and its `tuic-remote`, `tuic-bridge`, and `tuic` siblings into `~/Library/Application Support/com.tuic.commander/dev-bin/` before launch. These are real copies, not links into Cargo or mbx targets. Target cleanup can no longer remove the running desktop executable. The path stays the same across runs; files are replaced, not accumulated, and remain after exit. A second launch at that path is rejected while the runner holds its process lock. `TUIC_DEV_BIN_DIR` overrides the directory for isolated script checks. Direct `pnpm tauri dev` does not use this protection. Linux and Windows keep the normal Cargo launch.

The existing MCP bridge installation still verifies and preserves its own revisions; the dev runner keeps its adjacent source available. The bridge installer runs inside the application, so it cannot protect the desktop executable before launch.

The macOS application firewall uses code signing for initial and tracking decisions ([Apple TN2206](https://developer.apple.com/library/archive/technotes/tn2206/)). Its UI also selects applications by filesystem path ([Apple firewall settings](https://support.apple.com/guide/mac-help/block-connections-to-your-mac-with-a-firewall-mh34041/mac)). Both the executable path and signing identity matter: a stable existing path contains the missing-executable failure, but does not promise unchanged firewall permission after a rebuild changes the signature. Confirm LAN and tailnet access after the next launch; no firewall setting is changed by the runner.
Vite excludes backend files and repository tooling output such as `.tmp/`, `target/`, `dist/`, coverage, reports, plans, stories, and docs from its watcher. Mutation testing keeps its disposable checkout under `.tmp/`; its HTML files must not reload the live WebView. Changes to this watch configuration require restarting the Vite dev server.
The watch allowlist and Tauri version are resolved from the directory containing `vite.config.ts`, so starting Vite from another working directory still uses this checkout's inputs.

### Isolated agent desktop checks (macOS)

Use a linked worktree, not the daily checkout. Inspect the complete plan first:

```bash
make agent-test AGENT_TEST_DRY_RUN=1
make agent-test
# In another shell, stop only this supervised instance:
make agent-test-stop
```

The dry run does not build, create runtime files, read application configuration,
or start a desktop process. The launch runs in the foreground. It builds the
frontend, then uses one Cargo build through the mbx shim for
`tuicommander-agent-test`, `tuic-bridge` and `tuic`. The existing orchestrator
build-slot wrapper serializes these builds when installed. `mbx doctor` must
succeed. Do not set `CARGO_TARGET_DIR`. No Vite server, Tauri watcher, release
bundle, signing operation or installed executable is involved. Embedded frontend
assets use `tauri/custom-protocol` even though this is a debug build.

Defaults are `AGENT_TEST_INSTANCE=agent-test` and `AGENT_TEST_PORT=19876`.
Override with, for example, `make agent-test AGENT_TEST_INSTANCE=agent-check
AGENT_TEST_PORT=19877` (on one line). Use the same instance on the stop target.
Agents must connect through the instance's Unix socket, printed as `socket` and
as a ready-to-run `connect` command in the launch JSON:

```bash
curl --unix-socket '<socket from launch JSON>' http://localhost/health
```

Replace `/health` with the endpoint needed for the check. The socket does not
require HTTP authentication. TCP requests to `127.0.0.1:<port>` require Basic
auth (or another configured authenticated session); an unauthenticated request
can return 401. A ready TCP listener does not mean an agent can use its API
without credentials. Do not read or copy the daily instance's credentials.

These dedicated variables ignore inherited `TUIC_APP_INSTANCE`, `TUIC_PORT`,
`TUIC_SOCKET` and `TUIC_DEV_BIN_DIR`. Empty/default instance IDs and port 9876
are rejected, not interpreted as production defaults. The dedicated Rust entry
point also validates instance and port before any config read, and refuses
release, non-macOS or dictation-enabled builds.

The compiled executable has a distinct Cargo target name and is copied to
`<worktree>/.tmp/agent-test/<instance>/tuicommander-agent-test`. The running copy
is not a symlink into mbx's target cache. The default instance stores app state
in `~/Library/Application Support/com.tuic.commander/instances/agent-test/`.
It never seeds from the daily config. Never copy daily tokens or config there.

The test window is 640×420 logical pixels, with matching minimum dimensions,
at the bottom-right of the primary screen work area (16 logical pixels inset).
It is always on top so ordinary application windows do not cover it. A higher
or another floating window (a second agent-test instance in the same corner, a
system authentication prompt) can still cover it fully; WebKit would then mark
the page hidden and stop `requestAnimationFrame`. The agent-test WebView
therefore disables WebKit window-occlusion detection, so the document stays
visible while the window is ordered in. It cannot become key or take keyboard
focus. Background throttling is disabled (supported on macOS 14+). The
agent-test application uses the `Prohibited` activation policy, set between
Tauri build and run: Tao 0.37.1 still calls `activateIgnoringOtherApps(false)`
at launch, and macOS grants it when the launching application (usually the
daily TUIC) is frontmost. As a result the test instance has no Dock icon or
menu bar and never becomes the active application. A process-lifetime activity
prevents App Nap during live latency checks but still permits idle system sleep.
These changes apply only to the dedicated agent-test runtime; the daily app keeps
its existing activation, occlusion and power-management behavior.
It starts hidden: Tao 0.37.1 `app_state.rs::window_activation_hack` otherwise
orders every visible window key even when `focus=false`. At Tauri Ready, after
that sweep, the agent-test path calls native `NSWindow.orderFront` without
becoming key. Saved-window restoration is disabled for this profile because
the window-state plugin calls `show` and `set_focus` during restoration.
Do not call `window.show()` or `setFocus()` to present it: the pinned Tao backend
uses `makeKeyAndOrderFront` for those APIs. The agent-test capability override
denies `show` and `set-focus`; it adds no presentation permission. The daily
capability file is unchanged. Stop the instance when the check ends to remove
the floating window. The daily window configuration remains unchanged.

#### Native visibility regression (authorized macOS runs only)

The unattended `scripts/agent-test-tests.py` checks launcher/configuration and
source-wiring contracts. It does **not** observe a rendered window. The separate
native regression launches this worktree's `make agent-test`, observes the real
WebView through its Unix socket, and stops the supervised instance in cleanup:

```bash
python3 -B scripts/agent-test-visible.py --instance at-2090 --port 19883 \
  --evidence-dir "$HOME/Gits/.tmp/results/agent-test-visible-$(date +%s)"
```

Add `--duration 300` to observe presentation for five minutes. Add `--cover` as
well to put an opaque, non-activating panel from another process (accessory
policy; it absorbs clicks without activating anything) over the window for the whole observation; this is the
story 2095 regression for a covered test window. The observer records
visibility changes, animation-frame progress and gaps, native window-server layers,
application-hidden state, foreground identity, and compact-window geometry in
`presentation.json`. It fails if presentation or animation frames stop, the test instance becomes
frontmost or takes focus, or window geometry changes. Foreground changes between
other applications are recorded, not failures: the check must pass while Boss works.
The window-server query is read-only and does not activate or move other windows.
These observations do not read `NSWindow.occlusionState` directly or prove App Nap activity.

This needs explicit desktop-execution authorization, macOS, a linked worktree
with build inputs, an unused named instance/port, and an unrelated foreground
application at launch. Do not use an existing instance with sessions. The evidence directory
must be new and under `~/Gits`.

`agent_test_window_renders_visible_without_focus` checks document/native
visibility, positive terminal canvas area, actual `fillText` paint, no native or
document focus, and that the test instance is not frontmost at readiness or
after painting. It also verifies the compact viewport and corner geometry.
It retains JSON receipts, the native build log and canvas-only PNGs, deletes
only sessions created by the check, calls `make agent-test-stop`, and checks
`ps` for this worktree's executable. It never captures the desktop or drives
the daily instance. A failure exits nonzero after cleanup. Commenting out the
native `orderFront` presentation must make this observer fail; source contracts
alone are not behavioral RED evidence. This native check is intentionally not
part of the unattended shell/Rust/JS suites.

#### Shared-resource audit

| Resource | Agent-test policy |
| --- | --- |
| Daily binary, dev-bin and installed app bundle | No writes. Only the worktree's distinctly named binary and adjacent sidecars are copied. A held supervisor lock prevents replacing the running test executable. |
| Config, credential vault, databases and file locks | Existing named-instance namespace; debug credentials remain file-backed. The test never claims the global-input lock, even in its own namespace. |
| HTTP and MCP | Explicit loopback-only `127.0.0.1:19876`; never port 9876 or automatic next-port fallback. Unix MCP socket uses the selected instance hash and the launch's short `TMPDIR` under `~/Gits/.tmp/tuic-at/`. Child CLI/bridge processes inherit the same namespace. |
| Global window shortcut and Fn/Globe monitor | Input ownership refused and shortcut registration rejected at runtime, including later Settings changes. The verification window is non-focusable; drive its own HTTP/socket API. |
| Microphone and dictation | Not compiled into this binary. A dictation-enabled build refuses to start; no microphone permission request or capture path is available. Audio/voice hardware checks still belong to Boss. |
| Telegram bot | Supervisor never starts, tool availability is false, and setup changes are rejected. A copied or previously saved bot configuration cannot start polling. |
| Global CLI | Startup auto-update is skipped. Explicit installation, update and removal are rejected before sidecar resolution or filesystem changes, including elevated helper paths. |
| Global agent hooks | Hook enable and disable requests are rejected over IPC and HTTP before saving the instance flag or changing shared Claude, Gemini, Grok, Codex or OpenCode files. Read-only hook status remains available. |
| Global agent MCP configuration | Startup installation cannot claim ownership, even with an inherited owner override; explicit install/remove writes are rejected. |
| Updater | No native updater plugin, no channel checks and automatic updates default off. The test cannot replace the daily bundle. |
| Deep links, release single-instance lock and login items | Deep-link plugin is not installed. This debug-only binary never takes the release single-instance lock. Desktop startup has no login-item registration; remote service installation is an explicit operation, not part of this target. |
| Relay, Tailscale and sleep inhibition | Relay supervisor and startup TLS provisioning are skipped; sleep inhibition is a no-op. No remote configuration is copied. |
| Daily window-state repair | Skipped: the legacy repair scans daily config paths, not the test namespace. |
| WebView identity | `com.tuic.commander.agent-test`, titled **TUICommander Agent Test**, separate from the daily Tauri identity. |
| Stop ownership | PID is informational. The stop target writes to the instance's FIFO; only the live supervisor signals its own `Popen` child. It never kills a PID read from disk. |

The launch plan prints the exact binary, config directory, environment, socket,
PID file and stop FIFO. A crash releases the advisory supervisor lock; the next
start replaces stale runtime markers. Do not use `pkill`, `killall`, or the daily
app's Quit/Restart action to stop a test.

Desktop live proof is separate from a dry run or unit test. An authorized operator
must checksum the running daily binary and app bundle before and after, start
agent-test alongside it, verify both listeners without mutating the daily API,
and stop only agent-test. Until that proof is recorded, do not claim macOS TCC,
simultaneous listeners or global-input isolation have been verified live.

### Browser Mode

When the MCP server is enabled in settings, the frontend can run standalone:

```bash
pnpm dev
```

Connects to the Rust HTTP server via WebSocket/REST.

> **Note:** `pnpm dev` runs `scripts/dev-server.mjs`, not `vite` directly. The dev server is pinned to port 1421 (Tauri's `devUrl`), so the launcher checks the port first: if this checkout is already serving there it prints `reusing it` and exits 0 — the second Tauri app attaches to the running server. Starting a second Vite would wipe the shared `node_modules/.vite/deps` cache and kill hot reload for the session already running. If the port is held by another checkout or a stray process, the launcher fails with an explicit message instead of serving the wrong sources.

## Build

When using mbx, `src-tauri/build.rs` emits `MBX_SHARE_OUT_DIR=0` for the
`tuicommander` crate only, because Tauri's `generate_context!` writes assets into
`OUT_DIR` during macro expansion and the shared `OUT_DIR` of mbx is read-only.
Do not move this setting to a workspace `.mbx.toml`: there it also gives every
dependency with a build script a checkout-specific `OUT_DIR`, and each worktree
then misses the cache for those crates and all crates that depend on them.

```bash
pnpm tauri build
```

Produces platform-specific installers:
- macOS: `.dmg` and `.app`
- Windows: `.nsis` (`.exe` setup installer)
- Linux: `.deb` and `.AppImage`

> **Note:** The `.msi` bundle may fail on Windows due to WiX tooling issues. Use `--bundles nsis` to produce a working `.exe` installer:
> ```bash
> cargo tauri build --bundles nsis
> ```

## Testing

Test scratch comes from your temp directory, never from `$HOME`:

| Root | Resolution (first match wins) |
|---|---|
| host temp | `TUIC_TEST_HOST_TMPDIR` (exported by every entry point before it repoints `TMPDIR`, so a default never nests) → `$TMPDIR` → the OS temp dir |
| base | `TUIC_TEST_TMP_BASE` (opt-in) → `<host temp>/tuic-tests` |
| root | an inherited `TUIC_TEST_TMP_ROOT`, used as is and never deleted → `scripts/with-test-tmp.sh`: a fresh `<base>/tuic-run.XXXXXX`, removed on exit; nextest without the wrapper, a bare test binary and the shell tests: the per-checkout `<base>/tuic-co-<checkout hash>` |
| Unix sockets | `TUIC_TEST_SOCKET_ROOT` → the per-run root → `<host temp>/tuic-s<hash>` → `/tmp/tuic-s<hash>` |

`scripts/with-test-tmp.sh`, `src-tauri/scripts/nextest-test-tmp.{sh,ps1}`,
`scripts/mutants.sh` and the shell tests (all through `scripts/test-tmp-lib.sh`),
`tuic-test-support` and `scripts/test-tmp-root.mjs` (vitest, cycle checker) all
apply that order. The checkout hash is FNV-1a 64 of the checkout's physical path,
the same in every one, so parallel worktrees never share a default root.

`TUIC_TEST_TMP_BASE=<checkout>/.tmp/tuic-tests` still puts scratch in the
checkout, but only as an explicit opt-in: it is unsafe wherever writes to the
checkout's `.git/hooks` or `.git/config` are denied (observed in the agent
sandbox), and a fixture created inside the checkout can reach the real
repository through `find_repo_root`. Nothing defaults there any more.

Rust socket tests use `tuic-test-support::short_socket_test_temp_root()`. A
socket root must be at most 49 characters (104-byte `sun_path`, the NUL, eight
bytes of HOME margin and mdkb's `/sXXXXXX/.mdkb/daemon-hook.sock.4294967295.tmp`),
so on macOS, where `$TMPDIR` alone is about 48 characters, it is normally
`/tmp/tuic-s<checkout-hash>`. Each candidate must fit and pass a create-and-write
probe; the shared `/tmp` directories are created mode 0700, and a symlink or a
directory owned by another user is refused. If none qualifies, the panic names
every candidate and `TUIC_TEST_SOCKET_ROOT=<short writable dir>`.

The wrapper prunes, after more than six days unused, only what this tooling
creates: `tuic-run.*` and the per-checkout `tuic-co-*` (and the older
`tuic-proc-*`/`tuic-nextest-*`) under the base, the old
`<checkout>/.tmp/tuic-tests` and an inherited root,
and your own `tuic-s<16 hex>` socket directories in the host temp dir and `/tmp`.
A prune it cannot complete prints a warning and never fails the command. Earlier
versions used `~/Gits/.tmp/tuic-tests`, `~/Gits/.tmp/s<16 hex>` and
`<checkout>/.tmp/tuic-tests`; nothing touches those any more, and you may delete
them by hand. `make check` runs `scripts/check-no-home-gits.mjs`, which fails on a
`$HOME/Gits` path in code or tooling. Run standalone Rust tests through the
wrapper.

Nothing may be written, created or bound under a hard-coded `/tmp`, `/var/tmp`,
`/private/tmp` or `/dev/shm`: derive scratch from the caller's temp dir
(`std::env::temp_dir()`/`tempfile`, `os.tmpdir()`, `"${TMPDIR:-/tmp}"`, the test
roots above). `make check` runs `scripts/check-no-hardcoded-tmp.mjs`, which fails
on a write-shaped use of those literals in code or tooling (including the code
blocks of `.claude/skills/*/SKILL.md`). Other mentions — a fixture string, a
classifier input — are allowed only through its `file::fn` allow-list (each entry
with a reason) or the shrink-only per-file counts in
`scripts/hardcoded-tmp-baseline.json`: a count may go down (lower the baseline in
the same change, `node scripts/check-no-hardcoded-tmp.mjs --write-baseline`) but
never up. The `/tmp` socket fallbacks described above are the known exceptions,
allow-listed as `PHASE-B` until the socket roots move under `$TMPDIR`. New tests
use a fake root that never exists rather than a `/tmp/...` string.

### Live peer-mail wake canary

After a Rust rebuild, run this against the isolated test instance, not the
orchestrator instance. The script starts a disposable managed peer through MCP
`agent action=spawn`, waits for its idle composer, sends mail from a separate
MCP identity, checks that `PEER_MAIL_WAKE` appears in the PTY within 20 seconds,
then closes the PTY and its MCP connections. It exits with an error if readiness,
delivery, or cleanup fails.

```bash
TUIC_APP_INSTANCE=peer-mail-canary make dev  # in a separate terminal; use its reported HTTP port
TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude
TUIC_CANARY_URL=http://127.0.0.1:9877 python3 scripts/canary-peer-mail-wake.py claude --capacity
```

`--capacity` sends 100 messages to the disposable agent, reads them through a
second MCP connection bound to that agent, then checks that the next send and
read succeed. Use `codex` instead of `claude` for the optional Codex check. The
agent CLI must already be installed and authenticated. Run the script only
after the test instance is listening on the URL you supply.

```bash
pnpm test              # Run all tests
pnpm test:coverage     # Coverage report
```

**Test tiers:**
- Tier 1: Pure functions (utils, type transformations)
- Tier 2: Store logic (state management)
- Tier 3: Component rendering
- Tier 4: Integration (hooks + stores)

**Framework:** Vitest + SolidJS Testing Library + happy-dom

**Coverage:** ~80%+

### Rust build features

Plain application Cargo builds enable `desktop` but omit native speech. Add
`--features dictation` for Whisper/WebRTC and the voice adapters. Tauri dev and
release builds read `build.features = ["dictation"]` from `tauri.conf.json`, so
`make dev` and `make build` retain voice support. Desktop CI tests that feature;
headless daemon checks use `--no-default-features`. Workspace tests also select
the `tuic-dictation` crate explicitly, and therefore compile its native stack.

TLS uses the existing ring provider. Provider-free reqwest/axum-server features
avoid compiling AWS-LC while retaining HTTPS and WSS.

### Desktop frontend embedding gate

The desktop binary constructs the Tauri context and passes it to the library.
Library unit tests do not expand `generate_context!` or embed the ignored Vite
bundle. HTTP static-file tests use checked-in transport fixtures, including the
desktop and mobile HTML shells, instead of a worktree-specific `dist/`.

Run `make check-desktop-embedding` after `pnpm build` to compile the real desktop
entry point with `tauri/custom-protocol`. This gate fails if the frontend bundle
is missing. It does not launch or restart the application. `make check` and
desktop CI run this separate gate, so unit-test isolation does not hide desktop
startup or embedding errors. CI uses minimal HTML stubs; the release build still
embeds the real frontend bundle.

### Rust tests in linked worktrees

Run standalone Rust tests through `scripts/with-test-tmp.sh`. In a linked
worktree, the wrapper clears an inherited `CARGO_TARGET_DIR`, so Cargo selects
that checkout's artifacts instead of a target supplied by the parent
TUICommander process. The worktrees' `tuic-terminal` builds have the same Cargo
fingerprint key, while [Cargo checks path sources by file mtime](https://doc.rust-lang.org/stable/nightly-rustc/cargo/core/compiler/fingerprint/index.html);
sharing one target can therefore make an older checkout look fresh against an
artifact from another checkout. The wrapper also gives each run its own scratch
directory under `$TMPDIR/tuic-tests` (see Testing above).

### TypeScript mutation testing

Run Stryker on the changed source files and their relevant Vitest files. Keep both
lists narrow; a mutation run executes the selected tests for each mutant.

```bash
node scripts/ts-mutants.mjs \
  'src/utils/pathUtils.ts:65-65' -- \
  'src/__tests__/utils/pathUtils.test.ts'
```

List the changed source files or line ranges before `--`, and the tests that
exercise them after it. Read
`reports/mutation/mutation.json` and the console verdicts. A known behavior
change, such as inverting the `pathBasename` condition above, must be reported
`Killed` before treating the other verdicts as evidence. Install this checkout's
dependencies with `pnpm install --offline --frozen-lockfile` when its local
`node_modules/.bin/stryker` is missing; linked worktrees do not share the main
checkout's executables. The checked-in Stryker config copies frontend inputs,
Rust `.rs` files under `src-tauri/src/`, the terminal-grid source, and the generated
`command_table_paths.txt` into its sandbox. Add any new files read directly by a
Vitest test to that copy policy. It leaves the worktree source untouched and runs
Vitest as a separate command for each mutant. Stryker 10's
Vitest runner reports false survivors with this repo's Vitest 5, so it is not
used. Stryker's TypeScript preprocessor is pointed at an absent file because
this repo's TypeScript 7 does not expose the compiler API it calls; Vitest still
transforms and runs the TypeScript tests.

## Project Structure

See [Architecture Overview](../architecture/overview.md) for full directory structure.

## Key Files

| File | Purpose |
|------|---------|
| `src/App.tsx` | Central orchestrator |
| `src-tauri/src/lib.rs` | Rust app setup, command registration |
| `src-tauri/src/pty.rs` | PTY session management |
| `src/hooks/useAppInit.ts` | App initialization |
| `src/stores/terminals.ts` | Terminal state |
| `src/stores/repositories.ts` | Repository state |
| `SPEC.md` | Feature specification |
| `ideas/index.md` | Feature concepts under evaluation |

## Configuration

App config stored in platform config directory:
- macOS: `~/Library/Application Support/tuicommander/`
- Linux: `~/.config/tuicommander/`
- Windows: `%APPDATA%/tuicommander/`

See [Configuration docs](../backend/config.md) for all config files.

## Makefile Targets

```bash
make dev         # Tauri dev mode
make build       # Production build
make test        # Run tests
make lint        # Run linter
make docs        # Build this documentation + search index into docs/book
make docs-serve  # …and serve it at http://127.0.0.1:8123
make clean       # Clean build artifacts
```

---

## Documentation Site

The book you are reading is [mdBook](https://rust-lang.github.io/mdBook/), built
by `scripts/build-docs.sh` and deployed to GitHub Pages by
`.github/workflows/website.yml`. The script is the single source of truth — CI
runs the same one, so a local build matches the deployed site.

```bash
make docs-serve     # build + serve; open http://127.0.0.1:8123
```

Requires `mdbook` (`brew install mdbook` or `cargo install mdbook`) and `npx`.

Search is [Pagefind](https://pagefind.app/), generated after the mdBook build,
which is why `file://` previews have no search — the index is fetched over HTTP.
mdBook's own elasticlunr search is disabled in `book.toml`.

Adding a page means adding it to `SUMMARY.md`: mdBook only renders — and
Pagefind only indexes — chapters listed there.

---

## Windows Prerequisites

Building on Windows requires a few extra tools beyond the standard prerequisites. Install them in this order.

### 1. Visual Studio Build Tools (C++ compiler)

Download and install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select the **"Desktop development with C++"** workload. VS Build Tools 2019 or later is fine.

Or via winget:
```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

### 2. Rust

```powershell
winget install Rustlang.Rustup
```

Restart your terminal after installation, then verify:
```powershell
rustc --version
cargo --version
```

### 3. Node.js

```powershell
winget install OpenJS.NodeJS.LTS
```

### 4. CMake

Required to compile `whisper-rs` (the on-device dictation library).

```powershell
winget install Kitware.CMake
```

### 5. LLVM 18 (libclang — required for whisper-rs bindings)

`whisper-rs` uses `bindgen` to generate Rust bindings for `whisper.cpp`, which requires `libclang`. **Use LLVM 18** — LLVM 19+ produces broken bindings for this crate on Windows.

Download the LLVM 18 installer from [GitHub releases](https://github.com/llvm/llvm-project/releases/tag/llvmorg-18.1.8) (`LLVM-18.1.8-win64.exe`) and install it. Then set the environment variable so `bindgen` can find it:

```powershell
# Add to your PowerShell profile or set permanently in System Environment Variables
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

> If you have LLVM 22+ installed (e.g. from winget), install LLVM 18 to a separate directory and point `LIBCLANG_PATH` there instead.

### 6. Tauri CLI

```powershell
cargo install tauri-cli --version "^2"
```

### Full Windows Build Command

Always set `LIBCLANG_PATH` before building:

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"   # adjust path if LLVM 18 is elsewhere
cargo tauri build --bundles nsis
```

The installer will be at:
```
src-tauri\target\release\bundle\nsis\TUICommander_0.x.x_x64-setup.exe
```

### Windows Known Issues

| Symptom | Cause | Fix |
|---|---|---|
| `whisper-rs-sys` build fails with "couldn't find libclang" | LLVM not installed or `LIBCLANG_PATH` not set | Install LLVM 18 and set `LIBCLANG_PATH` |
| `whisper-rs-sys` compile error: `attempt to compute 1_usize - 296_usize` | LLVM 19+ generates broken bindings for this crate | Use LLVM 18 specifically |
| WiX `.msi` bundle fails | WiX `light.exe` tooling issue | Use `--bundles nsis` instead |
| App window opens but shows a **black screen** | Navigation guard in `lib.rs` blocked `http://tauri.localhost/` (Windows' internal Tauri URL) | Fixed in current code — `tauri.localhost` is explicitly allowed |
| `Update check failed: windows-x86_64-nsis not found` | Custom local build isn't listed in official release manifest | Harmless — auto-update simply won't trigger |
