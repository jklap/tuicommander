use super::api::BATCH_LIMIT;
use super::backoff::Backoff;
use super::mail::{MailPort, Update};
use super::{BotApi, Config, Error, Owner, Paths, offset};
use std::time::Duration;

pub(crate) enum Poll {
    Accepted(usize),
    Backoff(Duration),
}

/// One polling owner; the mail port owns the in-memory inbox and safe wake.
pub(crate) struct Inbound<P: MailPort> {
    paths: Paths,
    config: Config,
    _owner: Owner,
    api: BotApi,
    port: P,
    backoff: Backoff,
    stopped: Option<Error>,
    batch_limit: u8,
}
impl<P: MailPort> Inbound<P> {
    pub(crate) fn with_port(paths: Paths, port: P) -> Result<Option<Self>, Error> {
        let Some(config) = Config::load(&paths)? else {
            return Ok(None);
        };
        let owner = Owner::acquire(&paths)?;
        let api = BotApi::new(paths.clone())?;
        Ok(Some(Self {
            paths,
            config,
            _owner: owner,
            api,
            port,
            backoff: Backoff::default(),
            stopped: None,
            batch_limit: BATCH_LIMIT,
        }))
    }
    #[cfg(test)]
    pub(super) fn with_loopback(
        paths: Paths,
        address: std::net::SocketAddr,
        port: P,
    ) -> Result<Self, Error> {
        let mut inbound = Self::with_port(paths.clone(), port)?.ok_or(Error::Config)?;
        inbound.api = BotApi::loopback(paths, address);
        Ok(inbound)
    }

    /// One bounded poll. Stops last for this owner lifetime; restarting retries.
    pub(crate) async fn poll(&mut self) -> Result<Poll, Error> {
        if let Some(error) = self.stopped {
            return Err(error);
        }
        if let Some(delay) = self.backoff.remaining() {
            return Ok(Poll::Backoff(delay));
        }
        match self.poll_once().await {
            Ok(poll) => {
                self.backoff.reset();
                self.batch_limit = BATCH_LIMIT;
                Ok(poll)
            }
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
                    self.stopped = Some(error);
                    tracing::error!(source="telegram",error=%error,
                        "Telegram polling stopped; restart after resolving the reported fault");
                    return Err(error);
                }
                let delay = self.backoff.failed(error);
                match error {
                    Error::RateLimited(_) | Error::Transport | Error::ResponseTooLarge => {
                        Ok(Poll::Backoff(delay))
                    }
                    _ => Err(error),
                }
            }
        }
    }

    async fn poll_once(&mut self) -> Result<Poll, Error> {
        let current = Config::load(&self.paths)?.ok_or(Error::Config)?;
        if current.bot_alias != self.config.bot_alias
            || current.target_tuic_session != self.config.target_tuic_session
        {
            return Err(Error::State);
        }
        let saved = offset::read(&self.paths);
        if saved.is_none() {
            self.batch_limit = 1;
        }
        let values = self
            .api
            .get_updates_with_limit(
                saved.unwrap_or(-1),
                if saved.is_none() { 0 } else { 25 },
                if saved.is_none() { 1 } else { self.batch_limit },
            )
            .await?;
        if saved.is_none() {
            let next = match values.last() {
                Some(value) => value
                    .get("update_id")
                    .and_then(serde_json::Value::as_i64)
                    .filter(|id| *id >= 0)
                    .and_then(|id| id.checked_add(1))
                    .ok_or(Error::Protocol)?,
                None => 0,
            };
            offset::write(&self.paths, next)?;
            return Ok(Poll::Accepted(0));
        }
        let mut next = saved.ok_or(Error::State)?;
        // Revalidate after the long poll; never deliver from a stale allowlist snapshot.
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
        let mut accepted = 0;
        for update in updates {
            if update.id < next {
                continue;
            }
            let candidate = update.id.checked_add(1).ok_or(Error::Protocol)?;
            if let Some(mail) = update.mail {
                self.port.offer(&mail).await?;
                accepted += 1;
            }
            offset::write(&self.paths, candidate)?;
            next = candidate;
        }
        Ok(Poll::Accepted(accepted))
    }
}

#[cfg(test)]
impl Inbound<super::mail::TestInbox> {
    pub(super) fn open(paths: Paths) -> Result<Option<Self>, Error> {
        Self::with_port(paths, super::mail::TestInbox::default())
    }
    pub(super) fn loopback(paths: Paths, address: std::net::SocketAddr) -> Result<Self, Error> {
        Self::with_loopback(paths, address, super::mail::TestInbox::default())
    }
    /// Observe the test mail port, not an adapter-retained queue.
    pub(super) fn pending(&self) -> Result<Vec<super::PendingMail>, Error> {
        Ok(self.port.by_id.values().cloned().collect())
    }
}
