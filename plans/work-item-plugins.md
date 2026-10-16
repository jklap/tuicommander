---
status: in_progress
started_at: "2026-10-10T23:28:49.859Z"
updated: "2026-10-10T23:28:49.859Z"
---
# Plan: Work-item plugins (GitHub, Jira, GitLab)

**Created:** 2026-10-10 | **Status:** Approved scope, in execution | **Effort:** XL | **Branch:** `docs/work-item-plugins-plan` | **Baseline:** `99302122b`

## Summary

Give TUIC project-management features for GitHub, Jira and GitLab. The core owns accounts, plugin settings, host APIs and one generic Solid **Tasks** surface (list, detail, create, board, roadmap). Provider plugins supply data, field schemas and actions; they never render HTML and get no iframe. Phase 0 (defects D2-D8) is done on `main` (merge `df8c5ab52`, stories 1600-1606).

Authority (do not reopen): the work-item providers analysis in `ideas/` (main checkout, git-ignored), `~/Gits/personal/orchestrator/reports/tuicommander/2026-10-09-work-item-plugins.md`, `2026-10-10-plugins-scope.md` (all 15 rows kept), `~/Gits/.tmp/results/plugins-tweaks-1010.md`.

## Boss decisions (final)

- A1 migrate existing GitHub accounts into a new core account registry. A2 the core keeps the GitHub provider; `github-work` uses it through the core. A3 Jira and GitLab use tokens only (no OAuth).
- H5 sidebar branch badge for a worktree linked to an item. Board with drag and drop in the first slice. D3 done (`plugins.rs:307`).
- W: the Tasks view is generic in core, built from Solid controls and layout like Settings. No per-plugin HTML, no per-plugin iframes.
- Scope per provider exactly as the scope report rows 1-15. Rows 9-12 extend existing GitHub features; a second review, merge, diff or status engine is a rejection.
- Secrets go through the core secret store. Plugins and the model can never read a token back.
- Every new field kind or work-item view ships with its `plugin_dev_guide` entry (`src-tauri/src/mcp_http/plugin_docs.rs`; drift tests `plugin_docs_matches_live_plugin_surface`, `plugin_docs_matches_core_plugin_ux`).

## Architecture Context

- Plugin = ES module in the host realm (`src/plugins/pluginLoader.ts`), API object `PluginHost` (`src/plugins/types.ts:477`, built in `pluginRegistry.ts`). Capabilities are consent, not isolation.
- Plugin HTTP: `src-tauri/src/plugin_http.rs` (`validate_url`, redirect re-validation at `:64-95`); manifest load `src-tauri/src/plugins.rs`.
- Vault: `src-tauri/crates/tuic-core/src/credentials.rs:92` closed `Credential` enum, one blob. GitHub accounts: `src-tauri/src/github_account.rs`; device flow `src-tauri/crates/tuic-git/src/github_auth.rs:25` (D1: token reaches the webview).
- Settings UI: `GLOBAL_TAB_GROUPS` (`src/components/SettingsPanel/SettingsPanel.tsx:55`), controls in `SettingFields.tsx`, search `settingsSearchIndex.ts`.
- Reuse: virtual lists (`src/components/shared/DiffFileList.tsx:107`, TanStack Solid Virtual), `src/components/ui/ContentRenderer.tsx` (marked + DOMPurify), PR detail (`src/components/PrDetailPopover/PrDetailContent.tsx`), `src/utils/prMerge.ts`, `src/components/PrDiffTab/PrDiffTab.tsx`, worktree flow `src/hooks/git/createWorktreeWorkflowCoordinator.ts`, `src/utils/sendCommand.ts`, `git::get_remote_url` (`lib.rs:2055`, `mcp_http/git_routes.rs:204`).
- `plugins/` is a git submodule (`tuicommander-plugins`). Plugin tests: `pnpm test:plugins` (`node --test`).
- ADR: provider plugins run in JS and call HTTP through `host.httpFetch({account})` — the token is injected in Rust — rejected: `gh`/`glab` CLI (30 s exec cap, second auth model) and plugin-held tokens.

