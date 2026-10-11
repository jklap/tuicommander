//! Core-owned account registry. Public replies contain metadata and presence only.
mod api;
mod catalogue;
mod edit;
pub(crate) use catalogue::catalogue_schema;
mod model;
mod providers;
mod store;
mod verify;
pub(crate) use verify::read_verified_identity;
#[cfg(test)]
mod tests;

pub(crate) use api::execute;
pub(crate) use edit::{add_account, list_accounts, precheck_new, set_secret};
pub(crate) use model::{Account, AccountAction, AccountView, AuthKind, NewAccount, Registry};
pub(crate) use providers::{
    ProviderDescriptor, VerifyRequest, register_provider, unregister_providers,
};
#[cfg(test)]
pub(crate) use store::save_github_delta;
pub(crate) use store::{github_accounts, read_token, remove, upsert_github};

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn account_action(
    state: tauri::State<'_, std::sync::Arc<crate::state::AppState>>,
    request: AccountAction,
) -> Result<serde_json::Value, String> {
    execute(state.inner(), request).await
}
