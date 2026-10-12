# Plan: One GitHub path in TUIC, over HTTP

**Created:** 2026-10-11 | **Status:** Draft | **Effort:** L | **Branch:** `docs/gh-http-plan` | **Baseline:** `6ad7c7e90` | **Story:** 1708-a7a5

## Summary

Boss decision (2026-10-11): every GitHub access in TUIC uses the existing HTTP/GraphQL layer (`github.rs`, `github_pr_api.rs`, account token from `github_auth.rs` / `resolve_repo_for_rest`). Production code spawns `gh` zero times. `token_from_gh_cli` and the `gh-token` crate are removed, not kept as an optional source.

## Architecture Context

- Token: `resolve_repo_for_rest` (`github.rs:511`) → `(account, token, owner, repo)`. Ambient github.com uses `state.github.token` (chain `resolve_all_candidates`, `github_auth.rs:641`); named and GHE accounts use their vault token.
- GraphQL: `graphql_with_retry` (`github.rs:598`) — breaker, rate limit, 401 candidate fallback (ambient only). Single page.
- REST: `send_rest_with_breaker` (`github.rs:1350`) — breaker + rate-limit classification; callers keep their own status wording. `RepoApi` (`github_pr_api.rs:19`) implements the `PrApi` trait (`graphql`, `rest` JSON only) over both; tests use `impl PrApi for Stub` (`github_pr_actions_tests.rs:24`).
- Client: `state.http_client` (`state.rs:908`, 30 s request timeout). `accounts/verify.rs:19` builds its own no-redirect client on purpose (identity verification).
- Gap: four features bypass all of the above and spawn `gh` with gh's own login (CI logs, failing checks, merged-PR proof) or with the account token in `GH_TOKEN` (writes). Two more sources spawn `gh auth token` inside the token chain.
- PR-proof callback: `tuic_git::worktree::*_with_pr(…, impl Fn(&Path,&str,&str)->bool)` is sync and gets no `AppState`; the 7 wrappers in `worktree.rs:17-290` pass a fn item. Callers run on `spawn_blocking` (`git.rs:46`) or Tauri sync commands.
- ADR: PR-proof adapter bridges sync→async with `tokio::runtime::Handle::block_on` on blocking threads — the tuic-git domain stays sync and pure — rejected: a second `reqwest::blocking` client (third client, second error/breaker path).

## Inventory (25 in-scope rows)

Production `gh` spawns and token sources (P), wire types (T), frontend (F), tests (X), docs (D). "Lacks" = what the HTTP layer needs first.