## Settings-MCP dependency (named)

`plans/settings-mcp-redesign.md` (in progress) owns the settings catalogue. This plan adds no second settings path:

- Account registry = a catalogue store. Settings-MCP **Step 3a (GitHub accounts adapter)** must adapt the A1 registry service, not the old `github_accounts.json`. A1 exposes the service; the catalogue adapter is Step 3a.
- Plugin settings = settings-MCP **Step 5 (plugin scoped schema registration, `catalogue.register_plugin`)**. A2b renders those catalogue entries; it does not persist values itself.
- New field kinds (A2a) extend the catalogue descriptor type of **Step 1 (story 1675-ccd4)**. Edits to `settings/model.rs` serialize after 1675 lands.

## Parallel model

- Stories live in the main checkout store (git-ignored, like settings-MCP 1675/1676).

- A1 (account manager, agent `wi-a1-accounts`) and A4 (Tasks data contract + fake provider, agent `wi-a4-contract`) are already running and own their stories (`plan_step` "Phase A1" / "Phase A4"). They publish their interfaces by mail.
- Interfaces received 2026-10-10: A1 = `accounts::Account {id,provider,host,kind,label,login,created_at}`, `AccountView` with secret `{status:set|unset|unavailable}`, tagged `AccountAction` list/add/test/remove via IPC `account_action` and `POST /accounts/action`, generic `ProviderDescriptor` + registration API, GitHub registered in core. A4 = Rust `work_items::{model,protocol}` (checkpoint `066a97093`), generated `src/types/work-items.gen.ts`; `WorkItemRef {providerId,accountId,id}`; 8 field kinds; `WorkItemQuery` / page `{items,nextCursor}`; `WorkItemDetail` with tagged comments (normal, review_thread, review_summary); actions create/edit_field/transition/comment/start_work; start_work returns a host intent, never runs a PTY.
- Boundaries set by the coordinator: the native secret-entry UI is A2 (A2c); plugin-manifest provider declarations and `httpFetch({account})` injection are A3 (A3c).
- Every Phase B stream starts **now** against the A4 contract and the A4 fake provider. It does not wait for A1-A3.
- Provider plugins (B8-B10) code against an injected `fetch` port, tested with recorded fixtures. Wiring to `host.httpFetch({account})` is the last task of each plugin story and needs A3c merged.
- A stream that needs an interface not yet merged codes against the mailed signature and rebases when it lands. A changed contract is mailed by A4 to all streams.

```ts
// Provider plugin seam (B8-B10): the API client never sees a token.
export type Fetch = (req: { url: string; method?: string; body?: unknown }) => Promise<{ status: number; headers: Record<string, string>; json: unknown }>;
export const hostFetch = (host: PluginHost, account: AccountRef): Fetch => (r) => host.httpFetch(r.url, { ...r, account });
```

## Security Considerations

- Tokens never in JS: A1 injects auth by account reference, checks the URL against the account host rule, drops the header on a cross-host redirect. D1 closes with A1 (device-flow token stays in Rust).
- Plugin HTML: none. Comments and descriptions render through `ContentRenderer` (DOMPurify). Bot comments with `<details>`, tables, images are kept; scripts and styles are stripped.
- `plugin_id` is caller-supplied (accepted risk, `AGENTS.md`). The account host rule is the real boundary, not the plugin id.
- Prompts built from item text (A3a start work) go through `sendCommand` in an "untrusted content" block, 12k cap, default delivery = draft (not submitted).
- Fixtures: recorded from real servers, scrubbed of tokens, emails and account ids before commit.

## Performance Considerations

- Lists: virtualized rows and board lanes, provider-side pagination, bounded LRU query cache keyed by provider+account+query+sort+cursor, generation check so a stale response never overwrites a newer one, debounced search (250 ms), detail/comments/checks loaded on open only.
- One request budget per account shared by core GitHub (`github.rs:219` breakers) and plugin calls (A1). Honor `Retry-After` and 429 in every plugin.
- Hidden Tasks tab pauses refresh. Detail refresh runs only while the item is open and visible.

