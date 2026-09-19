# TUICommander — Settings Panel (`src/components/SettingsPanel/`)

Repo-wide and general-frontend rules live in the root `AGENTS.md` and
`src/AGENTS.md` — read those first. This file covers what's specific to the
Settings panel: tri-state inheritable settings, the search index, and
export/import of user-editable collections.

## Tri-State Inheritable Settings

Any per-repo or per-agent boolean setting that can inherit a global default (RepoWorktreeTab's file-handling/worktree/PR-visibility toggles, AgentsTab's per-agent overrides) is stored as `boolean | null`, `null` meaning "inherit," and rendered with the shared `TriStateToggle` (`src/components/shared/TriStateToggle.tsx`) — never a plain checkbox, which can only ever hold a concrete value and has no way to re-select "inherit" once touched.

**Resolution is always `override ?? globalDefault`** (see `src/stores/repoSettings.ts`'s `resolvers` map, e.g. `copyIgnoredFiles: (s, local) => s.copyIgnoredFiles ?? local()?.copy_ignored_files ?? repoDefaultsStore.state.copyIgnoredFiles`). Never invert this order — resolving `globalDefault ?? override` silently makes every explicit "Off" override unreachable whenever the global default is `true`. Any new inheritable field must go through `RepoSettingsEntry`/`AgentConfig`'s `Option<bool>` (Rust) / `boolean | null` (TS) shape and a `resolvers` entry, not a bare `bool`/`boolean` — a bare boolean silently drops the "use global" state.

**The trigger for TriStateToggle is a real global counterpart to inherit from, not merely `Option<bool>`.** `AgentsTab.tsx`'s `intent_tab_title`/`suggest_followups` qualify because a global `Settings > Agents` toggle exists for each (`global AND (per_agent ?? true)` — see `marker_flags_for_agent`, `config.rs`). `hook_instrumentation`, `auto_retry_on_error`, and `prefer_tuic_spawning`/`prefer_tuic_messaging` are also per-agent `Option<bool>` with `None` defaulting to a fixed literal (`false`, `false`, `true`, `true` respectively) — but there is no global-level flag any of them fall back to, so a plain `<input type="checkbox">` is correct for them, not a rule violation. Before converting a plain checkbox to `TriStateToggle` (or flagging one as non-compliant), check whether a matching global setting actually exists — `TriStateToggle` also has no `disabled` prop today, so a setting that needs a greyed-out state (like the two `prefer_tuic_*` checkboxes when the `agent` MCP tool is off) can't adopt it as a drop-in swap anyway.


## Settings Search Index — Every Setting Needs to Be Findable

The Settings panel's search box and its Command Palette entries
(`settingsSearchIndex.ts`) scroll to and highlight the matched control via
`scrollToSetting(root, section, label)` (`SettingsSearch.tsx`) — a text-based
scroll-target, not a stamped DOM id. Each `SettingsSearchEntry` carries
`{tab, section, label, hint}` (plus `*Key` variants for i18n-keyed labels).

**Coverage is still enforced, just relocated.**
`components/SettingsPanel/__tests__/settingsSearchIndex.test.ts` parses each
tab's own source via `extractSettings.ts`'s `extractTab`/`extractRenderedTabKeys`
(driven by a `TAB_SOURCES` nav-key→file map) and fails naming the exact
entry whose `label`/`section` text doesn't actually appear in that tab's
rendered output. Run it after touching any tab file that has entries in the
registry — `settingsSearch.test.tsx` and `settingsPanelSearch.test.tsx` cover
the search/scroll behavior itself.

*(This replaced an earlier `settingSlugId`/`getElementById` id-stamping
mechanism — if you find a stale reference to `settingSlugId` or
`settingsSearchCoverage.test.ts` anywhere, it's describing the old
mechanism; there is no `settingSlugId` function in the codebase today.)*

**A sentinel value sharing its namespace with real, externally-sourced data is a latent collision, not just a style nitpick.** `RepoWorktreeTab.tsx`'s "Branch From" dropdown uses the string `"automatic"` as a sentinel meaning "let TUIC detect the default branch" — fine while the dropdown was a hardcoded list, but once it was changed to render the repo's actual branches, a real branch literally named `automatic` (a legal git ref name) would render as a second `<option value="automatic">`, indistinguishable from the sentinel and permanently unselectable. Fixed narrowly by filtering any ref matching a reserved sentinel (`automatic`, `__inherit__`) out of the rendered list — a display-only fix, not a sentinel-scheme redesign (that would be a wire-format/migration concern). Any dropdown that mixes a fixed sentinel value with a dynamically-fetched, open-ended real-world value (branch names, file paths, user-typed strings) needs the same check: either namespace the sentinel so it can never collide (e.g. `__automatic__`), or filter colliding real values out of the rendered options.

## Export/Import Settings Collections & Native File Save

Smart Prompts and Smart Selection rules both have a Settings toolbar for moving a user-editable
collection between machines: a scope dropdown (`all`/`modified`/`custom`), **Export…**, and
**Import…** with a NEW/CONFLICT review dialog before anything is applied. Building a third one
(keybindings, theme presets, etc.) should reuse this infrastructure rather than re-implementing it:

- **`src/utils/jsonFileTransfer.ts`** — `saveJsonFile`/`exportJsonWithToast` open the native OS
  Save dialog on desktop (`@tauri-apps/plugin-dialog`'s `save()`, imported *dynamically* so browser
  mode never pulls the plugin in and so `vi.mock("@tauri-apps/plugin-dialog", ...)` is honored)
  then write through the existing `write_external_file` command — already path-validated
  (`fs.rs`'s `validate_external_write_path`: absolute path, no `..` traversal, parent must
  already exist — NOT home-dir-restricted despite that function's own doc comment; see its
  `validate_external_write_accepts_path_outside_home` test) and already has full IPC/HTTP
  parity, so a new export feature needs **no new Rust and no new `transport.ts` entry**. Falls back
  to a Blob + `<a download>` (`downloadText`) in browser mode, where the native dialog is
  unavailable. `pickJsonImportFile` is the matching `<input type="file">` picker for import.
- **`src/components/ImportReviewDialog/ImportReviewDialog.tsx`** — generic checkbox-list review
  dialog (NEW/CONFLICT badges, All/None/New-only bulk select, Escape-to-close via
  `stores/modalStack`) that `PromptImportDialog` and `RuleImportDialog` both adapt via `meta`/
  `conflictNote`/`reviewWarning`/`footnote` props. Add a new thin adapter, not a copy of the whole
  dialog — the shared component is what keeps a future accessibility/Escape-handling fix from
  drifting between copies.
- **Classify "modified" against your own defaults, don't reuse another feature's compare
  function.** `promptExport.ts`'s array-field compare (`normalizeField`) sorts before comparing —
  correct for a *set* field like `placement`, wrong for anything positional. `smartSelectionExport.ts`'s
  `actions` compare is deliberately unsorted because action order is the right-click menu order and
  decides which action is default; copy that positional-vs-set distinction, not the sort.
- **A collection stored as `[] means use built-in defaults`** (no `builtIn` flag on the item type)
  needs its own merge helper, not the prompt library's — see `mergeImportedRules` in
  `smartSelectionExport.ts`: it resolves to the *effective* list first, then upserts, and the
  caller must know that importing anything therefore materializes the full default set into
  `config.json` (same as editing one item already does for Smart Selection rules).
- **Any imported item that can run a command or write into a live PTY must import disabled** and
  be flagged `needsReview` in the dialog — the direct analogue of Smart Prompts' `shell`/`api`
  execution-mode handling. Force this at the actual write boundary (the merge/apply step), not
  just in the classifier that only drives the dialog's badge.

