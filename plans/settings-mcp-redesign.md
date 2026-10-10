# Plan: Complete settings control through MCP

**Created:** 2026-10-10 | **Status:** Draft; two Boss choices open | **Effort:** XL
**Branch:** `docs/settings-mcp-plan` | **Baseline:** `2b47fa997`

## Summary

Replace native `config` with `settings`, without alias or compatibility. One Rust-owned catalogue describes every TUIC setting and drives Settings UI and MCP describe/get/set/patch. Reuse each store's domain services, credentials and runtime effects. Full coverage and immediate UI refresh are release requirements.

Authority: “Boss decision” in `~/Gits/personal/orchestrator/reports/tuicommander/2026-10-10-settings-mcp.md`; it supersedes that report's old incremental recommendation. This task changes only this plan; implementation awaits the two choices below.

## Architecture Context / research

- Handwritten MCP schema/handler disagree on save: `src-tauri/src/mcp_http/mcp_transport_catalogue.rs:481`, `mcp_transport_ancillary.rs:507`.
- AppConfig/file families: `src-tauri/src/config.rs:804`, `:1819`. Preserve delta/file locking and effects (`:2515`, `:2550`); current patch misses omitted fields (`:2590`, `:2664`).
- GitHub services own host/token/bindings/cache rules (`src-tauri/src/github_account.rs:338`, `:459`, `:495`); cloud device login: `github_auth.rs:149`.
- Ego owns provider/model/perimeter settings: `src-tauri/src/ego_cli.rs:561`, `:588`, `ego_cli/perimeter.rs:20`; invoke configured binary, no ego-file edits.
- Upstream validation/auth/reconnect: `src-tauri/src/mcp_upstream_config.rs:179`, `:485`. Agent caches are machine-scoped: `src/stores/agentConfigs.ts:23`.
- UI hydration: `src/stores/settings.ts:489`; all-client confirm: `mcp_transport_ancillary.rs:2020`. Neither is a general settings invalidation mechanism.
- Schemars derive exists (`src-tauri/Cargo.toml:188`). Search extraction excludes dynamic/repo controls (`src/components/SettingsPanel/settingsSearchIndex.ts:1`). Plugin-private config exists (`examples/plugins/telegram-notifier/main.js:19`); Telegram has a setup service (`src/components/SettingsPanel/tabs/TelegramTab.tsx:28`).
- Source inspection only, no runtime/keychain/provider/tests. Existing seams suffice; no new framework/external research required.

## Catalogue design

Generate structural schema with existing schemars from Rust domain/persisted types and action DTOs. Required schema extensions beside fields provide semantics; helpers compose one catalogue and generate checked TypeScript UI descriptors. Fail missing metadata. Reuse service enum/range constants; domain validation remains authoritative.

ADR: Rust types plus mandatory annotations own definitions — cover unset fields without duplicating types — reject a handwritten registry or JSX schema. Avoid a new proc-macro framework.

Each leaf: stable ID; area/group/order; label/help i18n; JSON-pointer template; type/nullability/default/reset; ranges/enums/dynamic resolver; effect; apply mode (`live`, `next_session`, `service_restart`, `app_restart`) and explanation; store/owner; operations/availability; sensitivity (`public`, `private`, `secret`, `derived_secret`); confirmation class. Unavailable dynamic choices differ from empty. No secret defaults/examples.

```json
{"id":"app.ego_executable","path":"/ego_executable","area":"ego",
 "type":"string","store":"app","owner":"machine","sensitivity":"private",
 "apply":"next_session","effect":"Executable for new ACP connections",
 "confirmationClass":"execution_authority","operations":["get","set","reset"]}
```

Mandatory inventory before cutover:

