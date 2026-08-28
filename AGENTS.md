# TUICommander — Project Rules

## Doc Sync

Read [`docs/sync-matrix.md`](docs/sync-matrix.md) before any feature/API/config change — it maps code areas to docs that MUST be updated.

## Tests

- Before declaring a change complete, run `./scripts/check-gate.sh` (or `make check-gate`) —
  not a scoped test filter, and not a bare `make check 2>&1 | tee log` (that pipeline's exit
  code is `tee`'s, not `make`'s, so a real failure can go unnoticed). Repo-wide consistency
  tests — e.g. the IPC/HTTP Parity mapping test below — only run as part of the full suite and
  will not show up in a feature-scoped filter.
- Tests are the spec. When a test fails after a code change, investigate BOTH sides before deciding which to fix.
- **Finding a story partially implemented does NOT mean it's done.** When you pick up a story and discover the feature already exists, verify EVERY part of the story is honored — each acceptance criterion, edge case, and requirement — before marking it complete. Never assume the whole story is satisfied just because one part is implemented. Check each criterion against the code and prove it, or the story isn't done.
- `to-test.md` tracks features awaiting manual testing — add items there for minor features.
- **Mutation testing is per change, never per tree.** `make mutants RANGE=<base>` (default `HEAD~1`) runs cargo-mutants `--in-diff` over the Rust lines the range touched, `--in-place` in a disposable `git archive` export under `.tmp/` (not a worktree: a detached worktree is an orphan to the running app, which removes it), one job, through mbx. Every viable mutant costs one incremental build of the lib crate plus one test run, so the orchestrator runs it once per batch on the final HEAD — agents do not. A surviving mutant is a missing test: add the test, or `#[mutants::skip]` with the reason on the line. **Measured 2026-09-06:** baseline 197 s build + 224 s test with warm deps; each mutant ~3 min incremental build of the lib crate plus 0.5–3.5 min of tests, so ~5 min each and a 38-mutant story diff is ~3 h. During the day pass a function filter through the script (`scripts/mutants.sh HEAD~1 --re <function>`); the whole diff is an overnight or CI job. Config in `src-tauri/.cargo/mutants.toml`, mechanics in `scripts/mutants.sh`.
- **CI has never executed on this repository.** `.github/workflows/ci.yml` triggers on push and exists, but `gh run list` returns zero runs for this repo on any branch. "CI is green" is never evidence of anything — nothing in this repo's test suite has ever been machine-validated by CI. Treat any CI-shaped claim in a commit message or PR description as unverified until someone actually watches a run. `CONTRIBUTING.md` describes CI as an active gate; it isn't yet.
- `make check`'s "plugin tests" step is **currently expected to fail** — the `plugins` submodule has zero test files. This is known, tracked state (`c6cae61a`'s CI comment); don't spend time trying to "fix" a red `make check` caused by it.
- **When touching `src-tauri/patches/{alacritty_terminal,vte}/`, verification MUST include `cargo nextest run --workspace` (or `make check`), not a package-scoped `cargo test --lib`/`cargo test -p tuicommander`.** The vendored crates are separate workspace members with their own regression suite (`patches/alacritty_terminal/tests/ref.rs`, ~44 fixture-replay tests) that a package-scoped run silently skips. `cargo nextest run`/`list` without `--workspace` also silently scopes to zero tests for a vendored crate instead of erroring — this exact mistake produced a false "these tests were never wired in" diagnosis in commit `47217d2c`'s own message. A background-color-erase fix landed in `6dd165f5` without running the workspace suite and shipped two regressions caught only later.
- When resolving a rebase/merge conflict in a Rust function by taking one side's body wholesale, diff the full field set of any struct it returns — a dropped field can compile cleanly (mocked in the caller's own tests) while silently regressing a feature only an end-to-end test would catch. Prefer merging the logic, not picking a side outright, when the two versions diverge structurally.
- **`[HUMAN]` is a last resort.** Before marking a to-test item `[HUMAN]`, you MUST attempt verification through this escalation ladder:
  1. **Code inspection** — read the source, confirm the logic exists at file:line
  2. **Test execution** — `cargo nextest run` (doctests: `cargo test --doc`), `vitest run` with relevant filter
  3. **CLI probing** — `curl` HTTP endpoints, `grep` for patterns
  4. **MCP maccontrol** — take screenshots, click UI elements, verify visual state
  5. **MCP invoke/JS** — call Tauri commands, inspect store state, trigger actions programmatically
  Only use `[HUMAN]` when the item genuinely requires real hardware (audio, IME, touch), multi-app interaction (drag to Finder, global hotkey from another app), or timing-sensitive observation that none of the above can capture. When code-verifying, change `[HUMAN]` to `[x]` with a `_(verified: file:line explanation)_` annotation. When code reveals the description is wrong, change to `[ ]` with a `_(NOTE: ...)_` correction.

## The suite skips 15 tests on purpose — classify them, never pin the count

`cargo nextest run --lib` reports 15 skipped. All 15 are `#[ignore]`, each with a
reason string. None is a `cfg` exclusion and none is a filter artifact, so the
skips are not missing coverage and not a harness defect: `5171 run, 15 skipped`
is a **complete** result for what an unattended run can execute.

| Category | Count | Precondition an unattended run cannot meet |
|---|---|---|
| Environment | 9 | interactive Keychain (×4), network + GitHub token (×2), authenticated `gh` CLI, downloaded whisper model, real `openpty` |
| Corpus-driven | 4 | `TUIC_CAPTURE_CORPUS`, `TUIC_DAMAGE_CORPUS`, `TUIC_REPLAY_FILE`, plus 744-138c's evidence capture |
| Benchmark | 2 | `bench_chunk_path_replay`, `tunnels::audit::tests::bulk_insert_performance` |

**`dump_committed_tcap_fixture_event_sequences_744` is not a pass/fail test.** It
is an evidence-capture harness for story 744-138c and the comment above it says
so. Un-ignoring it during a tidy-up of ignored tests is the failure to avoid.

**Re-derive the classification; do not trust a count.** Counting `#[ignore]`
attributes in the source happens to give 15 today, which is the right answer for
the wrong reason — it counts one mechanism and cannot see the other two. This
does discriminate:

```
cargo nextest list --lib --run-ignored all   ->  5187
cargo nextest list --lib                     ->  5172
```

The delta is 15 and the set difference *is* the 15 names. A `cfg`-excluded test
is absent from **both** lists, so the delta would not close if any were excluded
that way; a filtered skip would move the second number alone.
`--run-ignored ignored-only` is stronger still: it lists the set instead of
implying it by subtraction, and run against both configurations it names the one
test in the difference rather than leaving a count to interpret.

**Parse that output carefully, because a wrong pattern returns zero and so does
an empty set.** `nextest list` prints `tuicommander <path>` with no leading
whitespace; a `grep -E '^\s+\S+::'` written on the assumption that it indents
returns 0 lines for every configuration, exits 0, and answers a different
question. That is the same failure as a vacuous `-E` filter and as counting
`#[ignore]` attributes — a command that ran and told you nothing, in a shape
indistinguishable from a real result.

**Never assert the skip count.** It breaks the first time someone adds a
legitimately ignore-worthy test, and it asserts nothing about whether the right
tests run — green-by-absence one level up. The mechanism is the durable fact;
the number is not. Note also that the raw totals above drift within a single
afternoon as agents land tests in a shared tree (5170 → 5171 → 5172 → 5173 on
2026-09-13, all benign), which is exactly why the re-derivation method belongs
here and the numbers do not.

`--no-default-features` reports 14, one fewer, because `mod dictation` is
`#[cfg(feature = "desktop")]` (`lib.rs`) and its ignored test does not exist in
that build — absent rather than skipped. No `#[ignore]` anywhere is
`cfg`-conditional, so nothing else moves between the two configurations.

## Which timing assertions are load-bearing

A test that waits on wall-clock time asserts one of three things, and they are not
interchangeable. Before adding an `Instant` deadline, decide which row you are writing.

