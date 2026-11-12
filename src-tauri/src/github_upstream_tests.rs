//! A fork's PRs are opened against its upstream repository. These tests pin how
//! the poller finds them, which ones it keeps, and where actions on them go.

use super::*;

fn pr_node(number: i64, branch: &str, base: &str, head: &str) -> serde_json::Value {
    serde_json::json!({
        "number": number, "title": format!("PR {number}"), "state": "OPEN",
        "url": format!("https://github.com/{base}/pull/{number}"),
        "headRefName": branch, "headRefOid": "abc", "baseRefName": "main",
        "baseRepository": {"nameWithOwner": base},
        "headRepository": {"nameWithOwner": head},
        "reviewThreads": {"nodes": [], "pageInfo": {"hasNextPage": false}},
    })
}

fn fork_repos() -> Vec<(String, String, String)> {
    vec![("/work".into(), "me".into(), "hello".into())]
}

fn upstream_repos() -> Vec<(String, String, String)> {
    vec![("/work".into(), "org".into(), "hello".into())]
}

async fn adopt(
    state: &AppState,
    response: serde_json::Value,
    viewer: &str,
    pr_results: &mut std::collections::HashMap<String, Vec<BranchPrStatus>>,
) {
    adopt_upstream_prs(
        state,
        &github_com_account(state),
        &fork_repos(),
        &upstream_repos(),
        &[("u0".into(), "/work".into())],
        false,
        viewer,
        &response,
        pr_results,
    )
    .await;
}

fn seeded_results() -> std::collections::HashMap<String, Vec<BranchPrStatus>> {
    let own = parse_pr_node(&pr_node(1, "own-branch", "me/hello", "me/hello")).unwrap();
    std::iter::once(("/work".to_string(), vec![own])).collect()
}

// Catches: PRs opened from the fork against its upstream never reaching the sidebar.
#[tokio::test]
async fn upstream_pr_from_the_fork_is_added_to_the_checkout() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [
        pr_node(42, "feature", "org/hello", "me/hello"),
    ]}}}});
    adopt(&state, response, "", &mut results).await;

    let prs = &results["/work"];
    assert_eq!(prs.len(), 2);
    let added = prs.iter().find(|p| p.number == 42).unwrap();
    assert_eq!(added.branch, "feature");
    assert_eq!(added.base_repo, "org/hello");
    // The snapshot REST actions read must carry it too.
    assert!(
        state
            .git_cache
            .github_status
            .get("/work")
            .unwrap()
            .iter()
            .any(|p| p.number == 42)
    );
}

// Catches: other contributors' upstream PRs showing up (or attaching to a local branch
// that merely shares a name) — the sidebar is for PRs from our fork only.
#[tokio::test]
async fn upstream_prs_from_other_forks_are_dropped() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    // Head repository deleted: not provably ours.
    let mut gone = pr_node(9, "gone", "org/hello", "me/hello");
    gone["headRepository"] = serde_json::Value::Null;
    let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [
        pr_node(7, "feature", "org/hello", "stranger/hello"),
        pr_node(8, "main", "org/hello", "org/hello"),
        gone,
    ]}}}});
    adopt(&state, response, "", &mut results).await;
    assert_eq!(results["/work"].len(), 1, "only the fork's own PR remains");
}

// Catches: owner-case differences ("Me/Hello" vs "me/hello") hiding our own PR.
#[tokio::test]
async fn head_repository_match_ignores_case() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [
        pr_node(42, "feature", "org/hello", "Me/Hello"),
    ]}}}});
    adopt(&state, response, "", &mut results).await;
    assert_eq!(results["/work"].len(), 2);
}

// Catches: the viewer search (the path that finds PRs beyond the newest N) skipping the
// fork filter, or being ignored for upstream repositories.
#[tokio::test]
async fn viewer_search_finds_old_upstream_prs_but_applies_the_same_filter() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    let mut mine = pr_node(11, "old-feature", "org/hello", "me/hello");
    mine["repository"] = serde_json::json!({"nameWithOwner": "org/hello"});
    let mut theirs = pr_node(12, "x", "org/hello", "stranger/hello");
    theirs["repository"] = serde_json::json!({"nameWithOwner": "org/hello"});
    let response = serde_json::json!({"data": {
        "u0": {"pullRequests": {"nodes": []}},
        "viewerPrs": {"nodes": [mine, theirs]},
    }});
    adopt(&state, response, "me", &mut results).await;
    let numbers: Vec<i64> = results["/work"].iter().map(|p| p.number as i64).collect();
    assert!(
        numbers.contains(&11) && !numbers.contains(&12),
        "{numbers:?}"
    );
}

