# Configuration

**Module:** `src-tauri/src/config.rs`

Manages all application configuration as JSON files in the platform config directory.

## Config Directory

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.tuic.commander/` |
| Linux | `~/.config/com.tuic.commander/` |
| Windows | `%APPDATA%/com.tuic.commander/` |

Legacy paths `{platform_config}/tuicommander/`, `{platform_config}/tui-commander/`
and `~/.tuicommander/` are auto-migrated on first launch.

`tuic-remote --instance <id>` selects an isolated named namespace at process
bootstrap. Named files live below the platform path above at
`instances/<id>/`; the default instance keeps the paths in the table unchanged.
The ID is a 1–63 character lowercase ASCII DNS label (alphanumeric ends,
internal hyphens allowed), and `default` is reserved. A named instance never
runs the default or legacy file migrations and never falls back to those
locations when its own files are absent.

The desktop binary has no `--instance` flag of its own (Tauri's own CLI/single-instance
plugin already owns argv there), but reuses the same `AppInstance::named` selection
via the `TUIC_APP_INSTANCE` env var, read once at the very top of `run()` in
`lib.rs`, before any `config_dir()` call. `TUIC_APP_INSTANCE=<id> make dev` (or any
debug/test launch) gets the same isolated `instances/<id>/` namespace `tuic-remote
--instance <id>` gets — an actual code-enforced boundary rather than a documented
risk, closing the gap in AGENTS.md's "Isolation caveat" where a second debug
instance previously shared `repositories.json` with Boss's production instance with
nothing but discipline preventing a throwaway test repo from being persisted there
(#763-d219). An invalid or already-selected id fails the process at startup rather
than silently falling back to the default instance.

The credential namespace follows the same immutable selection. The default
vault remains keyring service `tuicommander`, user `vault`; a named instance
uses service `tuicommander-instance-<id>`, user `vault`. Named instances never
scan, import, modify, or delete default or legacy credential entries, including
the dynamic legacy MCP credential locations. Release `tuic-remote` probes a
named vault before binding its socket and exits on failure; it does not interpret
a keyring error as an empty vault or fall back to a file. Debug builds retain the
file-backed credential adapter, scoped below the selected instance directory.
Instance selection precedes `--set-password`, so password setup writes only to
the selected namespace.

Desktop verification may also set `TUIC_PORT=<port>` to choose the process-local
HTTP listener port without changing `config.json`; an occupied port still uses the
existing next-port retry.

**For the default instance, debug and release builds share this one directory**
— `config_dir()` never branches on `cfg!(debug_assertions)`. The single-instance lock is release-only
(`lib.rs`, `#[cfg(not(debug_assertions))]`), so a `make dev` build runs happily
alongside the installed app, and both read and write the exact same
`config.json`, `repositories.json`, and every other file below. What makes that
safe is the locking model in `ConfigFile<T>` (see Core Functions): a
cross-process advisory file lock. Ordinary `AppConfig`, upstream MCP, and
repository writes additionally apply caller deltas to the latest value while
that lock is held, so independent edits from two processes compose instead of
becoming ordered whole-document overwrites. `repositories.json` used to be the
one exception, seeded into a separate `~/.tuicommander-dev/` directory on first
debug run; that seeding path is gone and it now lives here like everything
else (see below).

### A Rust test can never name the real directory

In a `cfg(test)` build `config_dir()` returns the override set by
`set_config_dir_override(dir)` when one is in scope, and otherwise a safe,
process-scoped fallback under the OS temp directory
(`test_fallback_config_dir`, a `OnceLock` computed once per test process:
`<temp_dir>/tuic-test-fallback-<pid>`) — **never** the user's platform config
directory, with or without an explicit override.

