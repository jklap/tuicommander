use super::config::{check_directory, private_open};
use super::mail::{PendingMail, Update};
use super::{Error, Paths};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

/// Bounded retained phone mail; capacity failure never acknowledges new updates.
const MAX_PENDING: i64 = 100;
/// Bound disk payload growth independently of the per-message native limit.
const MAX_PENDING_BYTES: i64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Fresh,
    BootstrapStarted,
    Ready,
    Unauthorized,
    Conflict,
    Forbidden,
    NotFound,
    OversizeUpdate,
}

pub(super) struct Journal {
    connection: Connection,
    paths: Paths,
}
impl Journal {
    pub(super) fn open(paths: &Paths, alias: &str, peer: &str) -> Result<Self, Error> {
        check_directory(&paths.directory)?;
        let database = paths.file("journal.sqlite3");
        if !database.exists() && paths.file("bootstrap.marker").exists() {
            return Err(Error::State);
        }
        // The containing directory is owner-only. Refuse sidecar links as well.
        for name in [
            "journal.sqlite3",
            "journal.sqlite3-wal",
            "journal.sqlite3-shm",
        ] {
            let path = paths.file(name);
            if std::fs::symlink_metadata(&path).is_ok() {
                private_open(&path, false)?;
            }
        }
        private_open(&database, true)?;
        let connection = Connection::open(&database).map_err(|_| Error::Store)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| Error::Store)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS state (
                singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                alias TEXT NOT NULL, peer TEXT NOT NULL,
                phase TEXT NOT NULL, offset INTEGER NOT NULL CHECK(offset>=0));
            CREATE TABLE IF NOT EXISTS updates (
                update_id INTEGER PRIMARY KEY CHECK(update_id>=0),
                mail_id TEXT UNIQUE, recipient TEXT, content TEXT,
                consumed INTEGER NOT NULL DEFAULT 0 CHECK(consumed IN (0,1)));",
            )
            .map_err(|_| Error::Store)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO state VALUES(1,?1,?2,'fresh',0)",
                params![alias, peer],
            )
            .map_err(|_| Error::Store)?;
        let binding: (String, String) = connection
            .query_row("SELECT alias,peer FROM state WHERE singleton=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|_| Error::Store)?;
        let phase: String = connection
            .query_row("SELECT phase FROM state WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| Error::Store)?;
        if phase == "fresh" && paths.file("bootstrap.marker").exists() {
            return Err(Error::State);
        }
        if binding != (alias.into(), peer.into()) {
            return Err(Error::State);
        }
        Ok(Self {
            connection,
            paths: paths.clone(),
        })
    }
    pub(super) fn state(&self) -> Result<(Phase, i64), Error> {
        let (phase, offset): (String, i64) = self
            .connection
            .query_row(
                "SELECT phase,offset FROM state WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| Error::Store)?;
        let phase = match phase.as_str() {
            "fresh" => Phase::Fresh,
            "bootstrap_started" => Phase::BootstrapStarted,
            "ready" => Phase::Ready,
            "unauthorized" => Phase::Unauthorized,
            "conflict" => Phase::Conflict,
            "forbidden" => Phase::Forbidden,
            "not_found" => Phase::NotFound,
            "oversize_update" => Phase::OversizeUpdate,
            _ => return Err(Error::State),
        };
        Ok((phase, offset))
    }
    pub(super) fn begin_bootstrap(&mut self) -> Result<(), Error> {
        let changed = self
            .connection
            .execute(
                "UPDATE state SET phase='bootstrap_started' WHERE phase='fresh'",
                [],
            )
            .map_err(|_| Error::Store)?;
        if changed != 1 {
            return Err(Error::State);
        }
        private_open(&self.paths.file("bootstrap.marker"), true)?
            .sync_all()
            .map_err(|_| Error::Store)?;
        Ok(())
    }
    pub(super) fn finish_bootstrap(&mut self, next: i64) -> Result<(), Error> {
        let changed = self
            .connection
            .execute(
                "UPDATE state SET phase='ready',offset=?1 WHERE phase='bootstrap_started'",
                [next],
            )
            .map_err(|_| Error::Store)?;
        if changed != 1 {
            return Err(Error::State);
        }
        Ok(())
    }
    pub(super) fn latch(&self, error: Error) -> Result<(), Error> {
        let phase = match error {
            Error::Unauthorized => "unauthorized",
            Error::Conflict => "conflict",
            Error::Rejected(403) => "forbidden",
            Error::Rejected(404) => "not_found",
            Error::OversizeUpdate => "oversize_update",
            _ => return Err(Error::State),
        };
        self.connection
            .execute("UPDATE state SET phase=?1", [phase])
            .map_err(|_| Error::Store)?;
        Ok(())
    }
    /// All accepted messages and their update cursor move in the same transaction.
    pub(super) fn accept(&mut self, updates: &[Update]) -> Result<usize, Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Store)?;
        let mut offset: i64 = tx
            .query_row(
                "SELECT offset FROM state WHERE singleton=1 AND phase='ready'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| Error::Store)?;
        let mut accepted = 0;
        for update in updates {
            if update.id < offset {
                continue;
            }
            let (mail_id, recipient, content) = match &update.mail {
                Some(mail) => (Some(&mail.id), Some(&mail.recipient), Some(&mail.content)),
                None => (None, None, None),
            };
            let inserted = tx.execute("INSERT OR IGNORE INTO updates(update_id,mail_id,recipient,content) VALUES(?1,?2,?3,?4)",params![update.id,mail_id,recipient,content]).map_err(|_|Error::Store)?;
            if inserted != 0 && update.mail.is_some() {
                accepted += 1;
            }
            offset = update.id.checked_add(1).ok_or(Error::Protocol)?;
        }
        let (count,bytes):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(content AS BLOB))),0) FROM updates WHERE content IS NOT NULL AND consumed=0",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|Error::Store)?;
        if count > MAX_PENDING || bytes > MAX_PENDING_BYTES {
            return Err(Error::Capacity);
        }
        tx.execute("UPDATE state SET offset=?1", [offset])
            .map_err(|_| Error::Store)?;
        // The offset itself is the tombstone for old updates; retain no private
        // text or growing per-update rows once consumed/ignored.
        tx.execute(
            "DELETE FROM updates WHERE consumed=1 OR content IS NULL",
            [],
        )
        .map_err(|_| Error::Store)?;
        tx.commit().map_err(|_| Error::Store)?;
        Ok(accepted)
    }
    pub(super) fn pending(&self) -> Result<Vec<PendingMail>, Error> {
        let mut statement=self.connection.prepare("SELECT mail_id,recipient,content FROM updates WHERE content IS NOT NULL AND consumed=0 ORDER BY update_id").map_err(|_|Error::Store)?;
        statement
            .query_map([], |r| {
                Ok(PendingMail {
                    id: r.get(0)?,
                    recipient: r.get(1)?,
                    content: r.get(2)?,
                })
            })
            .map_err(|_| Error::Store)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| Error::Store)
    }
    /// Revalidation erases revoked mail while the offset retains its tombstone.
    pub(super) fn purge_revoked(
        &mut self,
        ids: &std::collections::BTreeSet<i64>,
    ) -> Result<Vec<String>, Error> {
        let pending = self.pending()?;
        let mut revoked = Vec::new();
        for mail in pending {
            let body: serde_json::Value =
                serde_json::from_str(&mail.content).map_err(|_| Error::Store)?;
            let authorized = body["chat_id"]
                .as_str()
                .and_then(|id| id.parse::<i64>().ok())
                .is_some_and(|id| ids.contains(&id));
            if !authorized {
                revoked.push(mail.id);
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Store)?;
        for id in &revoked {
            tx.execute("DELETE FROM updates WHERE mail_id=?1", [id])
                .map_err(|_| Error::Store)?;
        }
        tx.commit().map_err(|_| Error::Store)?;
        Ok(revoked)
    }
    /// Called by the future native inbox consumption boundary, never by offer.
    pub(super) fn consume(&mut self, id: &str, peer: &str) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Store)?;
        let target: Option<String> = tx
            .query_row(
                "SELECT recipient FROM updates WHERE mail_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| Error::Store)?;
        match target {
            Some(target) if target == peer => {}
            Some(_) => return Err(Error::State),
            None => return Err(Error::State),
        }
        tx.execute("DELETE FROM updates WHERE mail_id=?1", [id])
            .map_err(|_| Error::Store)?;
        tx.commit().map_err(|_| Error::Store)
    }
}