// Catches: a failed fork poll being replaced by an upstream-only list, which the poller
// would publish as the checkout's complete PR set.
#[tokio::test]
async fn failed_fork_poll_is_not_replaced_by_upstream_only_prs() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = std::collections::HashMap::new();
    let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [
        pr_node(42, "feature", "org/hello", "me/hello"),
    ]}}}});
    adopt(&state, response, "", &mut results).await;
    assert!(results.is_empty());
}

// Catches: a denied/missing upstream alias erasing the fork's own PRs.
#[tokio::test]
async fn inaccessible_upstream_leaves_the_fork_prs_untouched() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    let response = serde_json::json!({"data": {"u0": null}, "errors": [{"type": "NOT_FOUND"}]});
    adopt(&state, response, "", &mut results).await;
    assert_eq!(results["/work"].len(), 1);
}

// Catches: an upstream PR duplicating a branch the fork's own list already shows.
#[tokio::test]
async fn existing_pr_for_a_branch_is_not_duplicated() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut results = seeded_results();
    let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [
        pr_node(42, "own-branch", "org/hello", "me/hello"),
    ]}}}});
    adopt(&state, response, "", &mut results).await;
    assert_eq!(results["/work"].len(), 1);
    assert_eq!(results["/work"][0].number, 1);
}

// Catches: the batch query not asking for the fields the filter and actions depend on.
#[test]
fn batch_query_selects_repository_fields_and_upstream_sections() {
    let (query, aliases, upstream_aliases) =
        tuic_git::github::build_unified_batch_query_with_upstream(
            &fork_repos(),
            &upstream_repos(),
            &[],
            false,
            "disabled",
            "me",
            false,
            None,
        );
    assert_eq!(aliases, vec![("r0".to_string(), "/work".to_string())]);
    assert_eq!(
        upstream_aliases,
        vec![("u0".to_string(), "/work".to_string())]
    );
    assert!(query.contains("u0: repository(owner: \"org\", name: \"hello\")"));
    assert!(query.contains("headRepository { nameWithOwner }"));
    assert!(query.contains("baseRepository { nameWithOwner }"));
    assert!(query.contains("parent { nameWithOwner }"));
    // The viewer search covers the upstream too.
    assert!(query.contains("repo:me/hello repo:org/hello"), "{query}");
    // Upstream gets PRs only.
    assert_eq!(query.matches("issues(").count(), 0);
}

// ---------------------------------------------------------------------------
// Which repository a PR action targets (pure)
// ---------------------------------------------------------------------------

fn snapshot(prs: &[(i64, &str)]) -> Vec<BranchPrStatus> {
    prs.iter()
        .map(|(number, base)| {
            parse_pr_node(&pr_node(*number, &format!("b{number}"), base, "me/hello")).unwrap()
        })
        .collect()
}

fn target(owner: &str, repo: &str) -> Option<(String, String)> {
    Some((owner.to_string(), repo.to_string()))
}

// Catches: merge/approve/close/diff/refs/CI calls going to the fork for an upstream PR (404).
#[test]
fn a_pr_that_lives_upstream_targets_its_base_repository() {
    let prs = snapshot(&[(1, "me/hello"), (42, "org/hello")]);
    assert_eq!(
        pick_pr_target(&prs, 42, None, "me/hello"),
        Ok(target("org", "hello"))
    );
    // A PR of the checkout's own repository resolves to itself.
    assert_eq!(
        pick_pr_target(&prs, 1, None, "me/hello"),
        Ok(target("me", "hello"))
    );
}

#[test]
fn an_unknown_pr_falls_back_to_the_bound_repository() {
    assert_eq!(pick_pr_target(&[], 5, None, "me/hello"), Ok(None));
    let prs = snapshot(&[(1, "me/hello")]);
    assert_eq!(pick_pr_target(&prs, 5, None, "me/hello"), Ok(None));
}

