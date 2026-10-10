# TUICommander — Rust Backend (`src-tauri/`)

Repo-wide rules (doc sync, git workflow, architecture split, TUIC protocol markers,
IPC/HTTP parity contract, accepted security decisions) live in the root
`AGENTS.md` — read that first. This file covers what's specific to the Rust/Tauri
backend: the PTY/terminal-emulation stack (`pty.rs`, `chrome.rs`, `crates/tuic-terminal`'s `terminal_grid.rs`,
the vendored `patches/alacritty_terminal`/`patches/vte` forks), agent-state detection,
worktrees, the MCP HTTP server, and build/test mechanics unique to `cargo`.

Crate-specific rules that don't belong here: `crates/tuic-cli/AGENTS.md`,
`crates/tuic-hook/AGENTS.md`, `crates/tuic-streamdock/AGENTS.md`.

## Tests (Rust)

General testing policy (the required `check-gate.sh` gate, CI-never-executed, the
`[HUMAN]` escalation ladder): root `AGENTS.md`. This section is Rust-suite specifics.

- **Mutation testing is per change, never per tree.** `make mutants RANGE=<base>` (default `HEAD~1`) runs cargo-mutants `--in-diff` over the Rust lines the range touched, `--in-place` in a disposable `git archive` export under `.tmp/` (not a worktree: a detached worktree is an orphan to the running app, which removes it), one job, through mbx. Every viable mutant costs one incremental build of the lib crate plus one test run, so the orchestrator runs it once per batch on the final HEAD — agents do not. A surviving mutant is a missing test: add the test, or `#[mutants::skip]` with the reason on the line. **Measured 2026-09-06:** baseline 197 s build + 224 s test with warm deps; each mutant ~3 min incremental build of the lib crate plus 0.5–3.5 min of tests, so ~5 min each and a 38-mutant story diff is ~3 h. During the day pass a function filter through the script (`scripts/mutants.sh HEAD~1 --re <function>`); the whole diff is an overnight or CI job. Config in `src-tauri/.cargo/mutants.toml`, mechanics in `scripts/mutants.sh`.
- Function-filtered mutation runs through make can use `make mutants RANGE=<base> -- --re <function>`; use `MUTANTS_ARGS='--re <function>'` for filters containing spaces or other complex arguments.
- **When touching `src-tauri/patches/{alacritty_terminal,vte}/`, verification MUST include `cargo nextest run --workspace` (or `make check`), not a package-scoped `cargo test --lib`/`cargo test -p tuicommander`.** The vendored crates are separate workspace members with their own regression suite (`patches/alacritty_terminal/tests/ref.rs`, ~44 fixture-replay tests) that a package-scoped run silently skips. `cargo nextest run`/`list` without `--workspace` also silently scopes to zero tests for a vendored crate instead of erroring — this exact mistake produced a false "these tests were never wired in" diagnosis in commit `47217d2c`'s own message. A background-color-erase fix landed in `6dd165f5` without running the workspace suite and shipped two regressions caught only later.
- **A second, independent trap in `vte` specifically: its `ansi` Cargo feature is not in `default = ["std"]`.** `cargo test -p vte` (or `cargo nextest run -p vte`) alone silently compiles ZERO of `src/ansi.rs`'s `mod tests` — the module itself, `mod ansi;`, is declared `pub mod ansi;` unconditionally in `lib.rs`, but everything inside it (including the `Handler`/`Processor` types most OSC-parsing tests actually exercise) is behind `#[cfg(feature = "ansi")]` at the crate level. `cargo test -p vte --features ansi` runs them; so does `cargo nextest run --workspace`, since `alacritty_terminal`'s own `[dependencies.vte]` unconditionally requests `features = ["std", "ansi"]`, which Cargo's feature unification applies workspace-wide. Confirmed 2026-09-15 while adding OSC 133 parse tests to `ansi.rs`: a bare `cargo test -p vte` reported "32 passed, 0 failed" and looked completely healthy while running only `lib.rs`'s generic OSC/CSI tests, not a single one of `ansi.rs`'s OSC-1337/OSC-133-specific tests. Same failure shape as the `--workspace` gotcha above — a scoped run that reports a clean pass while silently excluding the tests you actually meant to run — just triggered by a feature flag instead of a workspace boundary.
- **Test scratch lives under the caller's `$TMPDIR`, which on macOS is `/var/folders/…/T` — the directory with the exec-scan penalty.** `tuic-git/src/worktree.rs:7155-7178` (`shared_post_checkout_hook`) has paired measurements: a fresh executable inode cost 0.36-0.69 s there against ~0.24 s in `/tmp`, and 191-393 s against 1.9-4.5 s during an XProtect backlog. Many tests write and exec scripts (fake ssh, hooks, shims), so a `make check` that suddenly gets slow, or a timing test that flakes on a first exec, may be this rather than a regression. Mitigate by moving the base, not the code: `TUIC_TEST_TMP_BASE=/tmp/tuic-tests-$(id -u) make check` (or `<checkout>/.tmp/tuic-tests` for the pre-2026-10 in-checkout layout, with the `find_repo_root` trap below). Unix socket roots are unaffected: they already resolve to `/tmp/tuic-s<hash>` or `TUIC_TEST_SOCKET_ROOT` (see `docs/guides/development-setup.md` → Testing). Never point any of these at `$HOME`; `scripts/check-no-home-gits.mjs` fails `make check` on a `$HOME/Gits` path in code or tooling.
- When resolving a rebase/merge conflict in a Rust function by taking one side's body wholesale, diff the full field set of any struct it returns — a dropped field can compile cleanly (mocked in the caller's own tests) while silently regressing a feature only an end-to-end test would catch. Prefer merging the logic, not picking a side outright, when the two versions diverge structurally.

## The suite skips every test it cannot run on purpose — classify them, never pin the count

`cargo nextest run --lib` reports a handful of skips. Every one is `#[ignore]`
with a reason string. None is a `cfg` exclusion and none is a filter artifact,
so the skips are not missing coverage and not a harness defect: `N run, M
skipped` is a **complete** result for what an unattended run can execute.

| Category | Precondition an unattended run cannot meet |
|---|---|
| Environment | interactive Keychain (×4), network + GitHub token (×2), authenticated `gh` CLI, a downloaded whisper model, a downloaded Pocket TTS bundle (×3, `TUIC_POCKET_BUNDLE_DIR`), real `openpty` |
| Corpus-driven | `TUIC_CAPTURE_CORPUS`, `TUIC_DAMAGE_CORPUS`, `TUIC_REPLAY_FILE`, plus 744-138c's evidence capture |
| Benchmark | `bench_chunk_path_replay`, `tunnels::audit::tests::bulk_insert_performance`, `mcp_http::ws_compression::measurement::level_six_is_the_knee_of_the_curve` |

The per-category counts are deliberately not written down — see "Never assert
the skip count" below. On 2026-09-21 the three categories held 12, 4 and 3.

**`dump_committed_tcap_fixture_event_sequences_744` is not a pass/fail test.** It
is an evidence-capture harness for story 744-138c and the comment above it says
so. Un-ignoring it during a tidy-up of ignored tests is the failure to avoid.

**Re-derive the classification; do not trust a count.** Counting `#[ignore]`
attributes in the source happens to give the right answer for the wrong reason —
it counts one mechanism and cannot see the other two. This does discriminate:

```
cargo nextest list --lib --run-ignored all   ->  4851
cargo nextest list --lib                     ->  4832
```

The delta is the skip count and the set difference *is* those names (19 on
2026-09-21). A `cfg`-excluded test
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

