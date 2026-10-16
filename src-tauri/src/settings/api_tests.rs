//! Transport cutover contracts; every file and credential slot belongs to this fixture.
use super::{api, policy::Caller};
use crate::remote_connection::{RemoteConnection, RemoteConnectionStore};
use serde_json::{Value, json};

fn action(id: &str, operation: &str) -> Value {
    json!({"action":"perform","target":{"kind":"machine"},"store":"connections",
        "operation":{"action":operation,"id":id,"password":"cutover-secret-canary"}})
}

// Catches: flattened typed workflow schemas accepting fields that execution silently ignores.
#[test]
fn typed_owner_envelopes_decode_and_reject_unknown_fields() {
    for request in [
        action("one", "set_password"),
        json!({"action":"perform","target":{"kind":"machine"},"store":"telegram",
            "operation":{"action":"add_chat","chat_id":"123"}}),
        json!({"action":"perform","target":{"kind":"machine"},"store":"upstreams",
            "operation":{"action":"clear","id":"one","header":null}}),
    ] {
        assert!(
            serde_json::from_value::<api::Action>(request.clone()).is_ok(),
            "{request}"
        );
        let mut extra = request.clone();
        extra["confirmed"] = json!(true);
        assert!(serde_json::from_value::<api::Action>(extra).is_err());
        let mut wrong_owner = request;
        wrong_owner["store"] = json!("prompts");
        assert!(serde_json::from_value::<api::Action>(wrong_owner).is_err());
    }
}

// Catches: local credential writes being reached through a foreign target or remote MCP origin.
#[tokio::test]
async fn connection_credential_dispatch_rejects_foreign_target_before_write() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let connection = RemoteConnection::new_ssh("fixture", "not-contacted.example", "fixture");
    RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
    let mut request = action(&connection.id, "set_password");
    request["target"] = json!({"kind":"repository","id":"remote-target"});
    let result = api::execute(&state, request, Caller::native()).await;
    assert_eq!(result["error"]["code"], "not_found", "{result}");
    let remote = Caller::mcp("192.0.2.1:1234".parse().unwrap(), "fixture".into(), true);
    let result = api::execute(&state, action(&connection.id, "set_password"), remote).await;
    assert_eq!(result["error"]["code"], "forbidden_origin", "{result}");
    assert!(!crate::remote_connection::connection_password_exists(&connection.id).unwrap());
    assert!(state.remote.snapshot().is_empty());
}