// Catches: closing or approving the wrong repository's PR when both repos have a PR #N and
// the caller did not say which.
#[test]
fn a_number_shared_by_two_repositories_is_refused_without_a_hint() {
    let prs = snapshot(&[(5, "me/hello"), (5, "org/hello")]);
    let err = pick_pr_target(&prs, 5, None, "me/hello").unwrap_err();
    assert!(err.contains("more than one watched repository"), "{err}");
    assert!(err.contains("baseRepo"), "{err}");
}

// Catches: the collision still being unresolvable even when the caller names the repository.
#[test]
fn a_hint_resolves_a_number_shared_by_two_repositories() {
    let prs = snapshot(&[(5, "me/hello"), (5, "org/hello")]);
    assert_eq!(
        pick_pr_target(&prs, 5, Some("org/hello"), "me/hello"),
        Ok(target("org", "hello"))
    );
    assert_eq!(
        pick_pr_target(&prs, 5, Some("me/hello"), "me/hello"),
        Ok(None),
        "the bound repository is the default target"
    );
    // Owner/name case is irrelevant, as on GitHub.
    assert_eq!(
        pick_pr_target(&prs, 5, Some("ORG/Hello"), "me/hello"),
        Ok(target("org", "hello"))
    );
}

// Catches: a client-supplied hint pointing the token at a repository this checkout does not watch.
#[test]
fn a_hint_naming_an_unwatched_repository_is_refused() {
    let prs = snapshot(&[(5, "org/hello")]);
    let err = pick_pr_target(&prs, 5, Some("evil/hello"), "me/hello").unwrap_err();
    assert!(err.contains("not one of this checkout's watched"), "{err}");
    // Right repo, wrong number: also not a watched PR.
    let err = pick_pr_target(&prs, 6, Some("org/hello"), "me/hello").unwrap_err();
    assert!(err.contains("PR #6"), "{err}");
}

#[test]
fn base_repo_hint_must_be_owner_slash_name() {
    assert_eq!(parse_base_repo_hint(None), Ok(None));
    assert_eq!(parse_base_repo_hint(Some("  ")), Ok(None));
    assert_eq!(
        parse_base_repo_hint(Some(" org/hello ")),
        Ok(Some("org/hello"))
    );
    for bad in ["org", "org/", "/hello", "a/b/c", "org//hello"] {
        assert!(parse_base_repo_hint(Some(bad)).is_err(), "{bad}");
    }
}

// ---------------------------------------------------------------------------
// Watched upstream set and the fork-parent cache
// ---------------------------------------------------------------------------

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A real checkout whose remotes are `(name, url)`.
fn repo_with_remotes(remotes: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = crate::state::tests_support::create_temp_git_repo();
    for (name, url) in remotes {
        git(dir.path(), &["remote", "add", name, url]);
    }
    dir
}

fn repos_at(dir: &tempfile::TempDir) -> Vec<(String, String, String)> {
    vec![(
        dir.path().to_string_lossy().into_owned(),
        "me".into(),
        "hello".into(),
    )]
}

const FORK: &str = "https://github.com/me/hello.git";
const UPSTREAM: &str = "https://github.com/org/hello.git";

fn names(v: &[(String, String, String)]) -> Vec<String> {
    v.iter().map(|(_, o, n)| format!("{o}/{n}")).collect()
}

