# `tuic` / `tmux` shim script

`tuic` is a small bash shim meant to live in `~/bin` under three names (`tuic`,
`tmux`, `tuic-bridge`; the last two are symlinks to `tuic`). It finds the
`TUICommander.app` that owns the current shell and `exec`s that bundle's CLI,
passing every argument through.

It exists because several TUICommander builds can be installed side by side
(`~/Applications/TUICommander_<version>.app` plus `TUICommander.app`), and the
CLI must come from the build that actually spawned the shell, not just any
copy on disk.

Files in this directory (staged here until installed to `~/bin/`):

| File | Purpose |
|---|---|
| `tuic` | the script (install as `~/bin/tuic`; `~/bin/tmux` and `~/bin/tuic-bridge` stay symlinks to it) |
| `tuic.bats` | bats test suite (56 tests) |
| `tuic-script.md` | this document |

## Why it was rewritten

The original resolved the owning app by walking the process tree with `ps`.
Inside the srt sandbox `ps` and `pgrep` are blocked
(`/bin/ps: Operation not permitted`, `sysmond service not found`), so every
lookup failed and the script reported "no running TUICommander.app found",
which was false. Two things changed:

1. A sandbox-safe lookup that needs no process inspection.
2. Honest failure reporting: "none running" is only claimed when a working
   process scan proved it.

## How it works

### Lookup order (first hit wins)

| # | Stage | Function | Needs `ps`? | How it decides |
|---|---|---|---|---|
| 1 | Ancestor walk | `bundle_from_ancestors` | yes | Walks parent pids from `$PPID` until a process whose executable is `*/Contents/MacOS/*` inside a real TUICommander bundle. Identifies the exact app that spawned this shell, even with several versions running. |
| 2 | Env version | `bundle_from_env_version` | no | TUICommander injects `TUIC_APP_VERSION` into every shell it spawns. Matches it against `CFBundleShortVersionString` of bundles in `~/Applications` and `/Applications`. |
| 3 | Running scan | `bundle_from_running_instances` | yes | One `ps -ax` pass; succeeds only if exactly one distinct bundle is running. For shells launched outside any TUICommander (e.g. Terminal.app). |

`resolve_bundle` chains them. The ancestor walk goes first because it
identifies *this shell's* app; the env version is what makes the script work in
the sandbox.

### What counts as a TUICommander bundle

`is_tuicommander_bundle` checks `CFBundleIdentifier == com.tuic.commander` in
`Contents/Info.plist`, not the bundle's file name. A renamed copy still
matches; an unrelated app that happens to live under `Contents/MacOS` does not.

### Choosing the binary

`find_executable` picks `Contents/MacOS/<invoked name>` when the bundle ships
one (e.g. `tuic-bridge`), otherwise `Contents/MacOS/tuic`. It exits 1 if the
result is not executable. `main` then runs `exec -a "<invoked name>"`, which
preserves argv0 so `tuic-cli` still switches into tmux-compat mode when invoked
as `tmux`. With `shopt -s execfail`, a failed `exec` reports
`failed to exec <path>` and exits 126.

### Edge-case handling inside the lookups

- **Ambiguous installed copies.** If several bundles share the env version and
  one is the canonical `TUICommander.app`, that one wins. Otherwise the stage
  records the candidates as ambiguous and does not guess.
- **Several processes, one bundle.** The running scan de-duplicates, so a
  bundle with many processes counts once.
- **Bundle paths with spaces** are handled throughout.
- **Start pid empty, `0` or `1`** ends the walk (no infinite loop).
- **No `~/Applications` or `/Applications`** just yields no matches.
- **A failed or partial `ps`** is recorded as a reason, never treated as "not
  found".

## Failure reporting

When no stage resolves a bundle, `fail_lookup` prints exactly one of three
messages and exits 1. The decision is made from state, in this order:

| State | Message | Meaning |
|---|---|---|
| A stage found several candidates | `ambiguous — multiple TUICommander.app candidates (...); refusing to guess:` + candidate list + a hint | We know which apps exist but cannot tell which owns this shell. |
| `none_running=1` (a working `ps -ax` found no bundle) | `no running TUICommander.app found (not spawned from one, and none currently running)` | The only claim of absence, and only with proof. |
| Anything else | `could not determine which TUICommander.app to use (lookup failed, so one may still be running):` + one `- reason` line per stage | A failure, not an absence. |

Ambiguity takes precedence: if installed copies are ambiguous but the scan finds
nothing running, you still get the ambiguity message.

Example of the inconclusive message (sandbox, shell not started by TUICommander):

```
tuic: could not determine which TUICommander.app to use (lookup failed, so one may still be running):
  - cannot inspect the process tree: 'ps' is not permitted here (sandboxed?)
  - TUIC_APP_VERSION is not set (this shell was not started by TUICommander)
  - cannot scan running processes: 'ps' is not permitted here (sandboxed?)
```

Other errors: `has no executable Contents/MacOS/tuic` (exit 1) and
`failed to exec <path>` (exit 126).

## Structure (for testability)

Everything is a function. `main` only runs when the file is executed
(`[ "${BASH_SOURCE[0]}" = "$0" ]`), so tests can `source` the script and call
functions directly. Results travel through globals (`bundle`, `real_tuic`,
`ps_available`, `none_running`, `ambiguous`, `notes`), not command
substitution, because a subshell cannot update them.