| # | Site | Does | Auth | Origin | Lacks |
|---|---|---|---|---|---|
| P1 | `github.rs:2348-2531` `gh_api_args`/`run_gh_api_json_with_input`/`run_gh_write`; callers `create_pr_impl` 2533, `create_issue_impl` 2557, `post_pr_review_impl` 2578 | `gh api --input -` POST | account token in `GH_TOKEN` / `GH_ENTERPRISE_TOKEN`+`GH_HOST` | `daa98591c` 2026-07-10 (story 102-48fd: "routed through existing gh runner") | nothing: `RepoApi::rest("POST")` exists |
| P2 | `github.rs:3316-3507` `fetch_ci_failure_logs_impl`: `gh run list` 3374, `gh run view` 3421, `gh api …/jobs/{id}/logs` 3490 | auto-heal CI logs | **gh's own login** (no token passed); GHE refused 3324-3338 | `70d503a7b` 2026-03-19, `27fc16d6f` 2026-07-13 | text body, 302 redirect to blob storage, REST job shape (`id` not `databaseId`), pagination, one overall deadline |
| P3 | `github.rs:3197-3290` `list_failing_checks_cli(_with)`: `gh pr list --limit 100` + `gh pr checks` | failing checks incl. external CI | gh's own login | `27fc16d6f` 2026-07-13 | commit `statusCheckRollup` query with pagination; `bucket` is a gh-computed field |
| P4 | `worktree.rs:63-101` `query_merged_github_pr`: `gh api graphql --paginate --slurp --hostname` | merged-PR proof for worktree/branch removal and sidebar | gh's own login per host | `9851336af` 2026-09-26 (#1022-8291), cache `86e96cd4e` 2026-10-07 | GraphQL pagination; `AppState` in a sync callback |
| P5 | `github_auth.rs:540-631` `token_from_gh_cli`, `token_from_cli_command`, `GH_CLI_TIMEOUT`, `bounded`, `GH_CLI_SPAWNS` | `gh auth token` source | — | `238ed84ec` 2026-02-19 | removed (Boss) |
| P6 | `github_auth.rs:655-661` `gh_token::get()`; `Cargo.toml:250` `gh-token = "0.1"` | reads `hosts.yml`, else spawns `gh auth token` | — | `5673fd097` 2026-02-18 | removed |
| T1 | `tuic-git/github_auth.rs:56` `TokenSource::GhCli`; `github_auth.rs:474` label; `github.rs:496`; `order_token_candidates` `github_auth.rs:678` | source enum + order | | | |
| T2 | `tuic-git/github_account.rs:143` `AccountKind::GithubComGhCli`; `accounts/store.rs:102,131` (`auth_source "gh_cli"`); `src/generated/settingsCatalogue.ts:3925,3938,9649` | account kind | | `dc617be7d` 2026-06-06 | |
| T3 | `lib.rs:1624-1628` boot comment; `Cargo.toml:190-191` serde_yaml comment cites gh-token | stale text | | | |
| F1 | `GitHubTab.tsx:43,60,92,516-519`, `GitHubTab.module.css:13` | "gh CLI" source/kind/hint | | | |
| F2 | `useCiHeal.ts:84` | comment says `gh run view --log-failed` | | | |
| F3 | `smartPromptsBuiltIn.ts:177,186` | prompts tell the **agent** to run `gh pr create`/`gh pr edit` | agent's shell | | not TUIC access — see Q1 |
| X1 | `github.rs:3879,3904` tests of `list_failing_checks_cli_with` (fake gh command) | | | | replaced in Step 4 |
| X2 | `github_auth.rs` tests on `GH_CLI_SPAWNS`/`token_from_cli_command` | | | | deleted with the feature (Step 6) |
| X3 | `github_compat_tests.rs:139-152` candidate order characterization | | | | updated (Step 6) |
| X4 | `github.rs:5343` `test_graphql_pr_query_matches_gh_cli` (ignored, gh as parity oracle); `5589` `…falls_through_to_cli`; `5540` `…filters_empty_from_gh_token_crate` | | | | oracle: Q2; the other two go with P5/P6 |
| X5 | `useCiHeal.hook.test.ts:210` error text `gh pr list timed out` | | | | reword (Step 4) |
| D1 | `docs/user-guide/github-integration.md:20-22,300-302` | "Option 2: gh CLI", troubleshooting via `gh auth status` | | | |
| D2 | `docs/user-guide/troubleshooting.md:49` | | | | |
| D3 | `docs/user-guide/settings.md:205` | token priority | | | |
| D4 | `docs/FEATURES.md:1222-1232` | token priority, logout fallback | | | |
| D5 | `docs/backend/github.md:5,7,48,83,292` | gh writes, gh proof, gh source | | | |
| D6 | `docs/guides/development-setup.md:10` | "gh for GitHub features" | | | gh stays a release tool only |
| D7 | `docs/api/tauri-commands.md` `fetch_ci_failure_logs` | github.com-only note | | | |
| D8 | `to-test.md` | Rust changes need a restart | | | one item per step |

Out of scope (not TUIC runtime GitHub access): `Makefile` `gh-rate`/release targets, `.github/workflows/*`, `scripts/hooks/pre-push`, `docs/release-checklist.md`, `src-tauri/fixtures/github-review/README.md` (recorded with `gh api`), `secrets/tests.rs:375-390` argv fixtures, `circleci.rs` `"gh"` VCS slug, `gh run watch` terminal captures, `output-parser.md:495`, `release-notes.json`. The `plugins` submodule has no GitHub access.

## HTTP layer divergences