#[test]
fn watched_upstreams_come_from_the_remote_the_cache_or_both_without_duplicates() {
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let key = |o: &str, n: &str| parent_cache_key(&account, o, n);

    // Nothing known: nothing to watch.
    let plain = repo_with_remotes(&[("origin", FORK)]);
    assert!(watched_upstreams(&state, &account, &repos_at(&plain)).is_empty());

    // The remote alone.
    let with_remote = repo_with_remotes(&[("origin", FORK), ("upstream", UPSTREAM)]);
    assert_eq!(
        names(&watched_upstreams(
            &state,
            &account,
            &repos_at(&with_remote)
        )),
        ["org/hello"]
    );

    // The cached parent alone.
    state
        .git_cache
        .github_repo_parents
        .insert(key("me", "hello"), Some("org/hello".into()));
    assert_eq!(
        names(&watched_upstreams(&state, &account, &repos_at(&plain))),
        ["org/hello"]
    );

    // Both naming the same repo: one entry.
    assert_eq!(
        names(&watched_upstreams(
            &state,
            &account,
            &repos_at(&with_remote)
        )),
        ["org/hello"]
    );

    // Both naming different repos: both are watched.
    let other = repo_with_remotes(&[
        ("origin", FORK),
        ("upstream", "https://github.com/third/hello.git"),
    ]);
    assert_eq!(
        names(&watched_upstreams(&state, &account, &repos_at(&other))),
        ["third/hello", "org/hello"]
    );

    // A cached "not a fork" adds nothing, and a parent equal to the repo itself is dropped.
    state
        .git_cache
        .github_repo_parents
        .insert(key("me", "hello"), None);
    assert!(watched_upstreams(&state, &account, &repos_at(&plain)).is_empty());
    state
        .git_cache
        .github_repo_parents
        .insert(key("me", "hello"), Some("ME/Hello".into()));
    assert!(watched_upstreams(&state, &account, &repos_at(&plain)).is_empty());
}

#[test]
fn parent_cache_is_keyed_by_host_owner_and_name_case_insensitively() {
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    assert_eq!(
        parent_cache_key(&account, "Me", "Hello"),
        parent_cache_key(&account, "me", "hello")
    );
    assert!(parent_cache_key(&account, "me", "hello").starts_with("github.com/"));
}

fn aliases(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| (format!("r{i}"), format!("/p{i}")))
        .collect()
}

#[test]
fn learning_parentage_caches_forks_and_non_forks_but_not_unanswered_repos() {
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let repos: Vec<(String, String, String)> = vec![
        ("/p0".into(), "me".into(), "fork".into()),
        ("/p1".into(), "me".into(), "plain".into()),
        ("/p2".into(), "me".into(), "denied".into()),
    ];
    let response = serde_json::json!({"data": {
        "r0": {"isFork": true, "parent": {"nameWithOwner": "org/fork"}},
        "r1": {"isFork": false, "parent": null},
        "r2": null,
    }});
    let to_poll = learn_fork_parents(&state, &account, &repos, &aliases(3), &[], &response);
    assert_eq!(
        to_poll,
        vec![("/p0".to_string(), "org".to_string(), "fork".to_string())]
    );
    let cache = &state.git_cache.github_repo_parents;
    assert_eq!(
        cache
            .get(&parent_cache_key(&account, "me", "fork"))
            .unwrap()
            .value(),
        &Some("org/fork".to_string())
    );
    assert_eq!(
        cache
            .get(&parent_cache_key(&account, "me", "plain"))
            .unwrap()
            .value(),
        &None
    );
    assert!(
        !cache.contains_key(&parent_cache_key(&account, "me", "denied")),
        "an inaccessible repository is asked again, not remembered as a non-fork"
    );
}

#[test]
fn a_parent_the_upstream_remote_already_covers_is_not_polled_twice() {
    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let repos: Vec<(String, String, String)> = vec![("/p0".into(), "me".into(), "hello".into())];
    let watched: Vec<(String, String, String)> = vec![("/p0".into(), "Org".into(), "Hello".into())];
    let response = serde_json::json!({"data": {
        "r0": {"isFork": true, "parent": {"nameWithOwner": "org/hello"}},
    }});
    assert!(
        learn_fork_parents(&state, &account, &repos, &aliases(1), &watched, &response).is_empty()
    );
    // ...but it is still remembered for next time.
    assert!(
        state
            .git_cache
            .github_repo_parents
            .contains_key(&parent_cache_key(&account, "me", "hello"))
    );
}

// Catches: re-asking GitHub about fork status every poll once it is known.
#[test]
fn known_forks_skip_the_parent_probe_in_the_batch_query() {
    let repos = vec![
        ("/known".to_string(), "me".to_string(), "a".to_string()),
        ("/unknown".to_string(), "me".to_string(), "b".to_string()),
    ];
    let known = ["/known".to_string()];
    let (query, _, _) = tuic_git::github::build_unified_batch_query_with_upstream(
        &repos,
        &[],
        &known,
        false,
        "disabled",
        "",
        false,
        None,
    );
    let r0 = query.split("r1: repository").next().unwrap();
    let r1 = query.split("r1: repository").nth(1).unwrap();
    assert!(!r0.contains("isFork") && !r0.contains("parent"), "{r0}");
    assert!(
        r1.contains("isFork") && r1.contains("parent { nameWithOwner }"),
        "{r1}"
    );
}

