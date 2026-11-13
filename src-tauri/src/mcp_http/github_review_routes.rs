//! IPC-equivalent line review API. Repository resolution is shared with desktop.
use super::{json_result, validate_repo_path};
use crate::{github_pr_api::RepoApi, github_review_threads as reviews, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadQuery {
    repo_path: String,
    pr_number: i64,
    base_repo: Option<String>,
    after: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OpenThreadsQuery {
    repo_path: String,
    pr_number: i64,
    base_repo: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommentsQuery {
    repo_path: String,
    thread_id: String,
    after: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResolveBody {
    repo_path: String,
    thread_id: String,
    resolved: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReplyBody {
    repo_path: String,
    thread_id: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DraftBody {
    repo_path: String,
    pr_number: i64,
    base_repo: Option<String>,
    draft: reviews::DraftReview,
}

pub(super) async fn threads(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ThreadQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(
        reviews::get_pr_line_threads_targeted(
            &q.repo_path,
            q.pr_number,
            q.base_repo.as_deref(),
            q.after,
            &state,
        )
        .await,
    )
}
/// The `{pr_open_threads}` prompt variable (IPC `get_pr_open_threads`).
pub(super) async fn open_threads(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OpenThreadsQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(
        crate::pr_open_threads::get_pr_open_threads_targeted(
            &q.repo_path,
            q.pr_number,
            q.base_repo.as_deref(),
            &state,
        )
        .await,
    )
}
pub(super) async fn comments(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CommentsQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(&state, &q.repo_path).await?;
        reviews::read_comments(&api, &q.thread_id, q.after).await
    }
    .await;
    json_result(result)
}
pub(super) async fn resolve(
    State(state): State<Arc<AppState>>,
    Json(q): Json<ResolveBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(&state, &q.repo_path).await?;
        reviews::resolve_thread(&api, &q.thread_id, q.resolved).await
    }
    .await;
    json_result(result)
}
pub(super) async fn reply(
    State(state): State<Arc<AppState>>,
    Json(q): Json<ReplyBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api = RepoApi::resolve(&state, &q.repo_path).await?;
        reviews::reply_thread(&api, &q.thread_id, &q.body).await
    }
    .await;
    json_result(result)
}
pub(super) async fn submit(
    State(state): State<Arc<AppState>>,
    Json(q): Json<DraftBody>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let result = async {
        let api =
            RepoApi::resolve_for_pr(&state, &q.repo_path, q.pr_number, q.base_repo.as_deref())
                .await?;
        reviews::submit_review(&api, api.variables(q.pr_number), q.pr_number, q.draft).await
    }
    .await;
    json_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: baseRepo dropped from (or required by) the review-thread requests.
    #[test]
    fn base_repo_is_accepted_and_optional_on_the_review_requests() {
        let uri = |q: &str| q.parse::<axum::http::Uri>().unwrap();
        let t = Query::<ThreadQuery>::try_from_uri(&uri(
            "/x?repoPath=%2Fr&prNumber=7&baseRepo=org%2Fhello&after=c",
        ))
        .unwrap();
        assert_eq!(
            (t.0.base_repo.as_deref(), t.0.after.as_deref()),
            (Some("org/hello"), Some("c"))
        );
        let o = Query::<OpenThreadsQuery>::try_from_uri(&uri(
            "/x?repoPath=%2Fr&prNumber=7&baseRepo=org%2Fhello",
        ))
        .unwrap();
        assert_eq!(o.0.base_repo.as_deref(), Some("org/hello"));
        assert_eq!(
            Query::<OpenThreadsQuery>::try_from_uri(&uri("/x?repoPath=%2Fr&prNumber=7"))
                .unwrap()
                .0
                .base_repo,
            None
        );
        let d: DraftBody = serde_json::from_value(serde_json::json!({
            "repoPath": "/r", "prNumber": 7, "baseRepo": "org/hello",
            "draft": {"headSha": "abc", "body": "", "event": "COMMENT", "comments": []}
        }))
        .unwrap();
        assert_eq!(d.base_repo.as_deref(), Some("org/hello"));
    }
}
