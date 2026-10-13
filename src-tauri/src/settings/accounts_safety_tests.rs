use super::{
    accounts::{GithubAction, GithubSettings},
    accounts_tests::isolated,
};
use crate::{credentials::Credential, github_account::*};

// Catches: removal deleting an account/token before discovering corrupt bindings, then overwriting them.
#[tokio::test]
async fn corrupt_binding_metadata_refuses_removal_without_deleting_credentials() {
    let (dir, _guard, state) = isolated();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("corrupt.example").unwrap(), None);
    add_account_record(account.clone(), Some("corrupt-token-canary")).unwrap();
    let path = dir.path().join("github_bindings.json");
    std::fs::write(&path, "corrupt binding bytes").unwrap();
    let settings = GithubSettings::new(state);
    let error = settings
        .perform(GithubAction::RemoveAccount {
            id: account.id.clone(),
        })
        .await
        .unwrap_err();
    assert!(!error.persisted);
    assert!(
        settings
            .perform(GithubAction::RemoveAccount {
                id: account.id.clone()
            })
            .await
            .is_err()
    );
    assert!(GitHubAccountRegistry::load().get(&account.id).is_some());
    assert_eq!(
        crate::credentials::get(Credential::AccountToken(&account.id))
            .unwrap()
            .as_deref(),
        Some("corrupt-token-canary")
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "corrupt binding bytes"
    );
}

// Catches: a published action schema accepting undeclared secret fields or lacking write-only markers.
#[test]
fn typed_action_schema_rejects_unknown_fields_and_marks_tokens_write_only() {
    let schema = GithubSettings::action_schema();
    let variants = schema["oneOf"].as_array().unwrap();
    let secret_variants: Vec<_> = variants
        .iter()
        .filter(|variant| variant["properties"].get("token").is_some())
        .collect();
    assert_eq!(secret_variants.len(), 2);
    for variant in secret_variants {
        assert_eq!(variant["properties"]["token"]["writeOnly"], true);
    }
    assert!(serde_json::from_value::<GithubAction>(serde_json::json!({"action":"remove_account","id":"github.com","token":"unrecognized-canary"})).is_err());
}
