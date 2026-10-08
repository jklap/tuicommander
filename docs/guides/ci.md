# Continuous Integration

Five GitHub Actions workflows compile Rust:

| Workflow | Trigger | What it compiles |
|---|---|---|
| [`ci.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/ci.yml) | pull request, push to `main` | clippy + `cargo nextest` + doctests on Linux, `tuic-remote` without the desktop feature, macOS and Windows builds on push |
| [`release.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/release.yml) | version tag | the signed artifacts users install |
| [`remote-daemon.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/remote-daemon.yml) | called by `release.yml` and `nightly.yml` | `tuic-remote` and `tuic-bridge` for Linux x64/ARM64, Windows x64 and macOS ARM64, uploaded to the `v*` release or to the rolling `nightly` release |
| [`nightly.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/nightly.yml) | push to `main` | the rolling `nightly` release |
| [`audit.yml`](https://github.com/sstraus/tuicommander/blob/main/.github/workflows/audit.yml) | Monday 09:00 UTC | nothing — a prebuilt `cargo-audit` runs the advisory scan; accepted advisories live in `src-tauri/.cargo/audit.toml` |

## Nightly release ordering

The cleanup job moves the `nightly` tag, deletes the old release and recreates an
empty prerelease before the desktop and daemon matrices start. Successful daemon
builds can publish even if every desktop build fails. `tauri-action` reuses the
release with that tag and alone writes `latest.json`; daemon uploads contain only
`tuic-remote` and `tuic-bridge` binaries.

## The Rust toolchain is pinned

Every `dtolnay/rust-toolchain` step takes its version from a workflow-level
`RUST_VERSION` variable:

```yaml
env:
  RUST_VERSION: "1.98.0"

# ...

      - name: Install Rust ${{ env.RUST_VERSION }}
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: ${{ env.RUST_VERSION }}
```

The action's `toolchain` input overrides the `@stable` branch default, so the
branch name in `uses:` no longer decides the compiler.

### Why

`@stable` on its own floats. The day rustup publishes a new stable, the compiler
changes under the repository with no commit to point at: fresh clippy lints and
rustfmt reflows become a red `main` for whoever pushes next, not for whoever
wrote the code. That has happened three times — `7db734f9`, `20e58629`, and the
1.98 lints cleared on 2026-08-31 while CI was still green on an older stable, so
the tree failed locally and passed in CI at the same commit.

Pinning also makes the release reproducible: a tag built today and rebuilt in six
months uses the same compiler.

### How to bump the pin

1. Install the target toolchain locally (`rustup toolchain install <version>`)
   and make it the default for this checkout.
2. From `src-tauri/`, confirm both gates are clean:

   ```bash
   cargo clippy --all-targets -- -D warnings
   cargo fmt --check
   ```

   Fix whatever the new compiler flags. This is the work the pin defers to a
   deliberate moment instead of ambushing the next push.
3. Raise `RUST_VERSION` in **all five** workflows — `ci.yml`, `release.yml`,
   `remote-daemon.yml`, `nightly.yml`, `audit.yml` — in a single commit.
4. Push. CI runs on the new toolchain as part of that commit, so any breakage
   lands on its author and is bisectable.

Never raise the pin in one workflow only: `ci.yml` would then vouch for a
compiler that `release.yml` does not use.

> The local `rustc` is not pinned by a `rust-toolchain.toml`, on purpose —
> contributors keep their own rustup default. `cargo fmt --check` and clippy are
> the contract; the CI pin is what makes that contract stable over time.

## Note on clippy scope

CI runs `cargo clippy --workspace --exclude bm25 --all-targets -- -D warnings`.
It includes test and bench code. The vendored `bm25` patch is excluded from
Clippy, but remains in workspace tests.

## Windows CMake debug flags

CI, release and nightly set `CMAKE_C_FLAGS` and `CMAKE_CXX_FLAGS` to `-Z7`
on Windows for sccache-compatible MSVC debug information. MSVC accepts both
slash and dash prefixes. Use the dash form because Git Bash converts `/Z7`
in environment variables into a Windows path before launching native Cargo.
That path reaches the CMake compiler probe as a source filename and fails the
`whisper-rs-sys` build. The Tests step uses Git Bash through
`scripts/with-test-tmp.sh`, so these flags must be safe in both shell paths.

## Headless workspace scope

The remote job checks `tuic-remote` with `--no-default-features`. Dictation is
already an optional dependency behind `desktop`; the headless daemon does not
use WebRTC. Its workspace test command also excludes `tuic-dictation`, because
`--workspace` selects that desktop-only member independently of feature flags.
The desktop workspace jobs still compile and test dictation with Meson and Ninja.

## Nextest setup scope check

`make test-shell` runs the frontend job's tool-independent shell tests. The
Nextest fixture setup scope check is named `scripts/check-nextest-fixture-scope.sh`
and runs explicitly in the Linux Rust job after workspace tests. That job
installs Nextest and already has the compiled test binaries. The check retains
its positive fixture-consumer and negative non-consumer assertions.

## macOS CI test compilation

The macOS cross-platform job has a 90-minute timeout. On `9a49ff1cb`, Clippy
finished in 9m27s and the test build took 67m17s. Nextest fixture setup then
spent 532.140s building the headless fixture binaries. Actual test execution
had only about 4m18s before cancellation, with 6483 of 7391 tests passing.
The longest completed test took 41.268s. This log does not separate compiler
optimization from linking, and does not measure a complete suite duration.

The macOS Tests step sets `CARGO_PROFILE_TEST_OPT_LEVEL=0` to avoid optimizing
workspace test harnesses. Non-workspace dependencies retain the inherited
`[profile.dev.package."*"]` optimization level of 1. Windows keeps test
optimization level 1. Test selection, retries, hang detection and the job
budget stay the same. This does not change release builds. The next CI run
must measure the compile-time benefit and check for runtime regressions.

## Release notes size

The nightly `update-notes` job groups commits since the latest stable tag.
Before publication, `scripts/cap-release-notes.py` limits the notes to 120000
UTF-8 bytes, below GitHub's 125000-character body limit. Oversized notes end at
a complete line and include a link to the full comparison against `main`.
Short notes keep their original contents.

The version-tag workflow uses the matching `CHANGELOG.md` section and a download
legend. It does not use the nightly commit-history notes step.