// Catches: credential rotation retaining an old runtime token or reconnecting a sibling target.
#[tokio::test]
async fn connection_credential_dispatch_reconciles_only_addressed_runtime() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let requested = std::sync::Arc::new(tokio::sync::Notify::new());
    let notify = requested.clone();
    // A controlled refusal proves the real reconnect path ran without an external service.
    let router = axum::Router::new().route(
        "/health",
        axum::routing::get(move || {
            let notify = notify.clone();
            async move {
                notify.notify_one();
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let a = RemoteConnection::new_direct("changed", &url, "fixture");
    let b = RemoteConnection::new_direct("sibling", &url, "fixture");
    RemoteConnectionStore::save(&state.data_dir, &[a.clone(), b.clone()]).unwrap();
    state
        .remote
        .force_connected_for_test(&a.id, &url, Some("retired-token"));
    state
        .remote
        .force_connected_for_test(&b.id, &url, Some("sibling-token"));
    let before = std::fs::read(state.data_dir.join("connections.json")).unwrap();
    let result = api::execute(&state, action(&a.id, "set_password"), Caller::native()).await;
    assert_eq!(result["password"]["status"], "set", "{result}");
    tokio::time::timeout(std::time::Duration::from_secs(30), requested.notified())
        .await
        .expect("credential reconciliation never reached the fixture daemon");
    let statuses = state.remote.snapshot();
    assert!(
        statuses
            .iter()
            .find(|entry| entry.id == a.id)
            .unwrap()
            .token
            .is_none()
    );
    assert_eq!(
        statuses
            .iter()
            .find(|entry| entry.id == b.id)
            .unwrap()
            .token
            .as_deref(),
        Some("sibling-token")
    );
    assert_eq!(
        std::fs::read(state.data_dir.join("connections.json")).unwrap(),
        before
    );
    let read = json!({"action":"get","target":{"kind":"machine"},"store":"connections",
        "paths":[format!("/{}/credential", a.id)]});
    let result = api::execute(&state, read, Caller::native()).await;
    assert_eq!(
        result["values"][format!("/{}/credential", a.id)]["stored"]["password"]["status"],
        "set",
        "{result}"
    );
    assert!(!result.to_string().contains("cutover-secret-canary"));
    crate::remote_runtime::teardown(&state, &a.id);
    crate::remote_runtime::teardown(&state, &b.id);
    server.abort();
}

// Catches: clearing a disconnected connection's credential implicitly connecting it.
#[tokio::test]
async fn connection_credential_dispatch_clear_keeps_disconnected_owner_down() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let connection = RemoteConnection::new_ssh("disconnected", "not-contacted.example", "fixture");
    RemoteConnectionStore::save(&state.data_dir, std::slice::from_ref(&connection)).unwrap();
    let result = api::execute(
        &state,
        action(&connection.id, "set_password"),
        Caller::native(),
    )
    .await;
    assert_eq!(result["password"]["status"], "set", "{result}");
    let request = json!({"action":"perform","target":{"kind":"machine"},"store":"connections",
        "operation":{"action":"clear_password","id":connection.id}});
    let result = api::execute(&state, request, Caller::native()).await;
    assert_eq!(result["password"]["status"], "unset", "{result}");
    assert!(state.remote.snapshot().is_empty());
}

// Catches: new typed stores being omitted from the persistent shared registry.
#[tokio::test]
async fn shared_registry_reaches_registration_and_ui_owners() {
    let (dir, _guard, state) = super::accounts_tests::isolated();
    std::fs::write(
        dir.path().join("keybindings.json"),
        r#"[{"action":"toggle-sidebar","key":"Cmd+B"}]"#,
    )
    .unwrap();
    for (store, path) in [
        ("repositories_registration", "/repoOrder"),
        ("keybindings", "/bindings/toggle-sidebar/key"),
        ("pane_layout", "/root"),
    ] {
        let result = api::execute(
            &state,
            json!({"action":"get","target":{"kind":"instance"},"store":store,"paths":[path]}),
            Caller::native(),
        )
        .await;
        assert!(result["revision"].is_string(), "{store}: {result}");
        assert!(result["values"][path].is_object(), "{store}: {result}");
    }
}

// Catches: presence-only enum strings being treated as nonempty credentials (unset becomes set).
#[test]
fn owner_presence_projection_preserves_unset_and_unavailable() {
    let catalogue = super::native::native_catalogue().unwrap();
    let path = "/accounts/fixture/credential";
    for status in ["set", "unset", "unavailable"] {
        let result = super::service::snapshot(
            &catalogue,
            "github_accounts",
            vec![path.into()],
            json!({"accounts":[{"id":"fixture","credential":status}]}),
        );
        if status == "unavailable" {
            assert_eq!(result.unwrap_err().code, "secret_unreadable");
        } else {
            assert_eq!(
                result.unwrap().values[path].stored,
                json!({"status":status})
            );
        }
    }
}

// Catches: the shared facade reading core account wire fields against the GitHub-specific schema.
#[tokio::test]
async fn github_catalogue_paths_use_owner_projection() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let account = crate::github_account::GitHubAccount::ghe_pat(
        crate::github_account::GitHubHost::new("fixture.example").unwrap(),
        None,
    );
    crate::github_account::add_account_record(account.clone(), None).unwrap();
    let host = format!("/accounts/{}/host/host", account.id);
    let credential = format!("/accounts/{}/credential", account.id);
    let result = api::execute(
        &state,
        json!({"action":"get","target":{"kind":"machine"},
        "store":"github_accounts","paths":[host,credential]}),
        Caller::native(),
    )
    .await;
    assert_eq!(
        result["values"][&host]["stored"], "fixture.example",
        "{result}"
    );
    assert_eq!(
        result["values"][&credential]["stored"],
        json!({"status":"unset"}),
        "{result}"
    );
}
