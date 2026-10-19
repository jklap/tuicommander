# Wayland scanner upstream replacement evidence

Story: 2034-6823. Verified on 2026-10-19 from base `f48e9a3291`.

## Decision

Retain the local patch until a Linux desktop build passes. The assigned rb host
is unavailable because the VPN is down. Native generator tests do not establish
Linux client/backend ABI compatibility. Acceptance criterion 2 explicitly permits
retaining the fork and reporting this blocker.

No dependency version, generated output, runtime code, or license notice changes
in this commit. This is a dependency chore, not a runtime defect; no artificial
RED regression test was added.

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

## Remaining integration gate

When rb is available, evaluate removal in an isolated candidate change:

1. Remove the Wayland scanner patch declaration and its explanatory comments,
   remove its workspace exclusion, and resolve upstream 0.31.11 in Cargo.lock.
2. Verify the resolved Linux source graph selects the registry scanner and
   quick-xml 0.41 while retaining stable client/backend versions.
3. Run the Linux desktop build and generated-interface checks before committing
   fork retirement. Run batch clippy/full gates on rb.
4. Retire the fork files and check affected license notices only after the Linux
   gate passes. Leave unrelated patches and their shared license files intact.

Criteria 1 and 3 remain unverified. Criterion 2 is satisfied by retaining the fork
and reporting the unavailable Linux gate. The story is not complete.