`--no-default-features` reports four fewer, because `mod dictation` is
`#[cfg(feature = "desktop")]` (`lib.rs`) and its four ignored tests — one
whisper, three Pocket TTS — do not exist in that build: absent rather than
skipped. No `#[ignore]` anywhere is `cfg`-conditional, so nothing else moves
between the two configurations.

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
   echo '<html></html>' > dist/index.html` (from the repo root). **Also create `dist/mobile.html`**
   the same way (`echo '<html></html>' > dist/mobile.html`) — `static_files.rs`'s `spa_fallback_file()`
   maps every `/mobile`/`/mobile/*` path to `mobile.html` specifically, not `index.html`, so without
   it `mcp_http::tests::unknown_api_path_404s_while_spa_deep_links_still_load` fails on a fresh
   worktree's first `cargo nextest run` with a real 404 where the test expects the SPA shell.
2. **Sidecar binary placeholders** — `tauri.conf.json`'s `externalBin` lists `binaries/tuic-bridge`,
   `binaries/tuic`, `binaries/tuic-hook`; the build script checks these paths exist for the host
   target triple. None of them are tracked in git (`src-tauri/binaries/*` is gitignored). Empty
   files satisfy the resource-existence check: `target=$(rustc --print host-tuple); mkdir -p
   src-tauri/binaries; for b in tuic-bridge tuic tuic-hook; do touch
   src-tauri/binaries/$b-$target; chmod +x src-tauri/binaries/$b-$target; done`. That's enough
   for `cargo test`; to actually run the app, build real ones with `pnpm build:sidecar --force`.
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
   placeholders can block the sidecar build instead of getting replaced by a real one.**
   `build-sidecar.mjs` skips its `cargo build --release` step when the release binary is newer
   than its sources and `src-tauri/binaries/<bin>-<target>`'s size equals
   `target/release/<bin>`'s — and the same transient-0-byte read described in point 3 can hit
   the *release* profile too, so an untouched 0-byte stub can look like "0 == 0". Since
   #1325-394f that skip path calls `assertRealSidecar`, so this fails loudly (`Sidecar is a
   placeholder (empty or not executable)`) instead of logging `(skipped)` and leaving an empty
   sidecar behind — but it does not rebuild by itself. Fix: `pnpm build:sidecar --force`
   (rebuilds `tuic-bridge`/`tuic`/`tuic-hook` unconditionally), then separately
   `cargo build --package tuic-bridge` and `cargo build --package tuic-hook` (from `src-tauri/`)
   to populate their **debug** binaries too — `build-sidecar.mjs` only ever builds the
   `--release` profile, but `target/debug/tuic-bridge` is what an MCP client spawns directly. A
   0-byte `target/debug/tuic-bridge` once snapped the `tuicommander` MCP connection for a
   Claude Code session running out of that worktree (`ENOEXEC`, not a valid Mach-O): if a
   `tuicommander` MCP server fails to connect with an `ENOEXEC` on a `target/debug/*` path in a
   worktree you've been building in, check that file's size before assuming a code regression.

The `plugins/` git submodule is also frequently out of sync in worktrees — either pinned to a
stale commit (`src/__tests__/plugins/buildCleaner.test.ts` fails to resolve an import) or not
initialized at all (`git submodule status plugins` shows a leading `-`; `make check`'s `Plugin
tests` step / `pnpm test:plugins` reports 0 tests collected and exits 1). Both are pre-existing
environment drift, not a regression — don't spend time fixing the submodule pointer unless
explicitly asked.

**Plugins pin status (rebase onto main, 2026-10): the `497bb1f` breakage is fixed, but the pin is
undecided between two diverged plugin commits.** `497bb1f` predated the plugins repo's `20b2e09`
(`feat(plugins): externalize plan and stories tools`), so `plugins/plan/` and
`plugins/stories-ticker/` — which `plugins.rs`'s `SEEDED_PLUGINS` unconditionally `include_str!`s —
did not exist and the whole crate failed to compile (six `couldn't read ... No such file or
directory` errors). Both later pins have them: this tree records main's `670c36628` (adds
`sqlite-viewer/`); the pre-rebase `wip` branch recorded `6dc4b8047`, the plugins repo's `wip` tip
(adds `md-kanban/` and `tuic-voice/`). They diverge from `c964e7bb9`, so `src/__tests__/plugins/mdKanban.test.ts`
cannot pass against a checkout of `670c36628`. **The final pin needs a commit inside the plugins
repo that merges `670c36628` and `6dc4b8047`, pushed to that repo's remote** (see the two-commit
dance below — the parent repo's push does not carry it), and then a parent-repo commit pinning it.
If a similar gap reappears (a new bundled plugin referenced via `include_str!` before the pin
catches up), verify with `git -C plugins ls-tree HEAD --name-only | grep -E
'^(plan|stories-ticker|<new-plugin>)$'` (empty output confirms the gap), then advance the pin to a
commit — check both `origin/main` and `origin/wip` in the plugins repo — that has every directory
`SEEDED_PLUGINS` references.

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

**Audio-device-enumeration tests can stall behind an un-granted macOS permission prompt — not
a code bug.** Found 2026-09-30 (pre-rebase `wip` branch) running the full gate after an unrelated
`to-test.md`-only change: `cargo nextest run --workspace` hit its 120 s hard kill (`slow-timeout`
in `.config/nextest.toml`) on five tests that enumerate real CoreAudio devices through cpal. The
first cpal query waits on a system permission dialog (main's `audio_enumeration::ENUMERATION_TIMEOUT`
comment names the microphone prompt) when it was never granted for the process running the tests,
and in a headless run nothing is there to answer it. On this tree the five behave differently:

- `notification_sound::tests::list_output_devices_has_at_most_one_default_and_no_duplicate_names`,
  `::resolve_output_stream_falls_back_to_default_for_an_unknown_device_name` and
  `::resolve_output_stream_opens_a_named_device_when_one_is_available` call
  `notification_sound::list_output_devices`/`resolve_output_stream` directly
  (`rodio::cpal::default_host().output_devices()`/`default_output_device()`, unbounded), so they
  can still block until nextest kills them at 120 s.
- The device ROUTES are bounded: `GET /audio/output-devices` (`list_audio_output_devices_http` →
  `notification_sound::list_audio_output_devices`) and `GET /dictation/devices`
  (`dictation::commands::list_audio_devices` → `tuic_dictation::audio::list_input_devices`,
  microphone enumeration, `dictation` feature only) both run through
  `audio_enumeration::run_bounded` with `ENUMERATION_TIMEOUT` (30 s). So
  `mcp_http::config_routes::tests::list_audio_output_devices_http_returns_a_device_array` FAILS
  after ~30 s (a 500 instead of a 200) rather than hanging, and
  `mcp_http::tests::every_dictation_and_os_integration_path_has_a_route` — whose route sweep probes
  `GET /dictation/devices` and only asserts "not 404/405" — just takes ~30 s and passes.

This is the same *shape* as the "interactive Keychain" precondition in the ignored-tests table
above, but these are ordinary `#[test]`s, so they stall or fail the gate instead of being skipped.
If `check-gate`/`cargo nextest` stalls or fails on exactly these names and your diff doesn't touch
`notification_sound.rs`, `audio_enumeration.rs`, `tuic-dictation`'s `audio.rs` or the device
routes, grant/confirm the prompt for the terminal/process running the tests (System Settings →
Privacy & Security, or let it appear once and click through it interactively) and re-run — do not
treat it as a regression from whatever you just changed.

**`mdkb_daemon::tests::ensure_running_without_binary_uses_existing_daemon` can fail deterministically
from a stale, orphaned Unix socket file — not a code regression.** Found 2026-09-30 running the
full gate after an unrelated `terminals.ts`/`osc_title.rs` fix (neither `mdkb_daemon.rs` nor
`mdkb_client.rs` were touched). The test branches on `MdkbClient::socket_path().exists()`
(`~/.mdkb/daemon-hook.sock`) to decide which assertion to make — but existence of the socket
*file* doesn't mean a process is actually listening on it: a daemon that exited without cleaning
up its socket leaves the file behind, and `ensure_running()`'s subsequent connect attempt then
fails with "should connect to running daemon" every time, deterministically (confirmed: re-ran in
isolation, same failure; `ps aux | grep mdkb` showed zero running daemon processes; `ls -la
~/.mdkb/daemon-hook.sock` showed a day-old file). This is a real gap in the test itself (it should
probably try connecting, not just stat the path, to decide which branch it's in) but not something
to "fix" as a side effect of an unrelated change — if this test fails and your diff doesn't touch
`mdkb_daemon.rs`/`mdkb_client.rs`, check `ps aux | grep mdkb` and the socket file's age before
assuming a regression. This machine's own mdkb/rtk tooling (if in use) may recreate a fresh socket
on its own if restarted; deleting the stale file yourself is not this fix's call to make since
another process could own its lifecycle.


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

**`Path::is_absolute` answers for the host only.** `C:\…` and `\\…` are not absolute on unix, and a leading `/` is merely *rooted* on Windows — while `Path::join` there **replaces** the root, so an unrejected `/etc/passwd` lands at the root of the repo's drive. Any validator deciding whether a path may escape a boundary uses `fs::is_absolute_on_any_platform` (re-exported from `tuic_core::path_spelling`).


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

**`load_agents_config()` itself is now cached (2026-09-24), fixing a real CPU/latency bug: it
was called — with a full uncached locked read + JSON parse of `agents.json`, plus the codex-bypass
migration's stamp-file check — from
`osc_title::should_skip` on every OSC 0/2 title repaint for any session with an active agent
intent, and some agents repaint their title at ~8Hz+.** The cache (`AGENTS_CONFIG_CACHE` in
`config.rs`) is keyed on `(path, mtime, len)`, not on `CONFIG_DIR_OVERRIDE` directly — a cache
hit requires the exact same resolved path AND an unchanged mtime/len. This does not reintroduce
the cross-test hazard described above: two sequential tests sharing the exact same literal path
only share a stale cache entry if the file's content is *also* actually the same, which is the
correct (not stale) result anyway. Regression test:
`load_agents_config_picks_up_a_second_write_not_a_stale_cache`.

**mtime/len alone is not quite enough for this process's OWN writes — `save_agents_config` also
explicitly clears the cache on every successful save.** A code-review pass caught the gap:
two writes landing within the same filesystem mtime tick (a coarse-mtime mount, or simply two
writes fast enough to share a tick) that happen to produce equal-length JSON collide on
`(mtime, len)` with the first write's now-stale entry, and a stat-only check alone can't tell
them apart. Clearing unconditionally on save removes any entry regardless of whether the new
stat happens to collide, so there's nothing left to falsely match against. Regression test:
`load_agents_config_picks_up_a_second_write_of_equal_length_content` (can't force an actual mtime
collision portably, but exercises the same-length case the finding was about).

**A miss only populates the cache when the file's `(mtime, len)` is identical before and after
the load**, and never after a failed codex-bypass migration (that path falls back to a plain read
and retries next call, as before the cache). The migration inside `load_agents_config` can itself
rewrite `agents.json`, and another instance can write between the stat and the read — caching
under the pre-load stat in either case would pin a stale value until the next write.


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

`git worktree remove`'s dirty-worktree and lock refusals are independent — never collapse them into a single `force: bool` (removal takes `force` + a confirmed `expected_fingerprint` for dirty files, and a separate `override_lock`). Branch deletion after a worktree removal must always use `git branch -d` (safe), never `-D`, regardless of how the worktree itself was removed. A destructive worktree action gated behind a confirm dialog must default Enter to Cancel (`defaultButton: 'cancel'`) — verify every dialog in the removal/archive/delete family sets this explicitly; don't assume a sibling dialog's fix covers all of them. A heuristic detector (e.g. orphan = detached HEAD + no branch) must never drive an unrecoverable destructive action by default.

**A worktree with a submodule checked out is a third refusal, but a plain `--force` DOES lift it** — `fatal: working trees containing submodules cannot be moved or removed`. Confirmed empirically (git 2.55.0): `git worktree remove --force` succeeds outright here, even on an otherwise-dirty worktree, and the refusal fires *before* git's own dirty-worktree check — so a caller that relies on git to report dirtiness without `--force` never gets that report. This is why tuic-git's `remove_worktree_internal_with_lock` proves cleanliness itself (`dirty_files_at`, `verify_submodules_at`, recheck after) and then always passes one `--force` (a second `--force` would also override a lock, which needs `override_lock`). `git submodule deinit --force` does **not** help (the gitlink stays in the index). Before asserting a git force flag does or doesn't lift a given refusal, verify by actually running every relevant combination — don't reason from a flag's name or partial testing.

## Worktree File Sync (copy_ignored_files / copy_untracked_files / copy_paths)

`git worktree add` only ever checks out tracked, committed content — copying anything else
(ignored/untracked files, or a repo's explicit `copy_paths` list) into a freshly created worktree
is a separate step, done by `worktree_sync.rs` + `worktree::run_worktree_file_sync`, run in the
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


## Worktree Warming — Opt-Out, Bounded-Parallel, One Pipeline

Warming (clonefiling the parent's git-ignored build directories — `node_modules`, `target`, … —
into a freshly created linked worktree; `tuic_git::cow`) runs as the FIRST step of the single
post-create chain `worktree::spawn_worktree_setup_chain` (warm → file sync → Setup Script), with
main's warm state (`tuic_git::worktree` `begin_warm`/`finish_warm`, token-checked) as the only
status store. There is deliberately no second warm status cache or poll endpoint: a poller reads
the workspace's `warm_artifacts` from `worktree_list` / `GET /worktrees/paths`, which while
`pending` carries `phase` (`warming` with `copied`/`total`, then `file_sync_and_setup_script`) via
`tuic_git::worktree::update_pending_warm`.

- **Opt-out** `warm_ignored_directories` (default `true`; per-repo > `.tuic.json` > global, via
  `config::resolve_effective_warm_setting`) is resolved INSIDE the chain, so every creation path
  that hands the chain a warm token honours it identically. All four do: desktop `create_worktree`,
  HTTP `POST /worktrees` and MCP `repo worktree_create` (`create_worktree_shared`), and
  `POST /sessions/worktree` (which had no warm step until 2026-10-07). A disabled warm still goes
  through `run_background_warm_blocking` so a removal still stops the chain; it ends `done` with a
  `skipped` reason. A fifth creation path must pass a token too, or document why not.
- **Bounded parallelism**: `warm_candidates` runs every skip rule as a single-threaded pre-pass
  BEFORE any copy starts (symlink type, nested repo, destination containment, `to.exists()`,
  nesting inside an earlier candidate, the intermediate-symlink guard), then fans the copies out
  over at most `WARM_COPY_CONCURRENCY` (4) scoped threads. Keep that order: interleaving a check
  with the copies would reopen the symlink-planting class of bug. The nesting rule is explicit
  because the sequential copy got it implicitly from `to.exists()` (e.g. an externalBin sidecar
  inside an ignored binaries directory would otherwise be copied twice, concurrently). Warnings
  are slotted back in candidate order; only `on_progress` sees completion order. A panicking copy
  is caught per directory and becomes a warning.
- **Intermediate-symlink guard is shared**: `tuic_git::cow::first_symlinked_ancestor` is used by
  the warm and (through a thin wrapper) by `worktree_sync::sync_one` — same threat model (a branch
  commits a directory symlink at an intermediate component of an ignored path). A third writer into
  a freshly checked-out destination needs it too.
- **Events**: `worktree-warm-started/-progress/-completed` (`warm_with_events`, payload builders in
  `state.rs`, wire arms in `event_wire.rs`, dual-emitted through `AppState::emit_dual`) drive the
  sidebar "Warming…" badge; silent when nothing is copied. The badge matches rows by checkout path, never by branch.
- **Chain generations**: setup-status writes and the terminal `worktree-setup-script-completed`
  event are gated on `SETUP_CHAIN_GENERATIONS` (per `(repo, branch)`), with the map's read guard
  held across each write. A stale chain for a removed-and-recreated workspace therefore can neither
  wipe nor overwrite its successor's status nor resolve its successor's frontend waiter. That event
  now fires once per chain with `outcome` `completed` / `not_configured` / `stopped`, so the
  frontend waiter (`armSetupScriptWaiter`) is never left to its 20-minute timeout by a removal, an
  abort, or a repo the backend sees as having no script.
- **Known gap**: if the frontend believes NO script is configured while the backend runs one, the
  frontend does not wait and the Run Script can race it; the create response does not say whether a
  script will run. Removing a worktree mid-copy still cannot cancel in-flight copies (the token is
  checked before the copy starts, not during it).

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
- **A variant dimension checked for equality on a single cache slot is not the same as that dimension being part of the map key — even once it's checked correctly.** A later revision (2026-09-29, adding `DiffOptions`) added `include_subagents`/`options` as fields on `CachedReview` and correctly compared them before serving a hit — but the map itself was still keyed only by transcript path, one slot per path. Toggling "include subagents" (or a whitespace/case diff option) back and forth thrashed that single slot: each toggle was a correct cache *miss* (never served stale data), but it also evicted and recomputed instead of caching each variant, defeating `MAX_CACHED_REVIEWS`'s intent to hold several sessions' worth of reviews at once. Fixed by folding those fields into the map's actual key type (`(PathBuf, bool, DiffOptions)`), leaving only genuinely disk-derived freshness facts (`len`, `mtime`, the subagent-transcript fingerprint) as the equality check on a hit. When adding a new parameter that changes what a cached value *means* (not just whether it's still fresh), ask whether it belongs in the key (so multiple values can coexist) or the freshness check (so a stale one gets evicted) — they are not interchangeable, and getting this wrong doesn't corrupt anything, it just quietly defeats the cache.
- **A background-shaped tool call's result can report back several conversation turns after it was actually issued — attribute by when it was CALLED, not when its result LANDED.** `assign_turns` originally keyed a subagent's parent turn off the RESULT record's `promptId` (`tool_result_prompt_ids`), which is correct for an ordinary synchronous tool call (call and result share a turn) but wrong for a `meta.json`-flagged `requestShape: "background"` Agent/Task call: the assistant can move on to several more prompts before that call's result is finally reported, so every edit the subagent actually made during the ORIGINAL turn was misattributed to whatever LATER turn merely observed the delayed completion. Fixed by capturing a second map, `tool_call_prompt_ids` (`tool_use_id → promptId`, from the assistant record that *issues* the `tool_use` block, not the one that reports its result), and preferring it over `tool_result_prompt_ids`. Any future attribution logic that joins a call and its result through a shared id must check whether the transcript format allows that pairing to span turns before assuming "the result's own metadata" is equivalent to "the call's own metadata."

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
never plateaus. (Main landed the same fix independently — `TerminalGrid::screen_origin`, the
frontend's per-terminal `historyBase` — and the replay kept main's mechanism.) Every consumer that
needs to turn a stored `CommandBlock` row back into a live buffer-line row subtracts the
terminal's `historyBase` first (a negative result has been evicted) — see `terminals.ts`'s
`CommandBlock` doc comment. The heuristic (`synthesize_cc_block_events`) and transcript-dump
synthesizers build their rows from the same origin, not `history_size()`. Regression test:
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
`getCommandText`, subtracting `historyBase` from a stored block row): see
`src/components/Terminal/AGENTS.md`.

## Custom PTY Env Vars + Pane Shell-Readiness Gate (2026-09-24)

Two related features closing the p10k-wizard-hijack pane-spawn race
(`plans/p10k-wizard-hijack-agent-pane-spawn-race.md`): `AppConfig::custom_pty_env`
(user `KEY=value` pairs injected into every spawned PTY via the single choke point
in `spawn_pty_pair_with_retry`) and `tmux_routes::materialize`'s shell-readiness
gate (blocks until the freshly spawned shell reaches `SHELL_IDLE`, event-driven,
5s bound, fail-open). Full design: `docs/backend/pty.md`'s "Custom PTY
environment variables" and "Pane Shell-Readiness Gate" sections.

**A "materialize already returns this pane's session, skip everything else" fast
path must run the readiness gate too, not just the fresh-spawn path.** Found by
code review: `materialize()` records `pane.tuic_session_id` in topology *before*
its own readiness-gate call runs (so a second concurrent caller can find and
reuse the session rather than double-spawning) — which means a second concurrent
`materialize` call for the same pane, landing while the first call's gate is
still waiting, used to hit the "already materialized" early return and skip the
gate entirely. This is a real, not theoretical, shape: `tuic-cli`'s own
`respawn-pane` retry logic documents an eager-materialize race from a sibling
caller. Fixed by calling the gate on the fast path too — cheap when already idle
(the underlying `wait_for_shell_idle` returns immediately in that case).
**Any future "already have this, return early" fast path added near a
readiness/settling gate needs the same treatment** — a fast path is not exempt
from an invariant the slow path enforces just because it usually observes state
the slow path already established.

**A client-side timeout shorter than a server-side bounded-wait feature's own
timeout turns "slow but working" into an apparent client error.** `tuic-cli`'s
IPC client had one fixed 3s socket timeout applied to every request; the new
gate's `SHELL_READINESS_TIMEOUT_MS` is 5s. Without raising the client's budget
for this specific call (`ipc::post_with_timeout`, 8s), a legitimately slow (not
hung) shell startup — the literal motivating scenario for this feature — would
make the *client* time out and report failure before the *server's* own
fail-open path had a chance to return `Ok`. Any new server-side bounded-wait
feature reachable through this IPC client must check its timeout against
whatever fixed client-side budget the call goes through, not just against the
server's own request-handling timeout (if any).

**The readiness gate's timeout is one shared constant,
`mcp_transport::SHELL_READINESS_TIMEOUT_MS` — reuse it at any new gate call
site.** A review finding on the first version of this gate predicted that "a
future second readiness-gate call site would likely duplicate yet another ad
hoc constant instead of reusing one." (On the pre-rebase branch the gate was
also extended to a shared `pty::spawn_session_for_agent`; that function and its
callers — the `ai_terminal_*` MCP tools, the cron scheduler, the PR-review
watcher — do not exist on this codebase.) Since 2026-10-08 the plain-shell create
paths whose caller types into the new shell right away are gated too, through
`mcp_transport::gate_new_shell_session` (same primitive + constant, fail-open with a
warning log, skips a session that can never signal readiness): MCP `session
action=create`, MCP `repo worktree_create spawn_session=true`
(`create_session_in_dir`), HTTP `POST /sessions` and `POST /sessions/worktree`.
Agent spawns (`POST /sessions/agent`, MCP `agent action=spawn`, desktop
`spawn_agent`) are deliberately NOT gated — they exec the agent binary directly,
there is no shell startup to wait out. A shell with no OSC 133 integration is not
skipped: the 500 ms silence fallback still reaches `SHELL_IDLE` quickly (test
`http_create_session_without_shell_integration_is_ready_well_before_the_timeout`);
the one setting that used to disable zsh's integration and make every gate wait its
full bound, `ZDOTDIR` in `custom_pty_env`, is rejected. `tuic new` posts
`/sessions` with an 8 s client timeout for the reason in the previous paragraph.
If you add a new server path that spawns a shell and writes into it right away,
gate it with `gate_new_shell_session` + that constant.

**A test that subscribes to the event bus after a real materialized pane and
expects a specific event type to be the very next message is fragile the
moment anything makes the underlying shell run longer.** The readiness gate
above (and its fast-path fix) give real spawned shells more running
time before a test's next assertion, so ordinary background traffic (a
`PtyOsc133` prompt marker, most often) can land on the bus in between —
`rename_pane_is_idempotent_and_only_emits_on_real_change` actually flaked from
this; two sibling tests (`set_pane_accent_color_resolves_and_applies_when_already_materialized`,
`request_window_layout_emits_only_materialized_session_ids_in_pane_order`) had
the identical latent shape and were fixed proactively before they did. The fix
is always the same: drain-and-filter for the specific event type you care
about (a `while let Ok(event) = rx.try_recv()` loop, `break`ing when found, or
asserting the *specific* variant never appears rather than that the channel is
empty) — several tests in this file already did this correctly
(`materialize_applies_a_title_recorded_while_the_pane_was_still_virtual`'s own
comment names the exact same reason). When you add a test that subscribes
after a real materialize/spawn call, use that pattern from the start rather
than a bare single-shot `rx.try_recv()`.

**A real-PTY test that records a cwd fixture and reads it back must use a real,
existing directory once the code under test lets the shell actually run before
the read** — not a fictional path like `/explicit/repo`. This repo's own OSC 7
cwd tracking (`pty.rs`'s `parse_osc7_cwd`, `entry.lock().cwd = Some(cwd)`)
overwrites `PtySession.cwd` the moment a real shell reports its actual directory
— harmless when a test reads that field before the shell has had time to run
(the pre-readiness-gate case), but three `tmux_routes.rs` cwd tests broke the
instant the shell-readiness gate above started actually waiting for the shell to
reach a prompt, because a real shell given a non-existent directory falls back
to some real directory (its own `$HOME`, empirically) and OSC 7 duly reports
*that* instead of the fixture string. Fixed by switching the fixture to
`tempfile::tempdir()` for whichever cwd actually becomes the real spawn target
in each test (a "loser" cwd that's never actually spawned into, e.g. a session's
own cwd in a test proving a window's cwd wins instead, can stay fictional — only
the winning value needs to be real). If you add a new PTY-spawn test that reads
back `PtySession.cwd` after any code path that can give the shell real running
time, use a real directory for whichever cwd you expect to observe. (On this tree
`spawn_pty_session` also refuses a cwd that does not exist with a 400, so a fictional
spawn-target cwd now fails even earlier; a fictional "loser" cwd that is only recorded in
topology and never spawned into is still fine.)

## MCP Handshake Readiness Race (`agent action=spawn`, 2026-09-29)

A sibling race to the shell-readiness gate above, but on a completely different signal.
`mcp__tuicommander__agent action=spawn` used to embed the initial prompt directly in the
spawned `claude` process's launch argv (same as every other agent type still does) — which
meant a freshly spawned agent could answer its very first turn *before its own MCP client had
finished its `initialize` handshake with this server*, seeing zero `mcp__tuicommander__*`
tools for that turn. Reproduced live: a task designed to answer in ~2s with no tool call saw
zero tools; the identical spawn given a task that needed real tool calls (and so took longer)
reliably saw the full tool list, because MCP binding had time to complete first. The race is
only user-visible for a task trivial enough to never need to consult its tool list — there is
nothing to notice a missing tool if you never look.

**Fixed by decoupling "launch the process" from "deliver the initial task," scoped to
`agent_type == "claude"` AND non-print-mode spawns only** (`should_defer_prompt_for_mcp_bind`,
`handle_agent_with_parent_cwd`'s `"spawn"` arm in `mcp_transport.rs`). Print-mode is one-shot
with no later delivery opportunity, so its prompt stays in launch argv unchanged; other agent
types' MCP-tool-availability characteristics were never verified, so they're unchanged too.

**The flag must be threaded into EVERY branch that can produce a claude default-template
argv, not just one.** A first version of this fix only checked `defer_prompt_for_mcp_bind`
in the manual branch reached when a caller explicitly passes `binary_path` — a code review
caught that this left the fix dead for the realistic, undecorated `agent_type: "claude"` call
(no `binary_path`): that shape resolves `Some(ResolvedRunConfig { args: None, .. })` via
`resolve_run_config`'s Pass 2 and takes the shared `default_prompt_args`-template branch
instead, which never read the flag at all. Both of that version's own integration tests
happened to pass `binary_path` for their stand-in binaries, incidentally routing them into the
one branch that worked and masking the gap. Fixed at the real choke point: `finalize_spawn_args`
(and its caller `compose_mcp_spawn_args`/`McpSpawnArgs`) now takes `defer_for_mcp_bind: bool`
directly and returns TWO independent deferred-prompt slots —
`(argv, deferred_for_pending_injection, deferred_for_mcp_bind)` — never both `Some` for the same
spawn, kept separate because they're flushed by two unrelated readiness signals (BUSY→IDLE
screen detection vs. this MCP bind). If you add a FOURTH way to reach the default-template argv
shape, thread this flag into it too, or repeat this exact bug.

**`wait_for_mcp_identity_bound` cannot reuse `peer_agents.contains_key(...)` as its
predicate** — `agent action=spawn` pre-inserts a `PeerAgent` row for every managed child at
spawn time, before the child process even exists, with `mcp_session_id: String::new()`,
specifically so the peer is addressable (`list_peers`, `send`) independent of whether its own
MCP bridge ever connects. That row exists from the very first instant, so `contains_key` alone
would make the gate a permanent no-op. The real signal is whether `mcp_session_id` has been
overwritten by a real `apply_initialize_identity` bind. This is a short 50ms poll, not
event-driven like `wait_for_shell_idle` — `apply_initialize_identity`/`bind_peer_identity_locked`
are pure `DashMap` writes with no event-bus notification on a fresh bind, so there is nothing
to subscribe to.

**Delivery reuses `agent action=send`'s own primitives** (`push_agent_inbox` +
`pty::deliver_notice_to_managed_pty(..., PEER_MAIL_WAKE)` + `pty::settle_terminal_delivery`),
called directly server-side rather than through the normal caller-identity-resolved `"send"`
handler — the `"send"` handler's live-SSE-channel fast path and Waiter/orchestrator branches
are unreachable here anyway (nobody has registered a wait or a channel for a session that
hasn't started yet). It also replicates `send`'s existence guard: the deferred-delivery task
re-checks `state.peer_agents.contains_key(...)` under `PEER_IDENTITY_BIND_LOCK` immediately
before calling `push_agent_inbox`, and skips delivery entirely if the session was killed/retired
during the up-to-5s wait — a security review's own words: "never file under an identity removed
a moment later." `from_tuic_session` on the delivered message is the caller's real identity if
known, or an empty sentinel otherwise — never the recipient's own session id (a self-referential
"from myself" message this file's `enqueue_state_change_to_parent` already guards against for
the identical reason).

**The wait runs in a detached `tokio::spawn`, not inline before the tool call returns** —
`agent action=spawn` must keep returning immediately regardless of how long the identity-bind
wait takes; do not move it back to an inline `.await` before returning, which would add up to
`MCP_IDENTITY_BIND_TIMEOUT_MS` (5s) of latency to every affected spawn. Because of this, the
spawn response's `prompt_delivery` field (set when `prompt_deferred_for_mcp_bind` is true) is
the only signal a caller gets that the prompt hasn't landed yet — and it explicitly warns that
a follow-up `agent(action=send)` issued right after spawn is NOT gated on the same wait and can
arrive in the child's inbox before this original prompt does. This is a known, accepted
ordering hazard, not a bug: closing it would mean either blocking `spawn` itself on the wait
(reintroducing the latency this design avoids) or a queueing mechanism bigger than this fix's
scope.

**A test that mutates `PATH` to make `detect_agent_binary` resolve to a stand-in binary
(`agent_spawn_defers_claude_prompt_via_the_ordinary_no_binary_path_call_shape`) is only safe
under `cargo nextest`'s per-test-process isolation, not plain `cargo test`'s shared-process
thread model** — a different, unrelated, concurrently running test can transiently observe the
mutated `PATH` and get routed to the same stand-in binary. Confirmed to cause a real hang during
development. `check-gate.sh`/CI already run via nextest, so this is safe in the gate that
matters, but don't run this one test alongside the rest of this file via plain `cargo test`
without pinning `--test-threads=1` or running it in isolation.

**Also fixed (2026-09-29, second pass): `POST /sessions/agent` and the desktop
`agent::spawn_agent` command** — the other two paths that launch an interactive Claude with
the prompt as its positional argv. Both already bind `$TUIC_SESSION` (`bind_pty_identity`,
applied after caller env so identity always wins) and seed a `SessionState`; only the
prompt-in-argv race applied. Both now compute `should_defer_prompt_for_mcp_bind` (explicit
`agent_type`, else the `claude` default; never when the caller passes explicit `args`, which
never carry the prompt) and hand the withheld prompt to the shared
`spawn_deferred_prompt_delivery`. Both responses/flows gained one more fix on the way:

- `agent_routes::spawn_agent_session`'s common default-to-claude case (no `agent_type`, no
  `binary_path`) used to leave `session_state.agent_type`/`hook_instrumented` unset; it now
  seeds the effective type (`effective_agent_type`). The HTTP response carries
  `prompt_delivery` when the prompt was deferred.
- `agent::spawn_agent` seeds `SessionState` with the effective type too (it used to seed only
  an explicit `agent_type`), which `deliver_notice_to_managed_pty`'s wake path requires.
  **Test-coverage asymmetry, on purpose:** the HTTP route has end-to-end tests; the
  `#[tauri::command]` does not (this codebase has never tested one directly — no
  `tauri::test` scaffolding). It shares the fully-tested
  `should_defer_prompt_for_mcp_bind`/`spawn_deferred_prompt_delivery` with the other two
  paths, plus a `to-test.md` item for a real UI spawn.

**!! Known risk, mitigated 2026-10-08 (fixup B2.2): the deferred prompt could hang a spawn
for 500+ s.** Pre-rebase wip's own to-test notes recorded real-task Claude spawns sitting idle
for minutes on this path: the forced `PEER_MAIL_WAKE` write could land before the TUI was ready
(or inside its trust dialog), `note_submitted_input` still recorded it as a submitted turn (a
Protocol-rank busy latch held until `PROTOCOL_STALE_TIMEOUT`), and nothing retried. Also, the
HTTP and desktop paths never inserted a `PeerAgent` row, so a bind slower than 5 s LOST their
prompt, and the desktop path waited on the PTY key while a restored tab's child binds under its
persisted `$TUIC_SESSION`. Now: every spawn path calls `register_spawned_peer` (keyed by the
child's real identity), the forced write waits for `DEFERRED_PROMPT_QUIET_MS` of PTY silence, and
a `DEFERRED_PROMPT_WATCHDOG_MS` (20 s) watchdog types the prompt itself
(`pty::force_type_deferred_prompt`) when `turn_epoch` has not advanced since the notice and the
message is still unread — claiming it out of the inbox first (`take_unread_agent_message`) so it is
never delivered twice, and refusing (message kept) over a draft or an open dialog. The task holds
no per-session map entry. Do not remove the watchdog without replacing it with an equivalent
"did a turn start?" check; see the to-test.md entry for the live verification still owed.

The inline deferred-delivery logic is `pub(crate) fn spawn_deferred_prompt_delivery`
(`mcp_transport.rs`, next to `wait_for_mcp_identity_bound`) — all three spawn paths call the
same function rather than each reimplementing the wait/lock/push/deliver/settle sequence.
`should_defer_prompt_for_mcp_bind` is `pub(crate)` for the same reason.

**A Claude spawned with no prompt sits on its own `SessionStart` busy latch forever** —
`SessionStart` derives `state=busy` and no turn ever starts to produce the matching idle, so
`deliver_notice_to_pty`'s idle claim can never succeed and the wake notice would stay queued
forever. `stuck_on_pre_first_turn_session_start` (`pty.rs`: agent session, `turn_epoch == 0`,
busy evidence still `hook-busy`) recognizes exactly that case and writes the notice directly
(counted as the first submitted line); a non-zero `turn_epoch` always falls back to queueing.

**Fixed same day: `agent::spawn_agent`'s caller-supplied `pty_config.env` could silently
override `bind_pty_identity`'s own `TUIC_SESSION`/`TUIC_CONFIG_DIR`.** The per-key env loop
for "feature flags configured in Settings → Agents" ran AFTER
`bind_pty_identity`/`inject_worktree_env`; `CommandBuilder::env` takes the last write for a
key, so a flag named `TUIC_SESSION` would win and could rebind another live session's identity
via `bind_peer_identity_locked`. Only reachable via this app's own Tauri IPC — a footgun, not
an outside exploit. The loop now runs BEFORE them, so TUIC's identity/worktree env is always
authoritative. No new test (same untested `#[tauri::command]` closure as above).

There is no other spawn path to fix on this codebase: there is no
`pty::spawn_session_for_agent`/in-process agent loop (cron scheduler, PR-review watcher,
`ai_terminal_*` tools), and a caller typing `claude "task"` into an existing shell has no
structured prompt to defer.

## Agent Session Management

TUIC tracks each agent's session ID for resume-after-restart. Two strategies coexist:

**Discovery-based (Claude, Gemini, Codex, Grok).** TUIC does NOT inject `--session-id` at launch — the agent creates its own ID. TUIC discovers the active session and re-checks it on every idle↔busy transition and every 30s poll, so an agent that starts a replacement session is picked up. Resume uses `agentSessionId` (disk-discovered), not `tuicSession`.

Discovery has two tiers, and the difference is not cosmetic:

| Tier | Agents | Source |
|---|---|---|
| **Exact** | Claude, grok | the agent's own pid→session registry: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json`, `~/.grok/active_sessions.json`. `get_session_leaf_pid` returns the agent's pid (verified: it stays the agent even while a tool subprocess runs) |
| **Heuristic** | Gemini, Codex, and any Claude/grok too old to publish a registry | newest unclaimed session file under the project dir |

**The heuristic is not a binding, and no amount of tuning makes it one.** N agent tabs in one folder all scan the same directory, so whichever tab polls first takes the newest file regardless of whose it is; the rest take another tab's session or nothing. `claimed_ids` only stops two tabs holding the *same* id — it cannot tell whose is whose. That is issue #119: measured on a live instance, 3 of 6 Claude tabs held no id and one held a different tab's, so every tab resumed with `claude --continue` into the same conversation.

**Further hardened 2026-09-29 for hook-instrumented Claude specifically.** The pid-registry
"Exact" tier above (commit `9473819c4`) closes the common case, but it's still a file read plus
a pid match — a step behind at the instant a new session starts, and it still falls back to the
mtime heuristic for a Claude build with no registry file. `tuic-hook`'s `ccsession` verb
(see "Exit-time resume banner" below) gives a THIRD source that's stronger than either
discovery tier: it's tied directly to this exact pty's own OSC 7770 stream, so it can never read
another tab's entry — there's no file to scan, no pid to match, nothing to race. `SessionState
.agent_session_id` now carries this on the wire (previously internal-only, added purely for the
exit-banner snapshot), and the frontend prefers it unconditionally: `useAgentPolling.ts`'s
`applySessionState` writes it straight into `terminalsStore.agentSessionId` and sets
`agentSessionIdIsAuthoritative`, and `detectAgentForTerminal`'s own disk-discovery block is
skipped entirely while that flag is set — both at entry AND with a second freshness check
immediately before the disk-discovery write, since a hook-reported push can land during either
of that function's two awaits (`get_session_leaf_pid`, then `discover_agent_session` itself) and
must never be clobbered by a now-stale heuristic result resolving after it. The flag resets
alongside `agentSessionId` at the same two places that already clear it (an agent-type
transition in `detectAgentForTerminal`, and the whole-PTY-exit handler in `Terminal.tsx`), so a
NEW agent in the same pane — hook-instrumented or not — starts clean and can either get its own
authoritative id or fall back to discovery correctly. This is additive, not a replacement: a
non-Claude agent, or a Claude session with no hook installed, still resolves through discovery
exactly as before.

**Finding the id is only half of a resume — the other half is which store holds it.** A shell alias is expanded before `exec`, so a run config that reads `c2` with an empty `env` is not what runs: the process is `claude --dangerously-skip-permissions` under `CLAUDE_CONFIG_DIR=~/.claude-private`, and TUIC never sees the assignment. While the agent lives, discovery reads argv and env off the process and rebuilds the real command into `agentLaunchCommand`; at restore time the pid is gone and that string is the only record left. Do not re-derive the config dir from the run config: `c` and `c2` differ *only* in an env var neither one declares, so the default config verifies an id in `~/.claude` and then sends `--resume` to a binary that reads `~/.claude-private` — Claude answers `No conversation found with session ID`, and the transcript is sitting untouched in the other directory.

So when you add a discovery-based agent, look for a pid registry *first*. Codex 0.153 has none — `session_index.jsonl` carries only id/name/updated_at, the rollout `session_meta` has no pid, and there is no `--session-id` flag — so it stays heuristic on purpose, cwd-scoped by the rollout's recorded `cwd`. Gemini is worse and knowingly so: its scan visits every project's `chats/` dir, so it is not even cwd-scoped (see the `DEFERRED` note on `discover_gemini_session`). Do not close either gap by guessing a path-hashing scheme — verify against a real install.

**Forced injection (Goose).** Shell wrapper injects `--name $TUIC_SESSION` into `goose session/run` commands. The TUIC tab UUID IS the goose session name. Discovery returns `None` (SQLite storage, no filesystem scan). Resume uses `tuicSession`.

**No session tracking (Aider, Amp, Cursor, Droid, OpenCode, pi).** Either no local session files, cloud-only, or no UUID-based resume. `TUIC_SESSION` env var is available but unused.

When adding a new agent: choose discovery-based if the agent writes session files to disk (add `sessionDiscovery` to `agents.ts` and a Rust `discover_*_session` to `agent_session.rs`). Choose forced injection only when discovery is impossible (e.g., SQLite-only storage).

All of the above describes the **PTY** transport. The AI Chat `ego` uses ACP and is not an `AgentType`. Read SPEC.md → "PTY versus ACP routing" before wiring either transport: a session has exactly one transport, with no fallback.

`ego` has three distinct faces: standalone CLI outside TUIC; AI Chat over ACP as an orchestration peer with a host-issued, durable `TUIC_SESSION` but **no** tab, PTY or terminal parser; and a separately launched terminal CLI over PTY, which may have an `AgentType`. The ACP peer reaches terminals and repositories through TUIC's MCP bridge. Its peer identity permits mail and child-parent routing, but does not turn the AI Chat conversation into a terminal or place it in tab routing, split panes or PTY agent-state detection.

**Exit-time resume banner (Claude, hook-instrumented only).** A third, narrower
mechanism, layered on top of discovery rather than replacing it: while a
hook-instrumented Claude session is live, `tuic-hook`'s `ccsession`/`cctitle`/`cwd`
verbs write the session's live id/title/cwd directly into
`SessionState.agent_session_id`/`agent_session_title`/`agent_session_cwd`
(`pty.rs`'s OSC 7770 verb arms) — synchronously, in the PTY reader thread, not
through the async event-bus accumulator, so ordering against the exit signal below
is guaranteed. The hook-reported id/title is authoritative for this feature; when no
`cctitle` was ever sent, the title falls back to the last cleaned OSC 0 title
(`SessionState.agent_osc_title`, written by `osc_title.rs`'s `apply_osc_title`
independently of whether a custom tab name would otherwise block that write).

The snapshot itself is taken in exactly one place: `snapshot_resumable_session_on_agent_exit`
(`pty.rs`), called from `apply_foreground_agent_observation` at the moment the
foreground observation (`refresh_session_agent` — per PTY chunk, the headless
foreground timer, and HTTP/IPC foreground queries) sees the shell reclaim the
foreground and clears the agent identity; an armed run-config preset that never ran
is not an exit and takes no snapshot. It builds `SessionState.resumable_session` (a
`ResumableSession` — agent_type/session_id/title/cwd/end_reason) before clearing the
live trackers; the frontend reads it from the normal `SessionState` payload
(`session-state-changed` push and the lifecycle catch-up poll) — no new transport
surface. `end_reason` (from `tuic-hook`'s `ccend` verb,
SessionEnd's raw `reason` string) is diagnostics-only and never gates whether a
`resumable_session` is produced. A later `ccsession` for a different id (a fresh
agent launch in the same pane) clears a stale `resumable_session`, and
`SessionCreated` resets every field this feature added.


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

**Per-session attribution + SESSION OVERLOAD trigger, and the lag-disconnect fix
that motivated it (2026-09-28).** Diagnosing a flickering-title/high-CPU report on
one tab took ~45 minutes of manual correlation across `debug logs`,
`explain_state`, and `lsof`: a *different* session (a live Claude Code Agent
Teams/tmux-swarm test) was driving sustained 108%→212%→174% process CPU spikes for
several minutes via its own screen/lifecycle repaint churn, and — as an unrelated
but compounding symptom — a completely different tab's per-session WebSocket
broadcast (`AppState::subscribe_pty_events`, `mcp_http/session.rs`'s
`handle_ws_session`/`handle_ws_grid_session`) fell behind and never recovered:
`WebSocket broadcast lagged`/`grid WS broadcast lagged` climbed from 419ms to
12.4s with no self-correction, because the code that handled a lagging broadcast
receiver only ever logged a warning and looped forever. Nothing at the time named
*which* session was responsible for the CPU, and nothing bounded the lag.

Two fixes, plus new attribution:

- **`cpu_watchdog::should_disconnect_for_lag`** is now the shared decision behind
  every per-connection consumer of a `tokio::sync::broadcast` channel that can
  lag — the two per-session WS handlers above, and the global-bus SSE handler
  (`mcp_http/sse_routes.rs::sse_events`). Once a receiver's lag crosses
  `MAX_CUMULATIVE_LAG` (1000, ~4x a channel's 256 capacity) or it lags 3 times in
  a row with no clean recv between (`MAX_CONSECUTIVE_LAG`), the connection is
  closed instead of left to keep re-lagging — the client reconnects and gets a
  fresh snapshot, the same way it already has to handle any other disconnect.
  The grid *frame* `watch` channel was never part of this bug — it already
  resyncs correctly on a gap (`watch_dropped_frames` → a fresh full frame); only
  the lifecycle/diff-event lanes had no equivalent recovery.
- **Per-session attribution counters** (`AppState::session_maps`:
  `session_event_counts`, `session_output_bytes`, `session_ws_lag`) are bumped at
  the same single choke points the events/bytes/lag already flow through
  (`emit_pty_event`, the PTY reader's `bytes_emitted` bump, and the disconnect
  logic above) and read-and-reset once per watchdog tick
  (`state::drain_counter_map`) — a rate, not a lifetime total. `state::peek_counter`
  gives an on-demand read that does *not* reset them, for `GET
  /diagnostics/sessions` (below).
- **`SESSION OVERLOAD` log line** — independent of the existing process-wide
  CPU-spike trigger, runs every tick: if any single session's per-tick event
  count, output bytes, or WS lag crosses a hard threshold
  (`SESSION_EVENT_RATE_THRESHOLD`/`SESSION_OUTPUT_BYTES_THRESHOLD`/
  `MAX_CUMULATIVE_LAG`), it's named directly — this is the exact gap that let
  the swarm-test session drive minutes of elevated CPU without ever being
  individually flagged, since process-wide CPU alone never says which session
  caused it.
- The existing `CPU SPIKE` line now also carries `top_sessions_by_event_rate`,
  `top_sessions_by_output_bytes`, and `top_sessions_by_ws_lag` (top 5 each,
  `cpu_watchdog::top_n`) — computed from the same per-tick drain `SESSION
  OVERLOAD` uses, not a second one (draining twice in one tick would make
  whichever ran second see zeros).
- **`GET /diagnostics/sessions`** (new, mirrors `GET /diagnostics/markers`'s
  per-session-array shape) answers "who's hot right now" on demand, instead of
  only after the fact in a log line — a peek (not a drain), so reading it never
  disturbs the watchdog's own next-tick rate calculation.

Deferred, not forgotten: `in_flight_stuck` (the grid IPC ack gate) still reports
presence only, not how long a session has been stuck — weighting it by duration
would need a new timestamp on `GridGate` and wasn't worth the added surface for
this fix. If a future incident needs to tell "routinely backgrounded" apart from
"pathologically stuck" on this specific axis, that's the next thing to add here.

**Four real bugs a code review caught in the first version of this feature,
all fixed:** (1) `GET /diagnostics/sessions` originally listed any session id
that had EVER appeared in the three counter maps, because `drain_counter_map`
zeroes a counter's value but never removes its key — so a session the
watchdog had already drained back to zero sat there forever as a dead
all-zero row, exactly the outcome the endpoint's own doc comment said it was
avoiding. Fixed by filtering to entries with at least one nonzero value (or an
outstanding grid frame) at read time, since that's the only place "the
watchdog reset this" and "this never had activity" can be told apart.
(2) `SESSION OVERLOAD` had no cooldown analogous to `CPU SPIKE`'s
`COOLDOWN_BETWEEN_REPORTS` — a session sustaining overload for the
`cddded98` incident's actual multi-minute duration would have logged one
near-duplicate line every single tick the whole time. Fixed with
`SESSION_OVERLOAD_COOLDOWN` (60s), keyed per `(session_id, axis)` so one loud
session/axis never silences a report about a different one.
(3) `SESSION_EVENT_RATE_THRESHOLD`/`SESSION_OUTPUT_BYTES_THRESHOLD` compared a
raw per-tick count against a fixed constant with no regard for how long the
tick actually took — since diagnostic mode doubles the tick length
(`POLL_INTERVAL` 5s → `DIAGNOSTIC_POLL_INTERVAL` 10s), the same sustained
per-second rate used to take twice as long to trip the trigger the moment
diagnostic mode was turned on to investigate a live problem — backwards from
what enabling it should do. Fixed with `normalize_to_nominal_tick`, which
scales each raw count to what it would have been at a nominal-length tick
using the tick's real elapsed wall-time before comparing. Deliberately NOT
applied to the `ws_lag` axis — a lag backlog can build in a fraction of a
second and is exactly as bad regardless of tick length, so normalizing it
down would make a genuinely bad connection look artificially fine during a
long tick.
(4) `cleanup_session` used to remove a session with a bare
`.remove(session_id).is_some()`, dropping the returned `Mutex<PtySession>`
(and its `Box<dyn portable_pty::Child>`, which on Unix wraps a plain
`std::process::Child`) with no `kill()` and no wait — `std::process::Child`
does not terminate its process on drop, so an agent that ignores Ctrl-C
(`close_pty_core`'s own comment already named this) leaked its real OS
process, and the reader thread still blocked reading its now-orphaned PTY
master, for as long as that process happened to keep running on its own —
potentially forever for a long-lived shell/agent, not merely "until the
tombstone sweep reaps it" as originally assumed. This was worse than a
diagnostics-only quirk: the new per-session counters just made it newly
*visible* (an orphan's reader thread re-creating `session_event_counts`/
`session_output_bytes` entries via their lazy `.entry().or_default()` for a
session every caller believed gone), but the underlying process/thread leak
predates this feature entirely. Fixed by extracting `close_pty_core`'s
existing wait-then-SIGKILL-with-foreground-process-group-kill fallback into
a shared `terminate_child_with_grace`, now used by both `close_pty_core` and
`cleanup_session`. Regression test:
`pty::tests::cleanup_session_actually_kills_a_process_that_ignores_everything_but_sigkill`
(spawns a real `sleep 5` and asserts its pid is actually gone afterward, not
just that the bookkeeping says so).

**Boundedness (added when this landed on main):** every counter key is pruned on
the watchdog tick if its session is no longer in `session_maps.sessions` (after the
drain, so a session's last tick is still reported) — a reader thread or a late
`emit_pty_event` bumping a counter after `remove_live_session_state` would
otherwise re-create a key nobody ever removes; the `SESSION OVERLOAD` cooldown map
drops entries older than `SESSION_OVERLOAD_COOLDOWN` each tick; the hot-path bumps
use `get()` first and only allocate a key on a session's first bump.

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

Working / idle / awaiting is decided from bytes an agent writes **once**, and
nothing retains them in the shape the decision saw. The ring holds 2 MB
(`OUTPUT_RING_BUFFER_CAPACITY`) and the VT log 10.000 lines
(`VT_LOG_BUFFER_CAPACITY`), so a busy agent still rolls past both, and what
survives is rendered rows rather than the chunk boundaries and timing the
detector actually read. Do not reason about the code first — record the stream,
then replay it.

(The 8192 that looks like a ring size is the default byte `limit` of
`GET /sessions/{id}/output`, `mcp_http/session.rs:460` — a page size, not a
retention bound.)

**Ask the running session before reproducing blind.** `GET /sessions/{id}/explain-state`
(desktop: `explain_session_state`; MCP: `debug action=explain_state`) is a read-only dump of
exactly what produced the current badge: the held ranked evidence per axis, whether `decide()`
would flip the shell state right now, which rung of the `agent_state` ladder won, the
screen/silence-timer bookkeeping, the last `Notification` classification, and an always-on
64-entry decision trail — including **rejected** evidence attempts and what outranked them
(`record_busy`/`record_idle`'s `bool` return, previously discarded everywhere). A rejection is
usually the actual answer to "why didn't the badge update," and it survives the 8 KB output-ring
window closing, since the trail lives on `SilenceState` for the session's whole life, not in the
output buffer. Full design in `docs/backend/pty.md`'s "Session State Explain" section. This
still doesn't replace capturing — the trail explains a *decision*, not what the agent actually
printed — so a genuinely new mis-detection still needs the capture workflow below.

```bash
curl -X POST localhost:9876/diagnostics/capture -H 'content-type: application/json' \
     -d '{"enabled":true}'                      # every session
     -d '{"enabled":true,"session_id":"<id>"}'  # one session
curl localhost:9876/diagnostics/capture         # state + bytes written per session
```

Captures land in `<config dir>/captures/<session-id>.tcap`, capped at 512 KB each. TUICCAP2 preserves the initial terminal rows/columns plus output/input direction, original chunk boundaries, ordering, and monotonic timestamps. The decoder remains backward-compatible with geometry-less TUICCAP1 and legacy output-only `.raw` fixtures; a faithful replay of either old format must supply the observed geometry explicitly rather than silently assuming 41x128.
Off by default (one relaxed atomic load per chunk when off) — code in
`src-tauri/src/pty_capture.rs`.

**The tap is one global switch, but the UI no longer has to poll to see it change.** Toggling it — via `POST /diagnostics/capture`, the `set_pty_capture` Tauri command, the tab context menu's "Capture Session", or the Command Palette's "Toggle diagnostics capture (active tab)" (`isPerfDebug()`-gated, `actionRegistry.ts`) — dual-emits `AppEvent::PtyCaptureChanged { enabled, session_filter }` from `pty_capture::set_enabled_in_config_dir`, the single app-side mutation point every entry point funnels through (the engine itself is `tuic_terminal::pty_capture`, which has no `AppState`). Every open tab's recording badge (`TabViews.tsx`, next to the standby badge) updates live off that event (`useAppInit.ts` → `ptyCaptureStore.applyStatus`) regardless of which entry point flipped it — including a raw curl from outside the app. `session_filter: None` means every session is being recorded, so `ptyCaptureStore.isRecording(id)` treats a `null` filter as "yes, this one too," not just an exact match.

**`applyStatus()` merges, it must never fully replace.** A code-review pass on this feature (2026-09-24) caught that it originally did — the push event's payload only ever carries `{enabled, session_filter}` (no `dir`/`sessions`), and reusing `refresh()`'s own full-replace `adopt()` for it silently wiped whatever byte counts a prior `refresh()` had populated. This was reachable in a single window with no other window involved at all: `toggle()`'s own `invoke("set_pty_capture", ...)` call is exactly what makes the backend emit this event, and the ordering between that invoke's own response and this window's async event listener processing the resulting push is not guaranteed — so `toggle()`'s own `bytes(sessionId)` read (used for the "N KB written" stop toast) could read 0 even though the real `.tcap` file has content. Fixed by merging only `enabled`/`session_filter` into the existing signal, leaving `dir`/`sessions` untouched — those are `refresh()`'s job alone. If you add a second push-event-consuming store method anywhere in this codebase, check whether it's merging into partial state or replacing wholesale; whichever existing full-status adopter it's tempting to reuse is very likely the wrong shape for a partial payload.

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

A formatting-only change (`cargo fmt`: whitespace, line breaks, trailing commas)
no longer trips it, and neither does adding a missing `#[test]` to an otherwise
unchanged function. For a rename or a refactor that genuinely needs no capture,
say so and move on:

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
| OSC 777 `notify` | agent's own native desktop notification — **unconfirmed to ever actually fire** | unambiguous `needs your permission` / `approval required` wording only; Claude's generic `is waiting for your input` also follows an ordinary completed turn and never sets awaiting (the OSC 7770 `Notification` classification above handles the confirmed-live path for the same ambiguity) |
| `Enter to select` footer | rendered screen | Ink dialogs, including hook-instrumented sessions through the full-screen presence recovery; the changed-row parser's heuristic copy is suppressed for hooked agents |

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
pickers, skill menus, anything with `Type something` / `Chat about this`) uses
the open Ink footer's full-screen presence recovery. The generic OSC 777
notification is insufficient evidence: it also arrives after normal prose at
the ready composer. Unambiguous permission notifications remain raw-stream
signals because the VT parser consumes escape sequences before clean-row parsing.

**Every signal that sets awaiting needs a path that clears it.** The badge is
`SessionState.awaiting_input`, not an event, and it is sticky by construction —
whatever sets it owns nothing until something retracts it. These paths clear it:

| Clear | Fires on | Misses when |
|---|---|---|
| `user-input` | a non-empty typed line | the answer is a bare Enter |
| `status-line` | a parsed busy tick (low-confidence only) | busy is inferred from screen movement |
| `resolve_choice_prompt_input` | an option keypress | no `choice_prompt` was ever set |
| `question-cleared` | silence timer sees the question gone from the screen | — (the backstop; low-confidence only) |
| `protocol-question-cleared` | Claude renders `User declined to answer questions` after Esc and returns to a ready composer | a dialog is still open or a different question replaced it |
| `progress-superseded` | the same PTY reports `progress done` or journals a `delegated` hand-off after a `progress blocked` | a dialog or any other confident question replaced the blocked one — only `source=progress-blocked` evidence clears |

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

**Do not exclude hook-instrumented sessions from dialog recovery.** A retained
PTY capture on 2026-09-21 showed `state=awaiting`, then fifteen `state=busy`
markers, then an open dialog while the session reported working. Recovery must
use the last pending Question/UserInput event in a chunk, not the presence of
any Question: a later clear supersedes it. Keep the column-0 footer anchor and
the guards against an existing badge, choice prompt, or pending question.

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

**Update 2026-10-01 — an idle teammate is still listed `running`, so the flag alone can't be
trusted for teammates.** Measured in `.claude/hook-debug.log`: a lead's `Stop` keeps listing a
finished, idle teammate as `{type: "teammate", status: "running"}` for as long as the teammate
exists (minutes), and a teammate's completion fires **no hook in the lead** (it is its own Claude
session; `SubagentStart`/`SubagentStop` never fire for it). So a teammate counts as work only
while its own terminal is busy: `tuic-hook` sends `bgtasksummary` (`type/status*N` pairs),
`pty.rs` stores `DeclaredTaskSummary`, and `SilenceState::declared_background_work_for_epoch_with`
takes a lazy closure backed by `AppState::lead_teammates_may_be_working(lead, declared)` (tmux
topology + shell-state atomics only — never a `SilenceState` lock, which the caller holds). It is
fail-safe: more declared teammates than linked panes means "working", so a missing link is never
worse than the pre-change behavior. Lead↔teammate
linkage is `TmuxPane::lead_session_id`, sent by `tuic-cli` as `origin_session_id`; the
accumulator republishes the lead when a teammate's state moves. Read any *new* consumer of
`declared_background_work` through the `_with` variant (the plain one is the conservative
"everything counts" read). A background **subagent** is different: it drops out of the next
`Stop` list by itself, and its `SubagentStop` payload still lists itself as `running` and fires
*after* the parent's `Stop`, so `SubagentStop` is not a usable clear signal. Full write-up:
`docs/backend/pty.md`'s "An idle Agent-Teams teammate does not count as work".

**A handful of git-behavior-dependent unit tests can fail purely from the
running machine's global git config** (e.g. `merge.ff = only` turns an
expected merge conflict into a hard refusal; an `insteadOf` URL rewrite makes
a raw-config read disagree with `git remote get-url`) or from ambient `TMPDIR`
state, unrelated to any code change. If a regression shows up only in
git-merge/remote-URL tests with no plausible connection to your diff, suspect
the environment before the code. Seen on Boss's machine (`merge.ff = only` plus a
global `url.https://github.com/.insteadOf git@github.com:`):
`repo_watcher::tests::test_real_fingerprint_moves_across_stage_commit_conflict_and_clean`,
`git::tests::repo_info_status_reports_unmerged_paths_as_conflict`,
`git::tests::test_read_remote_url_matches_git_remote`, and in tuic-git
`a_squashed_pr_with_a_merge_commit_needs_github_proof` and
`monitoring_gitpoll_merge_refreshes_on_ref_move_and_keeps_dirty_badge`. A test temp
dir created *inside* the checkout is the other trap: `find_repo_root` walks up from it
into the real repo, so the test sees this checkout's own branch/`TUIC_MAIN_REPO_PATH`
— create fixtures under the test temp root, not under the repo. The default test temp
root is outside the checkout since 2026-10 (`$TMPDIR/tuic-tests/...`); the trap returns
only if you set `TUIC_TEST_TMP_BASE` to a directory inside it.

## Killing a Child Process: Signal the Group, Confirm the Reap, Bound Every Wait

`tunnels/supervisor.rs`'s `graceful_kill` (SIGTERM, wait up to 5s, escalate to SIGKILL) used to
signal only the direct spawned child's PID. **A single-PID signal is not enough whenever the
child can itself fork a further child that doesn't die with its parent** — found via a real
leaked-process bug (2026-09-23): a fake-ssh test fixture was a multi-line shell script (`#!/bin/sh`
... `sleep 3600`), and `/bin/sh` forked `sleep` as its own child rather than exec-replacing itself
with it. SIGTERM to the shell's PID killed the shell; the orphaned `sleep 3600` kept running for a
full hour, invisible to every test assertion (which only watched the *supervisor's* status, not
the real OS process) until it surfaced as a completely unrelated symptom — inheriting a duplicate
of an enclosing shell script's pipe file descriptor and making a `check-gate.sh` run *appear*
hung for ~27 minutes after the real work had already finished and passed. The fix: spawn into a
fresh process group (`Command::process_group(0)`) and signal the group (`kill(-pid, ...)`), not
the single PID — mirroring the tree-kill this same function already did for Windows via
`taskkill /T`, which turned out to be a real Unix gap too, not just a Windows-specific quirk. A
real `ssh` with a `ProxyCommand` has the identical shape, so this isn't purely a test-fixture
concern.

**Confirm the reap, don't just fire the signal.** Every kill path (SIGTERM's happy path, the
SIGKILL escalation, the PID-overflow fallback) now ends with an explicit `child.wait()` before
returning — "this function returned" is meant to be a real guarantee that the OS process is gone,
not an assumption. This matters because callers built on top of it (`stop_and_wait`,
`shutdown_all_and_wait`) exist specifically to give a caller that confirmation.

**Fire-and-forget shutdown (`TunnelSupervisor::stop()`/`TunnelManager::stop()`/`shutdown_all()`)
is fine for a live, long-running app process** — the async cleanup keeps running on the same
runtime and finishes within a few seconds regardless of who's watching — **but is a real gap
wherever the caller's own lifetime is about to end before that "eventually" arrives.** Two such
callers existed and both needed the wait-based variant instead: a `#[tokio::test]`'s per-test
tokio runtime (torn down the instant the test function returns, which can race the still-running
`graceful_kill` task and abandon it mid-flight) and real app exit (`RunEvent::Exit` in `lib.rs`,
which used to call the fire-and-forget `shutdown_all()` and then keep going — if the process
itself exited before the up-to-5s grace period elapsed, a real SSH child could be orphaned,
directly contradicting this repo's own shipped docs). Fixed with `stop_and_wait`/
`shutdown_all_and_wait`, both bounded by a shared `GRACEFUL_SHUTDOWN_TIMEOUT` (7s: the 5s SIGTERM
grace period plus a scheduling/signal-delivery buffer) so neither can hang indefinitely no matter
how many tunnels are running or how unresponsive one is — `RunEvent::Exit` blocks on the async one
via `tauri::async_runtime::block_on`, the same way the design-mode and ACP shutdown steps in that
same handler block on their own cleanup. **The general rule: before treating a
fire-and-forget stop/signal API as sufficient, check whether the caller's own process/runtime is
about to disappear** — if so, it needs a bounded wait-based variant, not just "send the signal and
trust it'll get handled."

**`TunnelManager::shutdown_all`/`shutdown_all_and_wait` had their own separate bug, found by code
review the same day: iterate-then-`clear()` is a real TOCTOU, not just a style nit.** Both methods
used to snapshot the map via `.iter()` into a `Vec`, then call a SEPARATE `self.tunnels.clear()`
afterward. A concurrent `start()` that publishes a brand-new tunnel into the map in the gap between
the snapshot and the clear was silently wiped out by `clear()` — never asked to stop, never waited
on — directly reproducing the exact "orphaned SSH process on exit" bug this whole fix exists to
close. Fixed by collecting just the *keys*, then removing each one individually
(`self.tunnels.remove(&id)`) instead of a blanket `clear()` — a tunnel published after the key
snapshot was taken is simply left alone (picked up by a later call) rather than destroyed. **Only
the wipe is fixed, not the race:** a tunnel started DURING exit, after the exit path's single
`shutdown_all_and_wait` took its key snapshot, is still never stopped — nothing makes a second
pass (Batch 34 review). **Any
"snapshot then bulk-clear a concurrent map" pattern has this same shape of gap — prefer per-key
remove over iterate-then-clear whenever the map can be mutated by another task/thread between the
two steps.**

**Same shape, fixed 2026-10-08 for the worktree Setup/Archive/Run Script runner:** tuic-git's
`git_cli::output_with_deadline_tree` (used by `run_shell_script`) starts the script as the leader
of its own process group and, on timeout, signals the GROUP — SIGTERM, a bounded 2 s grace, then
SIGKILL — or `taskkill /T /F` on Windows, then reaps. `graceful_kill` itself could not be shared:
it is async and lives in the app crate, which tuic-git must not depend on. The group-signal
primitive is `tuic_core::process_tree` instead (`terminate_process_group`/`kill_process_group`/
`kill_process_tree`), where `graceful_kill` can adopt it too. Plain `output_with_deadline` (git,
`lsof`) still kills only the direct child — those do not start children that must die with them.

**Same shape, fixed 2026-10-09 for the one-shot ssh/scp runner** `tunnels::exec::run_process`
(Test Connection's SSH check, remote deploy, SSH provisioning). It used to rely on
`kill_on_drop`, which ends only the direct `ssh`/`sh`, so a `ProxyCommand` or a forking fake-ssh
fixture orphaned its child on a timeout or a cancelled request. It now spawns with
`process_group(0)`; a timeout SIGTERMs the group, waits a bounded grace, SIGKILLs the group and
reaps the child within a bound; a dropped future signals the group through a drop guard. The pipes
are drained on their own tasks under the same deadline, so a straggler holding stdout cannot hang
the call either. Tests: `a_timed_out_one_shot_kills_the_grandchild_it_forked`,
`a_cancelled_one_shot_kills_the_grandchild_it_forked` (`tunnels/exec.rs`).

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

**Script timeouts must drain stdout/stderr on dedicated reader threads, not rely on a
bare `spawn()` + `try_wait()` loop.** A loop that doesn't actively drain the child's
pipes deadlocks the instant the child writes past the OS pipe buffer (16 KiB on
macOS) — `npm install` blows past this immediately. tuic-git's `run_shell_script`
(`crates/tuic-git/src/worktree.rs`, fixed 900 s `SCRIPT_TIMEOUT` via
`git_cli::output_with_deadline_tree`) already does this. On timeout it kills the
script's whole process group (`npm`'s own children too), and a script that exits
while something it backgrounded still holds stdout/stderr returns after a bounded
2 s drain instead of blocking on that straggler (which is left running) — see
"Killing a Child Process" below.

**Post-create order is CoW warm → file sync → Setup Script, all in one background
chain the caller never awaits (`worktree::spawn_worktree_setup_chain`).** The sync
awaits the warm (both write into the same destination; running them concurrently
let either clobber the other), and the script awaits the sync (a script depending on
a synced file could otherwise run before it exists). The warm status is published
only after the LAST step, so `warm_artifacts.status` stays `pending` until the Setup
Script has finished, and a removal that clears the warm token mid-chain stops the
remaining steps. This is why no worktree-creation response — desktop IPC, HTTP
`create_worktree_shared` (incl. MCP `repo worktree_create`), HTTP
`create_session_with_worktree` — returns `setup_script`/`setup_script_error` any
more: that information doesn't exist yet by the time the response is built. The
outcome is reported via a dual-emitted `AppEvent::WorktreeSetupScriptCompleted`
(`worktree-setup-script-completed`; one payload builder,
`state::worktree_setup_script_completed_payload`, for the window emit and the SSE
arm), silent when no script is configured (matching `worktree-sync-*`'s own
nothing-to-do-is-silent precedent). **An MCP client has no SSE/event stream to receive
this event on**, so the chain also records a pollable `state::WorktreeSetupStatus`
(`AppState::worktree_setup_status`, moka, 30 min TTL, keyed by `(repo_path, branch)`:
`running` → `not_configured`/`completed`, dropped — `unknown` — when a removal stopped
the chain, `completed` with an error when the chain task was aborted), read via
`repo action=worktree_setup_status` / `GET /worktrees/setup-status`. Do not
"fix" this by making the chain synchronous again — that reintroduces blocking
worktree creation.

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
