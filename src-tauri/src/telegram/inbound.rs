use super::journal::{Journal, Phase};
use super::mail::{MailPort, PendingMail, Update};
use super::{BotApi, Config, Error, Owner, Paths};
use std::collections::HashSet;
use std::time::Duration;

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
            Phase::Fresh | Phase::Ready => {}
        }
        if let Some(when) = self.not_before {
            if let Some(delay) = when.checked_duration_since(tokio::time::Instant::now()) {
                return Ok(Poll::Backoff(delay));
            }
        }
        let bootstrap = phase == Phase::Fresh;
        if bootstrap {
            self.journal.begin_bootstrap()?;
        }
        let result = self
            .api
            .get_updates(
                if bootstrap { -1 } else { offset },
                if bootstrap { 0 } else { 25 },
            )
            .await;
        let values = match result {
            Ok(values) => values,
            Err(error) => {
                if matches!(error, Error::Unauthorized | Error::Conflict) {
                    self.journal.latch(error)?;
                    tracing::error!(source="telegram",error=%error,"Telegram polling stopped; explicit operator recovery required");
                    return Err(error);
                }
                if bootstrap {
                    // Negative offsets are destructive. Never repeat after an
                    // unknown outcome, even if a transport fault seems transient.
                    tracing::error!(
                        source = "telegram",
                        "Telegram bootstrap uncertain; explicit operator recovery required"
                    );
                    return Err(Error::BootstrapUncertain);
                }
                let delay = match error {
                    Error::RateLimited(seconds) => Duration::from_secs(seconds),
                    Error::Transport => {
                        self.failures = self.failures.saturating_add(1);
                        let seconds = (1u64 << self.failures.saturating_sub(1).min(6)).min(60);
                        let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]);
                        Duration::from_millis((seconds * 1000 + jitter).min(60000))
                    }
                    _ => return Err(error),
                };
                self.not_before = Some(
                    tokio::time::Instant::now()
                        .checked_add(delay)
                        .ok_or(Error::Protocol)?,
                );
                return Ok(Poll::Backoff(delay));
            }
        };
        self.failures = 0;
        self.not_before = None;
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
        let ids = self.paths.allowlist()?;
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
        let ids = self.paths.allowlist()?;
        let mut offered = 0;
        for mail in self.journal.pending()? {
            if self.offered.contains(&mail.id) {
                continue;
            }
            let body: serde_json::Value =
                serde_json::from_str(&mail.content).map_err(|_| Error::Store)?;
            let authorized = body["chat_id"]
                .as_str()
                .and_then(|id| id.parse::<i64>().ok())
                .is_some_and(|id| ids.contains(&id));
            if !authorized {
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