| Bound | Belongs to | Rule |
|---|---|---|
| the behaviour under test | "a mute upstream gives up" | keep it — and arm it *after* setup succeeds |
| setup reaching a state | handshake, fetch, process start | it must not be able to fail: size it so it cannot, or delete it |
| "did this hang forever" | the outer harness bound | strictly larger than every bound inside it, or its message lies |

**Never let one deadline serve two rows.** `call_tool_gives_up_on_a_mute_upstream`
handed its 300ms give-up deadline to the handshake as well; a loaded machine pushed
the handshake past it and the test failed as `call_tool never returned` — accusing the
exact mechanism it exists to prove works. Two named budgets is the fix.

**A freshly written executable is not a cheap thing to run.** With exec-time code
scanning (macOS `syspolicyd` plus an endpoint-security agent) the first exec of a new
file blocks while it is scanned, while re-exec'ing the *same* file costs ~6ms. The
scan cost is **episodic, not a constant**: a quiet scanner charges ~0.25s for a fresh
inode and a backlog charges tens of seconds — 6s to 102s was measured here, 393s at
the worst. There is also a smaller persistent penalty (~1.5-2.7x) on `/var/folders/…/T`,
which is where `tempfile::TempDir` lands. How the two compound is unresolved on
purpose: a multiplicative model predicts ~9s where 393s was measured. None of that
changes the remedy, and chasing either variable is wasted time. A
per-run temp script pays that scan inside the test's own timing window on every run,
which is why four `tunnels::supervisor` tests failed with the suite idle and passed
under a full parallel run. `fake_ssh_script` now keys the script by test name under
`target/fake-ssh/`, compares content, and execs it once with `TUIC_FAKE_SSH_WARMUP` set
before any supervisor starts: ~44s cold, once per machine, then ~1s a run. Do not
simplify it back to a `NamedTempFile`. The same reasoning applies to any test that
writes and runs a script — `sh <script>` is free, `./script` is not.

**A fetch that ran out of time looks exactly like a fetch that failed.** Both fall
through to the remote-tracking ref, so `FETCH_TIMEOUT`'s 5s `cfg(test)` value turned a
`conflict_assist` test about *which base ref wins* into a test of how fast git had
been. Where the deadline is not the subject, pass an explicit bound instead
(`resolve_rebase_target` takes one) and let nextest's `slow-timeout` catch a real hang.

**The failure mode to fear is a bound you cannot tell apart from a bug.** `#[ignore]`,
`#[serial]` and `--test-threads=1` all hide it rather than fix it, and cost coverage to
do so. When a timing assertion does fire, its message must name what actually broke —
otherwise the next reader spends a day re-diagnosing the wrong subsystem.

## Test instance vs orchestrator instance — READ BEFORE TESTING

There are TWO running TUICommander instances; do not confuse them:

- **Orchestrator instance** — the one this agent is embedded in. The `tuicommander` MCP tools and `debug invoke_js` target THIS instance (Mission Control on `:14319`, app logs on `:9876`). It does **NOT** run your worktree build, so testing it proves nothing about your changes.
- **Test instance** — the worktree dev build you start with `make dev`. Test your changes against it **only via its HTTP API on `http://127.0.0.1:9877`**. MCP/`invoke_js` cannot reach it.

9877 endpoints (see `src-tauri/src/mcp_http/mod.rs`): `GET/POST /sessions`, `DELETE /sessions/{id}`, `POST /sessions/{id}/write`, `GET /sessions/{id}/output`, and terminal grid ops (all session-scoped) `POST /sessions/{id}/terminal/scroll {delta}`, `POST /sessions/{id}/terminal/scroll-to {line}` (absolute; `line`=top row, 0=oldest), `POST /sessions/{id}/terminal/scroll-to-offset {offset}` (coalesced display-offset jump; powers wheel + scrollbar-drag in browser mode), `GET /sessions/{id}/terminal/scroll-info`, `GET /sessions/{id}/terminal/lines?start&end`, `GET /sessions/{id}/terminal/row-text?row`, `POST /sessions/{id}/terminal/search-buffer {query}`. Create a throwaway session, exercise it, then `DELETE` it — never test against Boss's live sessions.

Canvas rendering (selection highlight, smooth-scroll visuals, cursor) is **not observable over HTTP** — those still need a visual check with Boss.

## Web-UI testing with agent-browser (browser mode, not Tauri)

You can exercise features through the **web UI** instead of the Tauri desktop app — every instance serves the full frontend at `http://localhost:<port>/` (`static_files.rs` `FRONTEND_DIST`). Loading it in a real browser is **browser mode** (`isTauri() === false`), the exact web/PWA path a remote client sees.

- **Port:** the primary/only instance serves on **:9876**; if that's free (no conflict), point the browser there. If the orchestrator already holds 9876, bring up a **second debug instance** — it auto-retries to **:9877** (the single-instance lock is `#[cfg(not(debug_assertions))]`, so a *debug* build can run a 2nd copy alongside the orchestrator). Note: `TUIC_PORT` is honored ONLY by the headless `tuic-remote` binary — the desktop `make dev` build **ignores** it and relies on the `9876→9877→9878` retry. So: use :9876 when unconflicted, else :9877.
- **Drive it with `agent-browser`**, always via the stealth wrapper (see global rules). `@ref` CDP clicks are trusted and work (open modals/panels/tabs); **JS-dispatched keydown is `isTrusted:false`** so app keyboard shortcuts (Cmd+P, etc.) are ignored — click UI, never synthesize keys. Use a persistent `--session <name>`, restart the browser periodically (snapshots/clicks degrade after many calls), and wrap each call in a `perl alarm` timeout (macOS has no `timeout`/`gtimeout`).
- **Isolation caveat:** a 2nd debug instance has isolated backend/sessions (its own PTY/agent state), but by default there is no config-dir split — debug and release builds read and write the exact same config directory and the same `config.json`/`repositories.json` (see `docs/backend/config.md`), protected by a cross-process file lock so concurrent saves don't clobber each other. A toggle flipped in the dev build IS visible to the installed app and vice versa. It still **shares the filesystem** with the orchestrator — never run repo-mutating tests against Boss's repos, and expect possible SQLite contention on `tunnel_audit.db` (same shared dir, not covered by the config lock). **Launch a verification/test instance with `TUIC_APP_INSTANCE=<id>`** (any lowercase DNS label) to get a real, code-enforced isolated config directory instead — `instances/<id>/`, entirely separate from Boss's production files (#763-d219; see `docs/backend/config.md`). Prefer this over the shared default whenever a test is going to add or remove repositories.
- **The Command Palette DOES render in web mode.** `App.tsx:984` mounts it unconditionally as `<CommandPalette actions={actionEntries()} browserMode={!isTauri()} />`; `browserMode` filters the action list down to the HTTP-safe subset in `BROWSER_ACTION_IDS`/`BROWSER_ACTION_PREFIXES` (`CommandPalette.tsx:27-103`), which **includes** `search-files` and `search-file-contents`. Only the *opening shortcut* is unavailable to `agent-browser`: a JS-dispatched Cmd+P is `isTrusted:false`, so click the UI instead — that is a test-harness limit, not a missing feature.
- **Desktop-only features do NOT render in web mode** — IdeLauncher, Dictation, Global Hotkey, detach-panel windows, updater, native file drop, user-plugin install, MCP/hooks config. Built-in plugins DO load. See mdkb `web-mode-verification-2026-07-02` for the verified inventory before reporting a feature "missing".
  - **If `agent-browser` isn't available in your environment** (confirmed missing in a 2026-08-27 session — no wrapper script found, and the `claude-in-chrome` skill requires an extension the user may decline), fall back to the **Playwright CLI**: `which playwright` resolves a global npm install (e.g. `/opt/homebrew/lib/node_modules/playwright` on macOS) even when the repo has no `playwright` devDependency. `require()` it by its full install path — `require('/opt/homebrew/lib/node_modules/playwright')` — from a throwaway Node script (scratchpad dir), since it won't resolve via bare `require('playwright')` outside that directory. Real CDP-dispatched `page.keyboard.press(...)` IS trusted (unlike JS-synthesized keydown), so app shortcuts like Cmd+, work fine for navigation. Pass `ignoreHTTPSErrors: true` on `newPage()` — the self-signed-HTTPS fallback 301-redirects plain `http://` to `https://` even on `127.0.0.1`, and Chromium otherwise refuses the untrusted cert before any page loads.
