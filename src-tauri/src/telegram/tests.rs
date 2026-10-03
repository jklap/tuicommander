use super::*;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, Uri},
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const PEER: &str = "11111111-1111-4111-8111-111111111111";

fn scratch() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("telegram-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap()
}
fn write_private(path: &std::path::Path, text: &str) {
    std::fs::write(path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn setup() -> (tempfile::TempDir, Paths) {
    let dir = scratch();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let paths = Paths::new(dir.path().into());
    write_private(
        &paths.file("config.json"),
        &json!({"enabled":true,"bot_alias":"test-bot","target_tuic_session":PEER}).to_string(),
    );
    write_private(&paths.file("allowed_chat_ids"), "1111111\n");
    write_private(&paths.file("bot.token"), "111:fake-one\n");
    (dir, paths)
}
fn public_update() -> Value {
    serde_json::from_str(include_str!("fixtures/public-text-update.json")).unwrap()
}

#[derive(Default)]
struct ServerState {
    responses: VecDeque<(StatusCode, Value)>,
    requests: Vec<(String, Value)>,
}
struct FakeServer {
    address: std::net::SocketAddr,
    state: Arc<Mutex<ServerState>>,
    task: tokio::task::JoinHandle<()>,
}
impl FakeServer {
    async fn start(responses: Vec<(StatusCode, Value)>) -> Self {
        let state = Arc::new(Mutex::new(ServerState {
            responses: responses.into(),
            requests: vec![],
        }));
        async fn respond(
            State(state): State<Arc<Mutex<ServerState>>>,
            uri: Uri,
            Json(body): Json<Value>,
        ) -> (StatusCode, Json<Value>) {
            let mut state = state.lock().unwrap();
            state.requests.push((uri.path().into(), body));
            let (status, value) = state
                .responses
                .pop_front()
                .unwrap_or((StatusCode::OK, json!({"ok":true,"result":[]})));
            (status, Json(value))
        }
        let router = Router::new().fallback(respond).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            address,
            state,
            task,
        }
    }
    fn requests(&self) -> Vec<(String, Value)> {
        self.state.lock().unwrap().requests.clone()
    }
}
impl Drop for FakeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// Catches: missing opt-in still opens credentials/locks, or malformed config grants a target.
#[test]
fn missing_disabled_and_invalid_config_never_enable_the_adapter() {
    let dir = scratch();
    let paths = Paths::new(dir.path().into());
    assert!(Config::load(&paths).unwrap().is_none());
    write_private(
        &paths.file("config.json"),
        &json!({"enabled":false,"bot_alias":"","target_tuic_session":""}).to_string(),
    );
    assert!(Config::load(&paths).unwrap().is_none());
    assert!(!paths.file("owner.lock").exists());
    write_private(
        &paths.file("config.json"),
        &json!({"enabled":true,"bot_alias":"x","target_tuic_session":"pe-3"}).to_string(),
    );
    assert!(matches!(Config::load(&paths), Err(Error::Config)));
}

// Catches: a second instance becomes poll owner, or an owner lock never releases after exit.
#[test]
fn sole_owner_lock_blocks_second_instance_until_drop() {
    let (_dir, paths) = setup();
    let first = Owner::acquire(&paths).unwrap();
    assert!(matches!(Owner::acquire(&paths), Err(Error::AlreadyOwned)));
    drop(first);
    assert!(Owner::acquire(&paths).is_ok());
}

// Catches: a cached token/allowlist survives rotation or revocation and sends unauthorized work.
#[tokio::test]
async fn each_request_reads_rotated_token_and_current_allowlist() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![]).await;
    let api = BotApi::loopback(paths.clone(), server.address);
    api.get_updates(0, 0).await.unwrap();
    write_private(&paths.file("bot.token"), "222:fake-two\n");
    api.get_updates(1, 0).await.unwrap();
    write_private(&paths.file("allowed_chat_ids"), "");
    assert!(matches!(api.get_updates(2, 0).await, Err(Error::Config)));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, "/bot111:fake-one/getUpdates");
    assert_eq!(requests[1].0, "/bot222:fake-two/getUpdates");
}

// Catches: Telegram's error description leaks a URL token, or status faults become successful polls.
#[tokio::test]
async fn bot_errors_are_typed_without_echoing_external_text() {
    let (_dir, paths) = setup();
    let server = FakeServer::start(vec![
        (
            StatusCode::UNAUTHORIZED,
            json!({"description":"111:fake-one secret"}),
        ),
        (
            StatusCode::CONFLICT,
            json!({"description":"111:fake-one secret"}),
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({"ok":false,"error_code":429,"parameters":{"retry_after":7}}),
        ),
        (
            StatusCode::OK,
            json!({"ok":false,"error_code":401,"description":"111:fake-one secret"}),
        ),
        (
            StatusCode::OK,
            json!({"ok":true,"result":[public_update()]}),
        ),
    ])
    .await;
    let api = BotApi::loopback(paths, server.address);
    for expected in [
        Error::Unauthorized,
        Error::Conflict,
        Error::RateLimited(7),
        Error::Unauthorized,
    ] {
        let error = api.get_updates(0, 0).await.unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error} {error:?}").contains("fake-one"));
    }
    let updates = api.get_updates(0, 0).await.unwrap();
    assert_eq!(updates[0]["message"]["text"], "/start");
}

// Catches: redirects expose the token to another origin, or transport errors print its request URL.
#[tokio::test]
async fn redirects_and_connection_failures_do_not_expose_credentials() {
    let (_dir, paths) = setup();
    let mut server = FakeServer::start(vec![(
        StatusCode::TEMPORARY_REDIRECT,
        json!({"description":"111:fake-one"}),
    )])
    .await;
    let api = BotApi::loopback(paths.clone(), server.address);
    assert_eq!(
        api.get_updates(0, 0).await.unwrap_err(),
        Error::Rejected(307)
    );
    let address = server.address;
    server.task.abort();
    assert!((&mut server.task).await.unwrap_err().is_cancelled());
    // A closed listener is a deterministic transport fault, with no arbitrary wall-clock deadline.
    drop(server);
    tokio::task::yield_now().await;
    let error = BotApi::loopback(paths, address)
        .get_updates(0, 0)
        .await
        .unwrap_err();
    assert_eq!(error, Error::Transport);
    assert!(!format!("{error} {error:?}").contains("fake-one"));
}

#[cfg(unix)]
// Catches: symlink/group-readable credentials or a non-regular file are accepted as private input.
#[test]
fn private_file_validation_rejects_links_permissions_and_malformed_allowlist() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_dir, paths) = setup();
    std::fs::set_permissions(
        paths.file("bot.token"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(matches!(paths.token(), Err(Error::PrivateFile)));
    std::fs::remove_file(paths.file("bot.token")).unwrap();
    symlink(paths.file("config.json"), paths.file("bot.token")).unwrap();
    assert!(matches!(paths.token(), Err(Error::PrivateFile)));
    write_private(&paths.file("allowed_chat_ids"), "1111111\nnot-an-id\n");
    assert!(matches!(paths.allowlist(), Err(Error::Config)));
    std::fs::remove_file(paths.file("allowed_chat_ids")).unwrap();
    std::fs::create_dir(paths.file("allowed_chat_ids")).unwrap();
    assert!(matches!(paths.allowlist(), Err(Error::PrivateFile)));
}