| Family | Authority / retained rules |
|---|---|
| App/services | Delta locking, vault, reflow, relay, rebind, tool-list change |
| GitHub | Cloud OAuth/enterprise PAT, registry, bindings, provenance, cache/pollers |
| Ego | App executable/profile/workspace; ego-owned model/provider/auth/perimeter; argument vectors; ACP distinct from PTY |
| Agents/run configs | IDs/env/launch/hooks, machine ownership; save never executes |
| Repositories | Registration/settings/defaults/local overrides/bindings, stored/effective/source; registered identity; no git/worktree deletion |
| Upstream MCP | Transports/auth/headers/filters/project enablement; uniqueness/self-reference/auth/reconnect |
| Notifications/voice | Config bounds/devices/assets/hotkeys/runtime effects; unavailable builds remain described |
| Prompts | Full text/label/pinned, stable-ID CRUD; no execution |
| Plugins | Enablement and every private setting via scoped schema/service; migrate existing plugins/credentials/callbacks |
| Other config | Keybindings/UI prefs/layout/remote connections/tunnels/Telegram and additional inventory families |

Exclude runtime/history (PTY output, notes, audits), with reviewed reasons on persisted non-setting fields. Catalogue-only settings render in expert “All settings”. Login/download/rotation are typed actions. Optional fields remain described; collections use stable IDs and key/value schemas, never indexes/arbitrary files.

Parity uses independent Rust schema traversal and UI source/control extraction. Require metadata for every configurable leaf and catalogue IDs for every control, including raw inputs/modals/repo/expert/platform/dynamic templates. Compare field-ID sets **both directions**; classify non-setting controls explicitly. Mount dynamic rows to check actual bindings. UI labels/help/options/search consume descriptors; custom layout remains. Plugins obey runtime parity too. No opaque JSON or blanket exclusions.

## Shared MCP/IPC/HTTP contract

`settings` uses a tagged Rust request enum with `deny_unknown_fields`; generate action schemas. Explicit target identifies instance/machine/repo/profile/plugin. Default is current instance, never an inferred remote owner. Owner services route configuration; no local MCP authority is forwarded into a forbidden remote network write.

```json
{"action":"describe","area":"github"}
{"action":"get","store":"app","paths":["/ego_executable","/ego_profile"]}
{"action":"set","store":"app","path":"/ego_profile","value":"coordinator","expectedRevision":"r1"}
{"action":"patch","store":"app","expectedRevision":"r2","operations":[
 {"op":"set","path":"/ego_executable","value":"/opt/ego/bin/ego"},
 {"op":"set","path":"/ego_profile","value":"coordinator"}]}
```

- **Describe:** area/store/ID filters, bounded cursor pages, schema version/actions/choices/availability; no implicit network refresh.
- **Get:** authoritative snapshot/revision and stored/effective/source. Secrets: `{status:"set"}`/`{status:"unset"}`; vault errors are unavailable, not unset. Revisions expose no secret material.
- **Set/patch:** set = one operation. Typed set/reset and stable-ID collection create/remove; explicit secret set/clear. Schema-resolve pointers, decode once; reject unknown/empty/traversal/prototype paths, invalid IDs/types/ranges/enums/owners. Redaction objects cannot replace secrets.
- Require expectedRevision; compare latest state under owner lock, including cross-process writes. Preserve unrelated fields; stale snapshots conflict/reread. Prevalidate whole per-store batches, atomic file commit. Reject multi-store patches. CLI/vault workflows report committed phases/recovery, never promise global atomicity.
- **Perform:** typed catalogue actions for cloud login begin/status/complete, enterprise account add/remove, bindings, ego login, credential rotation. Service DTOs, no arbitrary commands/assignments.
- **Reply:** revision/changed IDs/persisted/applied/effects/restart. No event on failed persistence. Persisted-but-failed effects return `persisted:true` and recovery/status action, not blind replay.
- **Error:** `{code,store,path,expected,allowed,retryable,nextAction,revision?,persisted?}`. Codes: unknown_path, invalid_type, out_of_range, conflict, not_found, unavailable, auth_required, confirmation_required/denied, forbidden_origin, secret_unreadable, store_failed, apply_failed. Sanitize secret input/subprocess/provider errors.

Tauri `settings_action` and `POST /settings/action` share service/schema/guards; add COMMAND_TABLE and regenerated route snapshot/parity tests. Direct/collapsed MCP paths agree. Remove config registry/dispatch/prompt actions/docs/default entries/tests; prompts remain through settings. Never silently transfer config enablement to settings.