## Open Questions

### Important (P2)
1. Detail live refresh: periodic refresh while the item is open and visible (recommended in the scope report, box unticked) or refresh button only. — Default: both; the timer is 60 s and stops when hidden.
2. Jira fixtures need a free Jira Cloud site created once by Boss (manual login), token stored in the vault. — Default: B9 codes against fixtures recorded there; if it does not exist, B9 stops at BLOCKED after the offline parts.
3. Headless Playwright on rb: the repository has no Playwright dependency. — Default: run it from rb's own install against the Vite dev server; never add it to `package.json` without asking.

### Unresolved
- `plugins/_shared/`: no shared source now. DEFERRED until a helper exists in all three plugins (rule of three); then one extraction story and a release-workflow watch.

## First shipped slice (decided)

- **Slice 1 = A1 + A2c + A3c + A4 + B1 + B2 + B4 + B9 (read path, transitions, board):** account registry with GitHub migration (fixes D1), Jira site accounts, Jira list / detail / status transition / board with drag and drop.
- Why Jira: GitHub already has a PR panel; Jira gives new value for WORK, and decision "board in the first slice" maps to Jira transitions. Gate: Open Question 2 (Jira Cloud site). If the site is absent when B9 needs it, slice 1 ships with B8 (GitHub) instead; the core steps do not change.
- Feature flag: none. The Tasks tab appears only when a provider plugin is installed, so an unfinished stream ships nothing visible.

## Steps

Size: S < 1 day, M 1-3 days, L > 3 days of one agent. Every story: contract in the worklog first, then code and tests, one targeted run through `build-slot.sh`, `fmt-changed.sh`. Browser checks: headless Playwright on rb against `pnpm dev` with the A4 fake provider; real plugin loading is desktop-only and goes to `to-test.md` for Boss after a restart. Shared registration files (`src/transport/commandTable.ts`, `src-tauri/src/lib.rs`, `mcp_http/mod.rs`, `plugin_docs.rs`, `types.ts` `PluginHost`) take small appended hunks; the coordinator serializes their merges.

### Phase A1: Account manager — story 1679-1419, owned by `wi-a1-accounts` (external)
### Phase A4: Tasks data contract + fake provider — story 1680-6b32, owned by `wi-a4-contract` (external)

### Phase A2a: New setting field kinds (S-M) — story 1682-1277
- **Owns:** `src/components/SettingsPanel/SettingFields.tsx` (new exports only), `src/components/SettingsPanel/fields/*` (new), their tests, `plugin_docs.rs` "Settings field kinds" section.
- **Implement:** `SettingAccount` (pick or add an account of a provider; uses A1 `accounts.list/add`), `SettingList` (editable rows), `SettingMultiSelect`, `SettingStatus` (read-only, tone), `SettingButton` (callback). Same props shape as `SettingInput`.
- **Depends on:** A1 interface (account list) for `SettingAccount` only; the other four start now. Descriptor typing follows 1675-ccd4.
- **RED:** `SettingList_remove_row_keeps_other_rows` — catches: index-keyed removal deletes the wrong row. `SettingAccount_never_renders_secret` — catches: token echoed into the DOM.
- **Validation:** `scripts/with-test-tmp.sh pnpm vitest run src/components/SettingsPanel/__tests__/fields`; Playwright screenshot on rb of the five controls in a fixture page.
```tsx
export const SettingList: Component<{ label: string; rows: Row[]; columns: Col[]; onChange(rows: Row[]): void }> = (p) => (
  <For each={p.rows}>{(row) => <ListRow row={row} columns={p.columns} onRemove={() => p.onChange(p.rows.filter((r) => r.id !== row.id))} />}</For>
);
```

