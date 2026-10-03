//! Critic tests for story 1435: the secret-form inspection block must hold on
//! every dispatch path, not only the ones that reach `handle_mcp_tool_call_with_context`.
use super::mcp_transport::mcp_post;
use super::tests::test_state;
use crate::secrets::{Field, FieldKind, Form};
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::IntoResponse,
};

async fn tools_call_text(state: std::sync::Arc<crate::AppState>, name: &str) -> String {
    let response = mcp_post(
        State(state),
        ConnectInfo("127.0.0.1:12345".parse().unwrap()),
        HeaderMap::new(),
        Json(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": name, "arguments": {}}
        })),
    )
    .await
    .into_response();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    body["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn open_form(state: &crate::AppState) {
    let form = Form::request(
        vec![Field {
            name: "TOKEN".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "critic".into(),
    )
    .unwrap();
    state.secrets.open(form).unwrap();
}

/// Catches: a direct `tools/call` of an upstream-namespaced tool (maccontrol
/// capture_screen / capture_window / do / shell) skips the secret-form block
/// because the `__` branch of `mcp_post` proxies without consulting the store,
/// so an agent can screenshot or keystroke the private form.
#[tokio::test]
async fn direct_upstream_tools_call_is_refused_while_secret_form_is_open() {
    let state = test_state();
    open_form(&state);
    let text = tools_call_text(state, "maccontrol__capture_screen").await;
    assert!(
        text.contains("private secret form"),
        "upstream tool was not gated by the open secret form: {text}"
    );
}

/// Catches: gating only the native names while `call_tool` wrapping an
/// upstream name slips through (control for the case above).
#[tokio::test]
async fn call_tool_wrapped_upstream_is_refused_while_secret_form_is_open() {
    let state = test_state();
    open_form(&state);
    let response = super::mcp_transport::handle_mcp_tool_call(
        &state,
        "127.0.0.1:12345".parse().unwrap(),
        "call_tool",
        &serde_json::json!({"tool_name": "maccontrol__capture_screen", "arguments": {}}),
        None,
    )
    .await;
    assert!(
        response["error"]
            .as_str()
            .unwrap_or_default()
            .contains("private secret form"),
        "{response}"
    );
}
