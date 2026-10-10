use super::*;
use crate::github_review_threads::{DraftReview, submit_review};
use std::sync::Mutex;

struct Stub {
    pr: Value,
    rules: Value,
    writes: Mutex<Vec<Value>>,
    rules_fail: bool,
    later_rules: Option<Value>,
}
impl Stub {
    fn recorded() -> Self {
        let data: Value = serde_json::from_str(include_str!(
            "../fixtures/github-review/actions-pr-recorded.json"
        ))
        .unwrap();
        Self {
            pr: data["data"]["repository"]["pullRequests"]["nodes"][0].clone(),
            rules: serde_json::from_str(include_str!(
                "../fixtures/github-review/actions-rules-recorded.json"
            ))
            .unwrap(),
            writes: Mutex::new(vec![]),
            rules_fail: false,
            later_rules: None,
        }
    }
    fn head(&self) -> &str {
        self.pr["headRefOid"].as_str().unwrap()
    }
}
impl PrApi for Stub {
    async fn graphql(&self, query: &str, vars: Value) -> Result<Value, String> {
        if !query.contains("mutation") {
            return Ok(json!({"repository":{"pullRequest":self.pr}}));
        }
        self.writes
            .lock()
            .unwrap()
            .push(json!({"query":query,"variables":vars}));
        // Mutation envelopes follow the recorded GitHub payload schema. These are
        // local effects, not claims that a write was made to the recorded PR.
        let entry = json!({"id":"local-queue-entry","position":1,"state":"QUEUED"});
        if query.contains("enqueuePullRequest(") {
            return Ok(json!({"enqueuePullRequest":{"mergeQueueEntry":entry}}));
        }
        if query.contains("dequeuePullRequest(") {
            return Ok(json!({"dequeuePullRequest":{"mergeQueueEntry":entry}}));
        }
        let enabled = query.contains("enablePullRequestAutoMerge(");
        let operation = if enabled {
            "enablePullRequestAutoMerge"
        } else {
            "disablePullRequestAutoMerge"
        };
        Ok(
            json!({operation:{"pullRequest":{"id":self.pr["id"],"autoMergeRequest":if enabled {json!({"enabledAt":"2026-10-11T00:00:00Z","mergeMethod":"SQUASH"})} else {Value::Null}}}}),
        )
    }
    async fn rest(&self, method: &str, endpoint: &str, body: Value) -> Result<Value, String> {
        if method == "GET" {
            if self.rules_fail {
                return Err("GitHub rules unavailable".into());
            }
            if endpoint.ends_with("page=2") {
                if let Some(rules) = &self.later_rules {
                    return Ok(rules.clone());
                }
            }
            return Ok(self.rules.clone());
        }
        self.writes
            .lock()
            .unwrap()
            .push(json!({"method":method,"endpoint":endpoint,"body":body}));
        if method == "PUT" {
            return serde_json::from_str(include_str!(
                "../fixtures/github-review/merge-documented.json"
            ))
            .map_err(|e| e.to_string());
        }
        Ok(Value::Null)
    }
}

// Catches: direct merge on a branch whose active rules require a merge queue.
#[tokio::test]
async fn merge_on_queue_required_branch_enqueues() {
    let api = Stub::recorded();
    let response = merge_or_enqueue(&api, json!({}), 46254, "squash", api.head())
        .await
        .unwrap();
    assert_eq!(response["action"], "enqueued");
    assert_eq!(response["mergeQueueEntry"]["id"], "local-queue-entry");
    let writes = api.writes.lock().unwrap();
    assert_eq!(writes.len(), 1);
    assert!(
        writes[0]["query"]
            .as_str()
            .unwrap()
            .contains("enqueuePullRequest(")
    );
    assert_eq!(
        writes[0]["variables"]["input"]["expectedHeadOid"],
        api.head()
    );
}