#[test]
fn remote_name_for_repo_prefers_origin_and_matches_host_owner_and_name() {
    let account_host = crate::github_account::parse_remote_url(FORK).unwrap().0;
    let dir = repo_with_remotes(&[
        ("zzz", "git@github.com:org/hello.git"),
        ("upstream", UPSTREAM),
        ("origin", FORK),
        ("ghe", "git@ghe.acme.com:org/hello.git"),
    ]);
    let at = |o: &str, n: &str| remote_name_for_repo(dir.path(), &account_host, o, n);
    assert_eq!(at("me", "hello"), Some("origin".into()));
    // Two remotes name org/hello on github.com: a stable pick, never the GHE one.
    assert!(matches!(
        at("ORG", "Hello").as_deref(),
        Some("upstream" | "zzz")
    ));
    assert_eq!(at("nobody", "hello"), None);
}

// Catches: the "diff too large" fallback fetching `pull/N/head` from the fork (`origin`)
// instead of the repository the PR lives in.
#[test]
fn local_diff_fetches_from_the_named_remote_not_origin() {
    let upstream = tempfile::tempdir().unwrap();
    let decoy = tempfile::tempdir().unwrap();
    let work = crate::state::tests_support::create_temp_git_repo();
    git(
        upstream.path(),
        &["init", "--quiet", "--bare", "-b", "main"],
    );
    git(decoy.path(), &["init", "--quiet", "--bare", "-b", "main"]);
    let up = upstream.path().to_string_lossy().into_owned();
    // Build upstream history from the work repo: main = initial, PR head = one more commit.
    git(work.path(), &["branch", "-M", "main"]);
    git(
        work.path(),
        &["remote", "add", "origin", &decoy.path().to_string_lossy()],
    );
    git(work.path(), &["remote", "add", "upstream", &up]);
    git(work.path(), &["push", "--quiet", "upstream", "main"]);
    git(work.path(), &["checkout", "--quiet", "-b", "feature"]);
    std::fs::write(work.path().join("only-in-pr.txt"), "hello\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "--quiet", "-m", "pr work"]);
    git(
        work.path(),
        &["push", "--quiet", "upstream", "feature:refs/pull/7/head"],
    );
    git(work.path(), &["checkout", "--quiet", "main"]);
    git(work.path(), &["branch", "-D", "feature"]);

    let refs = PrRefs {
        head_ref: "feature".into(),
        base_ref: "main".into(),
        head_sha: None,
        base_sha: None,
        head_from_fork: true,
    };
    let path = work.path().to_string_lossy().into_owned();
    let diff = local_pr_diff_from(&path, 7, &refs, "upstream").unwrap();
    assert!(diff.contains("only-in-pr.txt"), "{diff}");
    // The decoy `origin` has no such PR: asking it cannot produce this diff.
    assert!(local_pr_diff_from(&path, 7, &refs, "origin").is_err());
}

// Catches: an upstream MERGED PR resurrecting a branch the user has since moved on from.
#[tokio::test]
async fn merged_upstream_pr_is_kept_only_while_the_local_branch_is_at_its_head() {
    let repo = crate::state::tests_support::create_temp_git_repo();
    git(repo.path(), &["branch", "feature"]);
    let tip = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "feature"])
            .current_dir(repo.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let path = repo.path().to_string_lossy().into_owned();
    let state = crate::state::tests_support::make_test_app_state();
    for (oid, kept) in [
        (tip.as_str(), true),
        ("0000000000000000000000000000000000000000", false),
    ] {
        let mut node = pr_node(42, "feature", "org/hello", "me/hello");
        node["state"] = serde_json::json!("MERGED");
        node["headRefOid"] = serde_json::json!(oid);
        let response = serde_json::json!({"data": {"u0": {"pullRequests": {"nodes": [node]}}}});
        let own = parse_pr_node(&pr_node(1, "other", "me/hello", "me/hello")).unwrap();
        let mut results: std::collections::HashMap<String, Vec<BranchPrStatus>> =
            std::iter::once((path.clone(), vec![own])).collect();
        adopt_upstream_prs(
            &state,
            &github_com_account(&state),
            &[(path.clone(), "me".into(), "hello".into())],
            &[(path.clone(), "org".into(), "hello".into())],
            &[("u0".into(), path.clone())],
            true,
            "",
            &response,
            &mut results,
        )
        .await;
        assert_eq!(
            results[&path].iter().any(|p| p.number == 42),
            kept,
            "head {oid}"
        );
    }
}

