//! Telegram owner seam: `config.json` is written only through the telegram setup owner's locks.
//! The adapter never opens `owner.lock`, so a settings write cannot start a second poller; it
//! only wakes the poller that already owns the bot.
use super::{
    request::*,
    service::Store,
    transaction::{self, Prepared},
};
use crate::telegram::{Config, Paths, settings as setup};
use serde_json::Value;

const ID: &str = "telegram";

#[derive(Default)]
pub struct TelegramSettingsStore {
    paths: Option<Paths>,
}

impl TelegramSettingsStore {
    #[cfg(test)]
    pub(crate) fn at_paths(paths: Paths) -> Self {
        Self { paths: Some(paths) }
    }

    fn paths(&self) -> Result<Paths, Failure> {
        match &self.paths {
            Some(paths) => Ok(paths.clone()),
            None => setup::paths().map_err(|_| Error::new(ID, "store_failed")),
        }
    }
}

fn to_value(config: &Config) -> Result<Value, Failure> {
    serde_json::to_value(config).map_err(|_| Error::new(ID, "store_failed"))
}

/// Same shape `Config::load` accepts; the alias normally comes from the bot itself.
fn valid_alias(alias: &str) -> bool {
    alias.len() <= 64
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

impl Store for TelegramSettingsStore {
    fn id(&self) -> &str {
        ID
    }
    fn target(&self) -> Target {
        Target::Machine
    }

    fn read(&self) -> Result<Value, Failure> {
        setup::update_config(&self.paths()?, |config, _| {
            Ok::<_, Failure>((to_value(config)?, None))
        })
        .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn commit(
        &self,
        prepare: &mut dyn FnMut(&Value) -> Result<Prepared, Failure>,
    ) -> Result<CommitReply, Failure> {
        let paths = self.paths()?;
        setup::update_config(&paths, |config, token_set| {
            let latest = to_value(config)?;
            let Prepared { value, mut reply } = prepare(&latest)?;
            let next: Config =
                serde_json::from_value(value).map_err(|_| Error::new(ID, "invalid_type"))?;
            let canonical = to_value(&next)?;
            // Enabling needs a bot the owner can actually poll with: a token file and its alias.
            if !valid_alias(&next.bot_alias)
                || (next.enabled && (next.bot_alias.is_empty() || !token_set))
            {
                return Err(Error::new(ID, "out_of_range"));
            }
            reply.revision = transaction::revision(&canonical)?;
            if canonical == latest {
                reply.changed_ids.clear();
                reply.effects.clear();
                reply.restart.clear();
            }
            let changed = !reply.changed_ids.is_empty();
            reply.persisted = changed;
            Ok((reply, changed.then_some(next)))
        })
        .map_err(|_| Error::new(ID, "store_failed"))?
    }

    fn apply(
        &self,
        _: &CommitReply,
    ) -> impl std::future::Future<Output = Result<bool, Failure>> + Send {
        // The running owner re-reads config.json on this wake; nothing is spawned here.
        setup::notify_changed();
        std::future::ready(Ok(true))
    }
}