### Phase A2b: Plugin Settings tabs (M) — story 1683-c04b
- **Owns:** `src/components/SettingsPanel/PluginSettingsTab.tsx` (new), `SettingsPanel.tsx` (Integrations group entries only), `settingsSearchIndex.ts` (plugin entries), tests.
- **Implement:** one tab per plugin with a catalogue schema, in the Integrations group, rendered from settings-MCP plugin catalogue entries with A2a controls. Reads and writes only through the settings service. Plugin unload removes the tab.
- **Depends on:** A2a, settings-MCP Step 5 (plugin registration; Step 1-2 first).
- **RED:** `unloaded_plugin_tab_disappears` — catches: stale tab writing to a dead plugin. `plugin_tab_is_searchable` — catches: search index built only from static tabs. `plugin_tab_write_goes_through_settings_service` — catches: a second settings path.
- **Validation:** targeted vitest; Playwright screenshot on rb (Integrations group with a fixture plugin tab).

### Phase A2c: Secret-entry native window with vault sink (M) — story 1696-c6d4
- **Owns:** `src-tauri/src/secrets/account_sink.rs` (new), the call from the Accounts UI, tests. Serialize with 1520-46b1 on `src-tauri/src/secrets/*`.
- **Implement:** the `secrets/forms` pattern (window-label check, nonce, no devtools) with a sink into the A1 add/rotate action; GitHub PAT entry moves here. Settings shows presence only.
- **Depends on:** A1 interface.
- **RED:** `account_secret_never_crosses_ipc_to_webview` — catches: token in an IPC/HTTP response. `secret_form_rejects_foreign_window_label` — catches: any webview writing the vault. `cancelled_form_writes_nothing` — catches: account with an empty secret.
- **Validation:** targeted nextest via `build-slot.sh`; `to-test.md` (native window).

### Phase A3a: Start work and URL paste (M) — rows 13, 14 — story 1684-6f88
- **Owns:** `src/hooks/git/startWork.ts` (new, extracted from `handleAutofixIssue`), `createWorktreeWorkflowCoordinator.ts` (call site), `src/components/CommandPalette/urlPaste.ts` (new), `PluginHost.startWork` + `registerUrlResolver` in `types.ts`/`pluginRegistry.ts`, `plugin_docs.rs` section.
- **Implement:** `host.startWork({repoPath, branch, baseRef?, prompt, agentType?, delivery:'draft'|'submit', link?})`: worktree + branch, the repo's run config (no hardcoded `claude`), prompt through `sendCommand`, draft by default; `handleAutofixIssue` becomes a caller. Palette: pasted URL → first plugin resolver that claims it → jump to the linked worktree or offer start work.
- **Depends on:** A3b link store for `link` (stub through the A3b mailed signature).
- **RED:** `start_work_uses_repo_run_config` — catches: hardcoded `claude`. `draft_delivery_does_not_submit` — catches: prompt sent with Enter. `prompt_not_passed_as_shell_argument` — catches: shell injection from item text. `pasted_url_jumps_to_existing_worktree` — catches: duplicate worktree for a linked item.
- **Validation:** targeted vitest (`createWorktreeWorkflowCoordinator`, `urlPaste`).
```ts
await sendCommand(sessionId, untrusted(prompt, 12_000), { submit: opts.delivery === "submit" });
```

### Phase A3b: Item link store, remotes, sidebar badge (M) — rows 15, H3, H5 — story 1685-4b12
- **Owns:** `src-tauri/src/workitem_links.rs` (new; `{repo → {worktree_branch → {provider, key, url, title, state}}}` in the config dir with the config file lock), Tauri commands + HTTP routes + `COMMAND_TABLE` entries, `PluginHost.getRemotes/linkItem/unlinkItem`, `src/components/Sidebar/BranchItemBadge.tsx` (new), `RepoSection.tsx` (one badge mount in the branch row).
- **Implement:** link CRUD; `getRemotes` exposes `get_remote_url`; badge shows key and state tone; hover fetches live state through the provider `get` (A4). Badge click opens the item; it never toggles the branch (sidebar-click rule).
- **Constraint:** sidebar branch-row change approved (H5) for the badge only; no other click behavior changes.
- **RED:** `link_survives_restart` — catches: in-memory link store. `removed_worktree_drops_link` — catches: orphan links. `badge_click_does_not_toggle_branch` — catches: propagation to `branchIconToggle`. `command_table_paths_all_hit_a_registered_route` stays green — catches: IPC/HTTP drift.
- **Validation:** `cargo nextest run -E 'test(workitem_links)'` via `build-slot.sh`; `pnpm vitest run src/__tests__/transport.test.ts`; Playwright screenshot on rb of a branch row with a badge. Rust change → `to-test.md`.

