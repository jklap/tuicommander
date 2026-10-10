//! Critic regressions for story 1679-1419.
use super::{AccountAction, execute};
use crate::credentials::Credential;
use serde_json::json;

fn action(value: serde_json::Value) -> AccountAction {
    serde_json::from_value(value).unwrap()
}

// Catches: removing the ambient github.com default deleting its registry record for
// good, so a later device-flow login authenticates GitHub while the account list
// never shows github.com again.
#[tokio::test]
async fn removed_github_default_is_listed_again_after_relogin() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let _ = execute(&state, action(json!({"action":"remove","id":"github.com"}))).await;

    crate::github_auth::save_github_oauth_token("critic-relogin-token").unwrap();
    *state.github.token.write() = Some("critic-relogin-token".into());
    let listed = execute(&state, action(json!({"action":"list","provider":"github"})))
        .await
        .unwrap();
    crate::credentials::delete(Credential::GithubOauthToken).unwrap();

    let row = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["id"] == "github.com")
        .expect("the github.com default must stay listed after a fresh login");
    assert_eq!(row["secret"]["status"], "set");
}