// ---------------------------------------------------------------------------
// End to end through the real poll (loopback GraphQL)
// ---------------------------------------------------------------------------

/// Serves one fork + its upstream: the fork's own PR list, the upstream's PR list, the
/// parentage GitHub reports, and a canned error for anything else.
#[derive(Clone)]
struct Forge {
    /// `Some(parent)` for a fork, `None` for a plain repository.
    parent: Option<&'static str>,
    own: Vec<serde_json::Value>,
    upstream: Vec<serde_json::Value>,
    /// The upstream alias answers `null` (token cannot read it).
    upstream_denied: bool,
    /// The fork alias answers `null`.
    fork_denied: bool,
}

impl Forge {
    fn respond(&self, request: &serde_json::Value) -> serde_json::Value {
        let query = request["query"].as_str().unwrap();
        let alias = regex::Regex::new(r"\b([ru]\d+):\s*repository").unwrap();
        let probes = query.contains("isFork");
        let mut data = serde_json::Map::new();
        for caps in alias.captures_iter(query) {
            let name = caps[1].to_string();
            let node = if name.starts_with('u') {
                if self.upstream_denied {
                    serde_json::Value::Null
                } else {
                    serde_json::json!({"pullRequests": {"nodes": self.upstream}})
                }
            } else if self.fork_denied {
                serde_json::Value::Null
            } else {
                let mut node = serde_json::json!({"pullRequests": {"nodes": self.own}});
                if probes {
                    node["isFork"] = serde_json::json!(self.parent.is_some());
                    node["parent"] = match self.parent {
                        Some(p) => serde_json::json!({"nameWithOwner": p}),
                        None => serde_json::Value::Null,
                    };
                }
                node
            };
            data.insert(name, node);
        }
        data.insert(
            "rateLimit".into(),
            serde_json::json!({"cost": 1, "remaining": 4000, "resetAt": "2030-01-01T00:00:00Z"}),
        );
        serde_json::json!({"data": data})
    }
}

fn forge(parent: Option<&'static str>) -> Forge {
    Forge {
        parent,
        own: vec![pr_node(1, "own-branch", "me/hello", "me/hello")],
        upstream: vec![
            pr_node(42, "feature", "org/hello", "me/hello"),
            pr_node(43, "theirs", "org/hello", "stranger/hello"),
        ],
        upstream_denied: false,
        fork_denied: false,
    }
}

async fn serve(forge: Forge) -> crate::github::partial_tests::PollHttpFixture {
    crate::github::partial_tests::PollHttpFixture::with_response(move |r| forge.respond(r)).await
}

fn queries(fixture: &mut crate::github::partial_tests::PollHttpFixture) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(request) = fixture.requests.try_recv() {
        out.push(request["query"].as_str().unwrap().to_string());
    }
    out
}

async fn poll(
    fixture: &crate::github::partial_tests::PollHttpFixture,
    repos: &[(String, String, String)],
    results: &mut std::collections::HashMap<String, Vec<BranchPrStatus>>,
) {
    poll_one_account(
        &fixture.state,
        &github_com_account(&fixture.state),
        None,
        repos,
        false,
        "disabled",
        false,
        results,
        &mut Default::default(),
        None,
        &mut Default::default(),
    )
    .await
    .unwrap();
}

fn numbers(
    results: &std::collections::HashMap<String, Vec<BranchPrStatus>>,
    repo: &tempfile::TempDir,
) -> Vec<i32> {
    let mut n: Vec<i32> = results[&repo.path().to_string_lossy().into_owned()]
        .iter()
        .map(|p| p.number)
        .collect();
    n.sort_unstable();
    n
}