### Phase A3c: Manifest provider declarations and `httpFetch({account})` (M) — story 1697-4cdc
- **Owns:** `src-tauri/src/plugins.rs` (manifest `providers`), `src-tauri/src/plugin_http.rs` (account option), `HttpFetchOptions` in `src/plugins/types.ts`, `plugin_docs.rs` section.
- **Implement:** manifest providers register through the A1 registration API; Rust resolves the account, checks the URL against its host rule, injects the header, re-validates each redirect and drops the header on a cross-host hop; 401 recovery like `resolve_bearer`.
- **Depends on:** A1 interface.
- **RED:** `account_header_dropped_on_cross_host_redirect` — catches: token leak. `url_outside_account_host_rule_rejected` — catches: Jira token sent to another host. `plugin_cannot_use_undeclared_provider` — catches: consent bypass. `response_never_contains_authorization` — catches: token echoed to JS.
- **Validation:** targeted nextest via `build-slot.sh`; `to-test.md`.

### Phase B1: List view (L) — rows 2, 3 — story 1686-ce77
- **Owns:** `src/components/Tasks/list/*`, `src/stores/tasksQuery.ts` (new), tests, `plugin_docs.rs` "Tasks list" section.
- **Implement:** source bar, presets, filters from provider descriptors, debounced search, sort/group, virtualized rows, "load more" with provider cursors, bounded LRU cache, generation check, stale-on-error rows marked stale.
- **RED:** `stale_response_never_overwrites_newer_query` — catches: race on fast typing. `cache_is_partitioned_by_account` — catches: account A rows shown for account B. `list_renders_bounded_dom_for_10k_items` — catches: no virtualization. `next_page_appends_without_duplicates` — catches: cursor reuse.
- **Validation:** targeted vitest; Playwright on rb with the fake provider at 10k items (screenshot + DOM row count).
```ts
const gen = ++generation; const page = await provider.list(query, cursor); if (gen !== generation) return;
```

### Phase B2: Detail, comments, status change (L) — rows 4, 6 — story 1687-27a3
- **Owns:** `src/components/Tasks/detail/*`, tests, `plugin_docs.rs` "Tasks detail" section.
- **Implement:** header, fields with inline editors from the descriptor (A2a controls where they fit), markdown body and comments through `ContentRenderer`, all comment sources the provider declares (conversation, review threads, review summaries) with pagination, add comment, status action (close reason / transition / close-reopen as declared), optimistic update with rollback, refresh per Open Question 1.
- **RED:** `failed_status_change_rolls_back` — catches: optimistic state left after 4xx. `bot_comment_details_survive_sanitize` — catches: `<details>` stripped (fixture: a recorded CodeRabbit comment). `script_in_comment_is_removed` — catches: XSS. `refresh_keeps_open_details_state` — catches: re-render collapses sections.
- **Validation:** targeted vitest; Playwright screenshot on rb.

### Phase B3: Create item form (M) — row 5 — story 1688-518d
- **Owns:** `src/components/Tasks/create/*`, tests, `plugin_docs.rs` "Tasks create" section.
- **Implement:** form from the provider field schema (dynamic per item type, e.g. Jira `createmeta`), required-field validation, submit through the provider action, opens the created item.
- **Depends on:** A2a (field controls).
- **RED:** `type_change_reloads_required_fields` — catches: Jira required fields of the old type. `submit_blocked_until_required_filled` — catches: provider 400 on an empty field.
- **Validation:** targeted vitest; Playwright screenshot on rb.