- H1 Three token identities: writes use the account token; P2-P4 use whatever `gh` is logged in as; `ambient` may itself come from gh (P5/P6).
- H2 P2-P4 bypass the circuit breaker and rate-limit accounting (`with_account_breaker`).
- H3 GHE: P2 refuses GHE; P4 works only if `gh` is logged in to that host; writes work.
- H4 Header blocks are copied 10 times in `github.rs` (`grep -c '"User-Agent", "tuicommander"'` before tests) plus `RepoApi::rest`, `validate_token_impl` (`github_auth.rs:436`), `fetch_account_login` (`github_account.rs:402`).
- H5 `User-Agent` is `tuicommander` in `github.rs`/`github_pr_api.rs`, `TUICommander` in `github_auth.rs`, `github_account.rs`, `accounts/verify.rs`.
- H6 `X-GitHub-Api-Version: 2022-11-28` only in `validate_token_impl` and `fetch_account_login`.
- H7 Error text: `GitHub API failed ({status}): body` (`RepoApi`), `Failed to fetch {context} ({status}): msg` (`fetch_github_json`), per-site wording (merge, update branch), `GqlError` mapping, `gh api failed: stderr` (P1), `gh run list failed` (P2). No shared 401/403/404 wording exists; Step 1 keeps per-site wording (`send_rest_with_breaker` doc says callers read bodies on purpose).
- H8 Timeouts: gh subprocesses 5/20/30 s each, no sweep bound (`DEFERRED (2026-09-25)` at `github.rs:3411`); HTTP 30 s per request.
- H9 Clients: `state.http_client` everywhere except `accounts/verify.rs` (own client, no redirects — keep).
- H10 Debug log: `github_debug::log_api("CLI", …)` for gh sites.

## Security Considerations

- Job-log download follows a 302 to a storage host. The bearer token must not reach that host. reqwest 0.13.4 strips `Authorization` on a cross-host redirect; Step 4 pins this with a test.
- Token never enters a child process environment any more (P1 passed it in `GH_TOKEN`).
- Log bodies are untrusted text: keep `sanitize_ci_logs` before they reach an agent.

## Performance Considerations

- P4 runs on sidebar refresh per unmerged branch, cached 300 s (`PR_PROOF_TTL`); an HTTP call is cheaper than a `gh` process plus its own token lookup. Now counted by the breaker.
- Job logs can be very large. Read the stream and keep only the last `CI_LOG_MAX_CHARS` per job (failures are at the end, `truncate_ci_logs` already keeps the tail).
- Pagination loops have a page cap so a server bug cannot loop forever.

## Open Questions

### Important (P2)
1. Q1 Built-in smart prompts tell the agent to run `gh pr create`/`gh pr edit` (F3). That is the agent's tool in its own shell. Default: keep.
2. Q2 The ignored parity test `test_graphql_pr_query_matches_gh_cli` uses `gh` as a test oracle. Default: keep (test code only).
3. Q3 Users who only ran `gh auth login` lose github.com features until they sign in (Settings → GitHub, OAuth) or set `GH_TOKEN`/`GITHUB_TOKEN`. Default: no migration notice; the GitHub tab already shows "Not signed in" with a login button, and the docs say so.

### Dependencies
- Step 6 edits `accounts/store.rs` and `github_account.rs`: land after branch `chore/remove-legacy-github-accounts` (`d9b8fa839`) and coordinate with story 1679-1419.

## Steps

Each step adds its doc lines (sync-matrix "Git & Worktree") and one `to-test.md` item. Steps 2, 3 and 5 are independent after Step 1; Step 4 needs `graphql_pages` from Step 3; Step 6 is last.