It used to fall back to the user's platform directory instead, so a test that
forgot the override read and wrote Boss's live `config.json` and
`repositories.json` in silence. That silence is what let fifteen `tempfile`
roots become permanent repository rows (#763-d219): the damage was
indistinguishable from normal operation until someone diffed the file.

A first fix made the no-override branch **panic** instead of falling back —
correct in isolation, but it reproducibly broke the full `cargo nextest run
--lib` suite two different ways: (1) a `#[should_panic]` test exercising that
exact panic poisoned the guarding mutex on unwind (a temporary `MutexGuard`
was still alive through the panicking expression), and a later `Drop` call
locking the poisoned mutex aborted the whole process (SIGABRT) instead of
unwinding; and (2) any test that already held an explicit override (via an
`isolated_config()`-style helper) and then called a shared helper that also
tried to set one self-deadlocked, because the guarding mutex is not reentrant.
The silent process-scoped fallback removes both failure modes: nothing needs
the exclusive lock unless a test deliberately wants one, and dozens of
call chains that reach `config_dir()` without caring where it lives (most not
touching `repositories.json` at all) get a safe, stable-within-the-process
answer instead of a panic or a hang. `config_dir_in_a_test_never_names_the_real_user_directory`
and `fallback_config_dir_is_never_the_real_directory` (`config.rs`) prove the
fallback and the real directory can never coincide.

A thread-local fallback was considered and rejected: code under test that
offloads work to `spawn_blocking` (`finalize_merged_worktree`,
`merge_and_archive_worktree`) runs on a different OS thread than the test
itself, so a thread-local override set on the test's thread would not be
visible there — reintroducing the same gap on a thread the test can't reach.

`without_config_dir_override()` takes the same exclusive lock while
deliberately leaving the override unset — the one way to observe the fallback
branch without racing a concurrent test that did set an override. It is not an
escape hatch.

This guard covers unit tests only. Integration tests under `src-tauri/tests/`
compile the library without `cfg(test)`, and `make dev` is not a test at all —
both isolate with `TUIC_APP_INSTANCE=<id>` as described above.

The `plugins/` subdirectory holds external plugin packages. Version 1.7.7
externalizes Plan Tracker and Stories Ticker by seeding `plugins/plan/` and
`plugins/stories-ticker/` once. The root marker
`.externalized-plan-stories-v1` records completion: existing packages are never
overwritten, and removing either seeded package after migration is permanent.

## Core Functions

```rust
pub fn config_dir() -> PathBuf
pub fn load_json_config<T: DeserializeOwned + Default>(filename: &str) -> T
```

Config domains write through `ConfigFile<T>`:

```rust
impl<T: Serialize + DeserializeOwned + Default> ConfigFile<T> {
    pub fn load(&self) -> (T, Stamp)
    pub fn update<F: FnOnce(&mut T) -> bool>(&self, mutate: F) -> Result<(), String>
    pub fn update_with<R, F>(&self, mutate: F) -> Result<R, String>
    pub fn update_with_strict<R, F>(&self, mutate: F) -> Result<R, String>
    pub fn save_checked(&self, value: &T, stamp: Stamp) -> Result<(), ConfigWriteError>
    pub fn save(&self, value: &T) -> Result<(), String>
}
```

Two locks protect every write: an in-process `CONFIG_WRITE_LOCK` mutex, and a
cross-process advisory file lock (`std::fs::File::lock()` on a sibling
`<file>.lock`) that serializes writers across the debug/release instances that
now share one config dir. `save_checked` additionally compares a `Stamp`
(mtime+len, captured at `load()`) against the file's current on-disk state and
returns `ConfigWriteError::Conflict` instead of overwriting a change it never
saw — used by most whole-document per-domain files (`notifications.json`,
`ui-prefs.json`, `repo-settings.json`, etc.). Those callers capture the
stamp immediately before saving, so this narrows only the backend write race;
it is not a user-session conflict protocol. `config.json` (`AppConfig`) and
`mcp-upstreams.json` use delta-under-lock instead. `repositories.json` uses the
ID-keyed optimistic delta protocol documented below. See
[`2026-08-08-config-deltas-under-lock.md`](../decisions/2026-08-08-config-deltas-under-lock.md).

### Corrupt Files Are Moved Aside, Never Overwritten

A config file that exists but does not parse is renamed to
`<name>.corrupt-<uuid>` before anything falls back to defaults
(`preserve_corrupt_config`). The UUID is fresh per occurrence on purpose: a fixed
backup name would let a second corrupt load erase the document the first one
saved, which is the same data loss one step removed. Nothing ever deletes these
files — recovery is by hand.

Two entry points reach it. `load_json_config_strict` refuses to return `Default`
for a broken file on the **read** side (`notes.json`, GH #107), and
`update_with_strict` does the same on the **write** side, so a read-modify-write
aborts instead of persisting `Default` over real data (`repositories.json`,
`mcp-upstreams.json`).

`config.json` reaches neither, because `load_app_config` must return an
`AppConfig` and has no error channel to a caller. It preserves the file directly:
an unparseable `config.json` is moved aside and defaults are returned, so the
first-run branch that fills in a missing session token and VAPID key is free to
write a fresh document without destroying the old one. Only a **parse** failure
triggers the rename — an I/O error leaves the file alone, since the document may
be intact and only the read failed.

## Config Files and Commands

### Application Config (`config.json`)

**Type:** `AppConfig`

Frontend surfaces that update this full-document configuration use the shared
`updateAppConfig()` queue. It serializes each fresh load → owned-field mutation
→ save sequence so simultaneous General, Services, and plugin changes cannot
overwrite one another with stale snapshots.

**Ordinary saves merge under the cross-process lock; they do not replace the
document.** `PUT /config` and the MCP `config` tool (`action: "save"`) accept a
body that mentions only the fields being changed. IPC `save_config` retains its
typed full-config shape, but the backend derives the cache-to-request delta.
`commit_config_change` locks `config.json`, reloads and hydrates the latest disk
value, applies only the requested delta, persists it, and refreshes
`state.config` from the result. Objects merge key by key; arrays and scalars
replace wholesale (so an empty array still clears a list, `null` clears an
optional field, and `""` still blanks a string).
This is not cosmetic: every field carries `#[serde(default)]`, so deserializing a
partial body on its own reset the omitted ones — `services.server.enabled` defaults
to `false`, which is how a partial save used to switch remote access off on disk
while the already-bound listener kept serving, surfacing only at the next boot.

All three writers also share `server_settings_changed` and rebind the listener
through `restart_after_server_settings_change` when `services.server.{enabled,port,
ipv6_enabled}` or `services.auth.{username,password_hash}` move, so the running
process can never serve a configuration the disk disagrees with.

`commit_config_change` itself bumps `relay.config_revision` after every write.
`relay_client::supervise` — spawned at boot whether or not the relay is enabled —
watches that counter and starts, stops or restarts the relay client when
`services.relay.{enabled,url,token,session_id}` move. The signal is raised at the
choke point rather than by each writer on purpose: a `ConfigSaveEffects` flag is
only actioned by the callers that remember it, and a relay toggle that silently
needs an app restart is what that costs.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `shell` | `Option<String>` | `None` | Shell override (platform default if None) |
| `font_family` | `String` | `"JetBrains Mono"` | Terminal font family |
| `cursor_style` | `String` | `"bar"` | Terminal cursor style: `bar` (default), `block`, or `underline` |
| `terminal_renderer` | `String` | `"webgl"` | Terminal renderer: `webgl` (default, GPU-accelerated) or `canvas` (CPU, no atlas bugs) |
| `font_size` | `u16` | `14` | Terminal font size |
| `font_weight` | `u16` | `400` | Terminal font weight (100–900, e.g. 200 = ExtraLight, 400 = Regular) |
| `theme` | `String` | `"vscode-dark"` | Terminal theme |
| `ide` | `String` | `""` | IDE for "Open in..." |
| `ego_executable` | `String` | `""` | Absolute path to the one ego binary this host may launch for ACP. Read at each connect, so a correction takes effect without a restart. Empty means ACP is not configured here and every connect is refused. Deliberately unreachable over IPC or HTTP: a caller supplies a working directory and nothing else, so no request can choose which binary runs |
| `default_font_size` | `u16` | `13` | Default font size for reset |
| `mcp_server_enabled` | `bool` | `true` | Enable MCP HTTP server |
| `mcp_port` | `u16` | `9876` | Fixed port for MCP server (0 = OS-assigned) |
| `mcp_config_installed` | `bool` | `false` | Whether MCP config has been auto-installed in agent configs |
| `collapse_tools` | `bool` | `false` | Replace the full MCP tool list with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`). Discovered native schemas are unchanged: managed commands still use one `call_tool` invocation of `session action=submit` and receive the bounded receipt in that response. Grok sessions use this surface automatically without changing the stored value — see [`mcp-http.md`](mcp-http.md#lazy-tool-discovery-collapse_tools) |
| `services` | `ServicesConfig` | `{}` | Nested remote-access config: `server`, `auth`, `tls`, `relay`, `push` (replaces the former flat `remote_access_*`/`push_enabled`/`relay_enabled` fields) |

Remote-access secrets under `services` are not persisted in plaintext
`config.json`: `auth.session_token`, `relay.token`, and
`push.vapid_private_key` live in the OS keyring-backed credential vault. The
JSON file keeps only the non-secret settings plus `session_token_exists`,
`token_exists`, and `vapid_private_key_exists` booleans for UI state.

A vault **read failure** is never treated as "the secret is absent": on error
`hydrate_one_secret` keeps the `*_exists` flag that `config.json` recorded, so a
momentarily locked keychain cannot flip the flag to `false` and make the next
save delete a live credential. Plaintext still found in `config.json` is moved
into the vault at load time and the file is rewritten immediately, so the
cleartext copy does not survive on disk.

| `confirm_before_quit` | `bool` | `true` | Show quit confirmation |
| `confirm_before_closing_tab` | `bool` | `true` | Show tab close confirmation |
| `restore_window_geometry` | `bool` | `true` | Restore the main window's size and position from the last session on launch |
| `restore_shell_terminals` | `bool` | `true` | Restore plain shell tabs (not just agent tabs) from the last session on launch |
| `restore_scrollback` | `bool` | `false` | Persist each terminal's recent scrollback to disk and replay it above a fresh prompt on restore. Output is stored as plaintext in the config directory |
| `restore_scrollback_lines` | `u32` | `1000` | Maximum scrollback lines persisted per terminal when `restore_scrollback` is enabled |
| `copy_on_select` | `bool` | `true` | Auto-copy terminal selection to clipboard |
| `osc52_clipboard` | `bool` | `true` | Honor OSC 52 clipboard-write sequences from terminal output (a notice shows on each write; disable to ignore them) |
| `osc1337_focus_attention` | `bool` | `true` | Honor OSC 1337 `StealFocus` (bring window to front) and `RequestAttention` (bounce dock icon) sequences from terminal output |
| `show_last_prompt` | `bool` | `true` | Show last prompt overlay bar at the top of the terminal |
| `bell_style` | `String` | `"visual"` | Terminal bell: "none", "visual", "sound", "both" |
| `disabled_agents` | `Vec<String>` | `[]` | Agent IDs hidden from the Add menu |
| `disabled_native_tools` | `Vec<String>` | `[]` | Native MCP tool names disabled by the user (excluded from the `tools/list` response) |
| `disabled_plugin_ids` | `Vec<String>` | `[]` | Plugin IDs that the user has disabled (not loaded on startup) |
| `global_hotkey` | `Option<String>` | `null` | OS-level window toggle hotkey combo |
| `intent_tab_title` | `bool` | `true` | Show agent intent as tab title |
| `language` | `String` | `"en"` | UI language code |
| `max_tab_name_length` | `u32` | `25` | Max tab name display length |
| `split_tab_mode` | `SplitTabMode` | `"separate"` | Split tab mode: `"separate"` (each pane gets a tab) or `"unified"` (one shared tab) |
| `streamdock` | `StreamDockConfig` | `{}` | StreamDock M18 macropad integration: `enabled`, `device_serial` (`null` = first found), `screen_brightness` (0-100, default `70`), `led_brightness` (0-100, default `40`), `pinned_sessions` (never evicted from a slot). See `src-tauri/src/streamdock/` |
| `tab_ordering_mode` | `TabOrderingMode` | `"grouped-by-type"` | Tab ordering mode: `"grouped-by-type"`, `"terminals-first"`, or `"free"` |
| `tab_cycling_all_types` | `bool` | `false` | When true, next/prev-tab shortcuts cycle file/diff/markdown/editor tabs too (default cycles terminals only) |
| `tab_tree_enabled` | `bool` | `false` | When true, a branch with >1 terminal shows a collapsible nested list of its terminals under the branch row in the sidebar |
| `indicator_overrides` | `Vec<IndicatorOverride>` | `[]` | User overrides for the indicator registry (`src/indicators/registry.ts` — terminal status dots, tab types, sidebar symbols, PR badges, git repo status, diff stats). Each entry is `{ id, color?, icon?, animation? }`; `id` is a frontend registry id (e.g. `"terminal.busy"`), unvalidated on the Rust side — the frontend registry owns which ids are meaningful. A `Vec`, not a map keyed by id, because `commit_config_change` merges objects key-by-key but replaces arrays wholesale — a map would have no way to express removing an override. |
| `show_diff_stats` | `bool` | `true` | Show the sidebar branch row's diff stat badge (+N/-N) |
| `show_pr_badges` | `bool` | `true` | Show the sidebar PR status badge |
| `show_git_state` | `bool` | `true` | Show git repo status indicators — the sidebar's per-branch rebase/merge/cherry-pick/revert/bisect badge and the Changes tab's conflicts banner |
| `tab_type_highlighting` | `bool` | `true` | Tint tab backgrounds/borders by type (diff/editor/markdown/panel/etc). Off neutralizes the tint but keeps each type's icon color |
| `prevent_sleep_when_busy` | `bool` | `false` | Prevent macOS sleep when terminal is busy |
| `standby_timeout_minutes` | `u16` | `5` | Minutes of idle + unfocused before SIGSTOP on the process group. `0` disables it |
| `custom_launchers` | `Vec<CustomLauncher>` | `[]` | User-defined launchers shown in the "Open in" menu alongside built-ins |
| `additional_readable_dirs` | `Vec<String>` | `["~/.claude/plans"]` | Extra absolute directories the HTTP `read-external`/`read-editor-external` routes may serve, on top of registered repository roots. Global, read-only — never widens the write/copy/move/transfer routes. See [`mcp-http.md`](mcp-http.md#security-model) |
| `suggest_followups` | `bool` | `true` | Show `suggest:` follow-up actions |
| `issue_filter` | `Option<String>` | `"assigned"` | GitHub Issues filter: "assigned", "created", "mentioned", "all", "disabled" |
| `experimental_features_enabled` | `bool` | `false` | Master toggle for experimental features |
| `ai_chat_enabled` | `bool` | `false` | Sub-flag: enable AI Chat panel and shortcuts (requires `experimental_features_enabled`) |
| `ai_triage_enabled` | `bool` | `false` | Sub-flag: AI Triage (diff classification) |
| `ai_watchers_enabled` | `bool` | `false` | Sub-flag: AI Watchers (terminal event watchers) |
| `scroll_history_enabled` | `bool` | `false` | Sub-flag: scrollback history overlay on scroll-up in agent mode (requires `experimental_features_enabled`) |
| `ai_triage_enabled` | `bool` | `false` | Sub-flag: AI diff triage (requires `experimental_features_enabled`) |
| `ai_watchers_enabled` | `bool` | `false` | Sub-flag: terminal event watchers that trigger AI actions (requires `experimental_features_enabled`) |
| `ai_terminal_mcp_enabled` | `bool` | `false` | Expose `ai_terminal_*` tools to external MCP clients. Off by default — see [`mcp-http.md`](mcp-http.md#mcp-tools-ai_terminal_-external-agent-surface) |
| `index_strategy` | `String` | `"active_and_switch"` | Content index pre-warm strategy: `"active_and_switch"`, `"active_only"`, `"all_sequential"` |
| `auto_show_pr_popover` | `bool` | `false` | Auto-show PR popover when switching to a branch with a PR |
| `pr_hide_drafts` | `bool` | `false` | Exclude draft PRs from the Pull Requests list. Per-repo tri-state override: `RepoSettingsEntry.pr_hide_drafts` |
| `pr_hide_conflicting` | `bool` | `false` | Exclude PRs with merge conflicts from the Pull Requests list. Per-repo tri-state override: `RepoSettingsEntry.pr_hide_conflicting` |
| `pr_hide_ci_failing` | `bool` | `false` | Exclude PRs with failing CI checks from the Pull Requests list. Per-repo tri-state override: `RepoSettingsEntry.pr_hide_ci_failing` |
| `auto_update_enabled` | `bool` | `true` | Automatically check for app updates on startup |
| `auto_update_plugins_enabled` | `bool` | `true` | Automatically check for plugin updates on startup |
| `update_channel` | `String` | `"stable"` | Update channel: "stable" or "nightly" |
| `inline_blame_enabled` | `bool` | `true` | Show GitLens-style inline git blame on the code editor's active line |
| `terminal_link_activation` | `String` | `"click"` | How terminal links activate: `"click"` (opens on plain click), `"modifier"` (Cmd/Ctrl+click opens; underline only while held), or `"never"` (right-click Open/Copy-link menu only) |
| `double_click_action` | `String` | `"smart"` | What a plain double-click selects: `"word"` (character-class expansion) or `"smart"` (try the rule engine first, fall back to word). There is no master on/off switch for the rule engine — quad-click and the right-click smart-selection menu always run it, regardless of this setting |
| `word_selection_mode` | `String` | `"characters"` | How double-click word boundaries are defined: `"characters"` (a literal separator character class) or `"regex"` (`\|`-joined alternates, longest match wins — mirrors iTerm2's word-selection regex mode) |
| `word_separators` | `String` | `` " \"'`(){}[]<>\|;:,.!?@#$%^&*~=+/\\" `` | `"characters"` mode: characters that BREAK a word (the inverse of iTerm2's "additional word characters" — preserves today's default double-click behavior losslessly) |
| `word_selection_regex` | `String` | `""` (empty) | `"regex"` mode: `\|`-joined alternates. At each character, the longest anchored match becomes one word-class run (e.g. adding `https://` lets a double-click expand across a URL's scheme) |
| `smart_selection_rules` | `Vec<SmartSelectionRule>` | `[]` | User-defined smart-selection rules (regex, precision, actions). Empty means "use the built-in default set" (iTerm2's ten plus dev-terminal extras) — the frontend owns that default list. Importing a rule export (`smartSelectionExport.ts`) writes the merged *effective* set, so importing even a single custom rule permanently materializes every built-in rule here — "Restore built-in defaults" resets this field back to `[]` |
| `show_block_timestamps` | `bool` | `true` | **Deprecated** — superseded by `block_timestamp_mode`. Kept only so a config saved by an older build still deserializes; the frontend migrates it once at load time (`true` → `"modifier"`, `false` → `"off"`) and no longer writes this field on save |
| `block_timestamp_mode` | `String` | `"modifier"` | Command-block timestamp overlay display mode: `"off"` (never), `"always"` (always visible), or `"modifier"` (hold `Ctrl+Cmd` to reveal — the only behavior before this field existed) |
| `show_scrollbar_marks` | `bool` | `true` | Draw command-block marks on the terminal scrollbar. Frontend-gated, toggled from Settings > General > Terminal |
| `show_block_marks` | `bool` | `true` | Draw command-block boundary tick marks (blue/red) on the terminal scrollbar |
| `show_prompt_marks` | `bool` | `true` | Draw a tick mark on the terminal scrollbar for each line where the user submitted a prompt |
| `block_folding_enabled` | `bool` | `true` | Allow collapsing a command block's output. Toggled by `Cmd+Shift+.`, or by clicking the fold chevron on a block's header row in the gutter — clicking anywhere else in the block's gutter run still selects its output for copying |
| `scrollback_reflow` | `bool` | `true` | Re-wrap scrollback history on a column resize instead of truncating it. Backend-gated: `AppState::new_vt_log_buffer` applies it to a new grid and `commit_config_change` pushes a change to grids already open. Defaults `true` — including for a config.json written before the key existed — because the grid reflowed unconditionally before the flag had a consumer |
| `index_strategy` | `String` | `"active_and_switch"` | Which repos get a BM25 content index: `"active_and_switch"` (the boot repo plus every repo switched to), `"active_only"` (boot repo only), `"all_sequential"`, `"disabled"`. Read from the in-memory config on every switch and every `RepoChanged` — never `load_app_config()`, which takes a cross-process file lock |
| `index_memory_budget_mb` | `usize` | `1024` | Total heap the resident content indices may hold before `content_index::enforce_memory_budget` drops the least recently used. An ordinary repo indexes to 60–100 MB, so 1 GB holds 10–15 resident and only evicts for an outlier. An evicted index is snapshotted to `<data_dir>/content-index/` and reloaded on return, so eviction costs a stat walk rather than a rebuild. Configurable rather than a constant because the Rust backend does not hot-reload |

**Commands:** `load_app_config()`, `save_app_config(config)`

Every writer of `config.json` — IPC `save_config`, `PUT /config`, MCP
`config action=save`, session-token rotation, `set_global_hotkey`, the
`disabled_mcp_agents` toggle and the push auto-enable on first subscription —
goes through
`config::commit_config_change`, which holds one process-wide mutex across the
whole cache-delta → file-lock → latest-disk-read → delta-merge →
preserve-secrets → write → update-`state.config` sequence. The cross-process
file lock spans the authoritative disk read and write. This distinction matters:
locking whole-document saves merely orders lost updates, while applying the
delta after the locked read preserves unrelated fields written by another
debug or release process.
Rotation
(`config::rotate_session_token`, shared by the desktop command and
`POST /auth/rotate-session-token`) goes through the same path so the vault, the
file and `state.config` cannot disagree — previously the in-memory config kept
the pre-rotation token and the next unrelated save wrote it back.

The vault and `config.json` are one logical commit. Before changing any of the
three vault-backed fields, `save_app_config` snapshots their previous values.
If either a later vault operation or the atomic file replacement fails, all
three vault values are restored before the error returns; `state.config` and
the live authentication token are updated only after success. A rollback
failure is appended to the original persistence error instead of being hidden.
Individual credential `set` and `delete` operations also publish their
in-memory vault clone only after the OS keyring accepts it.

Routing every writer through it also guarantees the file is produced by
`config_for_disk`. A writer that serialized the config itself (the
`disabled_mcp_agents` toggle called `save_json_config("config.json", ..)`)
skipped the stripping step and wrote the session token, relay token and VAPID
private key to disk in cleartext.

### MCP Bridge Auto-Install

On every launch `agent_mcp::ensure_mcp_configs` writes the `tuicommander` bridge
entry into each supported agent's own MCP config, and repairs the path when the
sidecar moves. Each target is written in the format its tool reads:

| Agent | Config file | Shape |
|---|---|---|
| Claude Code | `~/.claude.json` | JSON `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | JSON `mcpServers` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | JSON `mcpServers` |
| VS Code | `<user dir>/mcp.json` | JSON `servers` |
| Zed | `~/.config/zed/settings.json` | JSON `context_servers` |
| Amp | `~/.config/amp/settings.json` | JSON `amp.mcpServers` |
| Gemini CLI | `~/.gemini/settings.json` | JSON `mcpServers` |
| Droid | `~/.factory/mcp.json` | JSON `mcpServers` |
| opencode | `~/.config/opencode/opencode.json[c]` | JSON `mcp`, `{type:"local", command:[…]}` |
| Codex | `~/.codex/config.toml` | TOML `[mcp_servers]` + `env_vars` allowlist |
| Grok | `~/.grok/config.toml` | TOML `[mcp_servers]` |
| goose | `~/.config/goose/config.yaml` | YAML `extensions` (`ExtensionEntry`) |
| pi | `~/.pi/agent/mcp.json` | JSON `mcpServers` (pi-mcp-adapter extension) |

Aider is absent because it has no MCP client.

**A target is written only when it is installed.** The writer creates every
missing parent directory, so an unconditional pass used to create `~/.cursor/`,
`~/.gemini/`, `~/.config/amp/` and friends for tools the user never had —
which makes *other* software report Cursor or Windsurf as installed. Presence is
proven two ways, cheapest first:

1. the config directory holds a file that is not the one we write (`.DS_Store`
   and stale `*.tmp` staging files do not count), or
2. one of the target's CLI binaries resolves via `cli::has_cli`.

Claude's config sits in `$HOME`, so it uses `~/.claude` as its presence
directory instead of the config file's parent. pi is stricter still: its MCP
support comes from the optional pi-mcp-adapter extension, which owns
`~/.pi/agent/mcp.json` — with no such file there is no adapter, so an
auto-written entry would configure nothing.

A target that already holds a `tuicommander` entry keeps getting path repairs
even when presence no longer resolves, so a stale bridge path is never left
behind. All gates live in `auto_install_allowed`, which only the launch pass
consults: Settings → Agents installs on demand through `ensure_spec_entry`
directly, because pressing Install states that the target is there — that is an
explicit request, not a guess.

**Shared settings files need an explicit install.** Zed, Amp and Gemini keep
their MCP server list inside the `settings.json` that also holds every other
user preference, not a dedicated `mcp.json`. Those three carry
`shared_settings_file: true` and the launch pass never creates or edits them:
TUICommander being on the machine is not consent to rewrite the user's editor
configuration. Settings → Agents installs them on request, and once installed
they receive path repairs like any other target. `get_agent_mcp_status` returns
the flag so the UI can say why the entry is missing.

### Never Reserializing a Third-Party Config

Configs that exist but do not parse are **never** overwritten (JSON, TOML and
YAML alike). Treating a parse failure as an empty document is what reduced a
user's 400-line Zed `settings.json` to our single entry
([#115](https://github.com/sstraus/tuicommander/issues/115)).

JSON targets go further: they are never reserialized at all. `jsonc_edit`
parses the document into a concrete syntax tree, splices exactly one member,
and prints it back, so text outside that member is byte-identical. This matters
three times over — `serde_json` rejects the comments and trailing commas Zed,
VS Code and opencode all document as supported; `serde_json::Map` is a
`BTreeMap`, so a round trip alphabetises the user's keys; and
`to_string_pretty` discards their indentation.

Only the documented dialect is accepted. Comments and trailing commas parse;
single-quoted strings, unquoted keys and hexadecimal numbers do not, because a
file using them is one the owning tool cannot read either — writing it back as
if it were fine would be worse than refusing.

Guard rails around the write:

- The edited text is re-parsed and the member compared against what was
  requested before anything reaches disk.
- The first time we modify a file, its original is copied to
  `<config dir>/mcp-backups/<agent>-<filename>.orig`. Written once and never
  overwritten — the state worth keeping is the one from before TUICommander
  ever touched it. It lives under our config directory rather than beside the
  original, where the owning tool might try to load or sync it.
- An edit that changes nothing skips the write entirely, so an idempotent pass
  never moves the mtime of a file an editor is watching.

### Removing Every Integration

`remove_all_mcp_integrations` drops the `tuicommander` entry from every target
that has one and adds them all to `disabled_mcp_agents` — without that the next
launch reinstalls them and the action does nothing. `list_installed_mcp_integrations`
backs the Settings → Agents panel that lists them. Uninstalling TUICommander
otherwise leaves a dangling `tuic-bridge` path in each client it ever
configured, and each one reports a broken MCP server on startup. One
unparseable config does not abort the sweep: the rest are still cleaned and the
failures are reported together.

### Upstream MCP Config (`mcp-upstreams.json`)

**Type:** `UpstreamMcpConfig`

Interactive saves carry both the configuration the caller loaded (`base`) and
its desired `config`. The backend derives additions, intentional removals,
order changes, and per-server field deltas keyed by stable server ID, then
applies them to the latest document inside `ConfigFile::update_with`. A popup
toggle therefore changes only `enabled`; an OAuth/DCR auth record written after
the popup loaded is preserved. Removing a server or clearing an optional auth
field remains explicit and is not mistaken for an omitted/unchanged field.

Validation and the runtime registry diff use the exact merged pre/post values
from the locked transaction. The lock is released before asynchronous reconnect
work starts.

**Commands:** `load_mcp_upstreams()`, `save_mcp_upstreams(base, config)`

### Notification Config (`notifications.json`)

**Type:** `NotificationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Global enable |
| `volume` | `f64` | `0.5` | Volume (0.0-1.0) |
| `sounds.question` | `bool` | `true` | Play on agent question |
| `sounds.error` | `bool` | `true` | Play on error |
| `sounds.completion` | `bool` | `true` | Play on completion |
| `sounds.warning` | `bool` | `true` | Play on warning |
| `sounds.info` | `bool` | `true` | Play on info |
| `sounds.attention` | `bool` | `true` | Play the agent-needs-you callback |
| `audio_device` | `Option<String>` | `None` | Specific output device name; `None` = system default |
| `sound_choices.<event>` | `SoundChoice` | `{preset: "default", custom_path: None}` | Per-event sound source, one entry each for `question`/`error`/`completion`/`warning`/`info`/`attention`. `preset` is a plain string interpreted by `notification_sound::resolve_sequence` (`"default"`, another event's name to borrow its tone, or `"custom"`) rather than a Rust enum, so a new preset value needs no matching change to this struct. `custom_path` is only read when `preset == "custom"` |
| `silence_remote_completions` | `bool` | `true` | Suppress the completion chime for HTTP/MCP-created sessions |
| `toasts_in_bell` | `bool` | `true` | Mirror every toast into the toolbar bell, under a MESSAGES section |

**Commands:** `load_notification_config()`, `save_notification_config(config)`

### AI Chat Config (`ai-chat-config.json`)

**Type:** `AiChatConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | `String` | `"ollama"` | AI provider: `ollama`, `anthropic`, `openai`, `openrouter`, `custom` |
| `model` | `String` | `""` | Model name |
| `base_url` | `Option<String>` | per-provider | Endpoint base URL |
| `temperature` | `f32` | `0.7` | Sampling temperature |
| `context_lines` | `u32` | `150` | VtLogBuffer rows injected per turn |
| `experimental_ai_block_enrichment` | `bool` | `false` | Enrich OSC 133 blocks with semantic intent |
| `agent_model_overrides` | `Option<HashMap<ToolPhase, String>>` | `None` | Per-phase model routing. Keys: `plan`, `search`, `read`, `write` |

**Commands:** `load_ai_chat_config()`, `save_ai_chat_config(config)`

### Cron Scheduler Config (`ai-cron.json`)

**Type:** `SchedulerConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `jobs` | `Vec<ScheduledJob>` | `[]` | List of scheduled agent jobs |

Each `ScheduledJob`:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Unique job identifier |
| `cron_expr` | `String` | Cron expression (validated on save) |
| `goal` | `String` | Agent goal to execute |

**Commands:** `load_scheduler_config()`, `save_scheduler_config(config)`

### UI Preferences (`ui-prefs.json`)

**Type:** `UIPrefsConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sidebar_visible` | `bool` | `true` | Sidebar visibility |
| `sidebar_width` | `u32` | `260` | Sidebar width in pixels |
| `diff_panel_visible` | `bool` | `false` | Diff panel open |
| `markdown_panel_visible` | `bool` | `false` | Markdown panel open |
| `notes_panel_visible` | `bool` | `false` | Notes panel open |
| `file_browser_panel_visible` | `bool` | `false` | File browser panel open |
| `plan_panel_visible` | `bool` | `false` | Plan panel open |
| `git_panel_visible` | `bool` | `false` | Git panel open |
| `outline_panel_visible` | `bool` | `false` | Outline panel open |
| `references_panel_visible` | `bool` | `false` | References panel open |
| `ai_chat_panel_visible` | `bool` | `false` | AI chat panel open |
| `ai_triage_panel_visible` | `bool` | `false` | AI triage panel open |
| `file_browser_view_mode` | `String` | `"flat"` | File browser listing: `flat` or `tree` |
| `diff_panel_width` | `u32` | `400` | Diff panel width in pixels |
| `markdown_panel_width` | `u32` | `400` | Markdown panel width in pixels |
| `notes_panel_width` | `u32` | `350` | Notes panel width in pixels |
| `plan_panel_width` | `u32` | `350` | Plan panel width in pixels |
| `git_panel_width` | `u32` | `380` | Git panel width in pixels |
| `settings_nav_width` | `u32` | `180` | Settings nav column width in pixels |
| `diff_view_mode` | `String` | `"split"` | Diff viewer: `split` or `unified` |
| `detached_panels` | `HashMap<String, String>` | `{}` | Panel id to detached window label |
| `github_section_collapsed` | `HashMap<String, bool>` | `{}` | Collapsed GitHub sections (`my-prs`, `prs`, `issues`); absent key means the section's own default |

The eight `*_panel_visible` flags for markdown, file browser, git, outline,
references, AI chat, AI triage and notes are **mutually exclusive** — the
frontend opens one and closes the rest. The backend does not enforce that; it
stores whatever it is sent.

Every key the frontend sends must be declared here. Serde has no
`deny_unknown_fields` on this struct, so an undeclared key is dropped on the
way in without an error and `load_ui_prefs` can never return it. The panel
then looks like it saves and silently fails to survive a restart.

**Commands:** `load_ui_prefs()`, `save_ui_prefs(config)`

### Repository Settings (`repo-settings.json`)

**Type:** `RepoSettingsMap` (HashMap of `RepoSettingsEntry`)

Per-repository fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | `String` | -- | Repository path |
| `display_name` | `String` | -- | Display name |
| `base_branch` | `Option<String>` | `null` | Tri-state override (`null` inherits `.tuic.json`, then the global default) naming which branch new worktrees are created from — `"automatic"` means detect the repo's default branch |
| `copy_ignored_files` | `Option<bool>` | `null` | Tri-state override (`null` inherits `.tuic.json`, then the global default) of whether `.gitignore`d files are copied into a new worktree |
| `copy_untracked_files` | `Option<bool>` | `null` | Tri-state override of whether untracked (never `git add`ed) files are copied into a new worktree |
| `copy_paths` | `Vec<CopyPathEntry>` | `[]` | Files/directories always copied (or symlinked) into every new worktree of this repo, regardless of the two toggles above. Repo-specific only — no `.tuic.json`/global tier, same as `branch_labels`. Each entry is `{ path: String, mode: CopyPathMode }` (`mode`: `"copy"` \| `"symlink"`) |
| `setup_script` | `String` | `""` | Script to run after worktree creation |
| `run_script` | `String` | `""` | Default run command |
| `auto_fetch_interval_minutes` | `u32` | `0` | Auto-fetch interval in minutes (0 = disabled) |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | `"off"` | Auto-delete branch when PR merged/closed (`off`/`ask`/`auto`) |
| `archive_script` | `String` | `""` | Script to run before archive/delete (non-zero exit blocks) |
| `pr_hide_drafts` | `Option<bool>` | `null` | Tri-state override of the global `pr_hide_drafts` — `null` inherits, `Some(bool)` overrides |
| `pr_hide_conflicting` | `Option<bool>` | `null` | Tri-state override of the global `pr_hide_conflicting` |
| `pr_hide_ci_failing` | `Option<bool>` | `null` | Tri-state override of the global `pr_hide_ci_failing` |
| `terminal_meta_hotkeys` | `Option<bool>` | `null` | Tri-state override of Cmd+1-9 terminal hotkeys (global default: `true` on macOS) |

`#[serde(default)]` on every field above means an unrecognized JSON key is normally dropped
without error — this is exactly how the frontend's camelCase/snake_case mismatch went unnoticed
for a while. `RepoSettingsEntry` carries a flattened `extra: HashMap<String, serde_json::Value>`
catch-all (never re-serialized) so an unrecognized key is captured instead of vanishing;
`load_repo_settings()` logs a `tracing::warn!` naming the repo path and the exact unrecognized
keys whenever `extra` is non-empty.

**Commands:** `load_repo_settings()`, `save_repo_settings(config)`, `check_has_custom_settings(path)`

`copy_ignored_files`/`copy_untracked_files`/`copy_paths` are resolved via
`resolve_effective_copy_settings(repo_path)` (three-tier for the two
booleans, repo-specific-only for `copy_paths`) and consumed by
`worktree::spawn_worktree_file_sync`, which runs in the background after a
worktree is actually created — see `src-tauri/src/worktree_sync.rs` for the
copy/symlink engine and `docs/sync-matrix.md`'s event table for the
`worktree-sync-*` events it emits.

### Repository Defaults (`repo-defaults.json`)

**Type:** `RepoDefaultsConfig`

Default values applied to new repositories when no per-repo override exists.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_branch` | `String` | `"automatic"` | Default base branch |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Default setup script |
| `run_script` | `String` | `""` | Default run command |
| `archive_script` | `String` | `""` | Default archive script |
| `setup_script_timeout_secs` | `u64` | `600` | Setup script timeout — a hung script is killed (process group, on Unix) and `run_setup_script` returns `Err` |
| `archive_script_timeout_secs` | `u64` | `120` | Archive script timeout — a timeout aborts the archive/remove, same as a non-zero exit |

**Commands:** `load_repo_defaults()`, `save_repo_defaults(config)`

### Repositories (`repositories.json`)

**Type:** `serde_json::Value` (flexible persisted JSON, shape defined by frontend)

Stored in the shared config directory like every other file (see Config
Directory) — debug and release builds read and write the same
`repositories.json`. Every write uses a versioned delta inside the existing
`save_repositories(config)` argument:

```json
{
  "mutationVersion": 1,
  "repos": [{ "id": "/repo", "before": {}, "after": {} }],
  "groups": [{ "id": "group-id", "before": null, "after": {} }],
  "repoOrder": { "before": [], "after": ["/repo"] },
  "activeRepoPath": { "before": null, "after": "/repo" },
  "groupOrder": { "before": [], "after": ["group-id"] }
}
```

`before` is the last value that client successfully loaded or persisted;
`after` is its intended value, and `null` in an ID-keyed mutation means absence
or deletion. The backend acquires the cross-process lock, strictly reloads the
latest document, and applies each repository/group mutation by ID. Mutations
to different IDs compose. Independent membership additions/removals in
`repoOrder` and `groupOrder` are three-way merged; incompatible reorders
conflict. Active-repository changes use the same `before`/`after` check.

A stale mutation of the same repository, group, active selection, or order is
rejected with a deterministic conflict instead of overwriting the newer value.

**Derived branch fields are exempt from that check.** `additions`, `deletions`,
`isMerged`, `lastActiveTerminal`, `lastCommitTs` and `lifecycleStatus`
(`DERIVED_BRANCH_FIELDS` in `config.rs`) are a cache each client recomputes from
the repository itself, on
its own refresh cadence — two windows legitimately hold two different values at
the same instant, so comparing them turns every save into a conflict. Measured
2026-08-31: `ego` moved 331 → 357 additions in 70 seconds while 29 consecutive
saves were rejected, and unrelated intent — registering a repository — was
wedged behind a number nobody edited. The conflict check compares records with
those fields stripped; the "already applied" check stays exact, so a
derived-only update still persists and the cache keeps moving. Everything a
human sets (name, order, grouping, active branch) is still fully guarded, and
concurrent edits to the derived fields themselves resolve last-writer-wins.
IPC reports that error to the frontend, where it creates a user-visible Errors
badge; HTTP returns `409 Conflict`. Malformed deltas return HTTP `400`. A
versioned delta is required; unversioned whole-document bodies are rejected.

**A successful write is announced, so the other clients converge instead of
colliding.** One backend serves the desktop WebView, the browser and the PWA at
once, and each keeps its own `before` baseline. Until this event existed, a save
told the other clients nothing: their baseline stayed stale until a conflict
taught them otherwise, which is late — by then the two documents have already
diverged. Both save paths (`save_repositories` over IPC, `PUT
/config/repositories` over HTTP) now call `AppState::notify_repositories_changed()`,
which dual-emits the payload-free `repositories-changed` event to the desktop
window and onto the `/events` SSE bus.

It fires **only when the document actually moved**:
`save_repositories_request` returns `Ok(true)` for a write and `Ok(false)` for a
delta already applied, so a no-op save does not wake every client. The payload is
empty by design — a receiver only needs "disk moved, re-read it", and shipping
the document would copy the whole repository set to everyone on every save.

On the receiving side `repositoriesStore` re-reads `repositories.json` and adopts
a key **only when it has no unsaved intent for it** — when its live value still
equals its baseline. Adopted keys move in the store *and* in the baseline
together: those two are diffed against each other on every save, so refreshing
one alone would make the next diff revert what the other client just wrote. Two
keys are deliberately never adopted: `activeRepoPath`, because which repo a
window is looking at is per-window and a background event must not move the
user's focus, and a repository whose disk record disappeared while it still holds
open terminals, because dropping it would orphan panes the user is looking at.
Keys this client did change are left untouched and still resolve through the
compare-and-swap rebase.

Four details make that gate hold up in practice:

- **The "unsaved intent" test runs on an intent view, not the whole record.**
  `repositoriesStore` mirrors the backend's `DERIVED_BRANCH_FIELDS` (plus
  `hadTerminals`, which is session state that happens to be persisted) and
  normalizes both sides through the same migration pass `hydrate` applies. Without
  the first, a repo under active work drifts from its own baseline every few
  seconds via `updateBranchStats`, which saves nothing — and adoption would be
  refused for exactly the repos the user is working in. Without the second, a
  baseline read off disk *before* the migration defaults were added never matches
  the migrated store, and a record written by an older build is never adopted.
- **The live-terminal rule applies per branch, not only per repository.** A repo
  record that survives on disk with one branch removed goes through the merge path,
  where the repo-level guard never runs; a branch this window still has a pane in is
  kept there instead.
- **Every repo the store holds stays in `repoOrder`.** The order arrives from the
  client that *did* drop the repo, so a record kept for its live terminals would
  otherwise leave the sidebar while its panes keep running. Grouping is an overlay:
  `getGroupedLayout` filters grouped paths out of `repoOrder` at render time, so
  putting a repo back there is always safe.
- **Adoption waits for an in-flight save.** A save ends by assigning the baseline
  it computed before it was sent, and nothing orders the broadcast against that
  save's own reply — so an adoption landing inside the window would have its
  baseline overwritten while its store changes stay, which is the one state this
  whole path exists to prevent.

**Version skew is the dangerous case, and it is one-sided.** A backend from
before the delta protocol does not decode `config` at all — it stores it as the
whole document, so `repositories.json` becomes the delta envelope and every
repository is lost. This is not theoretical: `make dev` never hot-reloads Rust,
so a hot-reloaded frontend meeting a stale backend did exactly that on
2026-08-21. The old binary cannot be fixed retroactively, so the guard lives at
the read end: `repositoriesStore.hydrate()` treats a root `mutationVersion`, or
`repos` as an array, as a poisoned file — it logs a user-visible error, refuses
to hydrate, and leaves `hydrated` false so **no save can run**. That last part is
what makes a hand-restored backup stick; without it, the running app clobbers the
restored file within seconds.

`repositories.json` used to be the one file exempt from the (then-real)
debug/release split: it was seeded into a separate `~/.tuicommander-dev/`
directory on first debug run so a dev instance wouldn't start with an empty
repo list. That seeding path is gone now that both builds share one
directory for `repositories.json` and all other config domains covered by
this document. (`~/.tuicommander-dev/` itself still exists for an unrelated
purpose — see `credentials.rs`'s debug-only credential store.)

**Commands:** `load_repositories()`, `save_repositories(config)`

#### Stale-temp repository repair (#763-d219)

Live evidence: 15 hydrated `repositories.json` rows pointed at paths under macOS
temp roots or `$HOME/Gits/.tmp` that no longer existed on disk — throwaway shell
repos a prior session or agent worktree run created and never cleaned up. Each
was non-git, held exactly one shell-only workspace, and carried no terminals,
saved terminals, diffstat, commit, parent, or user metadata: a maximally
"empty" record, which is exactly why a missing path alone is not sufficient
evidence — a legitimate repository on an unmounted drive or a machine the user
switched away from looks identical on that one axis.

**Classifier — `classify_stale_temp_repo` (`config.rs`), ALL of:**

1. `std::fs::metadata(path)` returns `NotFound` — the local path is proven not
   to exist. Permission and other I/O errors preserve the row because they do
   not prove absence.
2. The path falls under a recognized temp root (`recognized_temp_roots()`:
   `std::env::temp_dir()`, `/tmp`, `/private/tmp`, `/var/folders`,
   `/private/var/folders`, `$HOME/Gits/.tmp`).
3. `isGitRepo` is explicitly `false` (never merely absent or `true`).
4. Exactly one workspace, and it is shell-only: no live or saved terminals, no
   last-active terminal, no diffstat/commit/merge state, no parent, no run
   command, no CI auto-heal.
5. No user metadata on the repo record itself: `collapsed`/`parked` are their
   defaults (`false`), and no `connectionId`/non-empty `initials`.

Any single failing check leaves the record untouched — this is an ALL-of test,
not a heuristic score, so a legitimate offline/unmounted/renamed repository
always survives.

**Commands** (IPC + `GET`/`POST /config/repositories/stale-temp` HTTP twin):

- `list_stale_temp_repository_candidates()` — read-only preview; re-classifies
  the on-disk document fresh on every call, never a cached list.
- `repair_stale_temp_repositories(paths)` — the only mutating path, and the
  only place implicit or global deletion is refused: it takes the exact paths
  the user confirmed and re-validates every one of them against the classifier
  using the document as it stands on disk *inside the same file lock* as the
  write. If even one no longer classifies (reconnected, edited since the
  preview, or never stale to begin with), the **entire** request is rejected
  before anything is written — one versioned transactional delta, not a
  best-effort sweep. On success it writes an exact pre-repair backup to
  `repositories.repair-backup-<UTC timestamp>.json` in the config directory,
  then removes the validated rows from `repos`, `repoOrder`, every group's
  `repoOrder`, and clears `activeRepoPath` if it pointed at a removed row — all
  inside `ConfigFile::update_with_strict`'s file lock, then calls
  `notify_repositories_changed()` like `save_repositories`.

**Why there is no separate restore-on-failure path:** `ConfigFile::write_atomic`
(temp file + fsync + rename) already guarantees a failed write never partially
overwrites the live document — the original is simply untouched. The backup
file is therefore a *human* recovery artifact for undoing a repair that
succeeded but was unwanted, not a mechanism this code needs to invoke itself on
a write failure. Recorded as a deliberate design trade-off in story
763-d219's worklog rather than layering a second, redundant recovery path on
top of a write that is already atomic.

**Frontend:** classified candidates are hidden from the sidebar's normal repo
list (quarantined, never silently deleted) pending an explicit repair; see
`repositoriesStore` (`refreshStaleTempCandidates`/`repairStaleTemp`,
`src/stores/repositories.ts`) and the discoverable repair entry point — a red
flagged-repo icon with a count badge in the sidebar footer, opening
`StaleTempRepairPopover` (`src/components/Sidebar/StaleTempRepairPopover.tsx`),
next to the existing parked-repositories popover.

### Prompt Library (`prompt-library.json`)

**Type:** `PromptLibraryConfig`

```rust
struct PromptEntry {
    id: String,
    label: String,
    text: String,
    pinned: bool,
}
```

**Commands:** `load_prompt_library()`, `save_prompt_library(config)`

### AI Prompts (`ai-prompts.json`)

**Type:** `AiPromptsConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `diff_triage_system_prompt` | `Option<String>` | `None` | Custom system prompt for diff triage LLM classification. Falls back to built-in default when `None` or empty. |

**Commands:** `load_ai_prompts()`, `save_ai_prompts(config)`

**MCP actions:** `list_ai_prompts`, `load_ai_prompt` (requires `service`), `save_ai_prompt` (requires `service` + `prompt`, localhost only)

### Notes (`notes.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

**Commands:** `load_notes()`, `save_notes(config)`

### Keybindings (`keybindings.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

Custom keyboard shortcut overrides.

**Commands:** `load_keybindings()`, `save_keybindings(config)`

### Agents Config (`agents.json`)

Each agent entry may contain `native_status_signals: boolean`. For Claude and Codex, an absent value means `true`; `false` disables launch argument injection. `hook_instrumentation` controls only explicit global installation and remains off when absent.

**Type:** `AgentsConfig`

Per-agent run configurations (custom commands, arguments, environment variables).

```rust
struct AgentRunConfig {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    is_default: bool,
}

struct AgentSettings {
    run_configs: Vec<AgentRunConfig>,
}

struct AgentsConfig {
    agents: HashMap<String, AgentSettings>,
}
```

**Commands:** `load_agents_config()`, `save_agents_config(config)`

### AI Chat Config (`ai-chat-config.json`)

**Type:** `AiChatConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | `String` | `"ollama"` | Provider: `"ollama"`, `"anthropic"`, `"openai"`, `"openrouter"`, `"custom"` |
| `model` | `String` | provider-specific | Model name (free text; settings tab suggests per provider) |
| `base_url` | `Option<String>` | provider-specific | Pre-filled per provider, editable. Ollama default: `http://localhost:11434/v1/` |
| `temperature` | `f32` | `0.7` | Sampling temperature passed through to provider |
| `context_lines` | `u32` | `150` | Maximum `VtLogBuffer` lines injected into each turn's context |

**Commands:** `load_ai_chat_config()`, `save_ai_chat_config(config)`

API keys are stored in the OS keyring — service `tuicommander-ai-chat`, user `api-key` — via `save_ai_chat_api_key` / `delete_ai_chat_api_key`. Saved conversations live in `<config_dir>/ai-chat-conversations/<id>.json`.

Each file carries a `schema_version` stamped by `save_conversation` — 1 = chat text only, 2 = tool-call fields on messages, 3 = the `agent` block (`state`, `currentIteration`, `toolCalls`) that restores an interrupted agent run. Older files load unchanged: every field added since v1 has a serde default, and `load_conversation` re-stamps and rewrites anything below the current version. Types in `src-tauri/src/ai_agent/conversation.rs`.

### Dictation Config (`dictation-config.json`)

**Type:** `DictationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Dictation enabled |
| `hotkey` | `String` | `"CommandOrControl+Shift+D"` | Push-to-talk hotkey |
| `language` | `String` | `"en"` | Transcription language |
| `model` | `String` | `"large-v3-turbo"` | Whisper model name |
| `auto_send` | `bool` | `false` | Auto-submit after transcription |

**Commands:** `get_dictation_config()`, `set_dictation_config(config)`

## Cache Files

### Claude Usage Cache (`claude-usage-cache.json`)

**Module:** `src-tauri/src/claude_usage.rs`

Persistent cache for incremental JSONL parsing of Claude session transcripts. Stored in the config directory. The cache maps `project_slug -> (filename -> CachedFileStats)` and tracks per-file byte offsets so only newly appended data is parsed on subsequent scans.

This is an internal cache file, not user-editable. It is automatically pruned when projects or session files are deleted.

## First-Run Prompt Markers

Small empty-content marker files in `config_dir()` record that a one-time startup prompt was
already shown/dismissed, so it never repeats. Existence alone is the signal — content is unused.

| File | Module | Set by |
|------|--------|--------|
| `.cli-prompt-dismissed` | `tuic_cli.rs` | `dismiss_cli_prompt` — the "Install tuic CLI?" prompt |
| `.finder-service-prompt-dismissed` | `finder_service.rs` (macOS only) | `dismiss_finder_service_prompt` — the "Add Finder integration?" prompt |
| `.whats-new-seen` | `tuic_cli.rs` | `set_last_seen_version` — tracks the last version whose "What's New" was shown |

## Repo-Local Config (`.tuic.json`)

**Module:** `src-tauri/src/config.rs`

A `.tuic.json` file in the repository root provides team-shareable settings. It is read-only from the app — teams edit it directly in their repo and commit it.

**Precedence chain:** per-repo app settings (`repo-settings.json`) > `.tuic.json` > global defaults (`repo-defaults.json`) — an explicit per-repo choice in the app always wins over the committed team file, which itself only fills in what neither the app setting nor the user chose. `copy_paths` has no tier here at all: it's repo-specific-only, resolved straight from `repo-settings.json` with no `.tuic.json`/global fallback (same as `branch_labels`).

**Type:** `RepoLocalConfig` (all fields `Option<T>`, missing fields fall through to lower tiers)

| Field | Type | Description |
|-------|------|-------------|
| `base_branch` | `String` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | Copy untracked files to worktree |
| `worktree_storage` | `WorktreeStorage` | Storage strategy (sibling/app-dir/inside-repo) |
| `delete_branch_on_remove` | `bool` | Delete branch when removing worktree |
| `auto_archive_merged` | `bool` | Auto-archive merged worktrees |
| `orphan_cleanup` | `OrphanCleanup` | Orphan worktree handling |
| `pr_merge_strategy` | `MergeStrategy` | PR merge method preference |
| `after_merge` | `WorktreeAfterMerge` | Post-merge worktree action |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | Auto-delete on PR close |

**`setup_script`/`run_script`/`archive_script` are deliberately NOT fields of `RepoLocalConfig`** — executing a repo-committed script with no trust-on-first-use confirmation would let a malicious branch run arbitrary code the moment its worktree is created. Configure these per-repo (`repo-settings.json`, via Settings) or as a global default (`repo-defaults.json`) instead — see `resolve_archive_script`/`resolve_effective_setup_script` in `src-tauri/src/worktree.rs`/`config.rs`, both of which skip this tier explicitly.

**Command:** `load_repo_local_config(repo_path)` — returns `RepoLocalConfig` or `null` if file is missing or malformed.

## Progress Storage (`.tuic/progress.sqlite3`)

**Module:** `src-tauri/src/progress/` (`store.rs`, `ownership.rs`, `model.rs`,
`export.rs`)

Project-owned Rust history stores reported `started`, `milestone`, `blocked`,
and `done` outcomes plus derived workstream state independently of sessions and
workspaces. The MCP `repo` actions, Tauri commands, and HTTP routes share one
service layer (`service.rs`), which resolves the owning project before it opens
the store.

The database lives at `<owning-project-root>/.tuic/progress.sqlite3` with its
SQLite sidecars. Ownership resolution starts from an authoritative registered
project and follows recorded linked and nested workspace parent records;
it never uses the focused UI repository or a bare CWD. Unbound callers fail
with `project_required`.

Schema version 2 stores project revision, persistent collection/read-cursor
state, workstreams and rename aliases, events with monotonic sequence numbers
and UUIDv7 ids, independently active blockers, and preserved source snapshots
for merged events. Sequence and revision values are not reused after deletion
or clear.
Each operation opens a fresh SQLite connection in WAL mode with a five-second
busy timeout, leaving SQLite locking as the cross-thread and cross-process
serialization boundary.

Failures are explicit: `progress_store_unavailable`, `progress_store_busy`,
`progress_store_incompatible`, `progress_store_corrupt`,
`progress_store_recovered`, or `progress_store_recovery_failed`. Recovery is
serialized by `progress.sqlite3.recovery.lock`; it preserves the database and
any WAL/SHM files byte-for-byte under unique `.corrupt-<uuid>` names, creates a
validated empty replacement, and still returns an error so the caller must
retry rather than mistake the replacement for the original history.

On first open, the store adds its database, sidecars, recovery lock, corrupt
backup pattern, and the Markdown export lock (`.tuic/progress-export.lock`) to
the repository-local `.git/info/exclude`, never tracked `.gitignore`. Both the
repository watcher and content index honor that exclude, so Progress persistence
does not emit repository changes or trigger indexing. The exported
`progress.md` is deliberately outside that contract: it is the user's artifact
and must reach the working tree.

The Markdown export (`export.rs`) reads one committed database snapshot and
identifies it as `sha256:<digest>` over revision, snapshot time, options, and
the rendered Markdown. Preview returns that identity with the rendered text and
any existing target content; write repeats the snapshot, refuses a changed
identity or a changed/removed target, refuses symlink and non-regular targets,
and replaces the file atomically through a same-directory temporary file. The
export never writes to the database, and it never stages, commits, or pushes:
the only Git interaction it inherits is the exclude registration every store
open performs.

## Additional Commands

| Command | Module | Description |
|---------|--------|-------------|
| `hash_password(password)` | `lib.rs` | Bcrypt hash for remote access authentication |
| `list_markdown_files(path)` | `lib.rs` | List .md files in a directory |
| `read_file(path, file)` | `lib.rs` | Read a file's contents |
| `get_mcp_status()` | `lib.rs` | Get MCP server status (enabled, port, connected clients) |
| `clear_caches()` | `lib.rs` | Clear in-memory caches |
| `get_local_ip()` | `lib.rs` | Get primary local IP address |
| `get_local_ips()` | `lib.rs` | List all local network interfaces |
| `get_claude_usage_api()` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline(scope, days?)` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats(scope)` | `claude_usage.rs` | Scan JSONL transcripts for aggregated token/session stats |
| `get_claude_project_list()` | `claude_usage.rs` | List Claude project slugs with session counts |
| `fetch_plugin_registry()` | `registry.rs` | Fetch remote plugin registry index |
