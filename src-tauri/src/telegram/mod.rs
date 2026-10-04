//! Offline Telegram adapter boundaries; native delivery is not wired yet.
// DEFERRED (2026-10-03): start polling in run_remote only after 1419/1420 land
// and the native mail/wake port is implemented. Never auto-enable here.
mod api;
mod backoff;
mod config;
mod inbound;
mod mail;
mod offset;
mod outbound;
mod stop;
mod notifications;

pub(crate) use api::BotApi;
pub(crate) use config::{Config, Owner, Paths};
#[cfg(test)]
pub(crate) use inbound::Poll;
#[cfg(test)]
type Inbound = inbound::Inbound<mail::TestInbox>;
#[cfg(test)]
pub(crate) use mail::{MailPort, PendingMail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Config,
    State,
    Capacity,
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
            Self::State => "telegram_invalid_state",
            Self::Capacity => "telegram_mail_capacity",
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
mod adversarial_tests;
#[cfg(test)]
mod tests;
