use super::*;
use crate::github_pr_api::{PrApi, graphql_pages};
use serde_json::{Value, json};
use std::sync::Mutex;

// The recorded merged-PR nodes provide the schema; local stub effects substitute
// a local branch/head and pagination controls so no remote Git fetch is needed.
struct Stub {
    pages: Mutex<std::collections::VecDeque<Result<Value, String>>>,
    cursors: Mutex<Vec<Value>>,
}

impl Stub {
    fn new(pages: Vec<Result<Value, String>>) -> Self {
        Self {
            pages: Mutex::new(pages.into()),
            cursors: Mutex::new(Vec::new()),
        }
    }
}

impl PrApi for Stub {
    fn graphql(
        &self,
        _: &str,
        variables: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        self.cursors
            .lock()
            .unwrap()
            .push(variables["endCursor"].clone());
        std::future::ready(
            self.pages
                .lock()
                .unwrap()
                .pop_front()
                .expect("bounded requests"),
        )
    }

    fn rest(
        &self,
        _: &str,
        _: &str,
        _: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
        std::future::ready(Err("merged PR proof must use GraphQL".to_string()))
    }
}

fn page(nodes: Value, next: bool, cursor: Value) -> Value {
    json!({"data":{"repository":{"pullRequests":{
        "nodes":nodes,"pageInfo":{"hasNextPage":next,"endCursor":cursor}
    }}}})
}

fn merged_node(tip: &str) -> Value {
    let recorded: Value = serde_json::from_str(include_str!(
        "../fixtures/github-review/threads-recorded.json"
    ))
    .unwrap();
    let mut node = recorded["data"]["repository"]["pullRequests"]["nodes"][0].clone();
    node["state"] = json!("MERGED");
    node["headRefName"] = json!("feature");
    node["headRefOid"] = json!(tip);
    node
}

// Catches: stopping after page 1, losing the cursor, or failing to restore the
// full response expected by the unchanged ancestry verifier.
#[tokio::test]
async fn merged_pr_proof_reads_second_page_and_proves_only_the_recorded_tip() {
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let tip = git_cmd(repo.path())
        .args(["rev-parse", "HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    let api = Stub::new(vec![
        Ok(page(json!([]), true, json!("page-2"))),
        Ok(page(json!([merged_node(&tip)]), false, Value::Null)),
    ]);
    assert_eq!(
        query_merged_github_pr_with(&api, json!({}), repo.path(), "feature", &tip).await,
        Some(true)
    );
    assert_eq!(
        *api.cursors.lock().unwrap(),
        vec![Value::Null, json!("page-2")]
    );
    let api = Stub::new(vec![Ok(page(
        json!([merged_node(&tip)]),
        false,
        Value::Null,
    ))]);
    assert_eq!(
        query_merged_github_pr_with(&api, json!({}), repo.path(), "other-branch", &tip).await,
        Some(false)
    );
}

// Catches: returning/caching a partial answer after page 2 fails, including when
// page 1 already contains a matching merged PR.
#[test]
fn merged_pr_proof_page_two_error_is_unknown_and_never_cached() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let tip = git_cmd(repo.path())
        .args(["rev-parse", "HEAD"])
        .run()
        .unwrap()
        .stdout
        .trim()
        .to_owned();
    let cache = moka::sync::Cache::builder().max_capacity(8).build();
    for nodes in [json!([]), json!([merged_node(&tip)])] {
        let api = Stub::new(vec![
            Ok(page(nodes, true, json!("page-2"))),
            Err("page 2 unavailable".into()),
        ]);
        let answer = runtime.block_on(query_merged_github_pr_with(
            &api,
            json!({}),
            repo.path(),
            "feature",
            &tip,
        ));
        assert_eq!(answer, None);
        assert!(!cached_pr_proof(
            &cache,
            repo.path(),
            "feature",
            &tip,
            || answer
        ));
        assert_eq!(
            cache.get(&(repo.path().to_owned(), "feature".into(), tip.clone())),
            None
        );
    }
}

// Catches: accepting malformed or looping pagination as a definitive negative
// answer, or silently treating a page-cap truncation as a complete response.
#[tokio::test]
async fn graphql_pages_rejects_incomplete_and_cyclic_connections() {
    let continuing = page(json!([]), true, json!("cursor"));
    for (pages, cap) in [
        (vec![Ok(continuing.clone())], 1),
        (vec![Ok(page(json!([]), true, Value::Null))], 10),
        (vec![Ok(continuing.clone()), Ok(continuing)], 10),
        (
            vec![Ok(
                json!({"data":{"repository":{"pullRequests":{"nodes":[]}}}}),
            )],
            10,
        ),
        (
            vec![Ok(
                json!({"data":{"repository":{"pullRequests":{"nodes":[],"pageInfo":{"hasNextPage":false}}}},"errors":[{"message":"partial failure"}]}),
            )],
            10,
        ),
    ] {
        assert!(
            graphql_pages(
                &Stub::new(pages),
                "query",
                json!({}),
                "/data/repository/pullRequests",
                cap
            )
            .await
            .is_err()
        );
    }
}