| Function | Role |
|---|---|
| `is_tuicommander_bundle` | bundle-id check |
| `probe_ps` | one-time check that `ps` is runnable; sets `ps_available` |
| `bundle_from_ancestors` / `bundle_from_env_version` / `bundle_from_running_instances` | the three stages |
| `resolve_bundle` | runs the stages in order |
| `fail_lookup` | picks the failure message and exits 1 |
| `find_executable` | picks and validates the binary |
| `main` | wires it together and `exec`s |

## Tests

Run: `bats tuic.bats` (from this directory). 56 tests, no network, no server,
no real TUICommander, no real `ps`.

How it is hermetic:
- `HOME` points at a throwaway dir; fake `.app` bundles are built with
  `PlistBuddy`.
- `ps` is a fake first on `PATH`. `FAKE_PS_MODE` is `ok`, `blocked` (always
  fails, like the sandbox) or `midfail` (the probe works, every other call
  fails); `FAKE_PS_TREE` and `FAKE_PS_PROCS` supply process data.
- A fake bundle's `Contents/MacOS/tuic` is a symlink to `/bin/sh`, so
  `tuic -c 'echo "$0"'` reveals the argv0 the shim exec'd with.

### Result: 56 / 56 passing

| Group | Tests | Covers |
|---|---|---|
| `is_tuicommander_bundle` | 5 | correct id; other app's id; missing plist; garbage plist; renamed copy |
| `probe_ps` | 3 | ps works; ps blocked; ps error text does not leak |
| `bundle_from_ancestors` | 8 | finds app several levels up; skips wrong-id bundle; no app in chain; ps blocked (inconclusive, not "not a descendant"); ps fails mid-walk (names the pid); stops at pid 0/1; empty pid; path with spaces |
| `bundle_from_env_version` | 9 | unset; unique match; picks right version among several; stale version; ignores other apps with same version; ignores `.app` with no plist; canonical wins; ambiguous; works with ps fully blocked |
| `bundle_from_running_instances` | 7 | ps blocked; `ps -ax` fails; scan finds nothing (`none_running=1`); one bundle; same bundle many processes; two bundles (ambiguous); non-TUICommander process ignored |
| `resolve_bundle` ordering | 4 | ancestor beats env; env when ps blocked; running-scan fallback; all blocked gives one note per stage |
| Failure messages (end to end) | 9 | ps blocked never says "no running"; stale env; only a working empty scan says "no running"; ps failing mid-way; ambiguous installed copies; ambiguity beats an empty scan; two running instances; prefix uses invoked name (`tmux:`); generic fallback |
| `find_executable` | 5 | default `tuic`; same-named binary preferred; tmux falls back to `tuic`; non-executable; missing |
| End to end | 6 | env-version resolve and arg passthrough; argv0 is `tuic`; argv0 is `tmux` via symlink; real ancestor beats a different env version (verified by which bundle ran); exec failure exits 126; sourcing does not run `main` |

### Mutation check

To confirm the key tests can fail, two deliberate bugs were applied to a copy:

| Mutation | Tests that failed |
|---|---|
| Blocked `ps` wrongly sets `none_running=1` | 26, 36, 37, 38 |
| Env-version stage runs before the ancestor walk | 33, 54 |

### Manual checks run during development (before the suite existed)

Run against the real `~/Applications` bundles in the sandbox, plus a fake
`ps` where noted. All matched expectations and are now covered by the suite.

| Scenario | Result |
|---|---|
| Sandbox shell, `TUIC_APP_VERSION` set | `tuic 1.8.0-nightly.20261019.f19e9f1ba-dirty` |
| Invoked as `tmux` | `tmux 3.4` (argv0 preserved) |
| ps blocked, env unset | "could not determine", three reasons, no "none running" |
| ps blocked, stale env (`0.0.1`) | "could not determine", stale-version reason |
| Fake ps works, nothing running | `no running TUICommander.app found` |
| Fake ps, ancestor is old 20261015 bundle, env says 20261019 | old bundle chosen (ancestor wins) |
| Two running bundles | ambiguous, both listed |
| Two same-version copies, no canonical | ambiguous, both listed |
| Two same-version copies plus `TUICommander.app` | canonical chosen |
| Bundle without `Contents/MacOS/tuic` | `has no executable` error |
| ps fails mid-walk and in the scan | "could not determine", not "none running" |

Two mistakes in earlier manual testing, for the record: an off-by-one claim
about `PIPESTATUS` that turned out to be wrong (the original code was right),
and a fake process tree keyed on the wrong pid. The suite keys trees on the
script's real parent pid.

## Known limits

- A bare Terminal.app shell **inside the sandbox** cannot be resolved: `ps` is
  blocked and `TUIC_APP_VERSION` is not set. The script says so instead of
  guessing.
- `TUIC_APP_VERSION` reflects the app version when the shell started. If the
  app updated and the old bundle is gone, that shell reports a stale-version
  reason; open a new shell.
- The ancestor walk and running scan use `ps`; the real-`ps` paths are only
  exercised in tests through the fake `ps`, because the sandbox blocks the real
  one.
- Only `~/Applications` and `/Applications` are searched for the env-version
  lookup.

## Installing

Not installed yet (`~/bin` is read-only from the sandbox). From a normal shell:

```bash
cp tuic ~/bin/tuic        # ~/bin/tmux and ~/bin/tuic-bridge are symlinks to it
bats tuic.bats            # optional: re-run the suite against the repo copy
```