### Phase B4: Board with drag and drop (L) — row 7 — story 1689-4e53
- **Owns:** `src/components/Tasks/board/*`, tests, `plugin_docs.rs` "Tasks board" section.
- **Implement:** columns from a provider-declared field, cards virtualized per lane, pointer-capture drag ported from `plugins/wiz-kanban/main.js:612-716` to Solid, keyboard move alternative, move = provider action (field change or workflow transition), pending state, rollback on rejection (pattern: Mission Control `dashboard-app.ts:585`). No story-file writes.
- **Constraint:** D&D approved for this board only; do not touch `dragDrop.ts` / `useFileDrop.ts`.
- **RED:** `rejected_move_returns_card_to_origin` — catches: card stuck in the target column. `pointer_cancel_cleans_ghost` — catches: ghost left after leaving the window. `illegal_transition_column_not_droppable` — catches: Jira move to a column with no transition.
- **Validation:** targeted vitest; Playwright on rb: drag a card with the fake provider, accept and reject paths, screenshots.

### Phase B5: Roadmap view (M) — row 8 — story 1690-5d98
- **Owns:** `src/components/Tasks/roadmap/*`, tests, `plugin_docs.rs` "Tasks roadmap" section.
- **Implement:** timeline of items with start/target date fields declared by the provider; month/quarter zoom; items without dates in a side list; dates in local time zone.
- **RED:** `item_without_dates_listed_not_dropped` — catches: silent item loss. `span_uses_local_timezone` — catches: off-by-one day at UTC boundaries.
- **Validation:** targeted vitest; Playwright screenshot on rb.

### Phase B6: PR Files tab review threads (L) — row 12 (+ row 11 thread reading) — story 1691-bdff
- **Owns:** `src-tauri/src/github_review_threads.rs` (new: read threads with resolved/open state, reply, resolve; GraphQL), routes + `COMMAND_TABLE`, `src/components/PrDiffTab/*` (thread UI on `DiffFileList`/`DiffViewer`), tests.
- **Implement:** line threads on the existing diff, replies, resolve/unresolve, a draft review pinned to the reviewed head SHA, submit through the existing `github.rs:2390` review post. No second diff engine.
- **RED:** `draft_review_rejected_when_head_moved` — catches: comments on lines of a stale SHA. `outdated_thread_shown_as_outdated` — catches: thread attached to a wrong line. `resolve_updates_thread_state` — catches: UI-only resolution.
- **Validation:** `cargo nextest run -E 'test(github_review_threads)'` via `build-slot.sh`; targeted vitest; Playwright screenshot on rb. Rust → `to-test.md`.

### Phase B7: GitHub PR actions: rerun, auto-merge, merge queue, request changes (M) — rows 9, 10, 11 — story 1692-6fbb
- **Owns:** `src-tauri/src/github_pr_actions.rs` (new), routes + `COMMAND_TABLE`, `src/utils/prMerge.ts`, `PrDetailContent.tsx` (action buttons only), tests.
- **Implement:** rerun failed jobs (Actions API); enable/disable auto-merge and enqueue/dequeue (GraphQL), queue state; review submit with `REQUEST_CHANGES` through the existing review post. Reuse readiness/checks/logs code.
- **RED:** `merge_on_queue_required_branch_enqueues` — catches: direct merge refused by branch protection. `rerun_targets_failed_jobs_only` — catches: full re-run. `request_changes_requires_body` — catches: GitHub 422 surfaced as silent success.
- **Validation:** `cargo nextest run -E 'test(github_pr_actions)'` via `build-slot.sh`; targeted vitest. Rust → `to-test.md`.

### Phase B8: `github-work` plugin (L) — rows 1-8, 13-15 for GitHub — story 1693-a816
- **Owns:** `plugins/github-work/*` (submodule), `plugins/registry.json` (one entry).
- **Implement:** provider for the A4 contract: issues/PRs with qualifier search and presets, pagination, detail + comment sources, create with duplicate picker, close with reason, Projects v2 board and roadmap fields, URL resolver, start work. Calls the core GitHub provider (decision A2) through `host.httpFetch({account})` and exposes B6/B7 actions; no own rate limiter beyond honoring core budget responses.
- **RED:** `qualifier_query_maps_to_search_api` — catches: `label:` dropped. `projects_v2_move_changes_field` — catches: move closes the issue. `pr_url_resolves_to_item` — catches: issue/PR number confusion. Fixtures recorded from github.com.
- **Validation:** `pnpm test:plugins -- plugins/github-work`; desktop load → `to-test.md`.

