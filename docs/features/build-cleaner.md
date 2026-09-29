# Build Artifacts Cleaner

A preinstalled plugin (`plugins/build-cleaner/`, id `build-cleaner`) that scans registered
repos for stale build-artifact directories, shows per-repo size + last-build age in a
dashboard, and offers a guarded one-click cleanup. It warns via the Activity Center bell
and the status ticker when reclaimable disk crosses a configurable threshold.

Capabilities: `fs:scan` (read-only walk), `fs:delete` (guarded `remove_dir_all`),
`ui:panel` (dashboard), `ui:ticker` (status bar warning).

The Rust backend (`src-tauri/src/plugin_fs.rs`) owns every sharp edge — matching rules,
depth caps, symlink handling, and the delete/trim guard. The plugin's `main.js` only
renders the dashboard and orchestrates calls into it; see `docs/plugins.md`'s
`host.scanBuildArtifacts()` / `host.deleteBuildArtifact()` / `host.trimBuildArtifact()`
entries for the underlying PluginHost API reference.

## What it scans, and what it deliberately ignores

The scanner walks each registered repo root (`walk_artifacts` in `plugin_fs.rs`) looking
for directory names matched against a fixed rule table, `ARTIFACT_RULES`. This table is
the single source of truth — the plugin's `KIND_LABELS`/`ALL_KINDS` in `main.js` must stay
in sync with it (both files carry a comment saying so).

### 15 toolchains, 15 `kind`s

