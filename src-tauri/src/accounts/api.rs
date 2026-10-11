//! Dispatch account actions and apply GitHub runtime effects through owner services.
use super::edit::{add_account, add_site, list_accounts, list_sites, set_secret};
use super::model::{AccountAction, AccountView, AuthKind, SecretPresence, SecretStatus};
use super::{store, verify};
use crate::state::AppState;
use std::sync::Arc;

async fn test_account(id: String, state: &AppState) -> Result<AccountView, String> {
    let id_for_load = id.clone();
    let (mut account, token) = tokio::task::spawn_blocking(move || {
        let account = store::snapshot()?
            .accounts
            .remove(&id_for_load)
            .ok_or("Account not found")?;
        let token = if account.kind == AuthKind::Ambient {
            None
        } else {
            store::read_token(&id_for_load)?
        };
        Ok::<_, String>((account, token))
    })
    .await
    .map_err(|_| "Account storage task failed")??;
    let token = if account.kind == AuthKind::Ambient {
        state.github.token.read().clone()
    } else {
        token
    }
    .ok_or("Account has no credential; authenticate it first")?;
    let login = verify::verify(&account, &token).await?;
    let updated = account.clone();
    let login_for_save = login.clone();
    tokio::task::spawn_blocking(move || {
        store::with_registry(|registry| {
            let latest = registry
                .accounts
                .get_mut(&id)
                .ok_or("Account was removed during verification")?;
            if *latest != updated {
                return Err("Account changed during verification; retry the test".into());
            }
            let changed = latest.login.as_ref() != Some(&login_for_save);
            latest.login = Some(login_for_save);
            Ok(((), changed))
        })
    })
    .await
    .map_err(|_| "Account storage task failed")??;
    account.login = Some(login);
    Ok(AccountView {
        account,
        secret: SecretPresence {
            status: SecretStatus::Set,
        },
    })
}

fn remove_account(id: &str, state: &AppState) -> Result<(), String> {
    let record = store::snapshot()?.accounts.remove(id);
    if record
        .as_ref()
        .is_some_and(|account| account.provider == "github")
    {
        crate::github_account::remove_account_everywhere(id)?;
        state.github.ghe_state.remove(id);
        if id == crate::github_account::GitHubAccount::GITHUB_COM_ID {
            *state.github.token.write() = None;
            *state.github.token_source.write() = crate::github_auth::TokenSource::None;
            crate::github::invalidate_viewer_login(state);
        }
        let prefix = format!("{id}:");
        state
            .git_cache
            .github_repo_cooldown
            .retain(|key, _| !key.starts_with(&prefix));
    } else {
        store::remove(id)?;
    }
    Ok(())
}

pub(crate) async fn execute(
    state: &Arc<AppState>,
    request: AccountAction,
) -> Result<serde_json::Value, String> {
    match request {
        AccountAction::SetSecret { id, token } => {
            let reply = set_secret(id.clone(), token).await?;
            state.github.ghe_state.remove(&id);
            serde_json::to_value(reply).map_err(|_| "Account serialization failed".into())
        }
        AccountAction::ClearSecret { id }
            if id == crate::github_account::GitHubAccount::GITHUB_COM_ID =>
        {
            crate::github_auth::github_logout_impl(state).await?;
            Ok(serde_json::json!({"cleared":true}))
        }
        AccountAction::Add { account, token } => {
            serde_json::to_value(add_account(account, token).await?)
                .map_err(|_| "Account serialization failed".into())
        }
        AccountAction::Test { id } => serde_json::to_value(test_account(id, state).await?)
            .map_err(|_| "Account serialization failed".into()),
        AccountAction::TestSite { id, account_id } => {
            let site_id = id.clone();
            let target_id = account_id.clone();
            tokio::task::spawn_blocking(move || {
                let registry = store::snapshot()?;
                let account = registry
                    .accounts
                    .get(&target_id)
                    .ok_or("Account not found")?;
                if account.site_id != site_id {
                    return Err("Account does not belong to this site".to_string());
                }
                Ok(())
            })
            .await
            .map_err(|_| "Account storage task failed")??;
            serde_json::to_value(test_account(account_id, state).await?)
                .map_err(|_| "Account serialization failed".into())
        }
        request => {
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                let reply = match request {
                    AccountAction::List { provider } => {
                        serde_json::to_value(list_accounts(provider.as_deref(), Some(&state))?)
                    }
                    AccountAction::ListSites { provider } => {
                        serde_json::to_value(list_sites(provider.as_deref())?)
                    }
                    AccountAction::AddSite {
                        provider,
                        host,
                        label,
                    } => serde_json::to_value(add_site(&provider, &host, &label)?),
                    AccountAction::SetLabel { id, label } => {
                        store::set_label(&id, &label, false)?;
                        Ok(serde_json::json!({"updated":true}))
                    }
                    AccountAction::SetSiteLabel { id, label } => {
                        store::set_label(&id, &label, true)?;
                        Ok(serde_json::json!({"updated":true}))
                    }
                    AccountAction::ClearSecret { id } => {
                        store::with_registry(|registry| {
                            if !registry.accounts.contains_key(&id) {
                                return Err("Account not found".into());
                            }
                            crate::credentials::delete(
                                crate::credentials::Credential::AccountToken(&id),
                            )
                            .map_err(|_| "Account credential removal failed")?;
                            Ok(((), false))
                        })?;
                        state.github.ghe_state.remove(&id);
                        Ok(serde_json::json!({"cleared":true}))
                    }
                    AccountAction::Remove { id } => {
                        remove_account(&id, &state)?;
                        Ok(serde_json::json!({"removed":true}))
                    }
                    AccountAction::RemoveSite { id } => {
                        let registry = store::snapshot()?;
                        if !registry.sites.contains_key(&id) {
                            return Err("Site not found".into());
                        }
                        for account in registry
                            .accounts
                            .values()
                            .filter(|account| account.site_id == id)
                        {
                            remove_account(&account.id, &state)?;
                        }
                        store::with_registry(|latest| {
                            if latest
                                .accounts
                                .values()
                                .any(|account| account.site_id == id)
                            {
                                return Err(
                                    "Site acquired another account during removal; retry".into()
                                );
                            }
                            let removed = latest.sites.remove(&id).is_some();
                            Ok(((), removed))
                        })?;
                        Ok(serde_json::json!({"removed":true}))
                    }
                    _ => unreachable!("async actions handled above"),
                };
                reply.map_err(|_| "Account serialization failed".into())
            })
            .await
            .map_err(|_| "Account storage task failed")?
        }
    }
}
