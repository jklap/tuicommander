# GitHub Integration

**Modules:** `src-tauri/src/github.rs`, `src-tauri/src/github_auth.rs`, `src-tauri/src/github_account.rs`, `src-tauri/src/github_poller.rs`, `src-tauri/src/improvement_scan.rs`

Integrates with GitHub via GraphQL API for PR status, CI checks, and batch queries. Supports OAuth Device Flow login as an alternative to gh CLI tokens, plus **multiple accounts** (additional github.com logins and GitHub Enterprise Server) with per-repo bindings.

## Multi-Account Model (`github_account.rs`)

The integration is **account-centric**: the primary key is a stable `GitHubAccountId`, not the host. This keeps github.com behaving exactly as before behind an "ambient default" account while enabling additional accounts.

- **`GitHubHost`** — canonical (lowercased, validated) host. `is_cloud()` → github.com; `graphql_url()` / `rest_base()` return `api.github.com` (+`/graphql`) for cloud and `https://{host}/api/graphql` / `https://{host}/api/v3` for GHE. `is_ambient_default()` routes the global-vs-per-account branch points.
- **Account kinds** — `GithubComOAuth` / `GithubComEnv` / `GithubComGhCli` (the ambient default, existing auth chain), additional named github.com accounts, and `GhePat` (GitHub Enterprise Server via pasted PAT).
- **Credential storage** — github.com keeps `Credential::GithubOauthToken` (`github/oauth-token`) unchanged; per-account PATs use `Credential::GithubToken(account_id)` → `github/account/{id}/token`.
- **Repo bindings** — `{repo_path → account_id, owner, repo, remote_name}` persisted per canonical repo root (worktrees resolve to the main root). `resolve_repo_account(repo_path)` returns `RepoResolution::{Bound | NeedsBind(candidates) | NeedsAccount | Unmonitored}` — binding-first, single-candidate auto-confirm, ambiguity surfaces all candidates (never a silent `origin` pick).
- **Per-account isolation (hybrid)** — github.com keeps the global breaker/viewer/rate/cooldown fields byte-for-byte; GHE accounts get isolated `ghe_state: DashMap<AccountId, GheAccountState>`. The poller groups repos by resolved account and runs one batch per account, so a fault on one never opens another's breaker. Cooldown keys: `owner/repo` (cloud, unchanged) vs `{account_id}:owner/repo` (GHE).
- **Limitation** — `fetch_ci_failure_logs` (gh-CLI-assisted) is disabled with a clear message for non-github.com accounts; all REST + GraphQL paths route through `github_rest_url(host, path)` / account-scoped tokens (no hardcoded `api.github.com` outside `GitHubHost` + tests).

