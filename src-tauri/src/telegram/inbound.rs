use super::api::BATCH_LIMIT;
use super::journal::{Journal, Phase};
use super::mail::{MailPort, PendingMail, Update};
use super::{BotApi, Config, Error, Owner, Paths};
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

/// Avoid letting untrusted retry_after silence polling indefinitely.
const MAX_RETRY_AFTER: u64 = 60 * 60;

pub(crate) enum Poll {
    Accepted(usize),
    Backoff(Duration),
}

/// One polling owner with an explicit, presently unwired native-mail port.
pub(crate) struct Inbound {
    paths: Paths,
    config: Config,
    _owner: Owner,
    api: BotApi,
    journal: Journal,
    offered: HashSet<String>,
    not_before: Option<tokio::time::Instant>,
    failures: u32,
    batch_limit: u8,
}
impl Inbound {
    pub(crate) fn open(paths: Paths) -> Result<Option<Self>, Error> {
        let Some(config) = Config::load(&paths)? else {
            return Ok(None);
        };
        let owner = Owner::acquire(&paths)?;
        let journal = Journal::open(&paths, &config.bot_alias, &config.target_tuic_session)?;
        let api = BotApi::new(paths.clone())?;
        Ok(Some(Self {
            paths,
            config,
            _owner: owner,
            api,
            journal,
            offered: HashSet::new(),
            not_before: None,
            failures: 0,
            batch_limit: BATCH_LIMIT,
        }))
    }
    #[cfg(test)]
    pub(super) fn loopback(paths: Paths, address: std::net::SocketAddr) -> Result<Self, Error> {
        let mut inbound = Self::open(paths.clone())?.ok_or(Error::Config)?;
        inbound.api = BotApi::loopback(paths, address);
        Ok(inbound)
    }
    /// One bounded long poll. The caller sleeps for Backoff; repeated immediate
    /// calls cannot bypass it. No autonomous process/task is started in slices 1–2.
    pub(crate) async fn poll(&mut self) -> Result<Poll, Error> {
        if let Some(when) = self.not_before {
            if let Some(delay) = when.checked_duration_since(tokio::time::Instant::now()) {
                return Ok(Poll::Backoff(delay));
            }
        }
        match self.poll_once().await {
            Ok(poll) => {
                self.failures = 0;
                self.not_before = None;
                self.batch_limit = BATCH_LIMIT;
                Ok(poll)
            }
            Err(Error::BootstrapUncertain) => Err(Error::BootstrapUncertain),
            Err(mut error) => {
                if error == Error::ResponseTooLarge {
                    if self.batch_limit == 1 {
                        error = Error::OversizeUpdate;
                    } else {
                        self.batch_limit = (self.batch_limit / 2).max(1);
                    }
                }
                if matches!(
                    error,
                    Error::Unauthorized
                        | Error::Conflict
                        | Error::Rejected(403 | 404)
                        | Error::OversizeUpdate
                ) {
                    if let Err(store_error) = self.journal.latch(error) {
                        self.backoff(store_error);
                        return Err(store_error);
                    }
                    tracing::error!(source="telegram",error=%error,
                        "Telegram polling stopped; explicit operator recovery required");
                    return Err(error);
                }
                let delay = self.backoff(error);
                match error {
                    Error::RateLimited(_) | Error::Transport | Error::ResponseTooLarge => {
                        Ok(Poll::Backoff(delay))
                    }
                    _ => Err(error),
                }
            }
        }
    }

