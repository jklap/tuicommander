# Scoped compatible Rust dependency refresh — 2037-6c5a

## Contract and scope

Keep the existing HTTP/TLS and SQLite persistence behavior. Change only compatible
patch releases in the application workspace lockfile. Keep the existing manifest
ranges, feature sets, ZIP codecs, and paired Tauri policies.

Registry versions were checked against crates.io on 2026-10-19. Cargo resolved each
update separately. No product code or test assertions changed.

| Direct crate | Before | After | Complete transitive package delta |
| --- | --- | --- | --- |
| reqwest | 0.13.4 | 0.13.5 | Add base64 0.23.1; reqwest uses this instead of base64 0.22.1 |
| rusqlite | 0.40.1 | 0.40.2 | libsqlite3-sys 0.38.1 → 0.38.2 |
| rustls | 0.23.45 | 0.23.46 | None |

These are all changed package records in the initial compatible-patch phase. The existing base64 0.22.1 remains for
other consumers. The rustls patch is in scope because it is a direct dependency
and its package dependency list is unchanged.

## Graph review

Compared `cargo tree --workspace --target all -d` before and after, and reviewed
every lockfile hunk. The lockfile has 86 duplicate-name families before and after.
The active all-target duplicate tree has 107 duplicate-name families before and
after; its version roots increase from 250 to 251 because base64 gains a fourth
version (0.13.1, 0.21.7, 0.22.1, 0.23.1). This update does not reduce duplicates.

Cargo also rewrote unrelated dependency edges during resolution. Restored the
original edges for anstyle-query, anstyle-wincon, ctrlc, dirs-sys, fd-lock,
gix-hashtable, mio, muda, nu-ansi-term, parking_lot_core, rustix,
rustls-native-certs, socket2, tempfile, terminal_size, tray-icon, uds_windows,
winapi-util, window-vibrancy, and xattr. These rewrites selected other existing
windows-sys, hashbrown, or getrandom versions; they were not required for these
patches. Both `cargo tree --locked` and `cargo metadata --locked` accept the final
scoped lockfile.

Preserved without updates: dirs, ssh2-config, gix, rodio, cpal, wayland-scanner,
Tauri updater and notification pairs, and all ZIP packages/features. Other direct
crates were not selected: this is a bounded patch cluster, not a full refresh.
ZIP 9 is a major migration and is not part of this change.

## Validation

Local Cargo commands ran through build-slot and the mbx shim for resolution only.
`mbx doctor` reported zero failures and zero warnings. Rust tests, clippy, and
audit ran on rb through `build-slot.sh --remote`, with default desktop features.

```sh
scripts/with-test-tmp.sh cargo nextest run --locked \
  --manifest-path src-tauri/Cargo.toml --lib -p tuicommander \
  -E 'test(remote_runtime::tests::) | test(progress::store::tests::) | test(tunnels::audit::tests::) | test(mcp_oauth::discovery::tests::)'
cargo clippy --locked --manifest-path src-tauri/Cargo.toml \
  --workspace --all-targets -- -D warnings
cargo audit --file src-tauri/Cargo.lock
```

- Nextest: **137 passed**, 19.253 seconds execution after a 6m31s compile.
  The filter leaves unrelated tests unrun; this is not full-suite evidence.
  Existing cases exercise HTTP probing, TLS failure classification, OAuth
  discovery, SQLite persistence, migrations, paging, and concurrent writers.
- Workspace all-target clippy: **passed**, 2m52s. This checks all Cargo targets on
  the rb host, not every OS cross-compilation target or optional feature.
- Cargo audit: **failed**, six vulnerability findings. Both unchanged ZIP 4.6.1
  and ZIP 8.6.0 match RUSTSEC-2026-0367, RUSTSEC-2026-0368, and
  RUSTSEC-2026-0369. Each advisory requires ZIP >=9.0.0. Seven additional warnings
  concern paste, proc-macro-error, unsafe-libyaml, zbus_names, zvariant,
  zvariant_derive, and glib. No advisory was suppressed. Lockfile matches do not
  establish whether TUIC calls the affected extraction APIs.
- rb receipt: `20261019T212932Z-44804`, `stale=no`, `box_dirty=no`, aggregate exit 1
  from audit. Local log: `~/Gits/.tmp/chore-2037-rb-validation.log`.
- No desktop process was started or restarted. No native Mac compilation,
  cross-platform build, or hardware check is claimed. The running app will not
  load these dependency versions until Boss rebuilds/restarts it.

The initial audit acceptance criterion remained unmet. The coordinator then
authorized the direct ZIP 9 migration below; the updater blocker remains.


## Authorized ZIP 9 follow-up

On 2026-10-19 the coordinator authorized the direct ZIP major migration in this
same story, while prohibiting a patch or fork of its updater parent. The root and
`tuic-dictation` now use ZIP 9.0.5. The default codec feature list is identical to
8.6.0: aes-crypto, bzip2, deflate64, deflate, lzma, ppmd, time, zstd, xz.

Complete additional transitive package changes:

| Crate | Before | After |
| --- | --- | --- |
| cfg_aliases | existing 0.1.1 retained | add 0.2.2 |
| nix | existing 0.28.0 retained | add 0.31.3 |
| constant_time_eq | 0.4.2 | 0.6.1 |
| lzma-rust2 | 0.16.5 | 0.22.0 |
| ppmd-rust | 1.4.0 | 1.5.1 |
| zstd | 0.13.3 | 0.14.3 |
| zstd-safe | 7.2.4 | 8.2.0 |
| zstd-sys | 2.0.16+zstd.1.5.7 | 2.1.2+zstd.1.5.7 |

`constant_time_eq` now depends on the already locked rustversion. The existing
nix/cfg_aliases references gain version qualifiers because the new ZIP release
adds another version of each. There is no unrelated package churn. Final lock
has 88 duplicate-name families; the active all-target duplicate graph has 109
families and 255 version roots. The two new duplicate families are nix and
cfg_aliases. ZIP 4.6.1 remains alongside ZIP 9.0.5.

ZIP 9 makes `ZipFile::name` and `ZipArchive::name_for_index` fallible and returns
`Cow<str>`. Adapted speech library selection and plugin manifest discovery to
consume successfully decoded names. Plugin extraction still rejects traversal
and strips the single-directory prefix. The new
`zip_decoded_entry_names_preserve_runtime_selection` test protects against losing
a valid decoded name during the API adaptation, and checks actual extracted
bytes at the fixed runtime destination. No application behavior bug or pre-fix
behavioral RED is claimed for this dependency/API migration. An initial rb
compile exposed the old infallible call; it was corrected before the final run.

### Follow-up validation on rb

```sh
scripts/with-test-tmp.sh cargo nextest run --locked \
  --manifest-path src-tauri/Cargo.toml --lib \
  -p tuicommander -p tuic-dictation \
  -E '(package(tuicommander) & (test(plugins::tests::extract_zip_entries) | test(plugins::tests::find_manifest))) | (package(tuic-dictation) & test(speech::assets::tests::))'
cargo clippy --locked --manifest-path src-tauri/Cargo.toml \
  --workspace --all-targets -- -D warnings
cargo audit --file src-tauri/Cargo.lock
```

- 38 targeted tests passed in 0.223 seconds after a 5m39s compile.
- Workspace all-target clippy passed in 2m41s.
- Audit now reports **three** vulnerabilities, all on unchanged ZIP 4.6.1:
  RUSTSEC-2026-0367, RUSTSEC-2026-0368, RUSTSEC-2026-0369. ZIP 9.0.5 has no
  findings in this run. The seven previous warnings remain.
- Receipt: `20261019T215943Z-31822`, `stale=no`, `box_dirty=no`, aggregate exit 1
  from audit. Log: `~/Gits/.tmp/chore-2037-zip9-rb-final.log`.
- The earlier ZIP 9 run failed compilation on the changed `name()` API, before
  test execution; clippy/audit did not run then. No test was weakened.

### Remaining updater parent and reachability

`cargo tree --target all -i zip@4.6.1` shows exactly one parent:
`tauri-plugin-updater 2.11.0`, then TUIC. The crates.io API reports latest stable
updater **2.14.0**, which still requires **zip ^4**. The latest release including
prereleases is 3.0.0-alpha.3; that is not a compatible stable release. No supported
stable parent update removes ZIP 4.

The vulnerable API is reachable in the supported Windows updater path, not just
a stale lockfile entry. In installed updater 2.11.0 source:

- Its Cargo features enable `zip` by default, as TUIC's dependency does.
- `updater.rs:825` gates the installer implementation to Windows.
- `Update::install_inner` calls `self.extract(bytes)` at line 841.
- `extract` at line 948 recognizes ZIP bytes and calls `extract_zip`.
- `extract_zip` calls `ZipArchive::extract(&temp_dir)` at line 972.
- TUIC registers the updater at `src-tauri/src/lib.rs:1879` and calls the plugin
  from `src/stores/updater.ts:262` through `pendingUpdate.downloadAndInstall`.

This is decisive code reachability for ZIP-formatted Windows update payloads;
it is not a live Windows exploit or evidence that a malicious signed update has
been distributed. The macOS/Linux plugin installer does not use that Windows
ZIP extraction branch. No fork, parent update, feature removal, or advisory
suppression was applied. A dated comment at the updater dependency records this
remaining blocker, and `to-test.md` records the post-rebuild application checks.
The third acceptance criterion still cannot be checked without a coordinator
scope/criterion decision.


## Accepted residual updater exception — 2026-10-20

The coordinator authorized only the three remaining updater ZIP 4 advisory
exceptions after verifying release-format and signature-order constraints. This
supersedes the earlier pending audit decision, not the unsuppressed scan results.

### Release payload format