// Catches: using the whole-run rerun endpoint instead of the failed-jobs endpoint.
#[tokio::test]
async fn rerun_targets_failed_jobs_only() {
    let api = Stub::recorded();
    let run: Value = serde_json::from_str(include_str!(
        "../fixtures/github-review/actions-run-recorded.json"
    ))
    .unwrap();
    let response = rerun_failed_jobs(&api, run["id"].as_u64().unwrap())
        .await
        .unwrap();
    assert_eq!(response["accepted"], true);
    let writes = api.writes.lock().unwrap();
    assert_eq!(writes[0]["method"], "POST");
    assert_eq!(
        writes[0]["endpoint"],
        "actions/runs/38050905243/rerun-failed-jobs"
    );
}

// Catches: empty request-changes reviews reaching GitHub and surfacing as silent success.
#[tokio::test]
async fn request_changes_requires_body() {
    let state = crate::state::tests_support::make_test_app_state();
    let error = crate::github::post_pr_review_impl(
        "/not-a-repo",
        7,
        "  ",
        Some("REQUEST_CHANGES"),
        &[],
        &state,
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Request changes requires a review body");
    let api = Stub::recorded();
    let draft = DraftReview {
        head_sha: api.head().into(),
        body: "\n".into(),
        event: "REQUEST_CHANGES".into(),
        comments: vec![],
    };
    assert_eq!(
        submit_review(&api, json!({}), 7, draft).await.unwrap_err(),
        "Request changes requires a review body"
    );
    assert!(api.writes.lock().unwrap().is_empty());
    assert!(validate_review_event("Explain the required fix", "REQUEST_CHANGES").is_ok());
}

// Catches: fallback to direct merge when effective policy cannot be established.
#[tokio::test]
async fn unavailable_branch_rules_prevent_any_merge_write() {
    let mut api = Stub::recorded();
    api.rules_fail = true;
    assert!(
        merge_or_enqueue(&api, json!({}), 7, "squash", api.head())
            .await
            .unwrap_err()
            .contains("rules unavailable")
    );
    assert!(api.writes.lock().unwrap().is_empty());
}

// Catches: missing sha protection on a direct merge when no queue is required.
#[tokio::test]
async fn unprotected_branch_merges_with_reviewed_sha() {
    let mut api = Stub::recorded();
    api.rules = json!([]);
    let response = merge_or_enqueue(&api, json!({}), 7, "squash", api.head())
        .await
        .unwrap();
    assert_eq!(response["action"], "merged");
    let writes = api.writes.lock().unwrap();
    assert_eq!(
        writes[0]["body"],
        json!({"merge_method":"squash","sha":api.head()})
    );
}

// Catches: enabling auto-merge without pinning the head, or disable leaving it enabled.
#[tokio::test]
async fn auto_merge_enable_is_pinned_and_disable_returns_confirmed_state() {
    let api = Stub::recorded();
    let response = change_action(
        &api,
        json!({}),
        PrAction::EnableAutoMerge,
        Some(api.head()),
        Some("squash"),
    )
    .await
    .unwrap();
    assert_eq!(
        response["pullRequest"]["autoMergeRequest"]["mergeMethod"],
        "SQUASH"
    );
    let response = change_action(&api, json!({}), PrAction::DisableAutoMerge, None, None)
        .await
        .unwrap();
    assert_eq!(response["pullRequest"]["autoMergeRequest"], Value::Null);
    assert_eq!(
        api.writes.lock().unwrap()[0]["variables"]["input"]["expectedHeadOid"],
        api.head()
    );
}

// Catches: stale-head writes to both queue and auto-merge operations.
#[tokio::test]
async fn moved_head_blocks_merge_queue_and_auto_merge() {
    let api = Stub::recorded();
    for action in [PrAction::Enqueue, PrAction::EnableAutoMerge] {
        assert!(
            change_action(&api, json!({}), action, Some("old-sha"), Some("squash"))
                .await
                .unwrap_err()
                .contains("PR head changed")
        );
    }
    assert!(
        merge_or_enqueue(&api, json!({}), 7, "squash", "old-sha")
            .await
            .is_err()
    );
    assert!(api.writes.lock().unwrap().is_empty());
}

// Catches: rendering a queue-required branch as direct-merge capable or losing dequeue.
#[tokio::test]
async fn queue_state_and_dequeue_are_server_backed() {
    let api = Stub::recorded();
    let state = action_state(&api, json!({})).await.unwrap();
    assert!(state.queue_required);
    assert_eq!(state.base_ref_name, "main");
    assert!(state.merge_queue_entry.is_none());
    let response = change_action(&api, json!({}), PrAction::Dequeue, None, None)
        .await
        .unwrap();
    assert_eq!(response["mergeQueueEntry"]["id"], "local-queue-entry");
    assert!(
        api.writes.lock().unwrap()[0]["query"]
            .as_str()
            .unwrap()
            .contains("dequeuePullRequest(")
    );
}

// Catches: checking only the first page and missing a later merge_queue rule.
#[tokio::test]
async fn queue_rule_on_later_page_still_enqueues() {
    let mut api = Stub::recorded();
    let ordinary = api
        .rules
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["type"] != "merge_queue")
        .unwrap()
        .clone();
    api.later_rules = Some(api.rules.clone());
    api.rules = json!(vec![ordinary; 100]);
    let response = merge_or_enqueue(&api, json!({}), 7, "squash", api.head())
        .await
        .unwrap();
    assert_eq!(response["action"], "enqueued");
}