## Open Boss choices

Select one in each pair before security implementation. Neither recommendation is assumed approved.

**Secrets**

- [ ] **Write-only (recommended):** presence on all reads, owner-service set/clear; no reveal action. Redact replies/errors/logs/events/audits/confirmation previews/artifacts.
- [ ] **Readable:** ordinary get remains redacted; explicit `get` with `revealSecrets:true` retrieves requested recoverable secrets through the same origin gate. No broadcast/log/preview or frontend persistence. Derived hashes are marked nonrecoverable/read-only; password replacement hashes through its service. External/ego retrieval needs an owner-supported API before claiming full readable coverage, never credential-file scraping.

**Settings exposing TUIC or changing execution authority**

- [ ] **Boss confirmation (recommended):** remote listener/bind/enable, auth/password/session credentials/bypass, TLS/certificates/keys, relay URL/token and trusted ego executable, including weakened protection. Validate a pending patch. `ui action=confirm` takes its opaque proposal ID and displays a backend-built redacted diff via the existing all-client mechanism. Approval consumes exactly that connection/target/revision/content-bound proposal once. Refusal, 300s expiry or disconnect writes nothing; changed revision requires a new proposal. Never trust model-supplied `confirmed:true`; never hold locks while waiting. Confirmation policy is host-issued, not writable by the model.
- [ ] **No confirmation:** identical validation/revision/commit pipeline commits immediately, including exposure/executable changes. Document the resulting local-caller authority; keep origin checks and redaction.

## Security Considerations

- Preserve loopback-only **MCP** mutations/reveals and native IPC OS-user trust. Existing HTTP Settings allows local-or-authenticated callers (`src-tauri/src/mcp_http/config_routes.rs:54`); retain that UI/API access policy, including the new shared endpoint. Remote MCP cannot gain write authority by peer registration. This is not a host-wide ban on authenticated HTTP writes.
- Current handler has address and no actor (`mcp_transport_ancillary.rs:507`). Add server-issued connection context for proposal correlation/audit, not a new agent allowlist. Enabled headerless local clients retain ordinary writes. Strong caller identity is outside scope.
- Settings stays disabled by default; trusted UI onboarding enables it once. A startup model cannot enable its own access. Disabled checks cover direct/meta/reveal paths. HTTP/IPC retain their existing authenticated host guards; all writes share validation, secret policy, confirmation and events. An authenticated API user already has broader authority than a remote MCP connection; do not misrepresent loopback as a complete host authorization boundary. Reveal remains local-only.
- Audit target/connection/changed IDs/outcome without values. Bound retention/proposals. Full local authority and optional secret readability retain prompt-injection risk; do not claim a new process-identity boundary.

## Startup flow: GitHub and ego

1. Boss enables settings in trusted UI. Model describes GitHub and reads auth/accounts/provenance/preferences/bindings.
2. Cloud: perform device-login begin; return URL/code/flow ID. Boss authenticates; status/complete respects interval/expiry. Enterprise: typed host/PAT verifies identity before storage. Cloud PAT cannot bypass device flow (`github_account.rs:459`).
3. Patch preferences, bind registered repos with revisions, reread effective/auth state. UI refreshes. Env/gh credentials show provenance, not fabricated writable storage.
4. Describe/read ego launch/owner settings. Set trusted absolute executable through chosen confirmation policy. No execution of caller-selected probes before authority acceptance.
5. Query configured ego's model/provider/perimeter definitions. Existing interactive ego-login lets Boss handle browser/device/API-key prompts. Refresh explicitly; write model/profile/perimeter through owner services, read back; preserve workspace/profile/ACP semantics.
6. Report next-session effects; existing ACP conversations retain launch/options. Missing binary, owner capability or expired login returns recovery; no alternate executable/PTY fallback.

## Live refresh

