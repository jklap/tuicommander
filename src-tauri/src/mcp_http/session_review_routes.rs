use axum::Json;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};

use super::types::*;
use super::{json_result, validate_repo_path};

pub(super) async fn list_sessions_http(Query(q): Query<SessionListQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(
        crate::session_review::list_review_sessions(q.path, q.limit, q.include_counts, None).await,
    )
}

pub(super) async fn get_review_http(Query(q): Query<SessionReviewQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(
        crate::session_review::get_session_review(q.path, q.session_id, q.include_subagents, None)
            .await,
    )
}

pub(super) async fn revert_step_http(Json(body): Json<RevertStepRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    json_result(
        crate::session_review::revert_session_step(
            body.path,
            body.session_id,
            body.tool_use_id,
            body.dry_run,
            None,
        )
        .await,
    )
}

pub(super) async fn revert_file_http(Json(body): Json<RevertFileRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    json_result(
        crate::session_review::revert_file_to_session_start(
            body.path,
            body.session_id,
            body.abs_path,
            body.force,
            body.dry_run,
            None,
        )
        .await,
    )
}