### Multi-account commands

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_list_accounts` | `() -> Vec<GitHubAccount>` | Additional accounts beyond the ambient github.com default |
| `github_add_account` | `(host: String, pat: String) -> GitHubAccount` | Validate PAT against `{rest_base}/user`, store token + record (github.com rejected → device flow) |
| `github_remove_account` | `(id: String) -> ()` | Cascade-remove token + record + bindings + per-account caches |
| `github_bind_repo` | `(repo_path, account_id, remote_name) -> ()` | Persist a repo→account binding |
| `github_unbind_repo` | `(repo_path: String) -> ()` | Remove a repo binding |
| `github_list_bindings` | `() -> Vec<Binding>` | All persisted repo→account bindings |
| `github_resolve_repo` | `(repo_path: String) -> RepoResolutionDto` | `bound` / `needs-bind` / `needs-account` / `unmonitored` + candidates |

## Token Resolution

Priority order (first non-empty wins) for the ambient github.com account:

1. `GH_TOKEN` environment variable
2. `GITHUB_TOKEN` environment variable
3. OAuth keyring token (`github_auth.rs` — stored in OS keyring via `keyring` crate)
4. `gh_token` crate (reads `~/.config/gh/hosts.yml`)
5. `gh auth token` CLI subprocess

The active token source is tracked in `AppState.github_token_source` as a `TokenSource` enum (`Env`, `OAuth`, `GhCli`, `Pat`, `None`). `resolve_token_for_account(&GitHubAccount)` runs this exact chain for github.com and returns the vault PAT (`TokenSource::Pat`) for GHE accounts.

## Tauri Commands — Authentication (`github_auth.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_start_login` | `() -> DeviceCodeResponse` | Start OAuth Device Flow, returns user code |
| `github_poll_login` | `(device_code: String) -> PollResult` | Poll for token, saves to keyring on success |
| `github_logout` | `() -> ()` | Delete OAuth token from keyring, fall back to env/CLI |
| `github_auth_status` | `() -> AuthStatus` | Current auth status with login, avatar, source |
| `github_disconnect` | `() -> ()` | Disconnect GitHub — clear all tokens from keyring and env cache |
| `github_diagnostics` | `() -> Value` | Diagnostics: token sources, scopes, API connectivity |

## Tauri Commands — GitHub Data (`github.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_github_status` | `(path: String) -> GitHubStatus` | Remote presence, current branch, ahead/behind counts |
| `get_ci_checks` | `(path: String, pr_number: i64) -> Vec<Value>` | Detailed CI check list for one PR |
| `get_repo_pr_statuses` | `(path: String, include_merged: Option<bool>) -> Vec<BranchPrStatus>` | PR status for every branch of one repo (TTL-cached unless `include_merged`) |
| `approve_pr` | `(repo_path: String, pr_number: i64) -> ()` | Submit approving review via the REST reviews endpoint |
| `get_all_pr_statuses` | `(paths: Vec<String>, include_merged: bool) -> HashMap<String, Vec<BranchPrStatus>>` | Batch PR status across many repos in one GraphQL call |
| `get_pr_diff` | `(repo_path: String, pr_number: i64) -> String` | Get PR diff content; falls back to a local-clone `git diff` when GitHub rejects oversized diffs |
| `merge_pr_via_github` | `(repo_path: String, pr_number: i64, merge_method: String) -> String` | Merge PR via GitHub API |
| `fetch_ci_failure_logs` | `(repo_path: String, branch: String) -> String` | Fetch failure logs for the branch's latest head commit, for CI auto-heal |
| `run_improvement_scan` | `(repo_path: String, focus: ImprovementFocus) -> ImprovementScanResult` | Headless-slot one-shot AI scan for refactor/testing/perf proposals; emits `proposals-ready` |
| `create_issue_from_proposal` | `(repo_path: String, proposal: ImprovementProposal) -> CreatedIssue` | Explicit issue creation from a proposal; scan never creates issues automatically |

### Circuit breaker coverage

**Every** call out to GitHub goes through the account's circuit breaker:
GraphQL via `graphql_with_retry`, `gh api` writes via `run_gh_write`, and direct
REST via `send_rest_with_breaker` (close/reopen issue, merge PR, approve PR,
`fetch_github_json`, PR diff, PR refs). The availability breaker counts transport
errors and `5xx` responses, not deterministic `4xx` caller outcomes such as a
missing issue, merge conflict, validation failure, or permission denial. Rate
limits use their separate backoff: `429`, primary-limit `403` headers,
`retry-after`, or a secondary/abuse-limit message in an otherwise ambiguous
`403` body. Non-rate-limit bodies remain available to caller-specific error
formatting.

### Cached viewer login

`state.github_viewer_login` backs `author:@me` in the viewer-PR search and the
assignee/creator/mentioned issue filters. It is dropped by
`github::invalidate_viewer_login` on logout, disconnect and a successful device-flow
login — without that, switching accounts kept showing the previous account's PRs
and issues for the rest of the session. Named accounts cache their own login in
`ghe_state` and are deliberately untouched by that invalidation.

## Tauri Commands — Polling (`github_poller.rs`)

| Command | Signature | Description |
|---------|-----------|-------------|
| `github_start_polling` | `(paths: Vec<String>, issue_filter: String, pr_hide_drafts: bool) -> ()` | Start/reconfigure the background poller |
| `github_stop_polling` | `() -> ()` | Stop the poller task |
| `github_set_visibility` | `(visible: bool) -> ()` | Switch between the visible and hidden poll intervals |
| `github_poll_repo` | `(path: String) -> ()` | Request a debounced one-off poll of a single repo |
| `github_update_paths` | `(paths: Vec<String>) -> ()` | Replace the polled repo set |
| `github_set_issue_filter` | `(filter: String) -> ()` | Change the issue filter mode live |
| `github_set_pr_hide_drafts` | `(hide: bool) -> ()` | Toggle draft-PR filtering live |

## Data Types

### GitHubStatus

```rust
struct GitHubStatus {
    has_remote: bool,
    current_branch: String,
    ahead: i32,
    behind: i32,
}
```

PR and CI data are **not** on this type. They arrive through `BranchPrStatus`, which
the batch endpoints return per branch.

### BranchPrStatus (Batch Endpoint)

Full PR data for a single branch, returned by `get_repo_pr_statuses`:

```rust
struct BranchPrStatus {
    branch: String,
    number: i32,
    title: String,
    state: String,
    url: String,
    additions: i32,
    deletions: i32,
    checks: CheckSummary,        // passed/failed/pending/total
    author: String,
    commits: i32,
    mergeable: String,           // "MERGEABLE", "CONFLICTING", "UNKNOWN"
    merge_state_status: String,  // "CLEAN", "DIRTY", "BEHIND", etc.
    review_decision: String,     // "APPROVED", "CHANGES_REQUESTED", etc.
    viewer_did_approve: bool,    // Viewer's own latest review is APPROVED
    labels: Vec<PrLabel>,        // Labels with pre-computed colors
    is_draft: bool,
    base_ref_name: String,
    head_ref_oid: String,
    created_at: String,
    updated_at: String,
    merge_state_label: Option<StateLabel>,   // Pre-classified display label
    conflict_state: ConflictState,           // The single conflict verdict both surfaces render
    review_state_label: Option<StateLabel>,  // Pre-classified display label
    merge_commit_allowed: bool,  // Repo-level merge settings
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
}
```

### PrLabel

```rust
struct PrLabel {
    name: String,
    color: String,            // Hex color from GitHub
    text_color: String,       // Computed: black or white based on luminance
    background_color: String, // Computed: hex_to_rgba with alpha
}
```

### CheckSummary

```rust
struct CheckSummary {
    passed: u32,
    failed: u32,
    pending: u32,
    total: u32,
}
```

### StateLabel

```rust
struct StateLabel {
    label: String,     // Human-readable text (e.g., "Approved", "Behind")
    css_class: String, // CSS class for styling
}
```

## Utility Functions

### `parse_pr_node(v: &serde_json::Value) -> Option<BranchPrStatus>`

Parses one PR node out of the batched GraphQL response and enriches it with computed
fields (merge state classification, conflict verdict, review state classification,
label colors).

### `classify_merge_state(mergeable: Option<&str>, merge_state_status: Option<&str>) -> Option<StateLabel>`

Delegates the conflict question to `classify_conflict_state` first, then maps the
merge state to a display label:

| Condition | Label | CSS Class |
|-----------|-------|-----------|
| `ConflictState::Conflicting` | Conflicts | conflicting |
| `ConflictState::Checking` | *(no chip — GitHub is still recomputing)* | — |
| CLEAN | Ready to merge | clean |
| BEHIND | Behind base | behind |
| BLOCKED | Blocked | blocked |
| UNSTABLE | Unstable | blocked |
| DRAFT | Draft | behind |
| DIRTY | Conflicts | conflicting |
| UNKNOWN, HAS_HOOKS | *(no chip)* | — |

### `classify_review_state(review_decision: Option<&str>) -> Option<StateLabel>`

| review_decision | Label | CSS Class |
|-----------------|-------|-----------|
| APPROVED | Approved | approved |
| CHANGES_REQUESTED | Changes requested | changes-requested |
| REVIEW_REQUIRED | Review required | review-required |

### `hex_to_rgba(hex: &str, alpha: f64) -> String`

Converts a **bare** 6-char hex color (e.g. `"ff0000"`, no leading `#` — that is what
GitHub's label API returns) to an rgba string (e.g. `"rgba(255, 0, 0, 0.5)"`). Invalid
input parses as `(0, 0, 0)`.

