use serde::{Deserialize, Serialize};

/// Response from `POST https://github.com/login/device/code`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Result of a single poll attempt to `POST https://github.com/login/oauth/access_token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum PollResult {
    /// User hasn't authorized yet — keep polling.
    #[serde(rename = "pending")]
    Pending,
    /// Polling too fast — increase interval by 5 seconds.
    #[serde(rename = "slow_down")]
    SlowDown,
    /// User authorized — token received.
    #[serde(rename = "success")]
    Success { access_token: String, scope: String },
    /// Device code expired (15 min) — must restart flow.
    #[serde(rename = "expired")]
    Expired,
    /// User denied access.
    #[serde(rename = "access_denied")]
    AccessDenied,
}

/// Where the active GitHub token came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// GH_TOKEN or GITHUB_TOKEN environment variable
    Env,
    /// OAuth Device Flow token stored in OS keyring
    OAuth,
    /// gh CLI config or `gh auth token`
    GhCli,
    /// Personal Access Token pasted for a GitHub Enterprise Server account
    Pat,
    /// No token available
    #[default]
    None,
}

/// Authentication status returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub login: Option<String>,
    pub avatar_url: Option<String>,
    pub source: TokenSource,
    pub scopes: Option<String>,
    /// Human-readable error when the token exists but validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