mod critic {
    use super::super::*;
    use std::sync::Mutex;

    /// A base branch whose merge queue comes from classic branch protection:
    /// GitHub reports `isMergeQueueEnabled` on the PR, while the rulesets
    /// endpoint (`rules/branches/{branch}`) lists only ruleset rules.
    struct ClassicQueueApi {
        writes: Mutex<Vec<String>>,
    }

    impl PrApi for ClassicQueueApi {
        fn graphql(
            &self,
            query: &str,
            _variables: Value,
        ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
            if query.contains("enqueuePullRequest") {
                self.writes.lock().unwrap().push("enqueue".into());
                return std::future::ready(Ok(
                    json!({"enqueuePullRequest":{"mergeQueueEntry":{"id":"MQE_1","position":1,"state":"QUEUED"}}}),
                ));
            }
            let recorded: Value = serde_json::from_str(include_str!(
                "../fixtures/github-review/actions-pr-recorded.json"
            ))
            .unwrap();
            let pr = recorded["data"]["repository"]["pullRequests"]["nodes"][0].clone();
            std::future::ready(Ok(json!({"repository":{"pullRequest":pr}})))
        }

        fn rest(
            &self,
            method: &str,
            endpoint: &str,
            _body: Value,
        ) -> impl std::future::Future<Output = Result<Value, String>> + Send {
            if method == "GET" && endpoint.starts_with("rules/branches/") {
                return std::future::ready(Ok(json!([])));
            }
            self.writes
                .lock()
                .unwrap()
                .push(format!("{method} {endpoint}"));
            std::future::ready(Ok(
                json!({"merged":true,"sha":"0000000000000000000000000000000000000001"}),
            ))
        }
    }

    // catches: a merge queue required by classic branch protection, invisible to
    // the rulesets API, sent as a direct merge that GitHub refuses.
    #[tokio::test]
    async fn merge_enqueues_when_pr_reports_merge_queue_without_ruleset() {
        let api = ClassicQueueApi {
            writes: Mutex::new(Vec::new()),
        };
        let response = merge_or_enqueue(
            &api,
            json!({}),
            46254,
            "squash",
            "a2b8613ecf3dda46cc0b608efd69b8f309e8fbc0",
        )
        .await;
        assert_eq!(*api.writes.lock().unwrap(), vec!["enqueue".to_string()]);
        assert_eq!(response.unwrap()["action"], "enqueued");
    }
}