After durable commit dual-emit desktop and AppEvent/SSE `settings-changed`: `{target,store,revision,changedIds,originRequestId,effects}`. No values/secret hashes/credential-bearing keys. All writers converge on post-commit publisher (`src-tauri/src/state.rs:165`, `mcp_http/sse_routes.rs:302`). Partial external commits report actual phases; failed persistence emits nothing.

Subscribe once/window; reload affected owner/store via non-saving read/apply, never migration/bootstrap hydration. Update base/render state together. Preserve dirty deltas, merge untouched fields, flag same-field conflicts and cancel stale debounce writes. Own events acknowledge without saving; deduplicate revisions, reject out-of-order reads, dispose listeners. SSE reconnect refetches. Remote changes never save/reload local state.

## Performance Considerations

- Cached/generated catalogue, bounded filtered reads; no implicit network refresh/repo scans.
- One event/batch, coalesced targeted reloads; no polling/remount/terminal hot-path change.
- Bound payloads/operations/proposals/audit; disk/CLI off async executor; no locks across auth/confirmation.

## Stories and parallel execution

Coordinator assigns IDs with `plan`/`plan_step`; unrelated stories untouched. Sizes relative. New `settings/` paths below are proposed under `src-tauri/src/`. Independent workers own adapter files; coordinator serializes shared registrations/type annotations. Each story records consumer contract, named RED and targeted GREEN evidence.

| Step, size, dependencies | Owned files; acceptance | RED test → named bug; code pattern |
|---|---|---|
| 1 Catalogue (M, foundation) | `settings/{catalogue,model}.rs`, Rust annotations, `src/generated/settingsCatalogue.ts`; all inventory leaves/metadata or reviewed non-setting exclusions | `unannotated_optional_field_fails` → missing unset fields; `missing_sensitivity_fails` → unsafe defaults. `catalogue.require_complete_metadata()?` |
| 2 Transactions (M, after 1) | `settings/service.rs`, AppConfig seam; typed paths/reset/collections, locked revisions, apply errors | `patch_typo_rejected`, `stale_revision_rejected`, `invalid_batch_writes_nothing` → silent typo, lost update, partial write. `service.patch(target, store, revision, ops).await?` |
| 3 Accounts/ego batch (after 2; parallel 4/5) | `settings/{accounts,ego}.rs`, owner services; complete provider/auth/perimeter/readback; installed ego fixtures, capability gaps block completion | `cloud_pat_cannot_bypass_login`, `ego_uses_trusted_binary_and_profile`, `account_removal_cleans_bindings` → bypassed owner rules. `github.perform(GithubAction::BeginCloudLogin).await?` |
| 4 Domain-store batch (after 2; parallel 3/5) | `settings/stores.rs`, existing services; every remaining family/dynamic record/credential/effect, no raw-file bypass | Parametrized `adapter_preserves_other_fields`, `upstream_edit_cleans_auth`, `remote_write_cannot_touch_local` → clobber/stale vault/wrong owner. `registry.register(StoreId::Upstreams, adapter)` |
| 5 Plugins (M, after 2; parallel 3/4) | Scoped registration/services, `src/plugins/types.ts`, migrations; all existing configurable plugins, secrets/callbacks/unload, no opaque JSON | `plugin_without_schema_rejected`, `plugin_host_path_rejected` → invisible settings/authority escape. `catalogue.register_plugin(id, schema, scoped_adapter)?` |
| 6 Policy (M, after 2 + Boss ticks) | `settings/policy.rs`, confirm integration; selected branches across transports | `remote_patch_denied`, `secret_never_in_errors_events`, `approval_not_replayable` → origin/leak/replay; branch tests `write_only_reveal_denied`/`readable_reveal_scoped`, `expired_confirm_no_write`/`no_confirm_exposure_writes`. `approval.consume_for(&patch)?` |
| 7 Cutover (M, after 3–6) | MCP registry/dispatch, lib/router, COMMAND_TABLE/snapshot; same schemas/routes, config removed, prompts retained | `published_request_executes`, `disabled_meta_call_denied`, `old_config_unknown` → envelope drift/guard bypass/compatibility. `request.execute(&service, context).await` |
| 8 Refresh (M, after 3–7) | Publisher/AppEvent/SSE, frontend coordinator/non-saving apply; desktop/browser/remote, own/out-of-order/failure safe | `mcp_write_refreshes_ui_without_save`, `dirty_edit_survives_patch`, `sse_reconnect_refetches` → stale UI/feedback clobber/lost event. `reloadCoordinator.invalidate(event)` |
| 9 UI/parity (M, starts after 1, integrate after 3–8) | Settings controls/tabs/search, inspector/tests; global/repo/plugin/platform/expert parity, STYLE_GUIDE and screenshots | `raw_dynamic_control_missing_id_fails`, `catalogue_without_ui_fails`, `ui_options_match_schema` → both drift directions/invalid options. `<SettingField descriptor={catalogue.field(id)} />` |
| 10 Startup/docs (S, after 7–9) | Integration fixtures/tests; sync-matrix backend/API/Settings/GitHub/agents/remote/frontend/plugin docs, FEATURES/SPEC; both startup flows, complete inventory/policy docs | `startup_uses_only_published_schema` → unusable startup; refusal/auth expiry/conflict/apply failure cases. `assert_startup_flow(&host, published_schema()).await` |

