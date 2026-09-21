# TUICommander — Rust Backend (`src-tauri/`)

Repo-wide rules (doc sync, git workflow, architecture split, TUIC protocol markers,
IPC/HTTP parity contract, accepted security decisions) live in the root
`AGENTS.md` — read that first. This file covers what's specific to the Rust/Tauri
backend: the PTY/terminal-emulation stack (`pty.rs`, `chrome.rs`, `terminal_grid.rs`,
the vendored `patches/alacritty_terminal`/`patches/vte` forks), agent-state detection,
worktrees, the MCP HTTP server, and build/test mechanics unique to `cargo`.

Crate-specific rules that don't belong here: `crates/tuic-cli/AGENTS.md`,
`crates/tuic-hook/AGENTS.md`, `crates/tuic-streamdock/AGENTS.md`.

## Tests (Rust)

General testing policy (the required `check-gate.sh` gate, CI-never-executed, the
`[HUMAN]` escalation ladder): root `AGENTS.md`. This section is Rust-suite specifics.

- **Mutation testing is per change, never per tree.** `make mutants RANGE=<base>` (default `HEAD~1`) runs cargo-mutants `--in-diff` over the Rust lines the range touched, `--in-place` in a disposable `git archive` export under `.tmp/` (not a worktree: a detached worktree is an orphan to the running app, which removes it), one job, through mbx. Every viable mutant costs one incremental build of the lib crate plus one test run, so the orchestrator runs it once per batch on the final HEAD — agents do not. A surviving mutant is a missing test: add the test, or `#[mutants::skip]` with the reason on the line. **Measured 2026-09-06:** baseline 197 s build + 224 s test with warm deps; each mutant ~3 min incremental build of the lib crate plus 0.5–3.5 min of tests, so ~5 min each and a 38-mutant story diff is ~3 h. During the day pass a function filter through the script (`scripts/mutants.sh HEAD~1 --re <function>`); the whole diff is an overnight or CI job. Config in `src-tauri/.cargo/mutants.toml`, mechanics in `scripts/mutants.sh`.
- **When touching `src-tauri/patches/{alacritty_terminal,vte}/`, verification MUST include `cargo nextest run --workspace` (or `make check`), not a package-scoped `cargo test --lib`/`cargo test -p tuicommander`.** The vendored crates are separate workspace members with their own regression suite (`patches/alacritty_terminal/tests/ref.rs`, ~44 fixture-replay tests) that a package-scoped run silently skips. `cargo nextest run`/`list` without `--workspace` also silently scopes to zero tests for a vendored crate instead of erroring — this exact mistake produced a false "these tests were never wired in" diagnosis in commit `47217d2c`'s own message. A background-color-erase fix landed in `6dd165f5` without running the workspace suite and shipped two regressions caught only later.
- **A second, independent trap in `vte` specifically: its `ansi` Cargo feature is not in `default = ["std"]`.** `cargo test -p vte` (or `cargo nextest run -p vte`) alone silently compiles ZERO of `src/ansi.rs`'s `mod tests` — the module itself, `mod ansi;`, is declared `pub mod ansi;` unconditionally in `lib.rs`, but everything inside it (including the `Handler`/`Processor` types most OSC-parsing tests actually exercise) is behind `#[cfg(feature = "ansi")]` at the crate level. `cargo test -p vte --features ansi` runs them; so does `cargo nextest run --workspace`, since `alacritty_terminal`'s own `[dependencies.vte]` unconditionally requests `features = ["std", "ansi"]`, which Cargo's feature unification applies workspace-wide. Confirmed 2026-09-15 while adding OSC 133 parse tests to `ansi.rs`: a bare `cargo test -p vte` reported "32 passed, 0 failed" and looked completely healthy while running only `lib.rs`'s generic OSC/CSI tests, not a single one of `ansi.rs`'s OSC-1337/OSC-133-specific tests. Same failure shape as the `--workspace` gotcha above — a scoped run that reports a clean pass while silently excluding the tests you actually meant to run — just triggered by a feature flag instead of a workspace boundary.
- When resolving a rebase/merge conflict in a Rust function by taking one side's body wholesale, diff the full field set of any struct it returns — a dropped field can compile cleanly (mocked in the caller's own tests) while silently regressing a feature only an end-to-end test would catch. Prefer merging the logic, not picking a side outright, when the two versions diverge structurally.

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
   **This is not fresh-worktree-only** — confirmed 2026-08-31: `check-gate.sh`'s own
   clippy-unknown-lint NOTE tells you to run `rustup update` when the local toolchain is stale,
   but a toolchain bump invalidates the already-built `tuic-hook` binary. `cargo nextest run`
   then rebuilds it implicitly as part of the test binary graph, and the exact same
   `golden_wire_output` empty-output failure mode reappears afterward, in a worktree that had
   already passed this gate once before the `rustup update`. Same fix: `cargo build --package
   tuic-hook` (from `src-tauri/`), then re-run.

   **Also confirmed on a fresh worktree's very first full `check-gate.sh` run (2026-09-10):**
   an explicit `cargo build --package tuic-hook` right before `check-gate.sh` (per steps 1-3
   above) is not sufficient insurance — `check-gate.sh`'s own defensive rebuild step reported
   the binary up to date (correct, real size confirmed via `ls -la` immediately before), but by
   the time `cargo nextest run --workspace` actually executed the `golden_wire_output` tests
   later in the same run, `target/debug/tuic-hook` had gone back to 0 bytes again — most likely
   a from-scratch, fully-parallel workspace build racing `tuic-hook`'s own link step against the
   `tuicommander` test binary's build. `cargo build --package tuic-hook` a second time
   afterward, then re-running `cargo nextest run` (whether the full `--workspace` or scoped to
   `-p tuicommander agent_hook::tests::golden_wire_output`), passed cleanly and stayed real
   across repeated `ls -la` checks with no further intervening build. If `check-gate.sh` fails
   only on `agent_hook::tests::golden_wire_output::*` on a worktree's first-ever run, don't
   spend time root-causing the race further — rebuild `tuic-hook` and re-run the whole gate
   once; a second clean pass is expected and confirms it wasn't a real regression.
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