`src-tauri/tauri.conf.json:70` sets `bundle.createUpdaterArtifacts` to **true**,
not `"v1Compatible"`. Tauri utils 2.10.1 `src/config.rs:1549-1554` distinguishes the
legacy zipped-v1 setting from the boolean updater setting. The release job uses
`pnpm tauri` with `includeUpdaterJson: true` in `.github/workflows/release.yml`.
The recorded release fixture independently confirms Windows updater URLs point
to `TUICommander_1.7.6_x64-setup.exe`, not a ZIP:
`src-tauri/test-fixtures/updater/stable-manifest.json:32,36`.

Thus the configured release flow provides a Windows executable installer and
does not invoke `extract_zip`. The previous finding established a capability in
the upstream Windows installer for ZIP payloads, not that TUIC publishes such
payloads. No live Windows packaging run was performed here.

### Signature verification before extraction

In tauri-plugin-updater 2.11.0 `src/updater.rs`:

1. `download` verifies the complete byte buffer at line **740** and returns it
   only after verification succeeds at line **742**.
2. `download_and_install` awaits that verified download at line **766**, then
   calls `install` at line **767**.
3. `verify_signature` decodes the minisign public key and signature at lines
   **1524-1529**, and calls `public_key.verify` at line **1532**.
4. Only the later Windows install path calls `extract_zip` and its extraction
   API at line **972**. TUIC uses `pendingUpdate.downloadAndInstall` in
   `src/stores/updater.ts:262`.

This guarantee is for TUIC's download-and-install path. The upstream standalone
`install(bytes)` entry point does not itself verify arbitrary caller bytes.
Even if the release flow changed to signed ZIP payloads, exploiting these
updater extraction advisories through the current TUIC call would require a
malicious update validly signed by the trusted release key.

### Configuration, audit, and notices

Added only RUSTSEC-2026-0367, RUSTSEC-2026-0368, and RUSTSEC-2026-0369 to the
existing `src-tauri/.cargo/audit.toml`. The comment records the parent version
constraint, both acceptance reasons, review date, and
`DEFERRED (2026-10-20)` removal condition: remove when the updater uses ZIP >=9.
The direct application ZIP readers remain on ZIP 9. No historical ignore was
restored. Cargo-audit ignores advisory IDs globally, not per parent; this
exception is justified only for the remaining updater ZIP 4 graph.

Canonical audit command on rb, matching make/CI config discovery:

```sh
cd src-tauri && cargo audit
```

**Passed (exit 0)** with the three documented exceptions and seven existing
maintenance/unsoundness warnings. Receipt `20261019T222124Z-8494`,
`stale=no`, `box_dirty=no`; log `~/Gits/.tmp/chore-2037-accepted-audit.log`.
The prior audit commands from the repository root intentionally showed the
unsuppressed findings; future validation must use the crate working directory
so Cargo reads the shared audit configuration.

Ran `node scripts/third-party-notices.mjs` locally through build-slot and mbx
(metadata only), then `node scripts/third-party-notices.mjs --check`: passed.
Only the reqwest, rusqlite, rustls, and ZIP direct-version rows changed.
No application code or lockfile changed after the passing ZIP-domain tests and
all-target clippy, so those results remain relevant.


## Integration refresh after main advanced — 2026-10-20

Merged main `710554c917bd9732a5c96693548614ff4b80a03f` into this branch as
`f8dd429bbb91d22655a89ab03321dcff66473488`. Resolved only the root Cargo manifest
comment conflict by retaining both the updater ZIP audit rationale and main's
dirs follow-up comment, and the generated notices conflict by regenerating.
Preserved main's dirs/SSH, gix 0.89, rodio 0.23/cpal 0.18, upstream Wayland
scanner, and parser/mobile changes alongside this story's patches and ZIP 9.

The automatically merged lockfile passed offline locked metadata without
requiring `cargo update -w`. Generated notices and their freshness check passed.
The coordinator explicitly requested the complete root package suite for this
integration refresh, not another workspace suite:

```sh
scripts/with-test-tmp.sh cargo nextest run --locked \
  --manifest-path src-tauri/Cargo.toml -p tuicommander
cargo clippy --locked --manifest-path src-tauri/Cargo.toml \
  -p tuicommander --all-targets -- -D warnings
cd src-tauri && cargo audit
```

All ran on rb through build-slot:

- Nextest: **6032 passed**, 25 skipped by the harness, four test binaries;
  289.930 seconds execution after 10m07s test compilation and fixture setup.
- Root-package all-target clippy: **passed**, 2m26s.
- Audit: **passed**, with only the three documented updater ZIP 4 exceptions
  and seven existing maintenance/unsoundness warnings.
- Receipt `20261019T233445Z-21470`, aggregate exit 0, `stale=no`.
- Important provenance qualification: **`box_dirty=yes`**, with exactly one
  reported changed path, `./src-tauri/.tmp`. No production source path is listed;
  the cause of that scratch-path change was not established. Do not describe
  this receipt as box-clean. The local worktree is clean.
- Log: `~/Gits/.tmp/chore-2037-merged-rb-validation.log`.

Story preflight, changed-file formatting, and diff whitespace checks pass after
the merge. This integration check does not add live Windows, hardware, native
Mac compilation, or full-workspace runtime evidence. No application restart was
performed.
