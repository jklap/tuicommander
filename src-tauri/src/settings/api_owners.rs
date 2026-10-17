//! Typed workflows use the same host-issued target/origin checks as transactions.
use super::*;

// The store tag selects the action DTO, so a published workflow cannot decode as
// another owner's similarly named action. Credentials remain input-only.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "store", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerAction {
    GithubAccounts {
        operation: crate::settings::accounts::GithubAction,
    },
    Connections {
        operation: crate::settings::remote::ConnectionCredentialAction,
    },
    Telegram {
        operation: crate::settings::telegram::TelegramAction,
    },
    Upstreams {
        operation: crate::settings::upstream_credentials::CredentialAction,
    },
    // DEFERRED (2026-10-17) — typed interactive Ego login needs a separate owner contract
    // after feature freeze; services/egoLogin.ts remains a human UI workflow, not a Settings action.
    Ego {
        #[serde(rename = "expectedRevision")]
        expected_revision: String,
        operation: crate::settings::ego::EgoAuthAction,
    },
    #[cfg(feature = "dictation")]
    Voice {
        operation: crate::settings::voice::VoiceAction,
    },
}

impl OwnerAction {
    pub(super) fn section(&self) -> &str {
        match self {
            Self::GithubAccounts { .. } => "github_accounts",
            Self::Connections { .. } => "connections",
            Self::Telegram { .. } => "telegram",
            Self::Upstreams { .. } => "upstreams",
            Self::Ego { .. } => "ego",
            #[cfg(feature = "dictation")]
            Self::Voice { .. } => "voice",
        }
    }

    // These owner APIs return presence/readback rather than a transaction receipt.
    // Their successful return follows the durable owner write, including vault-only edits.
    pub(super) fn success_is_durable(&self) -> bool {
        matches!(self, Self::Connections { .. } | Self::Telegram { .. })
    }
}

impl Settings {
    pub(super) async fn perform(
        &self,
        state: &Arc<AppState>,
        owner: OwnerAction,
        caller: &Caller,
    ) -> Result<Value, Failure> {
        caller.check(true)?;
        match owner {
            OwnerAction::GithubAccounts { operation } => self.github.perform(operation).await,
            OwnerAction::Connections { operation } => {
                let owner = self.connections.clone();
                tokio::task::spawn_blocking(move || owner.perform(operation))
                    .await
                    .map_err(|_| Error::new("connections", "store_failed"))?
            }
            OwnerAction::Telegram { operation } => {
                crate::settings::telegram::TelegramOwnerSettings::default()
                    .perform(operation)
                    .await
            }
            OwnerAction::Upstreams { operation } => {
                // Request-scoped: the persistent facade must not retain its AppState.
                crate::settings::upstream_credentials::UpstreamCredentials::new(state.clone())
                    .perform(operation)
                    .await
            }
            OwnerAction::Ego {
                expected_revision,
                operation,
            } => {
                let ego = self.ego.clone();
                let reply = tokio::task::spawn_blocking(move || {
                    ego.auth_edit(&expected_revision, operation)
                })
                .await
                .map_err(|_| Error::new("ego", "store_failed"))??;
                serde_json::to_value(reply).map_err(|_| Error::new("ego", "store_failed"))
            }
            #[cfg(feature = "dictation")]
            OwnerAction::Voice { operation } => {
                let app = state
                    .app_handle
                    .read()
                    .clone()
                    .ok_or_else(|| Error::new("voice", "unavailable"))?;
                crate::settings::voice::VoiceSettingsStore::new(None)
                    .perform(&app, caller, operation)
                    .await
            }
        }
    }
}