// Catches: running Handle::block_on on a runtime worker rather than on the
// caller's blocking thread. An isolated repo with no remote fails closed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merged_pr_proof_closure_runs_on_spawn_blocking_without_panic() {
    let repo = tuic_git::test_fixtures::setup_test_repo();
    let _config = crate::config::set_config_dir_override(repo.path().join("config"));
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        repo.path(),
    ));
    let proof = pr_proof(state, tokio::runtime::Handle::current());
    assert!(
        !tokio::task::spawn_blocking(move || proof(repo.path(), "feature", "tip"))
            .await
            .unwrap()
    );
}

// ---------------------------------------------------------------------------
// A fork's PR is merged in the upstream: the proof must ask there too
// ---------------------------------------------------------------------------

mod upstream {
    use super::*;

    fn merged(number: u64, branch: &str, head: &str) -> Value {
        json!({"data": {"repository": {"pullRequests": {
            "nodes": [{"number": number, "state": "MERGED", "headRefName": branch, "headRefOid": head}],
            "pageInfo": {"hasNextPage": false, "endCursor": null}
        }}}})
    }

    fn empty() -> Value {
        json!({"data": {"repository": {"pullRequests": {
            "nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}
        }}}})
    }

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

    /// A checkout with `origin` = the fork and, optionally, an `upstream` remote.
    fn checkout(with_upstream: bool) -> (tempfile::TempDir, String) {
        let repo = crate::state::tests_support::create_temp_git_repo();
        git(
            repo.path(),
            &["remote", "add", "origin", "https://github.com/me/hello.git"],
        );
        if with_upstream {
            git(
                repo.path(),
                &[
                    "remote",
                    "add",
                    "upstream",
                    "https://github.com/org/hello.git",
                ],
            );
        }
        let tip = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo.path())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        (repo, tip)
    }

    /// `answer(owner)` is the GraphQL body for that repository's merged PRs.
    async fn ask(
        repo: &tempfile::TempDir,
        tip: &str,
        answer: impl Fn(&str) -> Value + Send + Sync + 'static,
    ) -> (Option<bool>, Vec<String>) {
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
        std::fs::create_dir_all(scratch.path().join("config")).unwrap();
        let mut fixture = crate::github::partial_tests::PollHttpFixture::with_response(move |r| {
            answer(r["variables"]["owner"].as_str().unwrap_or(""))
        })
        .await;
        let got = query_merged_github_pr(&fixture.state, repo.path(), "feature", tip).await;
        let mut owners = Vec::new();
        while let Ok(request) = fixture.requests.try_recv() {
            owners.push(
                request["variables"]["owner"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            );
        }
        fixture.close().await;
        (got, owners)
    }

    // Catches: a branch whose PR was merged upstream never being provably integrated, so its
    // worktree/branch could never be cleaned up.
    #[tokio::test]
    async fn a_pr_merged_upstream_proves_the_tip() {
        let (repo, tip) = checkout(true);
        let head = tip.clone();
        let (got, owners) = ask(&repo, &tip, move |owner| {
            if owner == "org" {
                merged(7, "feature", &head)
            } else {
                empty()
            }
        })
        .await;
        assert_eq!(got, Some(true));
        assert!(owners.contains(&"org".to_string()), "{owners:?}");
    }

    // Catches: the new lookup regressing the ordinary case (merged in the bound repo).
    #[tokio::test]
    async fn a_pr_merged_in_the_bound_repo_still_proves_the_tip() {
        let (repo, tip) = checkout(true);
        let head = tip.clone();
        let (got, _) = ask(&repo, &tip, move |owner| {
            if owner == "me" {
                merged(7, "feature", &head)
            } else {
                empty()
            }
        })
        .await;
        assert_eq!(got, Some(true));
    }

    // Catches: another contributor's same-named upstream branch forging a proof.
    #[tokio::test]
    async fn an_upstream_pr_with_a_different_head_proves_nothing() {
        let (repo, tip) = checkout(true);
        let (got, owners) = ask(&repo, &tip, |owner| {
            if owner == "org" {
                merged(7, "feature", &"1".repeat(40))
            } else {
                empty()
            }
        })
        .await;
        assert_eq!(got, Some(false));
        assert_eq!(owners.len(), 2, "both repositories were asked: {owners:?}");
    }

    // Catches: an unreadable upstream being cached as a definite "not merged".
    #[tokio::test]
    async fn an_unanswered_upstream_is_unknown_not_false() {
        let (repo, tip) = checkout(true);
        let (got, _) = ask(&repo, &tip, |owner| {
            if owner == "org" {
                json!({"errors": [{"type": "NOT_FOUND", "message": "nope"}], "data": {"repository": null}})
            } else {
                empty()
            }
        })
        .await;
        assert_eq!(got, None);
    }

    // Catches: ordinary single-repo checkouts paying a second request.
    #[tokio::test]
    async fn a_checkout_with_nothing_upstream_asks_one_repository() {
        let (repo, tip) = checkout(false);
        let (got, owners) = ask(&repo, &tip, |_| empty()).await;
        assert_eq!(got, Some(false));
        assert_eq!(owners, ["me"]);
    }
}