| `kind` | Label | Matched dir name(s) | Requires a marker file beside it? |
|---|---|---|---|
| `rust` | Rust (target) | `target` | `Cargo.toml` |
| `maven` | Maven (target) | `target` | `pom.xml` |
| `node` | Node (node_modules) | `node_modules` | — (always claimed) |
| `jscache` | JS cache (.next / .turbo / …) | `.next`, `.nuxt`, `.turbo`, `.parcel-cache`, `.svelte-kit`, `.astro` | — |
| `python` | Python (.venv / caches) | `.venv`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox` | — |
| `dotnet` | .NET (obj / bin) | `obj`, `bin` | a `.csproj`/`.fsproj`/`.vbproj`/`.sln`/`.slnx` file |
| `gradle` | Gradle (build / .gradle) | `.gradle` (always), `build` | `build` needs `build.gradle`, `settings.gradle`, `build.gradle.kts`, or `settings.gradle.kts` |
| `cmake` | CMake (build) | `build`, `cmake-build-*` (prefix match, e.g. `cmake-build-debug`) | `CMakeLists.txt` |
| `swift` | Swift (.build / Pods) | `.build`, `Pods` | `.build` needs `Package.swift`; `Pods` needs `Podfile` |
| `flutter` | Flutter (build / .dart_tool) | `build`, `.dart_tool` | `build` needs `pubspec.yaml`; `.dart_tool` is always claimed |
| `terraform` | Terraform (.terraform) | `.terraform` | — |
| `elixir` | Elixir (_build) | `_build` | — |
| `zig` | Zig (zig-out / .zig-cache) | `zig-out`, `.zig-cache` | — |
| `haskell` | Haskell (.stack-work / dist-newstyle) | `.stack-work`, `dist-newstyle` | — |
| `php` | PHP (vendor) | `vendor` | `composer.json` |

`target`, `bin`/`obj`, `build`, and `vendor` are **ambiguous names** — a Go sysroot `bin`,
Rust source under `src/bin`, an Xcode `PIFCache/target`, or a hand-rolled `vendor/`
directory with no `composer.json` are walked like any other directory, not claimed as an
artifact. The first rule in `ARTIFACT_RULES` whose name matches AND whose marker is
satisfied wins (`matching_rule`) — this is how a bare `target/` resolves to `rust` vs.
`maven` depending on which project file sits beside it.

### Traversal rules

- **Stop-at-match, never double-counted.** The moment a directory name resolves to a
  claimed artifact, its whole subtree is summed as one entry and the walk does not
  descend into it — a `node_modules` nested inside another `node_modules` (or inside a
  `target/`) is folded into the outer entry, not reported twice.
- **`.git` is always skipped.**
- **Symlinked directories are never followed**, so a symlink cycle cannot cause infinite
  recursion, and a symlink cannot be used to sneak the walk outside the repo.
- **Depth caps**: `MAX_SCAN_DEPTH` = 8 for the discovery walk (real project trees are far
  shallower); size measurement inside an already-matched artifact dir gets its own deeper
  cap, `MAX_SIZE_DEPTH` = 64 (`node_modules` nests heavily).
- **`.gitignore` is ignored on purpose** — artifact directories are gitignored by design,
  so respecting `.gitignore` would make the scanner blind to the exact thing it exists to
  find.
- **Read errors are non-fatal per directory** — an unreadable subdirectory is skipped, not
  a scan failure.

### Repo-root containment

Every `repoPaths` entry the plugin passes is validated with `validate_within_home` (must
resolve under `$HOME`) and then intersected with the backend's actual registered
repositories (`registered_repo_roots()`, derived from `repositories.json`). A path that
isn't equal to, or nested under, a genuinely registered repo is silently dropped — a
plugin cannot widen its scan surface by passing an arbitrary `$HOME` path that was never
actually registered as a repo.

## Trim vs. Clean

Every artifact row offers **Clean** (`host.deleteBuildArtifact`, full `remove_dir_all`
of the matched directory). Rows for a toolchain with known regenerable intermediates also
offer **Trim** (`host.trimBuildArtifact`) — deletes only the sub-paths that are pure,
locally-regenerable intermediates, leaving the built executables/artifacts in place.

Trim patterns exist today for four toolchains (`ArtifactEntry.trimmable_bytes` is `0` for
everything else, which is what hides the Trim button for e.g. `node_modules`/`.venv`):

| Toolchain | Trim removes | Trim keeps |
|---|---|---|
| Rust (`target/`) | `*/deps`, `*/build`, `*/incremental`, `*/.fingerprint` (and their cross-compilation `*/*/…` triple-nested equivalents) | the linked executables at the profile dir's root — measured across 5 real repos, these four dirs are 98.2–99.8% of `target/`'s size |
| Maven (`target/`) | `classes`, `test-classes`, `generated-sources`, `generated-test-sources`, `generated-test-resources`, `maven-status`, `maven-archiver`, `surefire-reports`, `failsafe-reports` | the packaged `*.jar`/`*.war` at the root |
| Gradle (`build/`) | `classes`, `tmp`, `kotlin`, `intermediates`, `generated`, `reports`, `test-results`, `jacoco` | `libs/`, `outputs/`, `distributions/`, `install/` — the dirs Gradle publishes final artifacts into |
| Swift (`.build/`) | `index-build` (SourceKit's separate index tree), `*/*/ModuleCache`, `*/*/index`, `*/*/*.build` (per-module object files) | `checkouts/`/`repositories/` (dependency **sources** — restoring them needs the network) and `Modules/` (`.swiftmodule` interfaces) |

**The bar for a trim pattern's inclusion: deleting it must cost only local CPU to
rebuild.** A directory that needs the network to restore (Swift's `checkouts`/
`repositories`, Cargo's registry cache) is deliberately excluded — trimming must never
turn "reclaim some disk" into "now you're offline and stuck." Python's `.venv` was
evaluated and deliberately deferred: `**/__pycache__` is only ~8.6% of a typical venv and
needs a recursive glob segment no other toolchain requires.

A trim pattern segment is matched one directory level at a time against real directory
names (never against a raw path string), so the same pattern constants work unmodified on
macOS, Linux, and Windows. Symlinked directories inside a trim expansion are never
followed either.

### The delete/trim guard

Both actions funnel through `assert_deletable` before touching disk. **All** of the
following must hold, or the path is refused outright:

1. The path's basename (after canonicalizing — so a symlink pointing outside a repo
   resolves to its real location and fails containment) matches a known artifact rule
   name.
2. It is **strictly inside** one of the caller's repo roots — `starts_with` a root AND
   not equal to it. A repo root itself can never be deleted.
3. For an ambiguous name, the same marker-file check the scanner used must still hold
   (re-checked independently at delete time, not trusted from the earlier scan) — this is
   what refuses e.g. Rust `src/bin` sources even if someone tries to delete a path that
   merely looks like an artifact.

The repo-root list handed to this guard is the same caller-supplied-but-intersected set
described above — never trusted as-is.

Trim additionally re-verifies that every expanded sub-path from the matched rule's trim
patterns still resolves strictly inside the already-verified artifact directory, and never
follows a symlink while expanding a pattern.

## Dashboard UI

Opened via the plugin's registered dashboard entry (Command Palette / plugin launcher,
`host.registerDashboard`). The panel opens instantly with the last completed scan (or a
"Scanning…" placeholder on first load) while a fresh walk runs in the background — a full
walk can take tens of seconds on large `target/` trees, so the UI never blocks on it.

- **Overview cards**: Reclaimable (total bytes across stale, enabled-kind artifacts, with
  a threshold meter), Safe to trim, Total on disk, Artifact count, Repo count.
- **Per-repo sections**, largest repo first, one table each, rows sorted largest artifact
  first. Each row shows path (relative to its repo), kind label, size, trimmable bytes (or
  `—`), age, and action buttons.
- **Recent badge**: an artifact whose age is inside the configured hot-window is tagged
  `recent` and excluded from the reclaimable total (see Thresholds below).
- **Two-step arm/confirm on every destructive button**, instead of a native `confirm()`
  dialog — the panel is an iframe with no `allow-modals`, so a real modal would silently
  return `false` and the action would never fire. First click arms the button (label
  changes to "Trim?"/"Delete all?"); a second click within 4 seconds fires it; the arm
  auto-reverts otherwise. Both actions on the same row disable together the moment either
  fires, since one operates on a tree the other is about to change.
- **Rescan button** forces a fresh walk (bypasses the 30-second scan cache described
  below).

After a Clean, the removed entry is dropped from the cached result. After a Trim, the
entry is kept but its `size_bytes` is reduced by exactly the bytes the backend reports
having removed (not the last scan's stale estimate — a build that happened between the
scan and the trim would make that estimate wrong), and `trimmable_bytes` resets to 0. In
both cases the background poll (see below) later reconciles any drift instead of forcing
an immediate full rescan.

## Settings & thresholds

Persisted via `write_plugin_data`/`read_plugin_data` (`config.json` inside the plugin's
own data directory), editable from the dashboard's collapsible "Settings & thresholds"
section:

| Setting | Default | Effect |
|---|---|---|
| Per-artifact warn (GiB) | 5 | a single stale artifact at or above this size trips "warn" on its own |
| Hot-window exclude (hours) | 24 | artifacts built within this window are excluded from the reclaimable total — a `target/` from 10 minutes ago doesn't nag mid-work |
| Total warn (GiB) | 50 | total stale, enabled-kind bytes at or above this trips "warn" |
| Total critical (GiB) | 150 | total stale bytes at or above this trips "critical" |
| Rescan while open every (minutes) | 60 | background poll cadence **while the dashboard is on screen** (floored at 5 minutes on save — anything faster is pure I/O waste) |
| Enabled kinds | all 15 | unchecking a kind hides its artifacts from the dashboard entirely and excludes them from every threshold/total |

Severity classification (`evaluateThresholds` in `main.js`): `none` unless total stale
bytes ≥ Total warn OR the single largest stale artifact ≥ Per-artifact warn (→ `warn`), or
total stale bytes ≥ Total critical (→ `critical`). "Safe to trim" bytes never drive a
threshold on their own — the nag is about total reclaimable disk, not about which action
you'd take to reclaim it.

## Bell + ticker (background monitoring)

A single scan runs at plugin load (`onload`) to seed the Activity Center bell and status
ticker for a user who never opens the dashboard. From then on, the recurring background
poll only runs **while the dashboard panel is open and visible** — a poll cycle is a full
stat-heavy walk of every registered repo, and its only outputs (the bell item, the ticker)
are wasted on a closed panel. Opening/closing/hiding the panel is the sole signal that
starts or stops the timer.

When severity is `none`, the bell item and ticker are cleared. Otherwise:

- **Bell item** (Activity Center, section "BUILD CLEANER"): title shows total reclaimable
  bytes, subtitle shows either the largest single artifact + its repo or a stale-directory
  count, severity maps `critical` → error, `warn` → warn. Dismissible; clicking it opens
  the dashboard.
- **Status ticker**: label "Build", text "`<size>` reclaimable", priority 90 for critical
  / 50 for warn (mirrors the tiering the Claude-usage ticker uses). Clicking it opens the
  dashboard.

## Caching

`scan_build_artifacts` shares an in-process cache keyed by the exact normalized,
deduplicated root-path set (`ArtifactScanCache` in `plugin_fs.rs`): a completed scan is
reused for 30 seconds, and a scan already in flight for the same root set is shared rather
than duplicated — two dashboard instances (or an HMR reload racing a startup poll) opening
at once cost one walk, not two. `forceRefresh: true` (the dashboard's Rescan button)
bypasses a completed cache entry; if a scan for that exact root set is already running
because it started on the TTL's own schedule (not as a forced refresh), the forced caller
marks it invalidated so its leader discards that in-flight pass and reruns instead of
returning stale-by-request data.

## Known deferred limitation

A background walk already in progress inside `host.scanBuildArtifacts` cannot be
cancelled when the dashboard panel is hidden mid-scan — the host exposes no abort. The
result is still applied when it completes, since it's fresh and correct; this only means a
walk started just before the panel closes still runs to completion once, not that it
double-runs or corrupts state.