### Phase B9: `jira` plugin (L) — rows 1-7, 13-15 for Jira — story 1694-161f
- **Owns:** `plugins/jira/*` (submodule), `plugins/registry.json` (one entry).
- **Implement:** provider descriptor (Cloud: email + API token, Basic, REST v3; DC: PAT Bearer, REST v2; verify `GET /myself`), multi-site, JQL + key shortcut + `text ~` fallback on 400, pagination, presets, transitions, `createmeta` create, ADF ↔ Markdown, agile board columns, 429 + `Retry-After`, non-browser `User-Agent`.
- **RED:** `jql_400_falls_back_to_text_search` — catches: error instead of results. `board_move_uses_transition_id` — catches: status field write rejected by Jira. `adf_roundtrip_keeps_lists_and_code` — catches: lossy writer. `retry_after_honored` — catches: retry storm on 429.
- **Validation:** `pnpm test:plugins -- plugins/jira` on fixtures from the Jira Cloud site (Open Question 2); desktop load → `to-test.md`.

### Phase B10: `gitlab` plugin (L) — rows 1-7, 9-11, 13-15 for GitLab — story 1695-9f3a
- **Owns:** `plugins/gitlab/*` (submodule), `plugins/registry.json` (one entry).
- **Implement:** PAT per host, multi-account, REST v4, issues / MRs / todos, sort + label filter, pagination, close/reopen, create with labels and assignees, issue boards, pipelines (jobs, retry, trace), merge with method choice, approve, `detailed_merge_status` → readable blockers, URL forms (`/-/issues/N`, `!N`).
- **RED:** `self_hosted_port_is_part_of_host_identity` — catches: account bound to the wrong instance. `merge_method_sent_as_chosen` — catches: default method always used. `board_move_changes_label` — catches: move without list label swap.
- **Validation:** `pnpm test:plugins -- plugins/gitlab` on fixtures recorded from GitLab CE in a container on rb; desktop load → `to-test.md`.

## Dependency graph

| Step | Size | Starts | Hard dependency |
|---|---|---|---|
| A1 (1679-1419), A4 (1680-6b32) | — | running | — |
| A2a | S-M | now | A1 interface for `SettingAccount` |
| A2b | M | after A2a | settings-MCP Step 5 |
| A2c | M | now | A1 interface; serialize with 1520-46b1 |
| A3a | M | now | A3b signature (mailed) |
| A3c | M | now | A1 interface |
| A3b | M | now | A4 `get` for live badge state |
| B1, B2, B4, B5 | L/L/L/M | now | A4 contract + fake provider |
| B3 | M | after A2a | A2a, A4 |
| B6, B7 | L/M | now | none (core GitHub) |
| B8, B9, B10 | L | now (fixtures) | A3c merged for final `httpFetch({account})` wiring; B8 also B6/B7 for those actions |

## Acceptance Criteria

- [ ] All 15 scope rows reachable for each provider marked in the scope report.
- [ ] No token readable from JS, plugin data, logs or MCP output (A1 + every plugin RED).
- [ ] Tasks views render only core Solid controls; no plugin HTML or iframe.
- [ ] Each new field kind and view has its `plugin_dev_guide` section; drift tests green.
- [ ] New Tauri commands have HTTP routes and `COMMAND_TABLE` entries.
- [ ] Batch validation on rb by the coordinator: `cargo nextest run --lib`, `pnpm vitest run`, `cargo build --bin tuic-remote --no-default-features`, `pnpm test:plugins`.

## Checklist

- [ ] `docs/sync-matrix.md` targets updated per step (plugins, settings, frontend docs, FEATURES.md).
- [ ] The work-item providers analysis in `ideas/` marked `moved` and its row moved in `ideas/index.md` when phase B lands.