Rows 3/4 expand into the following **separate stories**, each with its own files, contract and RED/GREEN evidence. They may run in separate worktrees after step 2; no worker edits another adapter or shared registration. Tests above apply where relevant, plus these domain regressions:

| Story / size | Own adapter; RED → bug; implementation pattern |
|---|---|
| 3a GitHub / M | `settings/accounts.rs`; `account_remove_cleans_cache_and_bindings` → dangling references; `github.remove_account(id)` |
| 3b Ego / M | `settings/ego.rs`; `ego_profile_edit_cannot_target_other_profile` → wrong profile; `ego.set_model(context, model)` |
| 4a Notifications / S | `settings/notifications.rs`; `notification_volume_out_of_range_rejected` → UI-only validation; `notifications.patch(edit)` |
| 4b Voice / M | `settings/voice.rs`; `voice_change_invalidates_cached_runtime` → old voice after save; `voice.apply(edit)` |
| 4c Agents / M | `settings/agents.rs`; `run_config_patch_preserves_siblings` → replaced launch collection; `agents.patch(id, edit)` |
| 4d Repositories / M | `settings/repositories.rs`; `repo_edit_cannot_escape_registered_owner` → arbitrary repo file write; `repos.patch(owner, edit)` |
| 4e Upstreams / M | `settings/upstreams.rs`; `upstream_path_edit_preserves_auth_cleanup` → stale auth/vault mismatch; `upstreams.save(edit)` |
| 4f Remote/tunnels / M | `settings/remote.rs`; `remote_edit_preserves_credential_reference` → orphaned connection secret; `remote.upsert(edit)` |
| 4g Keybindings/preferences/layout / S | `settings/preferences.rs`; `preference_patch_cannot_clobber_other_keys` → whole-object loss; `preferences.patch(store, edit)` |
| 4h Telegram / S | `settings/telegram.rs`; `telegram_settings_write_cannot_start_second_owner` → duplicate bot poller; `telegram.setup(edit)` |

Inventory additions become sized owner-specific stories before cutover; they cannot be hidden in a generic adapter or declared complete through AppConfig coverage alone.

## Validation and delivery

Agents use narrow changed-behavior filters through test-temp wrapper, mbx/build-slot and managed background rules. Reproducible fixes get a targeted pre-fix RED; one final targeted GREEN per story. Coordinator owns critic, batch suites/headless build and per-change Rust mutation on rb. Security/concurrency risk is high for implementation; plan-only change is low risk.

Use isolated named config instances/throwaway repos. Boss performs real authentication; never probe production token stores. UI visuals require screenshots; Rust changes need Boss's manual restart and `to-test.md` checks, not a hot-reload claim.

This plan: static citation/requirement/dependency/diff checks only. Open decisions are exactly the two pairs above. Missing owner capabilities/plugin contracts are acceptance obligations, not permission to omit configuration.
