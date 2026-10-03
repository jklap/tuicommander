//! Offline Telegram adapter boundaries; native delivery is not wired yet.
// DEFERRED (2026-10-03): start polling in run_remote only after 1419/1420 land
// and the stable-ID mail/consumption port is implemented. Never auto-enable here.
mod api;
mod config;

pub(crate) use api::BotApi;
pub(crate) use config::{Config, Owner, Paths};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Config,
    PrivateFile,
    AlreadyOwned,
    Transport,
    Protocol,
    Unauthorized,
    Conflict,
    RateLimited(u64),
    Rejected(u16),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No external body, path, URL, credential or chat text crosses this seam.
        f.write_str(match self {
            Self::Config => "telegram_invalid_config",
            Self::PrivateFile => "telegram_private_file_unavailable",
            Self::AlreadyOwned => "telegram_already_owned",
            Self::Transport => "telegram_transport_unavailable",
            Self::Protocol => "telegram_invalid_response",
            Self::Unauthorized => "telegram_unauthorized",
            Self::Conflict => "telegram_owner_conflict",
            Self::RateLimited(_) => "telegram_rate_limited",
            Self::Rejected(_) => "telegram_request_rejected",
        })
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
