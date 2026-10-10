//! Account IPC/HTTP parity: one write-only request and one presence-only reply.
use super::guards::{Authenticated, require_local_or_auth};
use crate::accounts::AccountAction;
use crate::state::AppState;
use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use std::net::SocketAddr;
use std::sync::Arc;

pub(super) async fn action(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(request): Json<AccountAction>,
) -> Response {
    if let Err(response) = require_local_or_auth(&addr, auth.is_some()) {
        return response.into_response();
    }
    super::json_result(crate::accounts::execute(&state, request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    // Catches: account mutation being reachable by unauthenticated remote callers.
    #[tokio::test]
    async fn account_action_requires_local_or_authenticated_origin() {
        let state = super::super::tests::test_state();
        let request = Request::builder()
            .method("POST")
            .uri("/accounts/action")
            .header("host", "127.0.0.1:9876")
            .header("content-type", "application/json")
            .extension(ConnectInfo(
                "192.0.2.10:1234".parse::<SocketAddr>().unwrap(),
            ))
            .body(Body::from(r#"{"action":"list"}"#))
            .unwrap();
        let response = super::super::build_router(state, false, true)
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("requires loopback access or authentication")
        );
    }

    // Catches: desktop and headless routers exposing different account request/reply shapes.
    #[tokio::test]
    async fn account_action_lists_presence_only_on_both_routers() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _config = crate::config::set_config_dir_override(dir.path().to_path_buf());
        crate::github_account::add_account_record(
            crate::github_account::GitHubAccount::github_com_named("a1-http"),
            Some("a1-http-secret"),
        )
        .unwrap();
        let state = super::super::tests::test_state();
        *state.session_token.write() = "a1-route-session".into();
        let routers = [
            super::super::build_router(state.clone(), false, true),
            super::super::build_remote_router(state),
        ];
        for router in routers {
            let request = Request::builder()
                .method("POST")
                .uri("/accounts/action")
                .header("host", "127.0.0.1:9876")
                .header("content-type", "application/json")
                .header("cookie", "tui-session=a1-route-session")
                .extension(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()))
                .body(Body::from(r#"{"action":"list","provider":"github"}"#))
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let accounts: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let row = accounts
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "a1-http")
                .unwrap();
            assert_eq!(row["secret"]["status"], "set");
            assert!(
                !String::from_utf8(bytes.to_vec())
                    .unwrap()
                    .contains("a1-http-secret")
            );
        }
        crate::credentials::delete(crate::credentials::Credential::AccountToken("a1-http"))
            .unwrap();
    }
}
