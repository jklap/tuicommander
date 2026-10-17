# Settings startup workflows and current limits

This document describes the published contract, not a claim that every startup
acceptance check has passed. The native inventory gap ledger and UI parity
ledgers have different purposes: `inventoryComplete` reports the native ledger,
not live UI convergence or complete authentication onboarding.

## Discover before editing

1. A person enables the `settings` MCP tool in trusted Settings UI. A startup
   model cannot enable its own access. The removed `config` tool has no alias.
2. Discover `settings` through the current MCP tool surface, then call
   `{"action":"describe","store":"app"}`. Follow `nextCursor` until it is absent.
   The response includes Rust-generated field metadata and the action schema.
   Use the current schema, not a saved list of paths or action names.
3. Read the required fields with an explicit target. `machine` addresses the
   current machine; `instance` addresses instance configuration; `repository`
   requires a registered repository ID; Ego `profile` requires the configured
   profile ID. A remote connection is not implicitly selected by a local request.
4. Preserve each owner's read revision. A set/reset or per-store patch uses
   `expectedRevision`. Never combine different stores in one patch. Stable
   collection keys are not array indexes or arbitrary file paths.

Tauri `settings_action`, HTTP `POST /settings/action`, and MCP `settings` use the
same request DTOs and owner services. MCP mutation is loopback-only. HTTP keeps
its local-or-authenticated caller policy; native IPC keeps host-issued trust.
Do not treat these different caller policies as interchangeable credentials.

## GitHub

Describe `github_accounts` and `github_bindings`, and inspect the published
`perform` operation schema. Workflow requests use `target:{"kind":"machine"}`
and `store:"github_accounts"`, including repository binding operations.

| Operation | Required behavior |
|---|---|
| `begin_cloud_login` | Return an opaque `flowId`, `verificationUri`, `userCode`, `expiresIn` and `interval`. Only the person authenticates with GitHub. `additional` selects an additional named account. |
| `cloud_login_status` | Read the host-owned flow state; it does not exchange the device credential with GitHub. |
| `complete_cloud_login` | Attempt a poll when the owner's interval permits. Handle `pending`, `busy`, `expired`, `access_denied` and `success`; `pending` is not authenticated. A slow-down updates the returned interval. |
| `add_enterprise` | Verify the host and PAT through the account owner before storing it. A github.com PAT cannot bypass cloud device login. |
| `set_label`, `remove_account`, `set_secret`, `clear_secret` | Use the existing account owner. Cloud token replacement is not an Enterprise PAT workflow. Reads expose presence, never token values. |
| `bind_repo`, `unbind_repo` | Use a registered repository, canonical remote and the binding owner's `expectedRevision`. Do not substitute the account snapshot revision for the binding revision. |

Do not persist a device credential, poll a completed flow again, or infer success
from a browser closing. Unknown flows are `not_found`; expiry or disconnect can
invalidate an existing flow. A successful durable account action includes an
owner snapshot for readback. A credential source such as environment, OAuth or PAT
remains provenance, not writable account-token storage.

The read-only `github_accounts.auth.source` descriptor publishes the existing
`/auth/source` owner projection. Its values come from the owner source enum:
`env`, `oauth`, `pat` or `none`; this build does not resolve a `gh` CLI login.
`auth.secret` stays unpublished: provenance is not permission to reveal a token,
credential reference or the full owner authentication object.

**Current publication gap:** the general account collection has no published
list workflow; keyed leaf reads require an already known account ID. Do not
claim that an unaided schema-only client can enumerate every existing account.
Owner completion remains an acceptance gate.

## Ego

Read `app.ego_executable`, `app.ego_profile` and `app.ai_chat_workspace` through
the `app` store before using the Ego owner. The executable change is a trusted
execution-authority edit and requires the existing human confirmation policy.
Do not run a caller-selected candidate to probe it before acceptance.

Describe `ego`; use targeted owner reads/choice refresh for model, roots, network
and executable-path settings. Ordinary describe is cache-only. The separate
`{"action":"refresh","store":"ego"}` explicitly refreshes the configured
owner's provider catalogue. It is not a login action.

The owner invokes the configured Ego process with argument vectors in the
configured AI Chat workspace. It does not edit Ego files. Profile edits target
only the configured profile; machine reads and base-only provider auth actions
use the base document. `perform` for `store:"ego"` takes an `expectedRevision`
and one of `set_headers`, `clear_headers`, `set_credential_command` or
`clear_credential_command`. Inputs are write-only; presence is not a secret
replacement value. Read back after an edit, and report next-session effects.
Existing ACP conversations retain their launch/options; no PTY fallback replaces
an unavailable ACP or settings owner.

**Current publication gap:** interactive `ego auth login PROVIDER` is a human
workflow opened by `services/egoLogin.ts` in the AI Chat Settings UI. The
published `OwnerAction::Ego` accepts only `EgoAuthAction` configuration edits.
There is no typed Settings login-start/completion action. Ending the terminal
command does not prove authentication; the person refreshes provider state.
A schema-only Ego login remains blocked. Do not invent a login action, claim
completion, scrape credential files or launch an alternate executable. A typed
interactive-login contract needs a separate owner follow-up after feature freeze.

## Refusal and recovery

| Outcome | Consumer response |
|---|---|
| `confirmation_required` | Retain the exact validated edit and proposal ID. MCP asks the existing `ui confirm` with `proposalId`, then replays the exact edit with `approval`. HTTP/IPC can replay it with `approval` and `confirm:true` to ask the same dialog. |
| Refusal, expiry or disconnect | Write nothing. A proposal expires after 300 seconds and cannot be reused for a different connection, target, revision or content. Do not forge `confirmed:true`. |
| `conflict` | Read the owner again and reconcile intent. Do not silently move a dirty draft to the new CAS token. |
| Auth `pending`, `expired` or `access_denied` | Report the actual state. Respect the owner interval; only start a new login after the old flow is no longer usable and the person wants another attempt. |
| `persisted:true` with a failed apply/readback | The durable phase happened. Read back and follow `nextAction`; do not repeat the mutation as a retry. |
| Lost transport outcome | Treat durability as unknown. Read back before another write. |
| Missing binary/capability | Report unavailable and the required owner recovery. Do not use arbitrary commands or another transport as a fallback. |

Secrets are write-only throughout replies, errors, previews and events. A failed
vault read is unavailable, not proof that no credential exists. After durable
writes, `settings-changed` contains only `section`, process-local `generation`,
`persisted` and `applied`. The generation is an invalidation cursor, not an owner
revision. Reconnect requires fresh reads; dirty UI drafts remain local.

## Evidence boundaries

The source tests, replay fixtures and generated catalogue do not authenticate a
real account or prove live desktop/browser convergence. The Ego replay uses
captured owner responses; real interactive authentication remains the person's
responsibility. Step 10 must remain incomplete while the publication gaps above
or its refusal/expiry/conflict/apply-failure integration criteria are unverified.
See [Settings](../user-guide/settings.md), [HTTP API](../api/http-api.md), and
[configuration ownership](config.md) for the corresponding user and transport
surfaces.