### Step 1: one GitHub REST request builder + no-gh guard
- **Test:** `github.rs` tests — `rest_request_targets_the_account_host`: cloud → `https://api.github.com/repos/o/r/pulls`, GHE → `https://ghe.acme.com/api/v3/…`; bearer, UA, Accept, API version present. Catches: a GHE call sent to api.github.com, or one site without the API version. `production_code_spawns_no_gh` (`lib.rs` tests): scans `src-tauri/src/**` and `crates/*/src/**` up to the first `#[cfg(test)]\nmod tests`, counts `resolve_cli("gh")`, `Command::new("gh")`, `gh_token::`; asserts equal to an allowlist `{github.rs: 2, worktree.rs: 1, github_auth.rs: 2}`. Catches: a new site shells out to gh. Steps 2, 3, 5 and 6 lower their entry first (RED), then remove the code.
- **Implement:** builder used by the 10 `github.rs` sites, `RepoApi::rest`, `validate_token_impl`, `fetch_account_login`; `graphql_request` uses the same UA constant. Status handling stays per site.
- **Code:**
```rust
pub(crate) const GITHUB_USER_AGENT: &str = "TUICommander";
pub(crate) const GITHUB_JSON: &str = "application/vnd.github+json";
pub(crate) fn github_rest_request(client: &reqwest::Client, method: reqwest::Method,
    host: &GitHubHost, path: &str, token: &str, accept: &str) -> reqwest::RequestBuilder {
    client.request(method, github_rest_url(host, path))
        .bearer_auth(token)
        .header("User-Agent", GITHUB_USER_AGENT)
        .header("Accept", accept)
        .header("X-GitHub-Api-Version", "2022-11-28")
}
```
- **Validation:** `cargo nextest run -E 'test(rest_request_targets) | test(production_code_spawns_no_gh)'`

### Step 2: PR writes over HTTP (P1)
- **Test:** `github_pr_actions_tests.rs`-style `Stub` recording `(method, endpoint, body)`: `create_pr` → `POST pulls` with `{title,body,base,head,draft}`; `create_issue` → `POST issues`; `post_pr_review` → `POST pulls/7/reviews` with inline comments, `side` default `RIGHT`. `rest_error_keeps_status_and_message`: a `Stub` returning `GitHub API failed (422 …)` reaches the caller unchanged. Catches: write still spawns gh; wrong endpoint; review comments dropped.
- **Implement:** `*_impl` take `&impl PrApi` (wrappers resolve `RepoApi`); delete `gh_api_args`, `run_gh_api_json_with_input`, `run_gh_write`; allowlist `github.rs` 2 → 1.
- **Code:**
```rust
pub(crate) async fn create_pr_with(api: &impl PrApi, title: &str, body: &str, base: &str, head: &str, draft: bool) -> Result<CreatedPr, String> {
    parse_created_pr(api.rest("POST", "pulls", build_create_pr_body(title, body, base, head, draft)).await?)
}
```
- **Constraint:** IPC/HTTP signatures of `create_pr`, `create_issue`, `post_pr_review` do not change.

### Step 3: merged-PR proof over GraphQL with pagination (P4)
- **Test:** `graphql_pages` with a `Stub` returning two pages (`hasNextPage: true` then the merged PR on page 2) → proof true. Catches: only the first page is read. Page 2 returns `Err` → lookup `None`, nothing cached. Catches: a partial read cached as "not merged" for 300 s. Called from `spawn_blocking` inside a multi-thread runtime → no panic. Catches: `block_on` on a worker thread.
- **Implement:** `graphql_pages(api, query, vars, connection_ptr, MAX_PAGES)`; `query_merged_github_pr` uses `RepoApi::resolve` + `graphql_pages` and feeds the pages to the unchanged `merged_pr_proof_from_pages`. The 7 wrappers take `&Arc<AppState>` and pass a closure. Allowlist `worktree.rs` 1 → 0.
- **Code:**
```rust
pub(crate) fn pr_proof(state: Arc<AppState>, rt: Handle) -> impl Fn(&Path, &str, &str) -> bool {
    move |repo, branch, tip| rt.block_on(query_merged_github_pr(&state, repo, branch, tip)).unwrap_or(false)
}
```
- **Constraint:** every caller of the 7 wrappers runs off runtime worker threads; list them in the story worklog.