// Catches: the upstream remote not being polled, or its non-fork PRs leaking into the sidebar.
#[tokio::test]
async fn polling_a_fork_with_an_upstream_remote_adds_only_its_own_upstream_prs() {
    let repo = repo_with_remotes(&[("origin", FORK), ("upstream", UPSTREAM)]);
    let mut fixture = serve(forge(Some("org/hello"))).await;
    let mut results = Default::default();
    poll(&fixture, &repos_at(&repo), &mut results).await;
    assert_eq!(numbers(&results, &repo), [1, 42]);
    let sent = queries(&mut fixture);
    assert_eq!(
        sent.len(),
        1,
        "the remote already names the parent: one request"
    );
    assert!(sent[0].contains("u0: repository(owner: \"org\", name: \"hello\")"));
    fixture.close().await;
}

// Catches: GitHub being asked for the parent on every poll, and a fork with no `upstream`
// remote never finding its upstream PRs.
#[tokio::test]
async fn parent_is_discovered_once_cached_and_then_polled_in_the_main_query() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let mut fixture = serve(forge(Some("org/hello"))).await;

    // Cycle 1: probe + follow-up for the freshly learned parent.
    let mut results = Default::default();
    poll(&fixture, &repos_at(&repo), &mut results).await;
    assert_eq!(
        numbers(&results, &repo),
        [1, 42],
        "found in the same cycle it was learned"
    );
    let first = queries(&mut fixture);
    assert_eq!(first.len(), 2, "{first:#?}");
    assert!(first[0].contains("isFork") && !first[0].contains("u0:"));
    assert!(first[1].contains("u0: repository(owner: \"org\"") && !first[1].contains("r0:"));
    let account = github_com_account(&fixture.state);
    assert_eq!(
        fixture
            .state
            .git_cache
            .github_repo_parents
            .get(&parent_cache_key(&account, "me", "hello"))
            .unwrap()
            .value(),
        &Some("org/hello".to_string())
    );

    // Cycle 2: cached. One request, no probe, the parent rides in the main query.
    let mut results = Default::default();
    poll(&fixture, &repos_at(&repo), &mut results).await;
    assert_eq!(numbers(&results, &repo), [1, 42]);
    let second = queries(&mut fixture);
    assert_eq!(second.len(), 1, "{second:#?}");
    assert!(!second[0].contains("isFork"));
    assert!(second[0].contains("u0: repository(owner: \"org\", name: \"hello\")"));
    fixture.close().await;
}

// Catches: a plain repository being probed (and a follow-up attempted) forever.
#[tokio::test]
async fn a_repository_that_is_not_a_fork_is_remembered_as_one() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let mut fixture = serve(forge(None)).await;
    for cycle in 0..3 {
        let mut results = Default::default();
        poll(&fixture, &repos_at(&repo), &mut results).await;
        assert_eq!(numbers(&results, &repo), [1], "cycle {cycle}");
        let sent = queries(&mut fixture);
        assert_eq!(sent.len(), 1, "never a follow-up (cycle {cycle})");
        assert_eq!(
            sent[0].contains("isFork"),
            cycle == 0,
            "probe only until known"
        );
    }
    fixture.close().await;
}

// Catches: a parent the token cannot read failing, or emptying, the fork's own poll.
#[tokio::test]
async fn an_unreadable_parent_never_breaks_the_forks_own_poll() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let mut f = forge(Some("org/hello"));
    f.upstream_denied = true;
    let fixture = serve(f).await;
    for _ in 0..2 {
        let mut results = Default::default();
        poll(&fixture, &repos_at(&repo), &mut results).await;
        assert_eq!(numbers(&results, &repo), [1]);
    }
    fixture.close().await;
}

// Catches: a repository GitHub did not answer for being cached as "not a fork".
#[tokio::test]
async fn an_unanswered_repository_is_not_cached() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let mut f = forge(Some("org/hello"));
    f.fork_denied = true;
    let fixture = serve(f).await;
    let mut results = Default::default();
    poll(&fixture, &repos_at(&repo), &mut results).await;
    assert!(fixture.state.git_cache.github_repo_parents.is_empty());
    fixture.close().await;
}