- **Isolation caveat:** a 2nd debug instance has isolated backend/sessions (its own PTY/agent state), but there is no config-dir split — debug and release builds read and write the exact same config directory and the same `config.json`/`repositories.json` (see `docs/backend/config.md`), protected by a cross-process file lock so concurrent saves don't clobber each other. A toggle flipped in the dev build IS visible to the installed app and vice versa. It still **shares the filesystem** with the orchestrator — never run repo-mutating tests against Boss's repos, and expect possible SQLite contention on `tunnel_audit.db` (same shared dir, not covered by the config lock).
- **Desktop-only features do NOT render in web mode** — Command Palette is `<Show when={isTauri()}>` (no Cmd+P/file/content search for browser users), plus IdeLauncher, Dictation, Global Hotkey, detach-panel windows, updater, native file drop, user-plugin install, MCP/hooks config. Built-in plugins DO load. See mdkb `web-mode-verification-2026-07-02` for the verified inventory before reporting a feature "missing".
- **When `agent-browser`/`chromium-cli` are unavailable and raw Playwright is the fallback, apply the same click discipline `@ref` gives you for free.** `@ref` resolves against a live accessibility snapshot, so it can't land on the wrong element; a blind CSS/text-label selector against the whole page can, and has, twice: a bare text-match on a branch name landed on the **double-click-to-rename** hit area and opened a real "Rename Branch" dialog on a live worktree, and a bare `input:not([type])` selector matched a **real terminal's hidden keyboard-capture input** instead of a dialog's own field, typing an unrelated string into a live shell prompt (nothing was executed in either case — no Enter was sent — but both required cleanup). Before any fallback-Playwright click: take a snapshot/screenshot first, and scope every locator to a specific dialog/container element (e.g. its heading's ancestor), never a bare `input`, `button`, or text-match against the whole page.

## Visual

- All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).
- **Plugin dashboards MUST follow [`docs/plugins-style.md`](docs/plugins-style.md)** — use the shared `.dashboard`/`.dash-*` classes from `PLUGIN_BASE_CSS`, never hand-roll inline layout CSS. The built-in Claude Usage dashboard is the reference.
- Icons: monochrome inline SVGs with `fill="currentColor"` — never emoji.
- Take a screenshot after EVERY visual/CSS/layout change to verify rendering.
- **The canvas-terminal frame protocol's bit layout is defined once** in `terminal_grid.rs`'s `serialize_dirty_rows` doc comment. Any bit added there MUST be mirrored in `canvasTerminalUtils.ts::decodeBinaryFrame` AND in `docs/frontend/canvas-terminal-audit.md`'s bit-layout paragraph, not just the two code sites — three separate bugs (the DECCKM `app_cursor` bit, the DECSCUSR `cursor_steady` bit, and a stale scrollbar-visibility condition next to them) have traced back to that three-way split staying out of sync.

## Branching

NEVER create branches autonomously — Boss works with multiple windows.

## Commits

When a commit resolves a **GitHub issue**, use a closing keyword so GitHub auto-closes it: `Fixes #N` / `Closes #N` / `Resolves #N` (anywhere in the message — `fix(scope): desc (closes #N)` in the subject is fine). A bare `(#N)` only *links* the issue, it does NOT close it. This repo pushes directly to `main` (the default branch), where closing keywords take effect on push — no PR merge required.

- Use the GitHub-issue keyword only for the commit that actually fixes it; reference-only commits keep `(#N)`.
- This is distinct from **mdkb story ids** (7-char hex like `#abc1234`): those follow the wiz convention — `(#abc1234)` for traceability, `(closes #abc1234)` on story completion — and are unrelated to GitHub issue auto-close.
- **Enforced by the `pre-push` hook** (`scripts/hooks/pre-push`, installed by `make hooks` / `make dev`): a push to `main` is blocked if a pushed commit references an **open** issue with a bare `#N` and no closing keyword. Reference-only pushes bypass with `git push --no-verify` (or `TUIC_SKIP_ISSUE_CHECK=1`). The hook skips silently when `gh` is missing/unauthenticated/offline — it never blocks on a verification failure.

## Fresh Worktree Setup

A brand-new git worktree is missing gitignored build artifacts that `cargo test`/`cargo build`
need — this is why the exact same commands "just work" in the main checkout or any
previously-built worktree but fail in a fresh one:

1. **Frontend `dist/` stub** — `src-tauri/src/mcp_http/static_files.rs` does
   `include_dir!("$CARGO_MANIFEST_DIR/../dist")` at compile time. Without it: `mkdir -p dist &&
   echo '<html></html>' > dist/index.html` (from the repo root).
2. **Sidecar binary placeholders** — `tauri.conf.json`'s `externalBin` lists `binaries/tuic-bridge`,
   `binaries/tuic`, `binaries/tuic-hook`; the build script checks these paths exist for the host
   target triple. Empty files satisfy the resource-existence check: `target=$(rustc --print
   host-tuple); mkdir -p src-tauri/binaries; touch src-tauri/binaries/tuic-bridge-$target
   src-tauri/binaries/tuic-hook-$target; chmod +x src-tauri/binaries/tuic-bridge-$target
   src-tauri/binaries/tuic-hook-$target` (the plain `tuic-$target` one needs to be a real binary
   to actually run the app, but an empty file is enough for `cargo test`).
3. **A real `tuic-hook` build** — unlike the other two, this can't be a stub.
   `src-tauri/src/agent_hook.rs`'s `golden_wire_output` tests execute the compiled `tuic-hook`
   binary and assert on its real output; a 0-byte placeholder fails every one of those tests with
   `assertion failed: !text.is_empty()`. Fix: `cargo build --package tuic-hook` (from
   `src-tauri/`) — small crate, few deps, fast — populates `src-tauri/target/debug/tuic-hook`,
   which `agent_hook.rs` resolves at test time. This binary can transiently read as 0 bytes
   (`ls -la`) immediately after a successful build reports "Finished"/"nothing to rebuild" — a
   second `ls -la` or re-running the build (which is a no-op) shows the correct size with the same
   mtime. If `golden_wire_output` tests fail with empty-output assertions right after a build
   that already succeeded once, re-check the file size before assuming a real regression.
4. **If you later run `make dev`/`pnpm build:sidecar` in the same fresh worktree, the step-2
   placeholders can leave `tuic-bridge` and `tuic-hook` permanently broken instead of getting
   replaced by a real build** — this also snapped the `tuicommander` MCP connection for a
   Claude Code session running out of that worktree (`ENOEXEC` spawning
   `target/debug/tuic-bridge`, a 0-byte file, not a valid Mach-O). Root cause:
   `build-sidecar.mjs` skips its `cargo build --release` step whenever
   `src-tauri/binaries/<bin>-<target>`'s size already equals `target/release/<bin>`'s size — and
   the same transient-0-byte-read described in point 3 can hit the *release* profile too, so if
   that read lands while the step-2 placeholder is still an untouched 0-byte stub, the script
   sees "0 == 0", logs `Sidecar built: ... (skipped)` (or `up to date`), and leaves both the
   `binaries/` copy AND `target/debug/<bin>` empty — silently, with no error, and it does not
   self-correct on a later `ls`/rebuild the way point 3's case does, because nothing re-invokes
   the build. Fix: `pnpm build:sidecar --force` (rebuilds `tuic-bridge`/`tuic`/`tuic-hook`
   unconditionally), then separately `cargo build --package tuic-bridge` and
   `cargo build --package tuic-hook` (from `src-tauri/`) to populate their **debug** binaries too
   — `build-sidecar.mjs` only ever builds the `--release` profile, but `target/debug/tuic-bridge`
   is what an MCP client spawns directly. If a `tuicommander` MCP server fails to connect with an
   `ENOEXEC` on a `target/debug/*` path in a worktree you've been building in, check that file's
   size before assuming a code regression.

