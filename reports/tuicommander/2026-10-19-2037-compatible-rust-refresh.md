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

These are all changed package records. The existing base64 0.22.1 remains for
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

The audit acceptance criterion remains unmet. Completing it requires an explicit
scope decision for the ZIP 9 migration and its paired updater dependency; these
compatible patches alone cannot make the audit pass.
