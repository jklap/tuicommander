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

// Every handler in this file hardcodes `claude_config_dir: None` when calling
// into `session_review` — there is no query/body field on any of the four
// request types above that could carry one. A browser/remote client can
// never point session review at a non-default `CLAUDE_CONFIG_DIR` the way an
// MCP client can via its own run config. Known gap, not exercised below;
// see plans/enchanted-puzzling-teacup.md's Phase 2 notes on threading a
// per-request config dir through HTTP if this needs closing.

#[cfg(test)]
mod tests {
    use super::*;

    async fn json_body(response: Response) -> serde_json::Value {
        let (_, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    /// Every handler here calls `crate::session_review::*` with
    /// `claude_config_dir: None`, which resolves to `$HOME/.claude/projects`
    /// (see `agent_session::claude_projects_dir`) — there is no way to point
    /// it at an isolated test directory through the HTTP layer itself. This
    /// guard temporarily redirects `$HOME` so a synthetic transcript under
    /// `<fake home>/.claude/projects/<slug>/` is exactly what the real
    /// fallback path would find, restoring the original value on drop.
    struct HomeOverride {
        original: Option<String>,
    }

    impl HomeOverride {
        fn set(path: &std::path::Path) -> Self {
            let original = std::env::var("HOME").ok();
            unsafe { std::env::set_var("HOME", path) };
            HomeOverride { original }
        }
    }

    impl Drop for HomeOverride {
        fn drop(&mut self) {
            match &self.original {
                Some(v) => unsafe { std::env::set_var("HOME", v) },
                None => unsafe { std::env::remove_var("HOME") },
            }
        }
    }

    /// Writes a single-edit transcript under `<home>/.claude/projects/<slug>/`
    /// and returns its session id.
    fn seed_transcript_under_home(
        home: &std::path::Path,
        repo_path: &str,
        abs_file: &str,
    ) -> String {
        let project_dir_str = crate::agent_session::claude_project_dir(repo_path.to_string(), None)
            .expect("claude_project_dir");
        let project_dir = std::path::PathBuf::from(project_dir_str);
        std::fs::create_dir_all(&project_dir).unwrap();
        let session_id = uuid::Uuid::new_v4().to_string();
        let record = serde_json::json!({
            "type": "user",
            "timestamp": "2026-01-01T00:00:00.000Z",
            "cwd": repo_path,
            "isSidechain": false,
            "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_route_test"}]},
            "toolUseResult": {
                "filePath": abs_file, "oldString": "a\n", "newString": "A\n",
                "originalFile": serde_json::Value::Null, "replaceAll": false,
                "structuredPatch": [], "userModified": false,
            },
        });
        std::fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            record.to_string() + "\n",
        )
        .unwrap();
        let _ = home;
        session_id
    }

    #[tokio::test]
    async fn list_sessions_http_and_get_review_http_round_trip_via_home_fallback() {
        let home = tempfile::tempdir().unwrap();
        let _guard = HomeOverride::set(home.path());

        let repo = crate::state::tests_support::create_temp_git_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let abs_file = repo.path().join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs_file, "a\n").unwrap();
        let session_id = seed_transcript_under_home(home.path(), &repo_path, &abs_file);

        let list_response = list_sessions_http(Query(SessionListQuery {
            path: repo_path.clone(),
            limit: None,
            include_counts: None,
        }))
        .await;
        assert_eq!(list_response.status(), axum::http::StatusCode::OK);
        let list_json = json_body(list_response).await;
        let sessions = list_json.as_array().expect("array body");
        assert!(
            sessions
                .iter()
                .any(|s| s["session_id"] == serde_json::Value::String(session_id.clone())),
            "list_sessions_http should find the seeded transcript under the \
             $HOME fallback, got: {list_json}"
        );

        let review_response = get_review_http(Query(SessionReviewQuery {
            path: repo_path,
            session_id,
            include_subagents: None,
        }))
        .await;
        assert_eq!(review_response.status(), axum::http::StatusCode::OK);
        let review_json = json_body(review_response).await;
        assert_eq!(
            review_json["steps"].as_array().map(|a| a.len()),
            Some(1),
            "get_review_http should parse the one seeded edit, got: {review_json}"
        );
    }

    #[tokio::test]
    async fn list_sessions_http_rejects_a_relative_path() {
        let response = list_sessions_http(Query(SessionListQuery {
            path: "relative/path".to_string(),
            limit: None,
            include_counts: None,
        }))
        .await;
        assert_ne!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn get_review_http_rejects_a_relative_path() {
        let response = get_review_http(Query(SessionReviewQuery {
            path: "relative/path".to_string(),
            session_id: "6d1d4349-dbe2-4a43-8f2e-9b1c3a4d5e6f".to_string(),
            include_subagents: None,
        }))
        .await;
        assert_ne!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn revert_step_http_rejects_a_relative_path() {
        let response = revert_step_http(Json(RevertStepRequest {
            path: "relative/path".to_string(),
            session_id: "6d1d4349-dbe2-4a43-8f2e-9b1c3a4d5e6f".to_string(),
            tool_use_id: "toolu_x".to_string(),
            dry_run: Some(true),
        }))
        .await;
        assert_ne!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn revert_file_http_rejects_a_relative_path() {
        let response = revert_file_http(Json(RevertFileRequest {
            path: "relative/path".to_string(),
            session_id: "6d1d4349-dbe2-4a43-8f2e-9b1c3a4d5e6f".to_string(),
            abs_path: "/some/file.ts".to_string(),
            force: None,
            dry_run: Some(true),
        }))
        .await;
        assert_ne!(response.status(), axum::http::StatusCode::OK);
    }
}