    /// Every recoverable failure shares this schedule, including parse and disk faults.
    fn backoff(&mut self, error: Error) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let delay = match error {
            Error::RateLimited(seconds) => {
                if seconds > MAX_RETRY_AFTER {
                    tracing::warn!(
                        source = "telegram",
                        "Telegram retry_after capped at one hour"
                    );
                }
                Duration::from_secs(seconds.clamp(1, MAX_RETRY_AFTER))
            }
            _ => {
                let seconds = (1u64 << self.failures.saturating_sub(1).min(6)).min(60);
                let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]);
                Duration::from_millis((seconds * 1000 + jitter).min(60000))
            }
        };
        self.not_before = Some(tokio::time::Instant::now() + delay);
        delay
    }

    fn revalidate(&mut self) -> Result<BTreeSet<i64>, Error> {
        let ids = self.paths.allowlist()?;
        for id in self.journal.purge_revoked(&ids)? {
            self.offered.remove(&id);
        }
        Ok(ids)
    }

    async fn poll_once(&mut self) -> Result<Poll, Error> {
        let current = Config::load(&self.paths)?.ok_or(Error::Config)?;
        if current.bot_alias != self.config.bot_alias
            || current.target_tuic_session != self.config.target_tuic_session
        {
            return Err(Error::State);
        };
        let (phase, offset) = self.journal.state()?;
        match phase {
            Phase::BootstrapStarted => return Err(Error::BootstrapUncertain),
            Phase::Unauthorized => return Err(Error::Unauthorized),
            Phase::Conflict => return Err(Error::Conflict),
            Phase::Forbidden => return Err(Error::Rejected(403)),
            Phase::NotFound => return Err(Error::Rejected(404)),
            Phase::OversizeUpdate => return Err(Error::OversizeUpdate),
            Phase::Fresh | Phase::Ready => {}
        }
        self.revalidate()?;
        let bootstrap = phase == Phase::Fresh;
        if bootstrap {
            self.journal.begin_bootstrap()?;
        }
        let result = self
            .api
            .get_updates_with_limit(
                if bootstrap { -1 } else { offset },
                if bootstrap { 0 } else { 25 },
                if bootstrap { 1 } else { self.batch_limit },
            )
            .await;
        let values = match result {
            Ok(values) => values,
            Err(error) => {
                if matches!(
                    error,
                    Error::Unauthorized | Error::Conflict | Error::Rejected(403 | 404)
                ) {
                    return Err(error);
                }
                if bootstrap {
                    if error == Error::ResponseTooLarge {
                        return Err(Error::OversizeUpdate);
                    }
                    // Negative offsets are destructive. Never retry an unknown outcome.
                    tracing::error!(
                        source = "telegram",
                        "Telegram bootstrap uncertain; explicit operator recovery required"
                    );
                    return Err(Error::BootstrapUncertain);
                }
                return Err(error);
            }
        };
        if bootstrap {
            if values.len() > 1 {
                return Err(Error::Protocol);
            }
            let next = match values.last() {
                Some(value) => value
                    .get("update_id")
                    .and_then(serde_json::Value::as_i64)
                    .filter(|id| *id >= 0)
                    .and_then(|id| id.checked_add(1))
                    .ok_or(Error::Protocol)?,
                None => 0,
            };
            self.journal.finish_bootstrap(next)?;
            return Ok(Poll::Accepted(0));
        }
        // Re-read authorization AFTER network return; a removed chat cannot
        // slip through using the snapshot taken before a long poll.
        let ids = self.revalidate()?;
        let mut updates = values
            .iter()
            .map(|value| {
                Update::parse(
                    value,
                    &ids,
                    &self.config.bot_alias,
                    &self.config.target_tuic_session,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        updates.sort_by_key(|update| update.id);
        if updates
            .windows(2)
            .any(|pair| pair[0].id == pair[1].id && pair[0].mail != pair[1].mail)
        {
            return Err(Error::Protocol);
        }
        Ok(Poll::Accepted(self.journal.accept(&updates)?))
    }
    /// Rehydrate unread mail once per owner lifetime. Port errors leave it
    /// durable. A process restart re-offers identical IDs; port MUST deduplicate.
    pub(crate) async fn deliver(&mut self, port: &mut impl MailPort) -> Result<usize, Error> {
        let current = Config::load(&self.paths)?.ok_or(Error::Config)?;
        if current.bot_alias != self.config.bot_alias
            || current.target_tuic_session != self.config.target_tuic_session
        {
            return Err(Error::State);
        };
        self.revalidate()?;
        let mut offered = 0;
        for mail in self.journal.pending()? {
            if self.offered.contains(&mail.id) {
                continue;
            }
            port.offer(&mail).await?;
            self.offered.insert(mail.id);
            offered += 1;
        }
        Ok(offered)
    }
    pub(crate) fn consumed(&mut self, id: &str, peer: &str) -> Result<(), Error> {
        self.journal.consume(id, peer)?;
        self.offered.remove(id);
        Ok(())
    }
    pub(crate) fn pending(&self) -> Result<Vec<PendingMail>, Error> {
        self.journal.pending()
    }
}