**Committing a change under `plugins/` needs TWO commits, in two separate git histories.**
`cd plugins && git add ... && git commit` lands a commit inside the submodule's own repo
(likely detached-HEAD, since worktrees don't check the submodule out onto a branch) — this is
what actually holds the new/changed plugin file. The parent repo's own `git add plugins &&
git commit` then only records that new commit SHA as the submodule's pinned pointer; it does
not carry the plugin's file contents itself. Neither commit is visible to another clone (or to
`github.com/sstraus/tuicommander-plugins`) until the **submodule's own commit** is pushed to
that remote separately — this is a manual step that isn't implied by, or done as part of,
committing/pushing the parent repo. If you add or edit a plugin, don't forget the submodule-side
push, and don't assume the parent repo's own push covers it.

**A plugin living under this repo's `plugins/` submodule is NOT auto-loaded by a running
instance.** That directory is only the source/distribution copy (mirrors the public
`tuicommander-plugins` repo, feeding the registry/release-zip pipeline) — an app instance loads
plugins from `{config_dir}/plugins/{id}/` (e.g. `~/Library/Application Support/com.tuic.commander/
plugins/{id}/` on macOS), a separate location entirely. To manually exercise a plugin you just
added or edited in `plugins/{id}/`, use Settings → Plugins → "Install from folder" pointing at it,
or copy the directory into the config dir's `plugins/` folder yourself, then enable it.

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

**`target/debug/incremental` can silently grow to 10+ GB over a long session of repeated rebuilds and fill the disk, producing a failure that looks like a toolchain/compiler bug, not a disk-space problem.** Confirmed 2026-09-15: a `cargo test --doc` failed with `rustc-LLVM ERROR: IO failure on output stream: No space left on device` and `could not compile ... (lib)` — the actual cause was `target/debug/incremental` alone at 13 GB, on a disk down to ~300 MiB free after a long multi-phase Rust implementation session. `rm -rf target/debug/incremental` is safe (purely a recompile-speed cache; the next build is just slower, not a full from-scratch rebuild of everything) and immediately unblocked the build. `df -h /` before assuming a compiler regression if you see an IO-failure-shaped error mid-build with no code change that plausibly explains it; `du -sh target/debug/incremental` to confirm before deleting.


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


**Terminal keydown vs. global shortcuts** (a Ctrl/Cmd + printable-key interaction
in `src/components/Terminal/terminalInput.ts`): see `src/components/Terminal/AGENTS.md`.

## macOS Finder Service ("New TUICommander Tab Here")

Right-click a folder/file in Finder → a terminal pane opens there via a placement ladder (owning
repo/worktree → active repo → ask the user). Chain: Finder → the bundle's shell action → `tuic
open-here <paths...>` → `tuic://open-terminal` deep link. See `docs/user-guide/finder-integration.md`
and `FEATURES.md` §17.4.2 for user-facing behavior; `docs/sync-matrix.md`'s matching row for what to
update when touching it.

**The Service bundle (`src-tauri/services/*.workflow/`) is a hand-authored Automator document, not
Automator-generated — verify any change to it with the `automator` CLI, not just `plutil -lint`.**
`plutil -lint` only proves the plist is well-formed XML; it says nothing about whether Automator's
runtime will actually execute the workflow. The `automator` CLI (ships with macOS) runs a `.workflow`
bundle directly and is the fastest way to prove a change actually works, fully scriptable, no Finder
GUI or right-click needed:
```bash
# single item
automator -i /path/to/folder "src-tauri/services/New TUICommander Tab Here.workflow"
# multiple items (the -i flag only takes one; use -i - with newline-separated stdin for a selection)
printf '%s\n%s\n' /path/one /path/two | automator -i - "src-tauri/services/New TUICommander Tab Here.workflow"
```
This is how the shipped bundle was actually verified (single item, multi-item selection, and a plain
file) before being committed — not just plist-linted. Reuse this technique for any future Automator
Service/Quick Action work in this repo rather than reasoning about the `.wflow` schema from memory.

**`automator -i` passing does NOT mean the Service works from Finder's real right-click menu — it
bypasses Gatekeeper's Services-menu dispatch entirely.** A 2026-09-14 bug report ("The Service cannot
be run because it is not configured correctly" from a real Finder right-click) traced to the
installed bundle having no code signature at all (`codesign -dv` → "code object is not signed at
all"), which Gatekeeper (`spctl --status` → assessments enabled) rejects when Finder's Services
menu dispatches a third-party Automator "Run Shell Script" action — but `automator -i` against the
same unsigned bundle runs it just fine, because it never goes through that Gatekeeper-gated path.
Fixed in `finder_service.rs`'s `install_into`: ad-hoc sign the bundle right after copying it
(`/usr/bin/codesign --force --deep --sign -` — absolute path, matching the sibling `pbs` call in
this same file), best-effort so a missing `codesign` or an unsignable bundle never fails the
install (though the caller now logs whether signing actually succeeded alongside the "Finder
Service installed" line, so a real failure isn't only a buried separate warning). **`--deep` is
required, not just defensive** — a `/code-review` pass suggested dropping it per Apple's TN2206
guidance (which generally warns against `--deep` for developer-authored signing), but testing a
fresh copy without it fails outright: `codesign` reports the bundle as still "not signed at all",
naming `Contents/document.wflow` as an unsigned "subcomponent" it refuses to seal without `--deep`.
If you touch this signing step again, verify any change against a fresh copy of the real bundle,
not just the guidance's general advice. If you change this bundle again, `automator -i` proves the
workflow logic is correct, but only a real Finder right-click (or `codesign -dv` on the installed
copy showing a signature) proves it will actually run from the Services menu.

**A stale second copy of TUICommander.app sharing the same bundle id silently breaks every
`tuic://` deep link (including `open-here`) while showing no error at all.** Also found
2026-09-14: with both `/Applications/TUICommander.app` (an older install) and a locally-built
`make dev`/release copy present on disk, both registered `com.tuic.commander` /
`CFBundleURLSchemes: [tuic]` with Launch Services, and macOS resolved `open 'tuic://...'` (which
is exactly what `tuic open-here`/`tuic_cli::open_deep_link` calls) to the stale copy instead of
the one actually running — the URL was silently dropped, with zero log output on either the Rust
or frontend side, even for a deep link that should trigger the JS handler's "unrecognised command"
warning. Confirmed via `open -a <the exact running app bundle path> 'tuic://...'`, which delivered
and logged instantly, proving the deep-link plumbing itself was never the problem. This is an
install-hygiene issue, not a code bug, and there's no code fix for it — if a `tuic://` link (or
the Finder Service, which goes through the same CLI call) appears to silently do nothing while the
app is definitely running, check for a duplicate `TUICommander.app` elsewhere (`/Applications` is
the usual culprit next to a dev build) before assuming the deep-link code regressed. See "Test
instance vs orchestrator instance" above — routinely running a second instance for testing is this
repo's normal workflow, which is exactly what makes this collision easy to hit by accident.

**`tuic://open-terminal` deliberately has no confirmation dialog**, unlike `open-repo`'s unknown-path
branch. This was a considered decision, not an oversight — do not "fix" it by adding one. Rationale:
spawning a pane starts an idle shell and executes nothing; `tuic new` already has zero confirmation
for the same reason; and both the CLI and the frontend deep-link handler independently cap at 5 panes
per invocation, so a crafted/repeated link can't fan out unboundedly. The one outcome that DOES mutate
persistent state — registering a new repo — is only reachable through an explicit click on the
picker dialog's "Add this folder as a repository" button; that click is the confirmation.


## MCP `initialize` Instructions: Per-Agent Config Tests Must Be `#[serial_test::serial]`

`build_mcp_instructions`/`build_mcp_instructions_for_mode` (`mcp_transport.rs`) resolve
several things per-agent-type off disk via `crate::config::load_agents_config()` —
`marker_flags_for_agent` (intent/suggest), `resolve_prefer_tuic_spawning`,
`resolve_prefer_tuic_messaging` — keyed by `resolve_agent_type(client_name)`, which maps
`"claude"`, `"claude-code"`, AND `"tuic-bridge"` all to the single agent type `"claude"`.
Any test that calls `build_mcp_instructions` with a Claude-ish `client_name` (including
`Some("claude-code")`, the value the CC-only-bullet test uses) reads whatever
`CONFIG_DIR_OVERRIDE` happens to be active *right now* — a single un-scoped global `static`
(`config.rs`) — even if that test never calls `set_config_dir_override` itself. If another
test elsewhere in the suite is mid-flight with an override that sets `prefer_tuic_spawning:
Some(false)` for `"claude"`, a concurrently-running non-serial test observes it and produces
a spuriously wrong result. `set_config_dir_override`'s own doc comment says it serializes
"callers" of itself, which undersells the actual hazard — the tests that need protecting are
every test that reads a per-agent config value for a Claude-family client, not just the ones
that write one. Confirmed empirically: `instructions_single_isolated_task_bullet_only_for_claude_code_client`
(pre-existing, calls `build_mcp_instructions(&state, Some("claude-code"))`, no override of
its own) started failing intermittently the moment sibling tests started overriding
`prefer_tuic_spawning` for `"claude"`, and was fixed by adding `#[serial_test::serial]` to
it too. When adding a new per-agent disk-config field read inside `build_mcp_instructions`,
audit every existing test that passes a Claude-family `client_name` and add the annotation
where it's missing — don't assume "I didn't touch that test" means it's safe.


## Git Ref Enumeration — Never Classify a Ref by Its Short Name

Any code that parses `git branch -a`/`for-each-ref` output to classify refs (is this
remote? is this the synthetic HEAD entry?) must make that decision from the **full**
refname (`%(refname)`, e.g. `refs/remotes/origin/main`), never the short name
(`%(refname:short)`, e.g. `origin/main`). Two real bugs, one already shipped, both traced
to this exact mistake (`git.rs`, fixed 2026-09-10):

- `is_remote = name.starts_with("origin/")` on the short name (`get_git_branches`)
  misclassified a local branch literally named `origin/foo`, and never detected a remote
  added under any name other than `origin` (a common fork-workflow shape, e.g.
  `upstream`). Fixed: `refname.starts_with("refs/remotes/")` on the full refname.
- **Every normally `git clone`d repo has a `refs/remotes/<remote>/HEAD` symref**, and
  git's `%(refname:short)` collapses this symref's short name down to just the remote's
  own name — `"origin"`, not `"origin/HEAD"` — for any remote name. A filter written as
  `name.ends_with("/HEAD")` on the *short* name (`get_branches_detail_impl`'s original
  "skip the synthetic origin/HEAD pointer" check) never matches this ref, so it leaked a
  phantom branch literally named after the remote into both the branch switcher
  (`BranchSwitcher.tsx`) and the Git Panel's Branches tab (`GitPanel/BranchesTab.tsx`) —
  for any ordinary cloned repo, not an edge case. This shipped for a long time undetected:
  an existing "real repo" test ran against this exact checkout (which has this exact
  symref) but asserted the wrong condition and silently passed. Fixed:
  `refname.starts_with("refs/remotes/") && refname.ends_with("/HEAD")` on the full
  refname.

Also watch for a **detached-HEAD state** (checked-out commit/tag, mid-rebase,
mid-bisect): `git branch -a` emits a synthetic pseudo-entry like
`(HEAD detached at abc1234)` whose fields aren't a real ref at all and contain spaces
(breaking any parser that assumes a ref name has none). `for-each-ref` is immune to this
by construction (it only ever walks real refs matching the given pattern), which is one
more reason to prefer it over `branch -a` for anything beyond a quick local-branch-name
listing.

**Testing note:** `git remote add` + `git fetch` does **not** create the remote's HEAD
symref — only `git clone` does that automatically. To exercise this class of bug in a
test fixture, explicitly run `git remote set-head <name> -a` after the fetch, or the test
will silently never hit the code path it's meant to guard (this happened once already in
this exact session — a code review had to point out the test setup was avoiding the
exact scenario it claimed to cover).


## Worktree Removal Safety

`git worktree remove`'s dirty-worktree and lock refusals are independent — never collapse them into a single `force: bool`. Branch deletion after a worktree removal must always use `git branch -d` (safe), never `-D`, regardless of how the worktree itself was removed. A destructive worktree action gated behind a confirm dialog must default Enter to Cancel (`defaultButton: 'cancel'`) — verify every dialog in the removal/archive/delete family sets this explicitly; don't assume a sibling dialog's fix covers all of them (`confirmRemoveLockedWorktree` shipped without it in the same commit that correctly set it on its two siblings). A heuristic detector (e.g. orphan = detached HEAD + no branch) must never drive an unrecoverable destructive action by default — give it an archive/move-aside default and require a separate, explicitly-labeled opt-in for a true hard delete.

**A worktree with a submodule checked out is a third refusal, but a plain `--force` DOES lift it** — `fatal: working trees containing submodules cannot be moved or removed`. Confirmed empirically (git 2.55.0): `git worktree remove --force` succeeds outright here, even on an otherwise-dirty worktree, so `RemovalMode::Dirty`/`::Forced` never even hit this refusal — their first attempt already passes `--force` and succeeds directly. Only `RemovalMode::Safe` (no force) hits it. It still fires *before* git's own dirty-worktree check though, so a Safe-mode caller can't rely on git to separately report dirtiness — `remove_worktree_internal` (`worktree.rs`) replicates that check itself via `git status --porcelain --untracked-files=all` at the worktree root (which git's own default submodule-summary behavior already extends to cover uncommitted/untracked content *inside* the submodule — verified: untracked files, modified tracked files, and a diverged submodule HEAD all show up as `M <submodule-dir>`) before retrying with `--force`. `git submodule deinit --force`/`--all --force` does **not** help on its own (the gitlink stays in the index/tree — `git ls-files -s` still shows `160000 ...` after deinit — so a bare retry without `--force` still fails identically); the fix that actually works is the plain `--force` retry. This was gotten wrong once already in-session — assumed the standard "deinit then retry" workaround worked without testing the simpler "just add `--force`" case first, and only a code review caught it. Before asserting a git force flag does or doesn't lift a given refusal, verify by actually running every relevant combination — don't reason from a flag's name, from partial testing, or from what a search result suggests.


## Worktree File Sync (copy_ignored_files / copy_untracked_files / copy_paths)

`git worktree add` only ever checks out tracked, committed content — copying anything else
(ignored/untracked files, or a repo's explicit `copy_paths` list) into a freshly created worktree
is a separate step, done by `worktree_sync.rs` + `worktree::spawn_worktree_file_sync`, run in the
background right after the worktree is created (both the desktop `create_worktree` command and the
MCP HTTP `create_worktree_shared` path call it, resolving effective settings themselves via
`config::resolve_effective_copy_settings` — no frontend involvement needed). Before 2026-09-10,
`copy_ignored_files`/`copy_untracked_files` were fully plumbed through persistence, per-repo
tri-state resolution, and the settings UI — and had **zero consumer anywhere**, so the toggles did
nothing. Don't assume a setting that's cleanly wired through config+UI is actually doing anything —
trace to a real consumer.

**Every path this module touches must be checked against symlinks planted in `dest`, not just the
final component.** `dest` is `git worktree add`'s checkout of whatever branch was requested — which
can be an attacker-influenced PR `head_ref` (the same threat model as the `--` end-of-options guard
in `create_worktree_internal`). A malicious branch can commit a directory symlink at any
*intermediate* path component (e.g. a directory named `config`, `node_modules`, or anything matching
this repo's own `copy_paths` entries); plain path joins and `fs::create_dir_all`/`fs::copy` follow
symlinks in every component except the final one, so a naive "does the final destination already
exist" check is not enough — it lets a synced file get written through the planted symlink to
wherever the branch pointed it, using this (trusted) repo's own content. `sync_one`'s
`first_symlinked_ancestor` check exists specifically for this — walk every intermediate component of
`dest.join(rel)` and refuse if any of them is already a symlink, before doing anything else. Any
future code that writes into a path freshly checked out from an untrusted branch needs the same
intermediate-component check, not just a final-leaf existence check.

An explicit `copy_paths` entry is user-authored (typed into the Settings UI only) and has
**no `.tuic.json`/global tier** — unlike every other worktree setting — specifically so a malicious
committed `.tuic.json` can never inject its own entries into it. `sync_one` also rejects a path of
`.` (the repo root) or containing a `.git` component: an explicit entry copying "." would otherwise
recursively copy the entire source repo — including its real `.git` — on top of the new worktree's
own linked-worktree `.git` *file*, corrupting it.


## Worktree Warming (`warm_ignored_directories`) — Background, Bounded-Parallel

Warming (clonefile-copying the parent's git-ignored build directories — `node_modules`,
`target`, etc. — into a freshly created linked worktree so it starts warm; see `cow.rs`)
used to run **synchronously inside `worktree::create_workspace_with`**, before
`CreatedWorkspace` was returned — on a repo with real `node_modules`/`target` this
blocked worktree creation for tens of seconds (~38s measured on this repo, 83k inodes).
Fixed (2026-09-21) by moving it into `spawn_worktree_setup_chain` as that chain's new
**first** stage (chain order is now warm → file-sync → setup script), gated by a new
tri-state setting `warm_ignored_directories` (`config::resolve_effective_warm_setting`,
same 3-tier resolution as `copy_ignored_files`, default `true` — a pure opt-out, not an
opt-in, so existing repos keep warming unless someone turns it off).

**`cow::warm_candidates_concurrent` fans out each candidate directory's copy across a
`WARM_COPY_CONCURRENCY`-capped (`4`) `Arc<tokio::sync::Semaphore>` + `tokio::task::JoinSet`
— but the pre-existing skip-rule pre-pass (symlink-ancestor check, `.git`-holding-directory
skip, destination-containment check, `to.exists()`) stays a single-threaded loop that runs
to completion BEFORE any dispatch begins.** This is load-bearing, not an implementation
detail: it means every check gates every candidate exactly the same way it did when
warming was sequential — nothing moved to the wrong side of the concurrency boundary. If
you touch this function again, keep that ordering (all skip-rules evaluated, `to_dispatch`
fully built, `on_started(total)` fired, only THEN does the `Semaphore`/`JoinSet` fan-out
begin) — interleaving a check with the concurrent copies would reopen exactly the class of
symlink-planting attack `first_symlinked_ancestor` (in the sibling file-sync module, same
threat model) exists to close.

**Warming gets its own sibling poll status (`WorktreeWarmStatus`/`worktree_warm_status`),
not a merge into the existing `WorktreeSetupStatus`/`worktree_setup_status`.** Considered
folding warm info into the setup-script status so a caller could learn "is this worktree
fully ready" in one call, but rejected: warm and the setup script are independently-lifecycled
stages with different outcome shapes, and the file-sync stage *doesn't* get its own poll
endpoint either — only the setup script did, because that's the one stage whose outcome a
caller needs to branch on programmatically. Warm now needs the exact same treatment (did
`node_modules` actually arrive?), so it gets the same one-stage-per-endpoint pattern rather
than widening `WorktreeSetupStatus`'s.

**The creation response's `instructions.warm_artifacts` no longer carries a synchronous
`warmed_directories` count** — a breaking response-shape change for any existing caller that
read it (see the same section's history for the desktop `create_worktree` command, the MCP
HTTP `create_worktree_shared` route, and the `repo action=worktree_create` MCP tool). It now
reports `{present, status: "pending", poll, note}`, pointing at the new
`repo action=worktree_warm_status` / `GET /worktrees/warm-status?repoPath=&branch=` instead.
`cow::warm_artifacts()` (the separate, disk-derived presence/size check used to build
`present`) is unaffected — it re-derives from disk each call and never depended on the
synchronous warm step having run.

**`mcp_http/session.rs`'s `create_session_with_worktree` (`/sessions/worktree`) gets warming
as a side effect, with no changes needed there** — it already called
`spawn_worktree_setup_chain` (for the file-sync fix documented above), so once warming moved
into that same chain, this third creation path started warming too, closing a gap it never
even had test coverage to notice (it went from "doesn't warm" to "warms" silently). Verified
by `create_session_with_worktree_also_warms` in that file's test module — if you add a
FOURTH creation path, make sure it either goes through `spawn_worktree_setup_chain` too, or
gets an equivalent explicit test proving it does (or doesn't) warm; don't assume.

**Security fix (2026-09-21): `warm_candidates_concurrent`'s skip-rule pre-pass was missing
the intermediate-symlink guard `worktree_sync.rs::sync_one` already has for the identical
threat model.** A malicious branch (the same `head_ref`-influenced threat model as the `--`
end-of-options guard in `create_worktree_internal`) could commit a directory symlink at any
intermediate component of a path matching one of the *parent's own* ignored-directory names
(e.g. a tracked symlink named `vendor` standing in for what the parent has as a real
directory containing `vendor/cache`) — `git worktree add` checks it out into `dest` as a real
symlink, and without a guard, warming's `create_dir_all`/copy would follow it and write the
parent's real ignored build content through to wherever the branch pointed, using this
(trusted) repo's own content. Fixed by making `worktree_sync::first_symlinked_ancestor`
`pub(crate)` and calling it from `warm_candidates_concurrent` too, exactly like `sync_one`
does — reject (as a warning, not a hard error; warming is best-effort) rather than follow.
Regression test: `cow::tests::warming_refuses_to_follow_a_symlinked_intermediate_component_in_dest`.
**If you add a third writer into an untrusted, freshly-checked-out `dest`, it needs this same
guard — don't assume "we already fixed this class of bug" covers a sibling module that
writes into the same kind of path independently.**

**Another instance of "Which timing assertions are load-bearing" (see that section below):**
`cow::tests::warming_runs_concurrently_not_sequentially` originally asserted
`elapsed < 500ms` to prove concurrent dispatch is faster than sequential would be —
reliable in isolation, but observed failing at 2.35s elapsed under `cargo nextest run
--workspace`'s full parallel load (thousands of tests contending for CPU). No wall-clock
margin fixes this; the OS scheduler simply isn't guaranteed to run the 4 capped threads
within any fixed window when the machine is this loaded. Fixed by dropping the timing
assertion entirely in favor of the same `max_in_flight` atomic-counter technique its sibling
`warming_never_exceeds_the_concurrency_cap` already uses — asserting `max_in_flight > 1`
proves real parallelism happened regardless of how slowly anything gets scheduled. Prefer
this structural approach over a wall-clock bound for any future "did concurrency actually
happen" test in this codebase.

**Known limitations, found in review, deliberately not fixed (2026-09-21):**
- `state.rs`'s `worktree_warm_status`/`worktree_setup_status` caches are both keyed only by
  `(repo_path, branch)`, with no generation/epoch guard. A rapid remove-then-recreate of a
  worktree on the *same branch name* while the first one's warm/setup-script task is still
  running (nothing cancels an orphaned `tokio::spawn`) can let the stale task's later write
  land after the new task's, showing a wrong progress count or a premature "completed" for
  the new worktree. This is the same pre-existing shape in both caches (not new to warming),
  self-corrects once no further stale writes land, and the trigger (remove+recreate the exact
  same branch name within seconds) is rare enough that a generation guard wasn't added for
  it — but if you're touching either cache for another reason, consider adding one to both
  at once rather than fixing only the one you're already in.
- `run_worktree_warm` reconstructs "did warming actually dispatch anything" via a side-channel
  `Arc<AtomicBool>` shared across `on_started`/`on_progress`/a post-await check, purely to
  decide whether to emit `worktree-warm-completed` — `cow::WarmingReport` has no field for
  this itself. Works correctly (verified: nothing here actually runs concurrently across real
  threads, so the atomic ordering is stricter than needed, just not incorrect), but the
  information properly belongs on `WarmingReport`, not reconstructed at the call site. Left
  as-is rather than reshaping the return type for a cosmetic-only fix.
- `remove_worktree_by_workspace_id` guards against live PTY sessions before removing a
  worktree, but has no equivalent guard against an in-flight background warm task for that
  same `(repo_path, branch)` — nothing stores a handle to the `tokio::spawn`'d warm chain, so
  nothing can wait for or cancel it. Removing a worktree immediately after creating it, before
  its warm step finishes, can race `git worktree remove` against `warm_candidates_concurrent`'s
  still-running copies: at best a harmless copy failure (silently absorbed as a warning nobody
  reads), at worst a copy that recreates a directory at the just-removed path after `rm -rf`
  has already walked past it, leaving orphaned build-artifact content on disk under a path the
  user believes was fully removed. Narrow trigger (create-then-immediately-remove the exact
  same worktree within the warm window) and not data corruption, so not fixed here — but if
  you're adding cancellation/wait plumbing for the setup-script chain for any other reason,
  extend it to cover this too rather than treating it as a warm-only gap.
- Reviewed and deliberately left as-is (negligible real cost, confirmed by direct inspection,
  not just asserted): `warm_candidates_concurrent`'s cheap metadata-only skip-rule pre-pass
  isn't wrapped in its own `spawn_blocking` (the copies and the COW probe already are); warm
  and file-sync settings resolution each independently re-read the same three config sources
  from disk once per worktree creation (small files, once per creation, not a hot path); the
  sidebar's poll-on-mount fires one cheap (no-filesystem-work, moka-cache-backed) HTTP request
  per worktree row on every mount/reconnect regardless of the row's age; the safety-timeout
  `createEffect` in `RepoSection.tsx` clears and re-arms its 900s timer on every progress tick
  (each event is a new object reference), not only on the null→non-null transition. Each of
  these is a real, verified observation — just not worth the risk of touching more code for
  the actual magnitude of savings involved.
- `build_worktree_warm_status_cache`/`build_worktree_setup_status_cache` (state.rs),
  `emit_worktree_warm_*`/`emit_worktree_sync_*`/`emit_worktree_setup_script_completed`
  (worktree.rs, the dual-emit shape), `resolve_warm_setting_from`/`resolve_copy_settings_from`
  (config.rs, the 3-tier resolution chain), and `get_worktree_warm_status_http`/
  `get_worktree_setup_status_http` (worktree_routes.rs) are each a close structural copy of
  an existing sibling, with no shared generic helper. This is consistent with how this
  codebase already handles every other per-setting resolver and per-stage poll surface (each
  is its own hand-written copy — confirmed by a full sibling survey during review: no generic
  `resolve_tier<T>` or `dual_emit(...)` helper exists anywhere for ANY existing setting/event),
  so warming isn't introducing a new inconsistency, just extending an existing one. If a THIRD
  background-creation-stage ever needs its own poll surface, that's the point to generalize
  the cache/route/query/MCP-action shape into one parameterized helper rather than hand-copying
  a third time — not before.


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


## Session Diff Review — Replay Semantics and Cache-Invalidation Gotchas

`session_review.rs` reconstructs a Claude Code session's edit history by literally replaying each `Edit`/`Write` tool call's substitution forward or backward against file content (see `docs/backend/session-review.md`). A 2026-09-14 code review of that feature surfaced two gotchas worth remembering for any similar string-replay or cache-over-a-file code:

- **`str::find("")` always returns `Some(0)`, never `None`.** An empty search string trivially "matches" at the very start of any haystack. `apply_reverse`'s per-step undo (and `revert_step_via_substitution`'s out-of-repo fallback) locate the substitution point via `content.find(needle)` — for a pure-deletion edit (`new_string == ""`, the common case for reversing a deletion), this silently "succeeds" by reinserting the deleted text at offset 0 instead of its real location, corrupting the file while reporting success. Neither direction (an empty `old_string` or `new_string`) can be located unambiguously from content alone without hunk/line-context, so the fix (`find_single_occurrence`) treats an empty needle as unlocatable and reports "not found" rather than guessing an offset. Any code that does `content.find(x)`/`content.replace(x, y)` where `x` can be empty and originates from a real edit operation has this exact landmine — check for it explicitly rather than trusting `find`'s `Option` to mean "not present."
- **A read cache keyed only by a *different* file's `(len, mtime)` goes stale the moment a sibling write mutates state the cache reflects.** `get_session_review`'s in-memory cache keys on the *transcript's* `(len, mtime)` — but `revert_session_step`/`revert_file_to_session_start` mutate the *working tree*, not the transcript, so a revert never changes the cache key at all. The fix (`invalidate_cached_review`) evicts the cache entry after any successful revert, even though the "input" the cache is keyed on didn't change. When a cache's freshness key doesn't cover every input that can affect the cached value (here: disk content, not just the transcript; also `include_subagents`, which wasn't part of the key at all until the same review), any commit that can invalidate the *uncovered* dependency needs an explicit evict/invalidate call — don't assume "the key didn't change" means "the cached value is still correct."


## Terminal Query/Reply Latency (DSR/CPR, DA1/DA2, DECRQM)

The alacritty fork's `Handler` impl (`patches/alacritty_terminal/src/term/mod.rs`) answers several terminal→app query sequences by queuing a `TermEvent::PtyWrite` — `device_status` (DSR-5/DSR-6 aka CPR), `identify_terminal` (DA1/DA2), `report_mode`/`report_private_mode` (DECRQM). These replies are latency-sensitive: real tools (pagers like `leaf`, readline libraries) set a short internal deadline waiting for them and print the raw escape sequence as visible garbage once it's late.

**Every code path that calls `Term::process`/replays bytes through this `Handler` must drain and flush any resulting `PtyWrite` events itself — draining is not automatic, and forgetting it does not fail loudly, it just silently delays or drops the reply.** Two call sites need this today, found one bug apart (2026-08-29):

- `pty.rs::process_chunk` — the ordinary per-PTY-read path. Flushes `PtyWrite` replies immediately after `grid_drain_events()`, *before* the chrome-filter/classification work below it (which clones the full screen on a chunk's first paint — exactly when a freshly-launched TUI queries cursor position, so this ordering is load-bearing, not cosmetic).
- The frame ticker's stalled-synchronized-update timeout flush (`flush_sync_timeout_if_needed`, `pty.rs`) — replays buffered bytes through the same `Handler` when a BSU never sees its ESU within 150ms, but only forwards screen frames; it never runs `process_chunk`. Drains via `terminal_grid.rs::drain_pty_write_events()` (a `PtyWrite`-only drain — it must NOT take everything with a full `drain_events()`, since other event kinds queued there, e.g. title/OSC 133/TUIC, have no handler on the ticker and must stay queued for the next real chunk).

If you add a third place that calls `.process()`/replays PTY bytes outside these two, check whether it can produce a `PtyWrite` and needs the same drain-and-flush. (The scrollback-restore replay path and session-teardown's `force_stop_sync_if_buffered` were checked and are NOT gaps *for `PtyWrite` specifically*: the former replays reconstructed SGR-styling bytes that can never contain a query sequence in the first place, since queries never left a trace in the stored `LogLine` screen model; the latter runs after the child has already exited, so there is nothing left to write a reply to.)

**This is not the only such queue — check every one, not just `PtyWrite`.** Kitty image decode (color-tools plan) added a second, separate, non-`TermEvent` queue (`pending_kitty_jobs`) that the *same* `Handler`-replay call sites can populate, and a code review caught that `flush_sync_timeout_if_needed` and `force_stop_sync_if_buffered` were missed for it — the exact same shape of gap this section already fixed once for `PtyWrite`, in the exact same two places, because a fix scoped to "the queue I'm looking at" doesn't audit every queue a shared replay path can produce. Unlike the `PtyWrite` case, session teardown genuinely IS a gap here even though the child has already exited: there's no reply to write, but a permanently-`is_pending()` placeholder would be wrong for any later read (`image_bytes`/`image_meta`) of that closing session's residual state. See `pty.rs::resolve_kitty_decode_jobs`'s doc comment for the fix and all three call sites. When you add a new per-session queue that a `Handler`-replay path can populate, grep for every existing `Term::process`/`stop_sync` call site and check each one against it — don't assume the `PtyWrite` list above is exhaustive for your new queue too.

**Flushing earlier must not mean flushing under a lock you didn't hold before.** The `process_chunk` fix above shipped once already holding the session's `vt_log` `Mutex` across the write — a real regression a review caught: `write_terminal_reply`'s `write_all`/`flush` can block (a SIGSTOP'd/standby child, a full PTY input queue), and blocking there while holding the lock stalls every other consumer of that session's grid (frame ticker, HTTP terminal reads). `process_chunk` now splits into two lock acquisitions — collect pending replies under the first, drop it, flush lock-free, re-lock for the screen-diff/classification work — specifically so the write is never inside a locked critical section. When moving a flush/write earlier in a hot path to fix a latency bug, check what lock is held at the new call site, not just that it now runs sooner.


## Every Step of Untrusted-Escape-Sequence Processing Needs Its Own Cap, Not Just a Final Byte Total

Any handler that reacts to a terminal escape sequence by doing real work — reserving grid rows, decompressing a payload, accumulating a buffer across multiple chunked wire messages — is processing attacker-controlled input (any process in the PTY: a `cat`'d file, a compromised CLI tool, remote SSH output) and needs an explicit bound at **every** step that scales with a wire-supplied number, not only a final "is the assembled result too big" check. A single final byte cap does not protect the *intermediate* steps that ran to produce that result.

A 2026-09-11 security review of the inline-images feature (`docs/backend/pty.md`'s Kitty/iTerm2 sections) found this violated three separate times in one feature, each independently a real DoS reachable from ordinary PTY input:

- **`reserve_image_footprint`** (`patches/alacritty_terminal/src/term/mod.rs`) took a row count straight from wire data (Kitty `r=`, iTerm2 `height=`) with no upper bound, then looped once per row *inside* `Term::process`, under the session's `vt_log` lock — a crafted `r=4000000000` could attempt ~4.3 billion iterations, hanging that session and everything else reading its grid. Fixed with a hard `MAX_FOOTPRINT_ROWS` clamp at the single choke point both protocols route through.
- **Kitty's `m=1` chunked-transmission accumulation** had no cap of its own — each individual chunk was already bounded (`vte`'s `MAX_OSC_RAW_STD`, 2 MiB), but nothing stopped an unbounded *number* of chunks from growing the pending buffer past any reasonable size before the app-level total-bytes cap ever ran. The sibling iTerm2 multipart path had already solved this exact problem for itself (`MAX_MULTIPART_B64_BYTES`, fail-closed) — the fix was only ever applied to one of the two parallel chunking paths. **When two protocol paths share a shape (both have chunked/multipart assembly), a safety fix applied to one needs an explicit check for whether the sibling has the same gap.**
- **`o=z` zlib decompression** used a plain unbounded `read_to_end` — a classic decompression bomb, allocating the full inflated buffer before any size check ran. Fixed by wrapping the decoder in `Read::take(cap + 1)` and checking whether the read filled that exact allowance (`Take` doesn't error on hitting its limit, it just stops — so the check must be `len() > cap`, not "did this return `Err`").

When adding a new escape-sequence handler that reserves resources, accumulates a wire-chunked buffer, or decompresses a payload, ask explicitly: what wire-supplied number drives a loop or an allocation here, and what stops it from being astronomically large? A cap added only after the expensive step (decompress-then-check, accumulate-then-check) still lets the expensive step itself run unbounded.


## Command Blocks — `line` Is Only A Valid Anchor On The Primary Screen

Command Blocks (gutter marks, scrollbar ticks, fold, `Cmd+Shift+Up/Down` jump-nav, block-scoped
search, "Copy Block Output") anchor to absolute row numbers computed as `history_size() +
cursor.point.line` against whichever screen (primary or alt) is *currently active* — both
`osc133()` and `osc7770()` (`patches/alacritty_terminal/src/term/mod.rs`) compute it this way,
with no `is_alternate_screen()` check in either handler.

**This silently breaks for any fullscreen (alternate-screen) TUI, and it's now the everyday case,
not an edge case** — Claude Code's default renderer draws entirely inside the alternate screen
buffer, and being a fully-repainted TUI (redraws its fixed viewport via cursor addressing,
never triggers a real terminal scroll), the alt screen's own `history_size()` never grows.
Confirmed live: a real fullscreen Claude Code session reports `total_lines == screen_lines` —
zero scrollback — for its *entire* life, even after many turns. During that time, `line` is just
the transient on-screen cursor row: small, non-monotonic across turns, and disconnected both from
the primary scrollback (`CanvasTerminal` isn't even rendering the durable log while alt-screen is
active) and from whatever real primary-screen content comes next once the agent returns to a
shell prompt.

Fixed (2026-09-15) by tagging every block-boundary event with `on_alt_screen`/`onAltScreen`. For
the direct OSC133/OSC7770 event path, this is captured **inside the vendored alacritty patch's
`osc133()`/`osc7770()` handlers themselves** (`self.mode().contains(TermMode::ALT_SCREEN)`,
threaded through `Event::Osc133`/`Event::Tuic` → `TermEvent::Osc133`/`Tuic`), atomically with
`line` — a code-review pass caught that reading `is_alternate_screen()` once per PTY chunk
downstream in `pty.rs` (the first version of this fix) mistags any event when the alt screen
toggles more than once within a single chunk (a 64KB PTY read buffer easily fits a full
enter+exit round-trip). The two heuristic, text-scanning paths
(`synthesize_cc_block_events`/`synthesize_transcript_dump_block_events`, which react to
*content* on `changed_rows` rather than a discrete VTE event) still take the coarser
per-chunk-sampled flag — achieving the same per-row precision for those would need each row
timestamped with its own alt-screen bit at diff time, a materially bigger change; the practical
risk is lower there since alt-screen transition sequences don't typically share a row with
visible `⏺`/`❯`/`✻` text.

The block still fires either way — `CommandOverview`'s prompt/duration/exit-status metadata never
depended on row validity and keeps working through a fullscreen turn. **Exception found by the
same review pass: `CommandOverview.tsx`'s `getCommandText` falls back to slicing the grid
(`ref.getBufferLines(commandLine, executionLine)`) whenever `promptText` is null — a real shell
OSC133 block with no hook-driven prompt text — and that fallback DOES depend on row validity, so
it must check `block.onAltScreen` too and return `""` rather than read.** Every row-anchored
consumer must read blocks through `rowAnchoredBlocks()` (`src/stores/terminals.ts`), which drops
`onAltScreen: true` entries, rather than rendering a mark at a meaningless row.

If you add a new row-anchored block consumer (frontend) or a new `AgentBlock`/`Osc133Event`
producer (backend), it needs the same treatment: filter through `rowAnchoredBlocks()` on the
frontend, thread `on_alt_screen` through on the backend. Full research (live introspection +
binary-string analysis of the installed `claude` CLI + code reading) and the fix's design
rationale are in `plans/command-blocks-fullscreen-mode-fix.md` (main checkout) and
`agent-signal-architecture.html`'s "Command Blocks & Scrollbar Ticks" section.

**Stretch addition, same fix:** Claude Code's fullscreen "transcript mode" (`Ctrl+O`) stays inside
the alt screen too — no help there, same limitation. But its `[` key ("write to native
scrollback") verified live to genuinely exit the alt screen and dump the whole conversation as
real, addressable primary-screen text. `synthesize_transcript_dump_block_events` (`pty.rs`)
recognizes that dump's own `❯ <prompt>` / `✻ … · done` markers and synthesizes real,
`on_alt_screen: false` blocks from it — narrowly scoped to a hook-instrumented Claude Code session
(`agent_type == "claude"`), since the `❯` glyph isn't exclusive to this dump (this repo's own zsh
prompt can use one too). Per Claude Code's own docs, `[` re-dumps the *entire* conversation from
scratch each time it's pressed (not an incremental append) — repeating the gesture used to leave a
second, overlapping set of blocks sitting in `commandBlocks[]` forever; fixed (2026-09-15) via
`new_dump_generation`/`fromTranscriptDump`, below.

**Scrollback-ring eviction (2026-09-15 fix):** `line` was originally computed as
`history_size() + cursor row` (`osc133()`/`osc7770()`, `term/mod.rs`) — `history_size()` *plateaus*
once the grid's scroll-limit cap starts evicting old lines (confirmed via `Grid`'s own doc
comment: "unlike `history_size()` it does not plateau or shrink when the scrollback cap evicts old
lines"), while the on-screen cursor row keeps cycling through the same small range. Once a session
accumulates enough real scrollback to saturate the cap (`GRID_SCROLLBACK`, `state.rs`, 10,000
lines), two blocks recorded far apart in real time could land on the identical `line` — the exact
same physical-row-coordinate-space collision the alt-screen fix above closes for a *different*
cause. Fixed by switching to `Grid::total_scrolled()` (a pre-existing, already-shipped primitive —
see its doc comment, and `reserve_image_footprint`'s `abs_row`/`serialize_styled_range`'s
`history_base`, which already used it for images and the scroll row cache): eviction-stable,
never plateaus. Every consumer that needs to turn a stored `CommandBlock` row back into a live
viewport/buffer-line row must convert first — see `terminals.ts`'s `CommandBlock` doc comment and
`canvasTerminalUtils.ts`'s `evictionStableToGridRelative`. Regression test:
`terminal_grid.rs`'s `osc133_line_stays_eviction_stable_and_never_aliases_past_scrollback_saturation`
(a tiny 5-line scroll cap makes real saturation cheap to reach in a unit test).

**Transcript-dump de-duplication (2026-09-15 fix, closes the gap above):** `[` is only reachable
from transcript mode (the alternate screen), so a visit back to the alternate screen since the
last dump activity is an unambiguous signal that whatever dump activity resumes next is a
genuinely *new* `[`-dump, not a continuation of the one already on screen.
`ChunkProcessor.dump_saw_alt_screen` (`pty.rs`) tracks exactly that, consumed (and reset) the next
time a primary-screen chunk synthesizes a dump `start` event — that event alone carries
`new_dump_generation: true`. On the frontend, `handleOsc133`'s `"A"` case responds by pruning every
existing `fromTranscriptDump: true` `CommandBlock` — including a still-open `activeBlock` left
unclosed by leaving transcript mode mid-turn — before adding the new one, so a repeat gesture
never leaves stale blocks behind. A real shell/hook/heuristic block (`fromTranscriptDump: false`)
is never touched by this prune. See `to-test.md`'s "§5/#11" entry for the manual check (this is a
Rust change; needs a rebuild).


Frontend consumer rules (`rowAnchoredBlocks()`, `CommandOverview.tsx`'s
`getCommandText`, eviction-stable-to-grid-relative conversion): see
`src/components/Terminal/AGENTS.md`.

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

**Three signals report awaiting, and they are not interchangeable.** OSC 7770 and
OSC 777 differ by one digit and both happen to use the word "notify" — this has
caused real confusion (see `agent-signal-architecture.html#osc-confusion`): OSC
7770 is **ours** (`tuic-hook`, written from Claude Code's own hook events); OSC
777 is a believed-but-**unconfirmed-live** native agent notification, empirically
retested 2026-08-29 and never observed firing even in a scenario built to trigger
it. Do not assume Claude Code natively emits OSC 777 — what actually fires for
Claude's own "waiting for your input" idle-timer heartbeat is `tuic-hook`
converting a real `Notification` **hook event** into OSC 7770's `notify=`/
`state=awaiting`, not a native OSC 777 write:

| Signal | Source | Applies to |
|---|---|---|
| OSC 7770 `state=awaiting` | TUIC hook (`tuic-hook`, from a real Claude Code hook event) | hook-instrumented agents. `PreToolUse(AskUserQuestion\|ExitPlanMode)` and `Elicitation` are always confident. `Notification` (12 possible `notification_type` reasons — permission prompt, MCP elicitation, quota resume, a background session finishing, Claude's own ~60s idle-timer heartbeat, …) is classified deterministically by `notification_type` (`pty.rs::notification_awaiting_outcome`): some types stay confident, purely informational ones never badge at all, and the idle-timer heartbeat (`idle_prompt`) is dropped outright once the shell is already idle. A wording fallback covers an unrecognized/absent `notification_type`. |
| OSC 777 `notify` | agent's own native desktop notification — **unconfirmed to ever actually fire** | if it does fire: any agent, any blocking prompt — the body decides the confidence: `needs your permission` / `approval required` latch, `is waiting for your input` is low-confidence because Claude also sends it on its 60s idle timer (mirrored by the OSC 7770 `Notification` classification above, which handles the confirmed-live path for the same ambiguity) |
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

**The same SET-without-CLEAR shape recurred in a sibling flag, not just
`awaiting_input`.** `session_states.agent_type` (mirrored by
`get_session_foreground_process`, `pty.rs`) is sticky *forever* once any agent
has run in a session — deliberately, to survive a real flaky case (a
short-lived grandchild like `git`/`sed`/`rg` transiently becoming the pgid
leader, which must not flip `agent_active_for_parse` off mid-stream) — so it
only clears via a *confirmed* shell foreground (`fg_is_shell`: a positive
match against the `SHELLS` list, OR the session's own recorded
`PtySession.shell` basename), never on a merely *unrecognized* foreground
(the actually-flaky case). If you add another sticky per-session flag
mirroring live process/foreground state, give it the same treatment — audit
which case its stickiness is protecting against, not a blanket never-clear.

Three sharp edges in this specific mechanism, still live:
- **Windows:** `process_name_from_pid` strips a trailing `.exe` (for
  consistency with `classify_agent`) but `session.shell` always keeps it
  (`COMSPEC`/`"powershell.exe"` fallback) — `clear_agent_type_on_confirmed_shell`
  must strip `.exe` case-insensitively from both sides before comparing, or
  the `PtySession.shell` fallback silently never matches on Windows.
- **Wrapper hops:** `agent_seen_running` only latches on the *ambiguous*
  fallback path (no direct `classify_agent` match) after it persists across
  `AGENT_SEEN_RUNNING_CONFIRM_MS` (1000ms) — a direct match still confirms
  immediately. Without this window, a multi-hop launcher (`direnv exec .
  mytool`) or a wrapper that fails before exec'ing the real target can
  prematurely confirm, or get wiped by, the wrong process.
- **OSC 133's prompt marker (`'A'`) is allowed to fast-clear `agent_type`
  the moment the target is `SHELL_IDLE`, but OSC 7770 (`state=idle`) is
  deliberately NOT wired to the same clear** — a hook-instrumented agent's
  own `state=idle` means it finished a turn while the same process stays
  alive; clearing there would wipe `agent_type` on every ordinary turn
  boundary instead of only on exit.

`apply_event_to_session_state`'s `SessionCreated` handler (`state.rs`) resets
`agent_seen_running`/`agent_seen_running_pending_since_ms` on both its
`.and_modify` and `.or_insert_with` branches — a session-id reuse or an
out-of-order bus replay landing `SessionCreated` against a pre-existing entry
must not leave a stale `agent_seen_running: true` that lets a subsequent
confirmed-shell foreground wipe a freshly-set preset before its launcher runs.

**A backgrounded tool call Claude explicitly reports as still running gets its
own signal, separate from `background_work`'s process-tree heuristic.**
Claude Code's `Stop`/`StopFailure` hook payload can carry a `background_tasks`
array naming a `run_in_background` tool call still outstanding as the turn
ends. `background_work` (`pty.rs`'s `background_work_from_snapshot`) can't be
the wire for this: its process-tree refresher is demand-gated on
`background_work` itself being `true`, so setting it from a hook activates the
very scanner that overwrites it, and clearing it that way spuriously
*publishes* a completion event. The frontend's `effectiveActivityState` also
deliberately renders `background_work=true` + idle shell as "Idle" (so a
Codex-left-running dev server doesn't permanently latch a row "Working") — a
carve-out that would be wrong here.

Fixed by following `completion_declared`'s shape instead: `tuic-hook` scrapes
`background_tasks`' raw per-task `status` strings (unclassified — see
`crates/tuic-hook/AGENTS.md`'s rule against baking Claude's evolving
vocabulary into that binary) into a `bgtasks` OSC 7770 verb; `pty.rs` classifies
"running" and writes `SilenceState::declared_background_work`, a
single-writer, epoch-stamped, self-expiring flag no polling loop touches, wired
as its own `SessionState.declared_background_work` field rather than merged
into `background_work`. See `docs/backend/pty.md`'s `declared_background_work`
section and `docs/backend/alacritty-integration.md`'s `bgtasks` verb entry.

**`background_tasks[].type` is not limited to `"shell"`** — Claude Code's
native Agent Teams feature reports dispatched teammates as
`{type: "teammate", status: "running"}`, which the scraper (which
discriminates only by `status`, never `type`) correctly turns into
`bgtasks=running`. The bug this caused: `reset_suggest_memory()` cleared
`declared_background_work` alongside `completion_declared`, but that method
is ALSO called from `apply_working_evidence`'s "reopen a stale idle/completed
turn on renewed screen evidence" path — which fires every time the
orchestrator's own screen shows a spinner again, e.g. polling its still-running
teammates. Polling children tells you nothing about whether they finished, so
every such poll silently erased an accurate `declared_background_work=true`.
Fixed by splitting the clear into `SilenceState::reset_declared_background_work()`,
called only from genuine new-turn-submission sites — `apply_working_evidence`'s
reopening no longer touches it. **This is the second reset method that needed
splitting because "a real new turn" and "renewed screen evidence reopens a
stale turn" got conflated** — audit any third reset method added near
`reset_suggest_memory`/`apply_working_evidence` for the same shape before
assuming it's safe to share.

**A handful of git-behavior-dependent unit tests can fail purely from the
running machine's global git config** (e.g. `merge.ff = only` turns an
expected merge conflict into a hard refusal; an `insteadOf` URL rewrite makes
a raw-config read disagree with `git remote get-url`) or from ambient `TMPDIR`
state, unrelated to any code change. If a regression shows up only in
git-merge/remote-URL tests with no plausible connection to your diff, suspect
the environment before the code.

## Notification Sound Playback (`rodio` decoder features, custom-file fallback)

`notification_sound.rs` generates its built-in tones procedurally (`EnvelopedTone`,
a custom `Source` impl) — this needs only rodio's `playback` feature. **Playing a
user-supplied audio file (`rodio::Decoder`) is a separate capability that needs
its own Cargo features.** `src-tauri/Cargo.toml`'s `rodio` dependency sets
`default-features = false`, so `Decoder::new(...)` compiles but silently has zero
format backends registered — every file fails to decode — unless the relevant
feature is explicitly listed. Currently enabled: `wav`, `mp3`, `vorbis` (ogg),
`flac`. Adding support for another container (e.g. `mp4`/m4a-aac) means adding
that feature to the `rodio = { ... features = [...] }` line, not just writing
Rust code that calls `Decoder::new` and expecting it to work.

**A custom sound file that fails to open/decode MUST still play something —
the sound's own default tone — never propagate an error that silences the
notification entirely.** `resolve_playback_source` (not `play()`/
`play_notification_sound` themselves, which are plain fire-and-forget with no
return value) owns this fallback: it only ever returns `PlaybackSource::Custom`
when the file opened and decoded successfully, logging a warning and falling
through to `PlaybackSource::Sequence` (the sound's default tone) for every
other case — missing file, corrupt/unsupported format, or "custom" selected
with no path configured yet. A first version of this feature used `?` to
propagate `open_custom_sound`'s error straight out of `play()`, which a code
review caught as a real regression: it meant a moved/deleted/corrupted custom
file made that notification silent forever, directly contradicting the
feature's own shipped docs. The lesson generalizes — a graceful-fallback
promise ("falls back to X" in a commit message or docs) needs a test that
actually exercises the fallback path end-to-end (what plays when the primary
source fails), not just a test that the function "doesn't return Err."

**`SoundChoice.preset` is a plain `String`, not a Rust enum, in both
`config.rs` (persisted config) and `notification_sound.rs` (the IPC command's
own copy) — deliberately.** The frontend (`src/notifications.ts`'s
`SoundPreset` union) is the sole source of truth for which preset names are
valid; both Rust sides just pattern-match known strings and fall back to that
sound's own default tone for anything unrecognized. This was a direct
reaction to fixing a real bug the same day: `src/plugins/types.ts`'s
`NOTIFICATION_SOUNDS` had silently drifted out of sync with the other two
definitions of the *sound name* enum (missing `"attention"` entirely). Adding
a second enum-typed concept (*preset* choice) duplicated across Rust and TS
would recreate the exact same drift risk one layer up. Don't "fix" this by
introducing a `SoundPreset` Rust enum matching the TS one — instead, where a
string→string mapping needs real drift protection (e.g. `preset_name_for` /
`TOAST_SOUNDS`), match *from* the enum *to* the string with an exhaustive
`match` and no `_` arm, so a new `NotificationSound` variant fails to compile
until every such mapping is updated — cheaper than a runtime parity test and
catches the gap before it ships, not after.


## Worktree Automation Script Context (`TUIC_*`) and the Setup-Script/Sync Ordering Fix

Setup Script, Archive Script, Run Script, and Smart Prompt shell/headless children all
now get a `TUIC_*` environment (`src-tauri/src/script_env.rs`'s `ScriptContext`) —
`TUIC_MAIN_REPO_PATH`, `TUIC_BRANCH`, `TUIC_BASE_REF`, `TUIC_BASE_BRANCH`,
`TUIC_WORKTREE_PATH`/`_NAME`/`_DIR`, `TUIC_IS_WORKTREE`, `TUIC_REPO_NAME`, plus the
kind-agnostic `TUIC_SCRIPT_KIND`/`TUIC_APP_VERSION`/`TUIC_CONFIG_DIR`.

**`ScriptContext::derive` is a pure function of a filesystem path — it never accepts
caller-supplied repo/branch/worktree facts.** This was a deliberate design choice, not
an oversight: the Run Script is typed into a PTY whose spawn config carries only
`cwd`/`shell`/`env`, so deriving from `cwd` was the *only* way that surface could ever
get this context at all; and `POST /worktrees/run-script` (added alongside this) is
remote shell execution, where caller-supplied context would let a client set
`TUIC_MAIN_REPO_PATH` to anything. If you add a new `TUIC_*` variable, derive it from
the path the same way (`git::canonical_repo_root`/`read_branch_from_head`, both file
reads, no subprocess) — do not thread it through from a caller "for convenience."

**Script timeouts must drain stdout/stderr on dedicated reader threads before the
poll-wait loop, not rely on `Command::output()`.** A `spawn()` + `try_wait()` loop that
doesn't actively drain the child's pipes deadlocks the instant the child writes past
the OS pipe buffer (16 KiB on macOS) — `npm install` blows past this immediately. See
`worktree.rs`'s `run_shell_capture` for the pattern (two `std::thread::spawn` readers,
joined only after the child's fate — finished or killed — is known). On timeout, kill
the **process group** (`setsid` via `pre_exec` + `killpg`), not just the shell itself —
killing only `sh -c "npm install"` leaves `npm`'s own children running.

**The worktree file sync must always finish before the Setup Script runs, and both
happen in a background chain the caller never awaits (`worktree::spawn_worktree_setup_chain`).**
This is why worktree creation's response — on the desktop Tauri command, the MCP HTTP
`create_worktree_shared` path, *and* the MCP `repo worktree create` tool — no longer
returns `setup_script`/`setup_script_error` synchronously: that information doesn't
exist yet by the time the response is built. The outcome is instead reported via a new
dual-emitted `AppEvent::WorktreeSetupScriptCompleted` (`worktree-setup-script-completed`),
silent when no script is configured (matching `worktree-sync-*`'s own
nothing-to-do-is-silent precedent). **An MCP client has no SSE/event stream to receive
this event on** — accepted as a deliberate tradeoff for fixing the ordering bug, confirmed
with Boss before implementing (see `feedback_confirm_scope_cuts_before_deferring.md` in
memory for why this needed asking rather than assuming). Do not "fix" this by making the
chain synchronous again — that reintroduces the blocking-worktree-creation behavior the
original (now-fixed) unsequenced design was built to avoid.

**Closed via a pollable status snapshot, not by making the event reach MCP clients.**
`AppState::worktree_setup_status` (`state.rs`) is a bounded, TTL-evicted
`moka::sync::Cache<(String, String), Arc<WorktreeSetupStatus>>` keyed by
`(repo_path, branch)` — the same pair the event carries. `spawn_worktree_setup_chain`
writes into it at each transition (`Running` synchronously before the background task
even starts, then `NotConfigured` or `Completed { exit_code, error }` once the chain
finishes) — the same three states, just pollable instead of push-only. Read via
`worktree::get_worktree_setup_status` / `repo action=worktree_setup_status` (requires
`path`+`branch`) / `GET /worktrees/setup-status?repoPath=...&branch=...` (no
`require_local_or_auth` gate — read-only, same as `list_worktrees_http`/
`get_worktree_paths_http`). A pair with no tracked entry (never created this way, aged
out past the 30-minute TTL, or the app restarted) reports `{"state": "unknown"}` — treat
that as "ask again differently," never as "definitely no script was configured." This is
purely additive: the event itself, its silence-when-nothing-configured behavior, and the
desktop frontend's own event-based flow are all unchanged.


Frontend consumers of these variables (the Smart Prompts `{var}` registry, and the worktree-creation wait for setup-script completion): see `src/AGENTS.md`'s "Worktree Automation Script Context" section.

**A subdirectory cwd used to get zero `TUIC_*` vars at all — `git::resolve_git_dir`
deliberately only checks its exact path, never walks up.** Reachable in practice: the
Finder Service ("New TUICommander Tab Here" on a subfolder) or any terminal `cd`'d into a
subdirectory before a Run Script command runs. Fixed with a separate, additive
`git::find_repo_root` (walks up ancestors to the nearest `.git`, like `git rev-parse
--show-toplevel`) that `script_env::ScriptContext::derive` now uses instead of the
exact-match check — `resolve_git_dir` itself is unchanged (its other caller,
`repo_watcher.rs`, always passes an already-resolved root, so an ancestor walk there would
be a no-op anyway, but changing its contract wasn't worth the risk for one caller).

**`resolve_single_var`'s `"base_branch"` arm now shares `config::resolve_effective_base_branch`
with `TUIC_BASE_BRANCH`**, instead of calling `prompt::detect_base_branch` directly — the
two used to diverge for any repo with a configured "Branch From" override (Smart Prompts'
`{base_branch}` ignored it; the script env var didn't).
