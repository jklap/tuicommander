//! Published facade integration; external authentication is never simulated as successful.
use super::{api, policy::Caller};
use serde_json::json;

// Catches: the published GitHub envelope bypassing cloud policy or dropping owner readback.
#[tokio::test]
async fn startup_github_workflows_preserve_cloud_policy_and_readback() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let account = crate::github_account::GitHubAccount::github_com_named("startup-viewer");
    crate::github_account::add_account_record(account, Some("startup-private-canary")).unwrap();
    let reply = api::execute(
        &state,
        json!({
            "action":"perform","target":{"kind":"machine"},"store":"github_accounts",
            "operation":{"action":"set_label","id":"startup-viewer","label":"Startup account"}
        }),
        Caller::native(),
    )
    .await;
    assert_eq!(reply["persisted"], true, "{reply}");
    let account = reply["snapshot"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "startup-viewer")
        .unwrap();
    assert_eq!(account["label"], "Startup account");
    assert_eq!(account["secret"], json!({"status":"set"}));
    let denied = api::execute(&state, json!({
        "action":"perform","target":{"kind":"machine"},"store":"github_accounts",
        "operation":{"action":"add_enterprise","host":"https://github.com","label":"Must not exist","token":"startup-pat-canary"}
    }), Caller::native()).await;
    assert_eq!(denied["error"]["code"], "auth_required", "{denied}");
    for action in ["cloud_login_status", "complete_cloud_login"] {
        let missing = api::execute(
            &state,
            json!({
                "action":"perform","target":{"kind":"machine"},"store":"github_accounts",
                "operation":{"action":action,"flow_id":"unknown-startup-flow"}
            }),
            Caller::native(),
        )
        .await;
        assert_eq!(missing["error"]["code"], "not_found", "{missing}");
        assert_eq!(missing["error"]["persisted"], false);
    }
    assert!(!reply.to_string().contains("startup-private-canary"));
    assert!(!denied.to_string().contains("startup-pat-canary"));
}

// Catches: facade replay accepting a refused proposal or an approved-but-expired proposal.
#[tokio::test(start_paused = true)]
async fn startup_refusal_and_expiry_cases() {
    let (dir, _guard, state) = super::accounts_tests::isolated();
    let read = api::execute(&state, json!({
        "action":"get","target":{"kind":"machine"},"store":"app","paths":["/services/auth/username"]
    }), Caller::native()).await;
    assert!(read["revision"].is_string(), "{read}");
    let before = std::fs::read(dir.path().join("config.json")).ok();
    for expired in [false, true] {
        let mut request = json!({
            "action":"set","target":{"kind":"machine"},"store":"app",
            "path":"/services/auth/username","value":"startup-unapproved-user","expectedRevision":read["revision"]
        });
        let proposal = api::execute(&state, request.clone(), Caller::native()).await;
        assert_eq!(
            proposal["error"]["code"], "confirmation_required",
            "{proposal}"
        );
        let id = proposal["error"]["proposalId"].as_str().unwrap().to_owned();
        let answer =
            super::policy_tests::answer(state.clone(), Caller::native(), id.clone(), expired).await;
        assert_eq!(answer["confirmed"], expired);
        if expired {
            tokio::time::advance(std::time::Duration::from_secs(301)).await;
        }
        request["approval"] = json!(id);
        let refused = api::execute(&state, request, Caller::native()).await;
        assert_eq!(refused["error"]["code"], "confirmation_denied", "{refused}");
        assert_eq!(refused["error"]["persisted"], false);
        assert_eq!(std::fs::read(dir.path().join("config.json")).ok(), before);
    }
}

// Catches: startup retrying a stale owner revision instead of preserving the first edit.
#[tokio::test]
async fn startup_conflict_preserves_the_committed_owner_value() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let get =
        json!({"action":"get","target":{"kind":"machine"},"store":"app","paths":["/font_size"]});
    let initial = api::execute(&state, get.clone(), Caller::native()).await;
    let set = |value| json!({"action":"set","target":{"kind":"machine"},"store":"app","path":"/font_size","value":value,"expectedRevision":initial["revision"]});
    let current = initial["values"]["/font_size"]["stored"].as_u64().unwrap();
    let first = if current == 23 { 24 } else { 23 };
    let second = if first == 23 { 24 } else { 23 };
    let committed = api::execute(&state, set(first), Caller::native()).await;
    assert_eq!(committed["persisted"], true, "{committed}");
    let conflict = api::execute(&state, set(second), Caller::native()).await;
    assert_eq!(conflict["error"]["code"], "conflict", "{conflict}");
    assert_eq!(conflict["error"]["persisted"], false);
    let read = api::execute(&state, get, Caller::native()).await;
    assert_eq!(read["values"]["/font_size"]["stored"], first);
}

// Catches: facade launch/profile routing diverging from the captured Ego owner contract.
#[cfg(unix)]
#[tokio::test]
async fn startup_ego_settings_use_configured_owner_and_readback() {
    let (_dir, _guard, _state) = super::accounts_tests::isolated();
    let fixture = super::ego_fixture::fixture();
    let base = super::ego_fixture::disk(&fixture, "base");
    let sibling = super::ego_fixture::disk(&fixture, "beta");
    let described = api::execute(
        &fixture.state,
        json!({"action":"describe","store":"ego"}),
        Caller::native(),
    )
    .await;
    let model = described["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["path"] == "/model")
        .unwrap();
    let get = json!({"action":"get","target":{"kind":"profile","id":"alpha"},"store":model["store"],"paths":[model["path"]]});
    let before = api::execute(&fixture.state, get.clone(), Caller::native()).await;
    assert!(before["revision"].is_string(), "{before}");
    let committed = api::execute(&fixture.state, json!({
        "action":"set","target":{"kind":"profile","id":"alpha"},"store":model["store"],
        "path":model["path"],"value":super::ego_fixture::expected_model(),"expectedRevision":before["revision"]
    }), Caller::native()).await;
    assert_eq!(committed["persisted"], true, "{committed}");
    assert_eq!(committed["restart"], json!(["next_session"]));
    let after = api::execute(&fixture.state, get, Caller::native()).await;
    assert_eq!(after["revision"], committed["revision"]);
    assert_eq!(
        after["values"]["/model"]["stored"],
        super::ego_fixture::expected_model()
    );
    assert_eq!(super::ego_fixture::disk(&fixture, "base"), base);
    assert_eq!(super::ego_fixture::disk(&fixture, "beta"), sibling);
    assert!(
        super::ego_fixture::calls(&fixture)
            .iter()
            .all(|call| call["cwd"] == fixture.dir.path().to_string_lossy().as_ref())
    );
}