// Catches: a PR action sent to the fork after the poll snapshot aged out of the cache.
#[tokio::test]
async fn an_expired_snapshot_is_repolled_before_choosing_a_pr_target() {
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
    std::fs::create_dir_all(scratch.path().join("config")).unwrap();
    let repo = repo_with_remotes(&[("origin", FORK), ("upstream", UPSTREAM)]);
    let path = repo.path().to_string_lossy().into_owned();
    let mut fixture = serve(forge(Some("org/hello"))).await;
    let account = github_com_account(&fixture.state);
    assert!(fixture.state.git_cache.github_status.get(&path).is_none());

    let resolved = resolve_pr_target(&fixture.state, &account, &path, 42, None, "me", "hello")
        .await
        .unwrap();
    assert_eq!(resolved, ("org".to_string(), "hello".to_string()));
    assert_eq!(queries(&mut fixture).len(), 1, "exactly one re-poll");

    // Warm cache: no further request.
    let again = resolve_pr_target(&fixture.state, &account, &path, 42, None, "me", "hello")
        .await
        .unwrap();
    assert_eq!(again, resolved);
    assert!(queries(&mut fixture).is_empty());
    fixture.close().await;
}

// Catches: an explicit base repo being ignored, or needing a cache that is not there.
#[tokio::test]
async fn a_hint_for_the_upstream_repolls_and_a_hint_for_the_fork_does_not() {
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
    std::fs::create_dir_all(scratch.path().join("config")).unwrap();
    // No upstream remote: only the hint tells us to look further.
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let path = repo.path().to_string_lossy().into_owned();
    let mut fixture = serve(forge(Some("org/hello"))).await;
    let account = github_com_account(&fixture.state);

    let own = resolve_pr_target(
        &fixture.state,
        &account,
        &path,
        1,
        Some("me/hello"),
        "me",
        "hello",
    )
    .await
    .unwrap();
    assert_eq!(own, ("me".to_string(), "hello".to_string()));
    assert!(
        queries(&mut fixture).is_empty(),
        "the bound repo needs no snapshot"
    );

    let up = resolve_pr_target(
        &fixture.state,
        &account,
        &path,
        42,
        Some("org/hello"),
        "me",
        "hello",
    )
    .await
    .unwrap();
    assert_eq!(up, ("org".to_string(), "hello".to_string()));

    let err = resolve_pr_target(
        &fixture.state,
        &account,
        &path,
        42,
        Some("nope"),
        "me",
        "hello",
    )
    .await
    .unwrap_err();
    assert!(err.contains("owner/name"), "{err}");
    fixture.close().await;
}

// Catches: an ordinary single-repo checkout paying for a poll on every action after the
// snapshot expires.
#[tokio::test]
async fn a_checkout_with_nothing_upstream_never_repolls_for_an_action() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let path = repo.path().to_string_lossy().into_owned();
    let mut fixture = serve(forge(None)).await;
    let account = github_com_account(&fixture.state);
    let resolved = resolve_pr_target(&fixture.state, &account, &path, 1, None, "me", "hello")
        .await
        .unwrap();
    assert_eq!(resolved, ("me".to_string(), "hello".to_string()));
    assert!(queries(&mut fixture).is_empty());
    fixture.close().await;
}

// Catches: the single-repo status refresh (fork only) dropping the upstream PRs the batch
// poll found.
#[tokio::test]
async fn single_repo_refresh_keeps_prs_found_against_the_upstream() {
    let repo = repo_with_remotes(&[("origin", FORK)]);
    let path = repo.path().to_string_lossy().into_owned();
    let fixture = serve(forge(None)).await;
    let upstream_pr = parse_pr_node(&pr_node(42, "feature", "org/hello", "me/hello")).unwrap();
    let stale_own = parse_pr_node(&pr_node(9, "own-branch", "me/hello", "me/hello")).unwrap();
    fixture
        .state
        .git_cache
        .github_status
        .insert(path.clone(), Arc::new(vec![upstream_pr, stale_own]));

    let statuses = get_repo_pr_statuses_impl(&path, false, &fixture.state)
        .await
        .unwrap();
    let mut numbers: Vec<i32> = statuses.iter().map(|s| s.number).collect();
    numbers.sort_unstable();
    // Fresh own PR (#1) from the query; the upstream PR carried over; the stale own one gone.
    assert_eq!(numbers, [1, 42]);
    fixture.close().await;
}