### Step 4: failing checks over GraphQL (P3)
- **Test:** fixture recorded from GitHub (`src-tauri/fixtures/github-ci/rollup-*.json`, `gh api graphql` as recorder): failing CheckRun, re-run duplicate, failing external StatusContext. Asserts names, links and `is_github_actions`. Catches: the PR is not among the first 100 open PRs → no checks (old `--limit 100`); a stale re-run listed as failing; external CI missing.
- **Depends on:** Step 3.
- **Implement:** commit query on `object(oid:)` → `statusCheckRollup.contexts` through `graphql_pages`; reuse `dedup_rollup_nodes` and `classify_check_node` (make `pub`). Delete `list_failing_checks_cli(_with)`, `pr_number_for_head`, gh-shaped `failing_checks_from_json`, tests X1; reword X5.
- **Code:**
```graphql
query($owner:String!,$name:String!,$oid:GitObjectID!,$endCursor:String){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{statusCheckRollup{contexts(first:100,after:$endCursor){nodes{__typename ...on CheckRun{name status conclusion startedAt detailsUrl} ...on StatusContext{context state targetUrl createdAt}} pageInfo{hasNextPage endCursor}}}}}}}
```

### Step 5: CI job logs over REST (P2)
- **Test:** (a) two mockito servers, `/logs` answers 302 to the second; the second matches `authorization` = `Missing`. Catches: token leaked to the blob host. (b) `Stub` with recorded `actions/runs?head_sha=` and `runs/{id}/jobs` fixtures; one run's jobs call fails → skipped, others still heal. Catches: one stale run aborts the heal. (c) GHE-bound repo is no longer refused. (d) a log over the cap keeps its last lines. Catches: the failure at the end is cut.
- **Implement:** `PrApi::rest_text` (default method `Err("unsupported")` so existing stubs compile); runs by `head_sha`, jobs `filter=latest&per_page=100`, paginated with a cap; `failed_jobs_from_run_json` reads REST `id`; async, no `spawn_blocking`, one `tokio::time::timeout` around the sweep — delete the `DEFERRED (2026-09-25)` note. Allowlist `github.rs` 1 → 0.
- **Code:**
```rust
let runs = api.rest("GET", &format!("actions/runs?head_sha={head_sha}&per_page=100"), Value::Null).await?;
let log = api.rest_text(&format!("actions/jobs/{job_id}/logs"), CI_LOG_MAX_CHARS).await?; // tail-bounded
```

### Step 6: remove the gh token sources (P5, P6, T1-T3, F1)
- **Test:** `order_token_candidates(gh_env, github_env, oauth)` keeps env → env → OAuth and dedups. Catches: a removed source still ranks. Accounts store: a persisted record with `auth_source: "gh_cli"` loads as the ambient OAuth account. Catches: registry load fails and every account vanishes (`GitHubAccountRegistry::load` logs and returns empty). `GitHubTab` renders `env` and `oauth` sources only.
- **Implement:** delete P5, P6 (`Cargo.toml` dep + comment), `TokenSource::GhCli`, `AccountKind::GithubComGhCli`, store arms, X2, X4 (5540, 5589); regenerate `settingsCatalogue.ts`; F1 union, label, hint; docs D1-D7. Allowlist `github_auth.rs` 2 → 0; the allowlist becomes empty.
- **Constraint:** no compatibility code: `github_account()` already maps an unknown `auth_source` to `GithubComOauth` (`accounts/store.rs:129-134`).
- **User-visible consequence:** a user signed in only through `gh auth login` sees "Not signed in" on the GitHub tab; PR status, CI rings, auto-heal and PR writes stop until they sign in with OAuth or set `GH_TOKEN`/`GITHUB_TOKEN`. macOS GUI launches do not see shell-profile variables, so OAuth is the practical path.
- **Depends on:** Steps 2-5 (allowlist), `chore/remove-legacy-github-accounts`.

## Acceptance Criteria

- [ ] `production_code_spawns_no_gh` passes with an empty allowlist.
- [ ] `gh-token` is not in `Cargo.toml`/`Cargo.lock` direct deps.
- [ ] CI logs, failing checks, merged-PR proof and PR writes work for a GHE-bound repo and record breaker outcomes.
- [ ] Docs D1-D7 describe one token chain: `GH_TOKEN` → `GITHUB_TOKEN` → OAuth keyring.
- [ ] Targeted tests of each step green.