The `plugins/` git submodule is also frequently out of sync in worktrees — either pinned to a
stale commit (`src/__tests__/plugins/buildCleaner.test.ts` fails to resolve an import) or not
initialized at all (`git submodule status plugins` shows a leading `-`; `make check`'s `Plugin
tests` step / `pnpm test:plugins` reports 0 tests collected and exits 1). Both are pre-existing
environment drift, not a regression — don't spend time fixing the submodule pointer unless
explicitly asked.

`src/__tests__/components/ChangelogModal.test.tsx` has a flaky async leak (an uncleaned
timer/effect from its `onMount`, unrelated to the file's own logic — confirmed pre-existing,
untouched by recent commits) that vitest's leak detector marks as a failed test FILE even when
every individual test in the run passes (`Test Files 1 failed | ... ` alongside
`Tests  N passed (N)`). This fails the whole `vitest` step — and therefore all of `make check` —
regardless of what else changed. Before treating a `make check`/vitest failure as a regression,
run `pnpm exec vitest run` directly and check whether the `Tests` line shows 0 real failures; if
so, this is that known flake, not your change. Use `./scripts/check-gate.sh` (or `make
check-gate`), which detects and calls this out automatically.

## Building

**NEVER use `cargo build --release` directly.** It produces a binary that points to the Vite dev server (`localhost:1420`) instead of embedding frontend assets — result: white screen. Always use `make build` or `pnpm tauri build`, which runs `beforeBuildCommand` (frontend build + sidecar) and embeds the dist/ into the binary.

To debug the WebView in a release build, temporarily add `"devtools"` to the tauri features in `Cargo.toml`, add `w.open_devtools()` in the `setup` closure (after getting the main webview window), and rebuild with `make build`. Remove both before committing.

`make build`/`make build-dmg` auto-disable updater-artifact signing (`createUpdaterArtifacts`) when `TAURI_SIGNING_PRIVATE_KEY` is unset, so local/dev builds don't fail; CI/release builds set the key and get signed artifacts. Calling `pnpm tauri build` directly (bypassing Make) does **not** get this override and will fail locally without the key.

## Dev Hot Reload

**`make dev` runs `pnpm tauri dev --no-watch` — the Rust backend NEVER hot-reloads.** The Tauri CLI file watcher is disabled on purpose: editing anything under `src-tauri/**` (including editor/RTK `.rs.tmp.*` scratch files) will NOT rebuild or restart the Rust process. Only Vite HMR reloads the UI (frontend runs as a separate `beforeDevCommand` process). This is intentional — a mid-session Rust restart tears down every live PTY/agent session Boss is running.

**Consequence for agents:** when your change touches Rust (`src-tauri/**`), it will NOT take effect in Boss's live `make dev` session. Do NOT assume it did. Instead:

1. Make the Rust change as normal.
2. **Add an item to `to-test.md`** describing what to check after the rebuild. Never open a story for this — a story whose criteria are all post-restart checks can never close itself, so they pile up. `to-test.md` is the only tracker for anything a human must verify.
3. **Tell Boss explicitly** that the Rust change is staged but requires a manual `make dev` restart (or `make build` for release) to load, and to run it when he's ready to lose the current session.

Never silently ship a Rust edit expecting hot reload — it will look like your fix did nothing.

## Cross-Platform

Targets macOS, Windows, Linux. Use Cmd/Ctrl abstractions, Tauri cross-platform primitives. Test in release mode (`cargo tauri build`) — release builds lack shell PATH and env vars.

**A Windows checkout needs `git config core.autocrlf false`** (CI sets it globally before checkout). Git for Windows otherwise rewrites every file to CRLF, and the suite compares bytes it wrote itself against bytes git handed back, and reads its own source to check route and boot invariants — both assert on different content under the default.

**Write a test's shell script once, through `test_support`.** `cmd` has no `printf`, `seq`, `cat`, `touch` or `sleep`, and `/tmp` and `/bin/*` do not exist there, so a POSIX one-liner in a test fails on the shell rather than on the behaviour it exists to check. `host_shell`, `print_file_script`, `replay_file_command`, `dir_outside_home` and friends hold the one spelling per step.

**`Path::is_absolute` answers for the host only.** `C:\…` and `\\…` are not absolute on unix, and a leading `/` is merely *rooted* on Windows — while `Path::join` there **replaces** the root, so an unrejected `/etc/passwd` lands at the root of the repo's drive. Any validator deciding whether a path may escape a boundary uses `fs::is_absolute_on_any_platform`.

**Terminal keydown vs. global shortcuts:** `keyToSequence()` (`src/components/Terminal/terminalInput.ts`) excludes `metaKey` but not `ctrlKey` from its printable-character PTY-forwarding fallback — so a global shortcut whose Windows/Linux form uses Ctrl+&lt;printable&gt; (macOS form: Cmd+&lt;printable&gt;) will be silently swallowed and typed into the terminal instead of bubbling to the document-level shortcut listener, unless the terminal's keydown handler explicitly bails out first (see `isGlobalShortcutPassthrough`, `terminalInput.ts`). When adding or rebinding a global shortcut that uses Ctrl/Cmd + a printable key, verify the Windows/Linux (Ctrl) form is special-cased the same way.

## Panel Refresh

Panels with repo-dependent data MUST use `repositoriesStore.getRevision(repoPath)` in `createEffect` — not file watchers or polling. `repo_watcher` emits `"repo-changed"` → `bumpRevision()`.

**A panel that renders ONLY committed history** (commit log, file history, stashes) uses `getGitRevision(repoPath)` instead, so a plain file save no longer re-runs its git processes. The two counters are nested, not parallel: `bumpGitRevision` bumps **both**, and `getRevision` still moves on every event. `getRevision` is therefore always the safe default — a panel left on it cannot go stale, while a panel wrongly moved to `getGitRevision` silently misses working-tree changes. Move a panel only after checking every command it calls ignores uncommitted state.

**Panel visibility gates must check the specific scope they care about.** A gate bundled in from cross-repo/global state (e.g. `globalWorkspaceStore.isActive()`) must check the scope (`MANUAL_SCOPE`) it actually cares about, not just `isActive()` — a per-repo auto-consolidated workspace is a different activation with a single well-defined repo, and a bare `isActive()` check will wrongly suppress panels for it. The same "boolean/flag check too coarse for a growing state space" shape has recurred more than once (File Browser/Git Panel suppression, sidebar branch-icon color falling through to the wrong case) — when a state space grows a new case, re-check every existing boolean gate against it rather than assuming the old check still covers the new state correctly.

## SolidJS `<For>` Index Staleness

`<For>`'s mapping callback is invoked once per distinct item **reference**, not once per render — it does not re-run just because filtering/sorting shifted that same item to a new position. `<For>` hands the callback an `index` **accessor** (a function) specifically so consumers can read the item's current position later; calling it immediately (`i()`) and stashing the plain number in a closure throws that liveness away. Any handler built from that captured number (a click/hover callback that indexes back into the filtered array) goes stale the instant the array's composition changes without that item's own identity changing — the callback still runs, but against a now-wrong (sometimes out-of-bounds) slot, so it silently no-ops instead of throwing.

This bit `BaseRefDropdown` in `CreateWorktreeDialog.tsx`: typing a query that filtered out an earlier match shifted a later option to a new index, and clicking or hovering it used the stale pre-filter index, so the click silently did nothing. Fixed by computing the index at use-time (`filteredRefs().indexOf(option)`) instead of caching it, and by having the click handler act on the option directly rather than through an index at all. The sibling branch list in the same file already did this correctly (it calls the accessor `index()` inline inside each handler rather than capturing `index()` up front) — use that as the reference pattern. When a `<For>`-rendered row's event handler needs its position, read the index accessor (or `array.indexOf(item)`) at the moment the handler runs, never before.

## Architecture

All business logic in Rust. Frontend only renders and handles interaction — no data reshaping, computation, or process orchestration.

## Tri-State Inheritable Settings

Any per-repo or per-agent boolean setting that can inherit a global default (RepoWorktreeTab's file-handling/worktree/PR-visibility toggles, AgentsTab's per-agent overrides) is stored as `boolean | null`, `null` meaning "inherit," and rendered with the shared `TriStateToggle` (`src/components/shared/TriStateToggle.tsx`) — never a plain checkbox, which can only ever hold a concrete value and has no way to re-select "inherit" once touched.

**Resolution is always `override ?? globalDefault`** (see `src/stores/repoSettings.ts`'s `resolvers` map, e.g. `copyIgnoredFiles: (s, local) => s.copyIgnoredFiles ?? local()?.copy_ignored_files ?? repoDefaultsStore.state.copyIgnoredFiles`). Never invert this order — resolving `globalDefault ?? override` silently makes every explicit "Off" override unreachable whenever the global default is `true`. Any new inheritable field must go through `RepoSettingsEntry`/`AgentConfig`'s `Option<bool>` (Rust) / `boolean | null` (TS) shape and a `resolvers` entry, not a bare `bool`/`boolean` — a bare boolean silently drops the "use global" state.

## TUIC Protocol Markers (ack / intent: / suggest:)

Top-level sessions only. Subagents (Task tool) and in-process teammates must NEVER emit `ack`, `intent:`, or `suggest:` — `suggest:` is the end-of-task signal, so a subagent emitting it flips the *parent* session to `completed` mid-work. See `docs/user-guide/ai-agents.md#tuic-protocol--output-markers` for the full protocol and `src-tauri/src/mcp_http/mcp_transport.rs`'s `build_mcp_instructions_for_mode` / `cc_agent_hint.suggested_prompt` for where this is enforced today.

## Worktree Removal Safety

`git worktree remove`'s dirty-worktree and lock refusals are independent — never collapse them into a single `force: bool`. Branch deletion after a worktree removal must always use `git branch -d` (safe), never `-D`, regardless of how the worktree itself was removed. A destructive worktree action gated behind a confirm dialog must default Enter to Cancel (`defaultButton: 'cancel'`) — verify every dialog in the removal/archive/delete family sets this explicitly; don't assume a sibling dialog's fix covers all of them (`confirmRemoveLockedWorktree` shipped without it in the same commit that correctly set it on its two siblings). A heuristic detector (e.g. orphan = detached HEAD + no branch) must never drive an unrecoverable destructive action by default — give it an archive/move-aside default and require a separate, explicitly-labeled opt-in for a true hard delete.

**A worktree with a submodule checked out is a third refusal, but a plain `--force` DOES lift it** — `fatal: working trees containing submodules cannot be moved or removed`. Confirmed empirically (git 2.55.0): `git worktree remove --force` succeeds outright here, even on an otherwise-dirty worktree, so `RemovalMode::Dirty`/`::Forced` never even hit this refusal — their first attempt already passes `--force` and succeeds directly. Only `RemovalMode::Safe` (no force) hits it. It still fires *before* git's own dirty-worktree check though, so a Safe-mode caller can't rely on git to separately report dirtiness — `remove_worktree_internal` (`worktree.rs`) replicates that check itself via `git status --porcelain --untracked-files=all` at the worktree root (which git's own default submodule-summary behavior already extends to cover uncommitted/untracked content *inside* the submodule — verified: untracked files, modified tracked files, and a diverged submodule HEAD all show up as `M <submodule-dir>`) before retrying with `--force`. `git submodule deinit --force`/`--all --force` does **not** help on its own (the gitlink stays in the index/tree — `git ls-files -s` still shows `160000 ...` after deinit — so a bare retry without `--force` still fails identically); the fix that actually works is the plain `--force` retry. This was gotten wrong once already in-session — assumed the standard "deinit then retry" workaround worked without testing the simpler "just add `--force`" case first, and only a code review caught it. Before asserting a git force flag does or doesn't lift a given refusal, verify by actually running every relevant combination — don't reason from a flag's name, from partial testing, or from what a search result suggests.

## Window Geometry Restore

`main` is permanently denylisted from `tauri-plugin-window-state`'s `SIZE` flag (`lib.rs`
plugin registration) — it owns its own size/position/maximized/fullscreen persistence via
`window_geometry.rs` instead. **Never re-enable `SIZE` for `main`**: the plugin round-trips
through `set_size()`/`outer_size()`, which drift under `titleBarStyle: Overlay`, and
re-enabling it silently reintroduces a compounding visual regression on every restart.

`apply_window_geometry`'s measure-and-correct step (`corrected_size`, a one-step Newton
correction for the `set_size`-sets-inner/`outer_size()`-reads-outer drift) must stay bounded
by `is_frame_offset_plausible` (`MAX_TRUSTED_FRAME_OFFSET`, 256px). This is not a cosmetic
guard: `wait_for_geometry_to_settle` can return believing geometry has settled while it's
actually still reading the window's stale pre-resize size (a real, reproduced race against
an async compositor/WM, not just a theoretical Wayland concern — see `settle_loop`'s tests).
Without the plausibility bound, a single stale read feeds a wildly wrong "correction" back
through `record_size` into persisted geometry, and because each restart's correction is
computed relative to the previous (already wrong) saved value, the error compounds
**geometrically** across restarts. This produced a real corrupted `window-geometry.json`
(`width: 4944, height: 2368` on a `3456x2234` physical display — window far wider than the
screen, off the edge) despite the feature shipping with unit tests; the tests covered only
plausible/small offsets, never an implausible one.

`window_geometry_fix` (the `ensure_window_visible` safety net) must check for **three**
independent failure modes, not two: too-small, off-screen-by-center, AND larger than the
combined bounding box of every monitor. The oversized case is easy to miss — a corrupted
window's *center* can still land on-screen even though the window itself dwarfs the
display, so folding "too large" into the on-screen check misses it entirely (this is
exactly how the corrupted value above sailed through validation on every launch).

## IPC / HTTP Parity

**Every Tauri IPC surface MUST have an HTTP/WS equivalent, and the two MUST stay consistent.** The desktop app talks over Tauri IPC; browser/PWA/remote clients talk over HTTP+SSE+WS. They are two transports for the *same* backend — never let them drift.

- A new `#[tauri::command]` (request/response) → add the matching axum route + a `COMMAND_TABLE` entry in `src/transport.ts`, with a mapping assertion in `src/__tests__/transport.test.ts`. If a command is deliberately desktop-only, add it to `INTENTIONALLY_UNMAPPED` (don't silently leave it unmapped).
- **A `COMMAND_TABLE` entry with no route is now a test failure, not a runtime 404.** The table is TypeScript and the router is Rust, so the gate is split: the Vitest half executes every mapper and snapshots the paths to `src-tauri/src/mcp_http/command_table_paths.txt`; the Rust half (`command_table_paths_all_hit_a_registered_route`) `PATCH`-probes each one against `build_router`. Adding an entry fails Vitest first — regenerate with `pnpm vitest run src/__tests__/transport.test.ts -u`, then the Rust half tells you whether the route exists. Do not hand-edit the generated file. Full mechanics in `docs/api/http-api.md` → "Route Parity Gate".
- A new push (`AppHandle.emit`, `Channel<T>`, or per-stream broadcast) → bridge it: low-frequency lifecycle/progress events go on `event_bus` → `/events` SSE (add arms to `sse_routes.rs`); high-frequency token streams get a dedicated per-id WS (mirrors the PTY log-mode WS). Keep the desktop `emit` AND the bus/WS path — there is **no** bus→window forwarder, so producers **dual-emit**.
- Request/response shapes (field names, casing, payload structure) MUST be identical across IPC and HTTP so the same frontend store code works unchanged on both transports.

## PTY Command Injection

NEVER write text + `\r` directly to a PTY. Always use `sendCommand()` from `src/utils/sendCommand.ts` — it handles agent-specific Enter semantics (Ink raw mode needs split writes). This applies to dictation, command palette, suggested actions, and any other feature that sends input to a terminal.

## Agent Session Management

TUIC tracks each agent's session ID for resume-after-restart. Two strategies coexist:

**Discovery-based (Claude, Gemini, Codex, Grok).** TUIC does NOT inject `--session-id` at launch — the agent creates its own ID. TUIC discovers the active session and re-checks it on every idle↔busy transition and every 30s poll, so an agent that starts a replacement session is picked up. Resume uses `agentSessionId` (disk-discovered), not `tuicSession`.

Discovery has two tiers, and the difference is not cosmetic:

| Tier | Agents | Source |
|---|---|---|
| **Exact** | Claude, grok | the agent's own pid→session registry: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json`, `~/.grok/active_sessions.json`. `get_session_leaf_pid` returns the agent's pid (verified: it stays the agent even while a tool subprocess runs) |
| **Heuristic** | Gemini, Codex, and any Claude/grok too old to publish a registry | newest unclaimed session file under the project dir |

**The heuristic is not a binding, and no amount of tuning makes it one.** N agent tabs in one folder all scan the same directory, so whichever tab polls first takes the newest file regardless of whose it is; the rest take another tab's session or nothing. `claimed_ids` only stops two tabs holding the *same* id — it cannot tell whose is whose. That is issue #119: measured on a live instance, 3 of 6 Claude tabs held no id and one held a different tab's, so every tab resumed with `claude --continue` into the same conversation.

**Finding the id is only half of a resume — the other half is which store holds it.** A shell alias is expanded before `exec`, so a run config that reads `c2` with an empty `env` is not what runs: the process is `claude --dangerously-skip-permissions` under `CLAUDE_CONFIG_DIR=~/.claude-private`, and TUIC never sees the assignment. While the agent lives, discovery reads argv and env off the process and rebuilds the real command into `agentLaunchCommand`; at restore time the pid is gone and that string is the only record left. Do not re-derive the config dir from the run config: `c` and `c2` differ *only* in an env var neither one declares, so the default config verifies an id in `~/.claude` and then sends `--resume` to a binary that reads `~/.claude-private` — Claude answers `No conversation found with session ID`, and the transcript is sitting untouched in the other directory.

So when you add a discovery-based agent, look for a pid registry *first*. Codex 0.153 has none — `session_index.jsonl` carries only id/name/updated_at, the rollout `session_meta` has no pid, and there is no `--session-id` flag — so it stays heuristic on purpose, cwd-scoped by the rollout's recorded `cwd`. Gemini is worse and knowingly so: its scan visits every project's `chats/` dir, so it is not even cwd-scoped (see the `DEFERRED` note on `discover_gemini_session`). Do not close either gap by guessing a path-hashing scheme — verify against a real install.

**Forced injection (Goose).** Shell wrapper injects `--name $TUIC_SESSION` into `goose session/run` commands. The TUIC tab UUID IS the goose session name. Discovery returns `None` (SQLite storage, no filesystem scan). Resume uses `tuicSession`.

**No session tracking (Aider, Amp, Cursor, Droid, OpenCode, pi).** Either no local session files, cloud-only, or no UUID-based resume. `TUIC_SESSION` env var is available but unused.

When adding a new agent: choose discovery-based if the agent writes session files to disk (add `sessionDiscovery` to `agents.ts` and a Rust `discover_*_session` to `agent_session.rs`). Choose forced injection only when discovery is impossible (e.g., SQLite-only storage).

All of the above describes the **PTY** transport. `ego` does not use it — it runs over ACP and is deliberately not an `AgentType`. Read SPEC.md → "PTY versus ACP routing" before wiring any assistant that speaks a protocol instead of a terminal: the hybrid PTY/ACP route, and every fallback between the two, are rejected by contract rather than merely unimplemented.

## Logging

Use `appLogger` from `src/stores/appLogger.ts` — never `console.log/warn/error`. Check app logs via `GET http://localhost:9876/logs` (supports `?level=`, `?source=`, `?limit=` filters) before asking Boss for logs.

## Diagnostics

Runtime diagnostics for debugging performance issues. Code: `src-tauri/src/cpu_watchdog.rs`.

**Always on (zero overhead when idle):**
- CPU spike detection via `getrusage(RUSAGE_SELF)` — only the TUIC process, not PTY children
- Logs `CPU SPIKE` warning when >80% for 10+ consecutive seconds with full snapshot
- Sleep/wake detection — skips stale ticks after lid close/open

**Diagnostic mode (toggle at runtime):**

```bash
# Enable diagnostic mode
curl -X POST http://localhost:9876/diagnostics -d '{"enabled":true}' -H 'Content-Type: application/json'

# Check status
curl http://localhost:9876/diagnostics

# Read diagnostic logs
curl 'http://localhost:9876/logs?source=diagnostics'
```

When enabled, emits health snapshots every 30s and alerts on FD/thread growth trends. Each snapshot includes: CPU% (TUIC-self only, via `RUSAGE_SELF`), `children_cpu` (aggregate %cpu of PTY children + hottest child — the spike trigger deliberately ignores children, so this is the only place a hot `cargo`/agent surfaces when TUIC itself is calm), thread count, FD count, PTY session count, content index build state, semaphore permits, sessions with grid frames outstanding (`GridGate`), event bus subscriber count, `head_emits_suppressed` (repo-watcher `head-changed` emits skipped by the resolved-HEAD-target guard — a high/climbing value signals a filesystem-event storm, issue #82).

**Frontend liveness (always on, desktop only):** the WebView beats every 5s from its main thread (`frontendHeartbeat.ts` → `frontend_heartbeat` → `frontend_liveness.rs`); after six missed beats the diagnostics thread logs **once**:

```
Frontend unresponsive: no heartbeat for 30s — the WebView main thread is blocked or gone.
```

**Lost document (always on, desktop only):** a *second*, different white screen. The `webview-recovery` thread reads the main frame's URL every 15s (`webview_recovery.rs`); anything on the `about:` scheme means the app is no longer in the DOM, and it navigates back to the last healthy URL by itself, logging once. This is not the heartbeat's job and the heartbeat cannot see it: the app is gone rather than blocked, so it never beats from the blank document, and "never beat" is deliberately silent (`tuic-remote` has no WebView).

Observed twice on 2026-09-08, both times after the Mac went to standby with the display off: the main frame came back on **`about:srcdoc`** holding `<html><body></body></html>`, while the WebContent process was alive throughout (so it is *not* the `about:blank` WebContent-crash case) — under the memory-pressure sweep macOS runs while asleep.

Recover manually without losing PTY sessions — they live in the backend, not the WebView:

```bash
curl -X POST http://localhost:9876/debug/reload_webview   # navigates back to the app
```

**That endpoint navigates; it must never go back to `reload()`.** There is no URL behind `about:srcdoc` to reload, so the reload version answered `{"ok":true}` and left the window white for an hour.

**Memory: `GET /diagnostics/memory`** names which structure is holding the process's footprint — entry counts for every map that grows with sessions, clients or repos, measured bytes for the four that hold payloads, sorted biggest first, plus `phys_footprint_bytes` (resident *plus compressed*; `ps` RSS read 0.52 GB while the process held 40 GB). Exists because on 2026-09-08 the backend reached **40.7 GB** and could not be asked what it was holding: `leaks` found only 47 MB unreferenced, so it is live, reachable state — but a 40 GB process is not debuggable and every candidate had to be excluded by reading code. `accounted_bytes` far below the footprint means the growth is outside `AppState`.

Two traps this exists to close. **`grid frame gate stuck` is not a frontend-liveness signal** — a hidden terminal deliberately never acks (`CanvasTerminal.onFrame`), so it fires constantly in normal operation; reading it as "the WebView is wedged" is a false positive on every backgrounded tab. And **`freezeDetector.ts` cannot report a block that never ends**, because its `setInterval` runs on the thread it watches — which is why a five-hour white screen on 2026-09-08 left no frontend log at all and had to be diagnosed from the *absence* of lines. `/debug/invoke_js` is useless in that state for the same reason: it needs the stuck thread to run the script.

**When to enable:** Boss reports sluggishness, CPU spikes, or UI freezes. Enable it, reproduce the issue, then check the logs. The snapshot at the time of the spike tells you what subsystem is overloaded.

**Known past failure patterns this catches:**
- IPC flush loop (ack_terminal_frame sending frames in ack path → 240+ IPC/sec)
- Content index build saturating CPU on large repos
- grid frames outstanding on a session (WebView JS thread blocked)
- FD/thread leak (progressive growth without cleanup)
- Sleep/wake false idle cascades (tokio timers firing stale)

## The bottom zone is not agent output — never parse it

Below an agent's input box sits a status line **the user configures**: a Claude
Code `statusLine` command, a HUD plugin, a shell theme. Its height, glyphs and
wording are arbitrary, differ per install, and it may be absent entirely.

```
  ✻ Simmering… (5m 48s · ↓ 20.7k tokens)      ← agent output. Parse this.
  ─────────────────────────────────────────
  ❯                                           ← input box (2 rows)
  ─────────────────────────────────────────
  [Opus 5 (1M) | Team] ██░░ 22% | 📚 8        ← user's status line, ANY height.
  5h: 0% | 7d: 2% | $15.48 | 📅 $136.41         Ignore all of it.
  ◐ Bash: cargo test | ✓ Bash ×14
  ⏵⏵ bypass permissions on (shift+tab)        ← agent chrome. Also ignore.
```

**Rule: nothing at or below the input box may reach a parser.** Whatever is down
there is coincidence — a path reads as a plan file, a `?` as a question, a
numbered list as a choice prompt, `$15.48` as a token count. The agent's own
spinner sits *above* the input box, so trimming costs no signal.

Enforced by `chrome::find_chrome_cutoff`, applied to changed rows in `pty.rs`
before `parse_clean_lines`. It anchors on the input box and extends upward past
its padding. The unwindowed fallback accepts either a strict empty prompt or a
separator followed within four rows by a prompt; the latter preserves a draft
in a non-empty input box above an arbitrarily tall HUD without treating a lone
separator or markdown quote as chrome.

**When you touch that cutoff, the failure mode to fear is failing open:** no
anchor found returns `None`, and `None` means no trim, so *every* status-line row
reaches *every* parser. That is exactly what happened with a status line taller
than `CHROME_SCAN_ROWS` — silent and total. Hence the unwindowed fallback to the
lowest empty prompt row (`lowest_input_box_row`). Never widen the loose
`is_prompt_line` search: unwindowed it matches a markdown blockquote.

**Deliberate exceptions** — these read the full screen on purpose:

| Site | Why |
|---|---|
| `parse_slash_menu` | Claude Code v2.1+ renders autocomplete items *below* the prompt chrome |
| `parse_choice_prompt` | scans bottom-up for a strict dialog shape (title + ≥2 numbered options) |
| question dedup screen-absence check (`pty.rs`) | asks "is this prompt still visible anywhere", not "is this content" |

## Agent state detection — capture before you theorise

Working / idle / awaiting is decided from bytes an agent writes **once**. The
per-session output ring holds only the last 8 KB, which one Ink repaint overruns
in seconds, so by the time a wrong badge is reported the evidence is gone. Do not
reason about the code first — record the stream, then replay it.

```bash
curl -X POST localhost:9876/diagnostics/capture -H 'content-type: application/json' \
     -d '{"enabled":true}'                      # every session
     -d '{"enabled":true,"session_id":"<id>"}'  # one session
curl localhost:9876/diagnostics/capture         # state + bytes written per session
```

Captures land in `<config dir>/captures/<session-id>.tcap`, capped at 512 KB each. TUICCAP2 preserves the initial terminal rows/columns plus output/input direction, original chunk boundaries, ordering, and monotonic timestamps. The decoder remains backward-compatible with geometry-less TUICCAP1 and legacy output-only `.raw` fixtures; a faithful replay of either old format must supply the observed geometry explicitly rather than silently assuming 41x128.
Off by default (one relaxed atomic load per chunk when off) — code in
`src-tauri/src/pty_capture.rs`.

**A reproduced failure becomes a fixture, always.** Drop the `.tcap` in
`src-tauri/src/fixtures/agent_prompts/` and add a case to the
`Awaiting-signal fixtures` block in `pty/tests.rs`: it replays the capture
through `raw_stream_events` + `parse_clean_lines` + `suppress_heuristic_question`
— the same composition production runs, shared on purpose so a test can never
assert against a pipeline that does not exist. Unit tests on the individual
parsers were never the gap; the pipeline around them was.

**That rule is mechanical, not honour-system.** `scripts/hooks/pre-commit`
(installed by `make hooks` / `make dev`) blocks a commit that changes detection
logic without staging anything under `src-tauri/src/fixtures/agent_prompts/`.
It is deliberately narrow — only added/removed lines count, comments and blank
lines are stripped, and outside `chrome.rs` (detection end to end) a detection
symbol must be named by a changed line or by the hunk's enclosing function.
Touching `pty.rs` is not the trigger; touching `awaiting_input`, or any line
inside `suppress_heuristic_question`, is. Replayed over the last 120 commits
that touch a gated file it fired on 24 — every `fix(agent-state):` among them.

For a rename or a refactor that genuinely needs no capture, say so and move on:

```
TUIC_SKIP_FIXTURE_GATE=1 git commit ...     # or: git commit --no-verify
```

**Three signals report awaiting, and they are not interchangeable:**

| Signal | Source | Applies to |
|---|---|---|
| OSC 7770 `state=awaiting` | TUIC hook | hook-instrumented agents, **only** on `PreToolUse(AskUserQuestion)` |
| OSC 777 `notify` | agent's own desktop notification | any agent that emits it, any blocking prompt — but the body decides the confidence: `needs your permission` / `approval required` latch, `is waiting for your input` is low-confidence because Claude also sends it on its 60s idle timer |
| `Enter to select` footer regex | screen scrape | non-hook agents (dropped for hook-instrumented ones by `suppress_heuristic_question`) |

Busy/idle evidence is ranked within one submitted-turn epoch. Lower-ranked
evidence never closes a turn held busy by a protocol signal, and the same rule
protects protocol-ranked awaiting state from the `question-cleared` screen
backstop. A stable Ready screen may recover a lost protocol completion only
after `PROTOCOL_STALE_TIMEOUT` (five minutes) with no PTY output; that
exceptional transition logs `activity_source=protocol-stale` at warn level so a
missing completion hook remains observable.

| Rank | What it knows | Recorded by |
|---|---|---|
| `Silence` | nothing moved for a while | the silence timer |
| `Screen` | what the rendered screen currently looks like | ready/working screen adapters, **and OSC 133** — see below |
| `Process` | the process itself changed | `protocol-stale` only, today (#771-4733) |
| `Protocol` | this turn began or ended | OSC 7770 `state=`, Codex `notify` turn-complete, a submitted line on a ready-adapter agent |

**Rank is about what a signal knows, not how it travelled. Arriving in an escape
sequence does not make something Protocol rank.** OSC 133 is the worked example
and the mistake to not repeat: it is *shell* integration, so `133;C` fires when a
foreground command starts and `133;D` when it exits — on a long-lived TUI agent,
once at launch and once at death. It cannot tell one turn from the next, so it
records at `Screen` rank and a stable Ready screen is allowed to close it.
Ranking it `Protocol` strands the tab BUSY for the agent's whole lifetime, which
is issue #535-d4f5.

That distinction is easy to lose because `SilenceState::explicit_busy()` accepts
`osc133-busy` alongside `hook-busy`. It is a **provenance** predicate — an
explicit marker set this, rather than inferred screen/activity — and deliberately
*not* a rank predicate; its sources do not share a rank. Anything deciding
whether evidence may hold a turn reads `evidence.busy.rank`. Reading
`explicit_busy()` instead is exactly how a past commit came to widen the
`note_ready_screen` guard and then invert one of three byte-identical tests to
match (#745-8ff1). A `SilenceState` carries no agent type, so the three
`*_recovers_long_lived_shell_busy` tests must always agree; if one of them is
red, making the trio disagree is never the fix.

The footer regex anchors at **column 0 of the rendered row**, never the trimmed
text (`is_ink_dialog_footer_row`). A dialog is drawn full-bleed; everything an
agent streams is indented inside its own frame, so the indentation is the whole
difference between the footer and an agent quoting it. Trim first and an agent
that pastes a screen it just read marks *itself* awaiting, confidently, with
nothing to retract it.

A hook-instrumented agent showing a picker that is *not* AskUserQuestion (plan
pickers, skill menus, anything with `Type something` / `Chat about this`) reports
through OSC 777 and nothing else. Prefer protocol signals over screen scraping,
and parse them off the **raw** stream — the VT parser consumes escape sequences,
so they never reach the clean rows.

**Every signal that sets awaiting needs a path that clears it.** The badge is
`SessionState.awaiting_input`, not an event, and it is sticky by construction —
whatever sets it owns nothing until something retracts it. Four paths clear it,
and three of them wait for an event that may never arrive:

| Clear | Fires on | Misses when |
|---|---|---|
| `user-input` | a non-empty typed line | the answer is a bare Enter |
| `status-line` | a parsed busy tick (low-confidence only) | busy is inferred from screen movement |
| `resolve_choice_prompt_input` | an option keypress | no `choice_prompt` was ever set |
| `question-cleared` | silence timer sees the question gone from the screen | — (the backstop; low-confidence only) |

`question-cleared` is the backstop that catches the rest. It never touches a
confident question: grok repaints while it waits, so "not on screen this tick"
is not proof of an answer.

**The mirror failure is a SET that never comes back.** A multi-question
`AskUserQuestion` answers one sub-question at a time; each repaints its title and
options while the `Enter to select` footer stays byte-identical. The changed-rows
parser needs a row to *change*, so sub-questions 2+ produce no signal at all and
the tab reads "working" while the agent waits. `rearm_awaiting_for_open_dialog`
(`pty.rs`) closes it by reading that footer off the **full screen** as a presence
level, not an edge, and re-arming only when the badge is off — one event per
spurious clear, never one per repaint. Do not extend it to parse the title,
options or the `⊠ … ✓ Submit` tab bar: those all move as the wizard advances,
which is precisely why the footer is the key.

**Legacy output-only `.raw` fixtures cannot reproduce a latched badge.** New
`.tcap` captures include user input and can replay SET/CLEAR ordering, but the
`Awaiting RETRACTION` block must still drive the real event-bus accumulator and
assert `SessionState` — the thing a tab actually renders.

## Frontend performance instrumentation (`perfDebug`)

The frontend counterpart to backend Diagnostics. **One master flag gates ALL frontend perf/debug instrumentation** — `isPerfDebug()` from `src/utils/perfDebug.ts`.

- **Default = `import.meta.env.DEV`** → active in dev, **dormant in release**. We ship a quiet binary; we never distribute hyper-logging.
- **Runtime-toggleable, NOT tree-shaken** — a release build can be woken up to diagnose a field issue:
  ```js
  window.__TUIC__.setPerfDebug(true)   // persists to localStorage; starts the freeze detector
  window.__TUIC__.perfDebug()          // read current state
  ```
  (Run via the WebView devtools / MCP `debug invoke_js`. After toggling on in a build that started dormant, the freeze detector is (re)started automatically.)
- **Dormant cost:** a single boolean read at each entry point — negligible even per-frame.

**What it gates** (all in `src/utils/`): `markPerf`, `timeSync`, `timeBatch` (`perfTrace.ts`), `noteFrameRequest` (frame-burst detector), and `startFreezeDetector` (`freezeDetector.ts`). `frameTiming.ts` is a **heavy opt-in sub-harness subordinate to this master gate** — it has its own local enable (`__terminalFrameTiming.enable(true)`), but cannot record unless `perfDebug` is also on.

**RULE for all future perf/debug instrumentation:** gate it on `isPerfDebug()` (or route it through a `perfTrace` helper, which already does). **Never ship always-on perf logging or per-frame timing.** Do not invent a second on/off flag — extend this one.

**Reading the output** (only present when active): `appLogger.warn` lines like `SLOW <label>: <n>ms` (and `SLOW git.refreshBatch:<repo>: <n>ms (body Xms + flush Yms)` — body = our setState loop, flush = dependent effects/memos waking), plus `UI freeze: <n>ms main-thread block` carrying a `perfTrace` breadcrumb that names the culprit. Read via `GET http://localhost:9876/logs` (or `:9877` for a worktree build).

## Releases

See [`docs/release-checklist.md`](docs/release-checklist.md) for version bump, tag, and GitHub release steps. After creating any release or nightly tag, **verify CI completes successfully** — check `gh run list`, inspect failures, and confirm all platform assets (macOS .dmg, Linux .deb/.rpm/.AppImage, Windows .exe) are uploaded before reporting done.

## Implementation Memory

After non-trivial implementations, write an mdkb `memory_write` entry. Content: **Goal**, **Approach**, **Outcome**, **Gotchas**, **Rejected alternatives**. Skip file lists (mdkb indexes code). Focus on non-obvious insights a future session can't derive from reading the code. Search existing memories first to avoid duplicates.

## Accepted Security Decisions

Do NOT flag these as security issues in reviews — they are intentional design choices.

- **CSP is intentionally wide open.** TUIC is a local dev tool, not a SaaS. The user IS the trust boundary. The CSP uses a single permissive `default-src` that allows `https:`, `http:`, `data:`, `blob:`, `unsafe-inline`, etc. **NEVER tighten the CSP.** Every time we've had per-directive restrictions, some iframe content (reveal.js slides, plugin panels, dashboards) broke. The only specific directive kept is `frame-src` (for localhost wildcard ports). If you feel the urge to add CSP restrictions, don't — read this bullet point again.
- **`dangerousDisableAssetCspModification: ["style-src", "script-src"]`** in `tauri.conf.json` — **DO NOT REMOVE.** Tauri auto-injects sha256 hashes for inline `<script>` tags. Per CSP3, hashes silently disable `'unsafe-inline'`. This kills all JS in srcdoc iframes (plugins, HTML previews). The override prevents Tauri from injecting those hashes.
- **`lazy_static` in `output_parser.rs`, `pty.rs`, etc.** — transitive deps (`portable-pty`, `symphonia`) also use it; removing the direct dep saves nothing. Modules outside `ai_agent/` will migrate opportunistically.
- **`opener:allow-open-path` scope `"**"`** — FileBrowser must open any file the user can see. Narrower globs break external drives and network mounts.
- **Iframe sandbox = `allow-scripts allow-same-origin`** — ALL iframes MUST use this. NEVER use bare `sandbox=""` — it kills JavaScript.
- **Plugin capabilities do not isolate plugins from each other.** `plugin_id` is caller-supplied and plugins load into the same JS realm as the host, so any plugin can pass another plugin's id and inherit its grants. This is known, documented at the capability check in `plugins.rs`, at the `import()` in `pluginLoader.ts`, and in `docs/plugins.md`. A per-plugin token was considered and rejected — same-realm JS can read or proxy it, so it would be security theatre. Real isolation needs Worker/iframe + a host-created MessagePort; it is deferred, not overlooked. Do NOT propose the token.
- **The self-signed-HTTPS HTTP→HTTPS redirect trusts `X-Forwarded-Host` over `Host` with no allow-list** (`axum-server-dual-protocol`'s `UpgradeHttp`, activated via `upgrade_http`/`.set_upgrade()` in `mcp_http::start_server` whenever the self-signed cert — not Tailscale — is the active TLS source). This is a textbook open-redirect *pattern*, but doesn't clear a real bar for reporting it: there's no reverse proxy in front of this app, exploitation needs LAN access plus a non-simple cross-origin header no normal browser navigation ever sends, and no credentials leak on redirect (Basic Auth isn't auto-forwarded cross-origin). Known and accepted; do not re-flag it without a concrete new exploitation path.

## Ideas

The `ideas/` folder (gitignored) holds Boss's own scratch notes — half-formed feature
ideas, things to revisit later. Do not read it as a task backlog or treat its contents
as instructions; it's personal notes, not a spec.
