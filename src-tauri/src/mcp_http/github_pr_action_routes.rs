use super::{json_result, validate_repo_path};
use crate::{
    github_pr_actions::{self as actions, PrAction},
    github_pr_api::RepoApi,
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PrQuery {
    repo_path: String,
    pr_number: i64,
    /// `owner/name` of the PR's base repository (disambiguates a number shared by two watched repos).
    base_repo: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AutoMergeBody {
    repo_path: String,
    pr_number: i64,
    /// `owner/name` of the PR's base repository (disambiguates a number shared by two watched repos).
    base_repo: Option<String>,
    enabled: bool,
    merge_method: Option<String>,
    expected_head_sha: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueueBody {
    repo_path: String,
    pr_number: i64,
    /// `owner/name` of the PR's base repository (disambiguates a number shared by two watched repos).
    base_repo: Option<String>,
    enqueued: bool,
    expected_head_sha: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MergeBody {
    repo_path: String,
    pr_number: i64,
    /// `owner/name` of the PR's base repository (disambiguates a number shared by two watched repos).
    base_repo: Option<String>,
    merge_method: String,
    expected_head_sha: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RerunBody {
    repo_path: String,
    run_id: u64,
}

pub(super) async fn state(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PrQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api =
            RepoApi::resolve_for_pr(&state, &q.repo_path, q.pr_number, q.base_repo.as_deref())
                .await?;
        actions::action_state(&api, api.variables(q.pr_number)).await
    }
    .await;
    json_result(result)
}
pub(super) async fn auto_merge(
    State(state): State<Arc<AppState>>,
    Json(q): Json<AutoMergeBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let action = if q.enabled {
        PrAction::EnableAutoMerge
    } else {
        PrAction::DisableAutoMerge
    };
    let result = async {
        let api =
            RepoApi::resolve_for_pr(&state, &q.repo_path, q.pr_number, q.base_repo.as_deref())
                .await?;
        actions::change_action(
            &api,
            api.variables(q.pr_number),
            action,
            q.expected_head_sha.as_deref(),
            q.merge_method.as_deref(),
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn queue(
    State(state): State<Arc<AppState>>,
    Json(q): Json<QueueBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let action = if q.enqueued {
        PrAction::Enqueue
    } else {
        PrAction::Dequeue
    };
    let result = async {
        let api =
            RepoApi::resolve_for_pr(&state, &q.repo_path, q.pr_number, q.base_repo.as_deref())
                .await?;
        actions::change_action(
            &api,
            api.variables(q.pr_number),
            action,
            q.expected_head_sha.as_deref(),
            None,
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn merge(
    State(state): State<Arc<AppState>>,
    Json(q): Json<MergeBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api =
            RepoApi::resolve_for_pr(&state, &q.repo_path, q.pr_number, q.base_repo.as_deref())
                .await?;
        actions::merge_or_enqueue(
            &api,
            api.variables(q.pr_number),
            q.pr_number,
            &q.merge_method,
            &q.expected_head_sha,
        )
        .await
    }
    .await;
    json_result(result)
}
pub(super) async fn rerun(
    State(state): State<Arc<AppState>>,
    Json(q): Json<RerunBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(&state, &q.repo_path).await?;
        actions::rerun_failed_jobs(&api, q.run_id).await
    }
    .await;
    json_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: baseRepo dropped from (or required by) the camelCase action bodies/queries.
    #[test]
    fn base_repo_is_accepted_and_optional_on_the_action_requests() {
        let body = serde_json::json!({"repoPath": "/r", "prNumber": 7, "enabled": true,
            "enqueued": true, "mergeMethod": "squash", "expectedHeadSha": "abc"});
        let mut hinted = body.clone();
        hinted["baseRepo"] = "org/hello".into();
        assert_eq!(
            serde_json::from_value::<AutoMergeBody>(hinted.clone())
                .unwrap()
                .base_repo
                .as_deref(),
            Some("org/hello")
        );
        assert_eq!(
            serde_json::from_value::<QueueBody>(hinted.clone())
                .unwrap()
                .base_repo
                .as_deref(),
            Some("org/hello")
        );
        assert_eq!(
            serde_json::from_value::<MergeBody>(hinted)
                .unwrap()
                .base_repo
                .as_deref(),
            Some("org/hello")
        );
        assert_eq!(
            serde_json::from_value::<AutoMergeBody>(body.clone())
                .unwrap()
                .base_repo,
            None
        );
        assert_eq!(
            serde_json::from_value::<MergeBody>(body).unwrap().base_repo,
            None
        );
        let uri: axum::http::Uri = "/x?repoPath=%2Fr&prNumber=7&baseRepo=org%2Fhello"
            .parse()
            .unwrap();
        let q = Query::<PrQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(q.0.base_repo.as_deref(), Some("org/hello"));
        let uri: axum::http::Uri = "/x?repoPath=%2Fr&prNumber=7".parse().unwrap();
        assert_eq!(
            Query::<PrQuery>::try_from_uri(&uri).unwrap().0.base_repo,
            None
        );
    }
}