### `is_light_color(hex: &str) -> bool`

Computes BT.601 luma (`(r*299 + g*587 + b*114) / 1000 > 128`) on the same bare hex to
decide whether a label needs dark text.

## Tauri Commands — Issues

| Command | Signature | Description |
|---------|-----------|-------------|
| `get_all_issues` | `(paths: Vec<String>, filter_mode: String) -> HashMap<String, Vec<GitHubIssue>>` | Fetch issues for multiple repos in one batched GraphQL call |
| `close_issue` | `(repo_path: String, issue_number: i64) -> ()` | Close an issue via the REST issues endpoint |
| `reopen_issue` | `(repo_path: String, issue_number: i64) -> ()` | Reopen a closed issue via the REST issues endpoint |

### GitHubIssue

```rust
struct GitHubIssue {
    number: i32,
    title: String,
    state: String,           // "OPEN", "CLOSED"
    url: String,
    created_at: String,
    updated_at: String,
    author: String,
    labels: Vec<PrLabel>,    // Reuses PrLabel with computed colors
    assignees: Vec<String>,
    milestone: Option<String>,
    comments_count: i32,
}
```

### Issue Filter Modes

The `filter_mode` parameter controls which issues are fetched. It is applied as a
GraphQL `filterBy` argument (`issues_filter_clause`), **not** a Search API qualifier:

| filter_mode | GraphQL `filterBy` | Description |
|-------------|--------------------|-------------|
| `assigned` | `{ assignee: "{viewer}" }` | Issues assigned to the authenticated user |
| `created` | `{ createdBy: "{viewer}" }` | Issues created by the authenticated user |
| `mentioned` | `{ mentioned: "{viewer}" }` | Issues mentioning the authenticated user |
| `all` | *(none)* | All open issues in the repo |
| `disabled` | *(issues sub-selection omitted)* | Issue fetching disabled — `get_all_batch_impl` drops the whole section |

### Issue Query Construction

`build_multi_repo_issues_query` builds one aliased `repository(owner:, name:)` block
per repo, each embedding an `issues(first: 30, states: [OPEN], orderBy: UPDATED_AT
DESC)` sub-selection. It uses `repository().issues()` rather than `search()` because it
costs fewer GraphQL points. Results are parsed via `parse_issue_node`, which extracts
labels with `hex_to_rgba` color computation (same opacity constant
`LABEL_BG_OPACITY = 0.7` as PRs).

## GraphQL Batching

`get_repo_pr_statuses` reuses the multi-repo query builder (`build_multi_repo_pr_query`)
and sends it through `graphql_with_retry` — one GraphQL call returns every branch with
PR data. `gh pr list` is used only as the parity oracle in tests.

**Polling budget:** `github_poller.rs` batches **all** polled repos into one GraphQL
call per tick, so the cost does not scale with repo count. Intervals: `BASE_INTERVAL`
60 s when visible, `HIDDEN_INTERVAL` 120 s when hidden, backing off to `MAX_INTERVAL`
300 s on failures or a critically low rate budget.

## PR Approval & Merge

### `approve_pr`

`POST {rest_base}/repos/{owner}/{repo}/pulls/{n}/reviews` with `{"event": "APPROVE"}`,
sent through `send_rest_with_breaker`. Used by the remote-only PR popover.

### PR Diff Fetching

PR diff reads use the GitHub REST diff representation first. If GitHub returns the oversized-diff `406 Not Acceptable` response, the backend fetches the PR refs into the local clone and returns a local `git diff base...head` unified diff instead, so AI Review can still run on PRs that exceed GitHub's rendered diff file cap.

### CI Auto-Heal (`fetch_ci_failure_logs`)

Lists workflow runs for the branch's latest head commit, inspects their jobs, and downloads logs for every completed failed job through the GitHub Actions jobs API. Job-level retrieval works while sibling jobs are still running, before the containing workflow has a final `failure` conclusion. Used by the CI auto-heal hook (`useCiHeal`) to inject failure context into agent terminals for automatic fix cycles (up to 3 delivered attempts per cycle).

**GitHub Actions only.** The aggregated PR check summary (which triggers `ci_failed`) also counts external CI — CircleCI, Codacy, etc. — but this fetcher reads only GitHub Actions logs. When the red checks are all external, it returns a clear error naming them (`… failing checks run on external CI (not supported): ci/circleci: lint-blades, on_pr …`) instead of the misleading "no jobs found". The auto-heal hook surfaces that message as a warn toast and does **not** consume an attempt (attempts increment only after a fix prompt is delivered). Provider is classified from the check's detail link (`is_github_actions_link`: GHA links contain `/actions/runs/`).

## Stale PR Filtering

When `include_merged` is true, `get_repo_pr_statuses` includes recently merged PRs. Stale merged PRs are filtered: if a branch has been recreated after a PR was merged (detected via branch creation timestamp vs PR merge timestamp), the old merged PR is excluded to prevent ghost badges.
