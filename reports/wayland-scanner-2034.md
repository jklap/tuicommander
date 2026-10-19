# Wayland scanner upstream replacement evidence

Story: 2034-6823. Verified on 2026-10-19 from base `f48e9a3291`.

## Decision

Replace the local patch with registry wayland-scanner 0.31.11. The Linux desktop
build and all five upstream generator/parser tests passed on rb on 2026-10-19.
The resolved graph keeps quick-xml 0.41.0, wayland-client 0.31.15 and
wayland-backend 0.3.17. Only the scanner version and source change.

Remove the patch declaration, workspace exclusion and scanner fork files. The
MIT license is unchanged; `THIRD_PARTY_NOTICES.md` has no scanner-specific entry
or reference to its removed license file. Other patches and notices stay intact.

This is a dependency chore, not a runtime defect; no artificial RED regression
test was added. The initial evidence below records the earlier VPN blocker and
native checks. The Linux continuation resolves that blocker.

## Published upstream source

Downloaded the published crate from:

https://static.crates.io/crates/wayland-scanner/wayland-scanner-0.31.11.crate

Archive SHA-256:
`338e30461b3a2b67d70eb30a6d89f8e0c93a833e07d2ae89085cd070c4a00ac0`.

Compared every file under the local fork's `src/` and `tests/` with the matching
archive path, using byte equality. All ten Rust source files and all five test
assets match. This includes the parser, client/server generators, low-level
interfaces, and their golden Rust output.

The upstream original manifest declares version 0.31.11, Rust 1.71, edition 2021,
quick-xml 0.41, proc-macro2 1.0.11, and quote 1.0. Its test-only `similar` dependency
is version 3; the retained fork uses version 2.

## Retained application dependency graph

Ran both commands through the local orchestrator `tools/build/build-slot.sh`,
with the mbx Cargo shim active:

```sh
cargo tree --locked --manifest-path src-tauri/Cargo.toml \
  --target x86_64-unknown-linux-gnu -i wayland-scanner
cargo tree --locked --manifest-path src-tauri/Cargo.toml \
  --target x86_64-unknown-linux-gnu -p wayland-scanner
```

Relevant output (worktree paths abbreviated):

```text
wayland-scanner v0.31.10 (proc-macro) (src-tauri/patches/wayland-scanner)
├── wayland-client v0.31.15
│   ├── wayland-protocols v0.32.13
│   │   ├── wayland-protocols-wlr v0.3.12
│   │   │   └── wl-clipboard-rs v0.9.4
│   │   │       └── arboard v3.6.1
│   │   │           └── tuicommander v2.0.0
│   │   └── wl-clipboard-rs v0.9.4 (*)
│   ├── wayland-protocols-wlr v0.3.12 (*)
│   └── wl-clipboard-rs v0.9.4 (*)
├── wayland-protocols v0.32.13 (*)
└── wayland-protocols-wlr v0.3.12 (*)

wayland-scanner v0.31.10 (proc-macro) (src-tauri/patches/wayland-scanner)
├── proc-macro2 v1.0.107
│   └── unicode-ident v1.0.24
├── quick-xml v0.41.0
│   └── memchr v2.8.3
└── quote v1.0.47
    └── proc-macro2 v1.0.107 (*)
```

The application lockfile retains wayland-backend 0.3.17 and wayland-client
0.31.15. This is target-specific resolution evidence, not a Linux build or an
upstream-resolved application graph.

## Native upstream generator checks

Ran the published crate's existing focused generator/parser tests, not the TUIC
application suite. The archive was extracted under `~/Gits/.tmp/wayland-2034/`.

```sh
export PATH="$HOME/Library/Application Support/mbx/bin:$PATH"
TMPDIR="$HOME/Gits/.tmp/tuic-tests/" scripts/with-test-tmp.sh \
  "$HOME/Gits/personal/orchestrator/tools/build/build-slot.sh" \
  cargo test --manifest-path \
  "$HOME/Gits/.tmp/wayland-2034/wayland-scanner-0.31.11/Cargo.toml" \
  --lib tests::
```

Result: five passed, zero failed. Tests:

- `client_gen::tests::client_gen`
- `server_gen::tests::server_gen`
- `interfaces::tests::interface_gen`
- `parse::tests::xml_parse`
- `parse::tests::headerless_xml_parse`

Compilation took 4.28 seconds; test execution took 0.09 seconds. The preceding
build-slot admission waited 180 seconds. `mbx doctor` reported zero failures and
zero warnings. Compilation emitted a nonfatal cache-bypass warning about missing
Cargo build-script declarations; the tests still completed successfully.

These tests compare generated code against the published golden assets and cover
both ordinary XML and headerless XML. They ran on macOS, not Linux. No desktop
instance, browser, or backend process was started.

## Linux continuation: completed gate

Continued from commit `49b8c18a7e` with the candidate removal applied. The
lockfile change is limited to scanner 0.31.10 (path) becoming 0.31.11 (registry),
with the published archive checksum above. No other locked package changed.

Ran one serialized command on rb through
`~/Gits/personal/orchestrator/tools/build/build-slot.sh --remote bash -lc ...`.
The command body was:

```sh
set -euo pipefail
cargo tree --locked --manifest-path src-tauri/Cargo.toml -i wayland-scanner
cargo tree --locked --manifest-path src-tauri/Cargo.toml -p wayland-scanner
cargo tree --locked --manifest-path src-tauri/Cargo.toml -p wayland-backend --depth 0
scanner=$(find "$HOME/.cargo/registry/src" \
  -path "*/wayland-scanner-0.31.11/Cargo.toml" -print -quit)
test -n "$scanner"
scripts/with-test-tmp.sh cargo test --manifest-path "$scanner" --lib tests::
cargo build --locked --manifest-path src-tauri/Cargo.toml --bin tuicommander
```

Result: exit 0, `RB_BOX_DIRTY=no`. Evidence is retained at
`~/Gits/.tmp/rb/20261019T212635Z-11967/` and
`~/Gits/.tmp/chore-2034b-linux.log`.

The consumer graph matches the retained graph above, except its root is now
registry `wayland-scanner v0.31.11 (proc-macro)` rather than the patched path.
The client, protocols, WLR protocols, clipboard and arboard versions are unchanged.
Scanner dependencies still resolve to proc-macro2 1.0.107, quote 1.0.47,
quick-xml 0.41.0, memchr 2.8.3 and unicode-ident 1.0.24. Backend remains 0.3.17.

All five upstream tests listed above passed on Linux: compilation 1.93 seconds,
test execution 0.06 seconds. They use the published crate's own test lockfile
(which selects proc-macro2 1.0.106, quote 1.0.46 and memchr 2.8.2), not the
application lockfile. The application build separately compiles the generated
interfaces with its locked dependency graph.

The default-feature Linux desktop binary build passed in 11 minutes 36 seconds
in the dev profile. Its output includes scanner 0.31.11, client 0.31.15,
protocols 0.32.13 and WLR protocols 0.3.12. This establishes compile/link
compatibility with the retained client/backend versions; it is not a live Wayland
compositor test, release package build or full application test-suite run.

Static checks: the manifest and lockfile parse as TOML; only the scanner lock
entry changes; no active manifest or notice references the retired fork. The
instruction-link check passes. No desktop instance, browser or backend was
started. No additional Rust build is needed for the evidence-only report edit.
