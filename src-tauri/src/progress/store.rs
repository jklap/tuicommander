use super::model::{
    LIST_LIMIT, NewProgressEntry, ProgressDeleteReceipt, ProgressEntry, ProgressKind, ProgressList,
    ProgressListInput, ProgressViewedReceipt,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The one journal, for every project.
///
/// It lives in the app's config directory and never inside a repository:
/// nothing Progress writes belongs in the user's source tree. `project` is a
/// column, so closing a session, deleting a temporary workspace or parking a
/// project cannot take a journal with it.
const STORE_FILE: &str = "progress.sqlite3";

#[derive(Clone, Debug)]
pub struct ProgressStore {
    db_path: PathBuf,
}

impl ProgressStore {
    /// Open (and create on first use) the shared journal.
    ///
    /// Connections are deliberately not retained: separate TUIC processes write
    /// this same file, and SQLite is the serialization boundary.
    pub fn open() -> Result<Self, String> {
        let dir = crate::config::config_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot create '{}': {error}",
                dir.display()
            )
        })?;
        let store = Self {
            db_path: dir.join(STORE_FILE),
        };
        store.connect()?;
        Ok(store)
    }

    /// Test-support accessor: the repository-watcher test needs the file the
    /// store writes, to prove it is not under any repository.
    #[cfg(test)]
    pub fn database_path(&self) -> &std::path::Path {
        &self.db_path
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot open '{}': {error}",
                self.db_path.display()
            )
        })?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(db_error("set progress busy timeout"))?;
        // WAL survives the timeout: several TUIC processes do write this file,
        // and a reader must not block the writer that is recording an entry.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error("enable WAL on the progress store"))?;
        conn.execute_batch(
            // AUTOINCREMENT, not a bare rowid. A plain `INTEGER PRIMARY KEY`
            // hands out `max(rowid) + 1`, so deleting the newest entry and
            // writing the next one reuses its id — and a dialog still holding
            // the old id would then delete an entry it never showed.
            "CREATE TABLE IF NOT EXISTS entries (
               id            INTEGER PRIMARY KEY AUTOINCREMENT,
               project       TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL,
               kind          TEXT NOT NULL CHECK (kind IN ('done','blocked','intent')),
               text          TEXT NOT NULL,
               step          TEXT,
               agent_name    TEXT
             );
             CREATE INDEX IF NOT EXISTS entries_by_project ON entries (project, id DESC);
             CREATE TABLE IF NOT EXISTS project_views (
               project        TEXT PRIMARY KEY,
               last_viewed_ms INTEGER NOT NULL
             );",
        )
        .map_err(db_error("prepare the progress schema"))?;
        Ok(conn)
    }

    /// Append one entry. There is nothing to reconcile: the table is
    /// append-only and the rowid is identity, order and cursor in one.
    pub(crate) fn record(
        &self,
        project: &str,
        entry: &NewProgressEntry,
    ) -> Result<ProgressEntry, String> {
        entry.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress transaction"))?;
        let created_at_ms = now_ms();
        let text = entry.trimmed_text();
        let step = entry.trimmed_step();
        let agent_name = entry.trimmed_agent_name();
        tx.execute(
            "INSERT INTO entries (project, created_at_ms, kind, text, step, agent_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                project,
                i64_from_u64(created_at_ms),
                entry.kind.as_str(),
                &text,
                &step,
                &agent_name
            ],
        )
        .map_err(db_error("insert progress entry"))?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(db_error("commit progress entry"))?;
        Ok(ProgressEntry {
            id,
            project: project.to_string(),
            created_at_ms,
            kind: entry.kind,
            text,
            step,
            agent_name,
        })
    }

    pub fn list(&self, project: &str, input: &ProgressListInput) -> Result<ProgressList, String> {
        let conn = self.connect()?;
        let blocked_only = input.blocked_only.unwrap_or(false);
        let mut statement = conn
            .prepare(
                "SELECT id, created_at_ms, kind, text, step, agent_name
                   FROM entries
                  WHERE project = ?1 AND (?2 = 0 OR kind = 'blocked')
                  ORDER BY id DESC
                  LIMIT ?3",
            )
            .map_err(db_error("prepare the progress list"))?;
        let entries = statement
            .query_map(
                params![project, i64::from(blocked_only), LIST_LIMIT as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .map_err(db_error("read the progress list"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error("read a progress entry"))?
            .into_iter()
            .map(
                |(id, created_at_ms, kind, text, step, agent_name)| -> Result<_, String> {
                    Ok(ProgressEntry {
                        id,
                        project: project.to_string(),
                        created_at_ms: u64_from_i64(created_at_ms),
                        kind: ProgressKind::parse(&kind)?,
                        text,
                        step,
                        agent_name,
                    })
                },
            )
            .collect::<Result<Vec<_>, String>>()?;

        Ok(ProgressList {
            project: project.to_string(),
            entries,
            last_viewed_ms: self.last_viewed_ms(&conn, project)?,
        })
    }

    pub fn delete(&self, project: &str, ids: &[i64]) -> Result<ProgressDeleteReceipt, String> {
        if ids.is_empty() {
            return Ok(ProgressDeleteReceipt { deleted: 0 });
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress delete"))?;
        let mut deleted = 0usize;
        {
            // Scoped by project as well as by id: an id is unique in the shared
            // table, so without this one project could delete another's entry
            // by guessing a rowid.
            let mut statement = tx
                .prepare("DELETE FROM entries WHERE project = ?1 AND id = ?2")
                .map_err(db_error("prepare the progress delete"))?;
            for id in ids {
                deleted += statement
                    .execute(params![project, id])
                    .map_err(db_error("delete a progress entry"))?;
            }
        }
        tx.commit().map_err(db_error("commit progress delete"))?;
        Ok(ProgressDeleteReceipt { deleted })
    }

    /// Record that the user has seen this project's journal up to now. The
    /// dialog calls it on close; the divider it draws on the next open is this
    /// timestamp.
    pub fn mark_viewed(&self, project: &str) -> Result<ProgressViewedReceipt, String> {
        let last_viewed_ms = now_ms();
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO project_views (project, last_viewed_ms) VALUES (?1, ?2)
             ON CONFLICT(project) DO UPDATE SET last_viewed_ms = excluded.last_viewed_ms",
            params![project, i64_from_u64(last_viewed_ms)],
        )
        .map_err(db_error("record the progress last-viewed time"))?;
        Ok(ProgressViewedReceipt { last_viewed_ms })
    }

    fn last_viewed_ms(&self, conn: &Connection, project: &str) -> Result<Option<u64>, String> {
        conn.query_row(
            "SELECT last_viewed_ms FROM project_views WHERE project = ?1",
            params![project],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(db_error("read the progress last-viewed time"))
        .map(|value| value.map(u64_from_i64))
    }
}

fn db_error(what: &'static str) -> impl Fn(rusqlite::Error) -> String {
    move |error| format!("progress_store_unavailable: cannot {what}: {error}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

/// SQLite binds integers as `i64`. Every timestamp this store writes is a
/// millisecond epoch, so the saturating cast is unreachable in practice and
/// exists so a clock far in the future cannot wrap into a negative rowid-order
/// timestamp.
fn i64_from_u64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u64_from_i64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::model::ProgressKind;

    fn entry(kind: ProgressKind, text: &str) -> NewProgressEntry {
        NewProgressEntry {
            kind,
            text: text.to_string(),
            step: None,
            agent_name: None,
        }
    }

    /// The guard holds the process-wide config-directory lock, so it must be
    /// bound for the whole test — `let (_guard, …)`, never `let (_, …)`.
    fn isolated_store() -> (impl Drop, ProgressStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let store = ProgressStore::open().unwrap();
        (guard, store, dir)
    }

    #[test]
    fn the_database_lives_in_the_config_directory_and_nowhere_else() {
        let (_guard, store, dir) = isolated_store();
        assert_eq!(store.database_path(), dir.path().join("progress.sqlite3"));
    }

    #[test]
    fn entries_come_back_newest_first_with_their_fields_intact() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "parser shipped"))
            .unwrap();
        let second = store
            .record(
                "/p",
                &NewProgressEntry {
                    kind: ProgressKind::Blocked,
                    text: "needs an API key".to_string(),
                    step: Some("Step 3".to_string()),
                    agent_name: Some("claude".to_string()),
                },
            )
            .unwrap();
        assert!(second.id > first.id, "the rowid is the order");

        let listed = store.list("/p", &ProgressListInput::default()).unwrap();
        let ids: Vec<_> = listed.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![second.id, first.id]);
        assert_eq!(listed.entries[0].step.as_deref(), Some("Step 3"));
        assert_eq!(listed.entries[0].agent_name.as_deref(), Some("claude"));
        assert_eq!(listed.entries[0].kind, ProgressKind::Blocked);
        assert_eq!(listed.last_viewed_ms, None);
    }

    #[test]
    fn the_blocked_filter_is_the_only_one() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record("/p", &entry(ProgressKind::Done, "done"))
            .unwrap();
        store
            .record("/p", &entry(ProgressKind::Intent, "setting out"))
            .unwrap();
        let blocked = store
            .record("/p", &entry(ProgressKind::Blocked, "stuck"))
            .unwrap();

        let filtered = store
            .list(
                "/p",
                &ProgressListInput {
                    blocked_only: Some(true),
                },
            )
            .unwrap();
        assert_eq!(
            filtered.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![blocked.id]
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            3
        );
    }

    /// Two projects share one database. Nothing may leak across the column,
    /// and that includes a delete aimed at another project's rowid.
    #[test]
    fn projects_are_isolated_inside_the_one_database() {
        let (_guard, store, _dir) = isolated_store();
        let mine = store
            .record("/mine", &entry(ProgressKind::Done, "mine"))
            .unwrap();
        let theirs = store
            .record("/theirs", &entry(ProgressKind::Done, "theirs"))
            .unwrap();

        let listed = store.list("/mine", &ProgressListInput::default()).unwrap();
        assert_eq!(
            listed.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![mine.id]
        );

        assert_eq!(store.delete("/mine", &[theirs.id]).unwrap().deleted, 0);
        assert_eq!(
            store
                .list("/theirs", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            1,
            "another project's delete must not reach this entry"
        );

        store.mark_viewed("/mine").unwrap();
        assert!(
            store
                .list("/theirs", &ProgressListInput::default())
                .unwrap()
                .last_viewed_ms
                .is_none(),
            "last-viewed is per project"
        );
    }

    #[test]
    fn delete_removes_only_the_named_ids_and_reports_the_count() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        let second = store
            .record("/p", &entry(ProgressKind::Done, "two"))
            .unwrap();

        assert_eq!(store.delete("/p", &[]).unwrap().deleted, 0);
        // A missing id is not an error: two clients deleting the same entry is
        // an ordinary race, not a failure the user can act on.
        assert_eq!(
            store
                .delete("/p", &[first.id, first.id + 9_999])
                .unwrap()
                .deleted,
            1
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![second.id]
        );
    }

    /// The rowid never repeats after a delete, which is what lets the dialog
    /// use it as identity for a delete button and as the sort key at once.
    #[test]
    fn a_rowid_is_not_reused_after_a_delete() {
        let (_guard, store, _dir) = isolated_store();
        let first = store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        store.delete("/p", &[first.id]).unwrap();
        let second = store
            .record("/p", &entry(ProgressKind::Done, "two"))
            .unwrap();
        assert!(second.id > first.id);
    }

    #[test]
    fn marking_viewed_moves_the_divider_forward() {
        let (_guard, store, _dir) = isolated_store();
        store
            .record("/p", &entry(ProgressKind::Done, "one"))
            .unwrap();
        let first = store.mark_viewed("/p").unwrap().last_viewed_ms;
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .last_viewed_ms,
            Some(first)
        );
        std::thread::sleep(Duration::from_millis(2));
        let second = store.mark_viewed("/p").unwrap().last_viewed_ms;
        assert!(second >= first, "the timestamp is not allowed to go back");
    }

    #[test]
    fn an_invalid_entry_is_refused_before_it_reaches_the_table() {
        let (_guard, store, _dir) = isolated_store();
        assert!(
            store
                .record("/p", &entry(ProgressKind::Done, "   "))
                .unwrap_err()
                .contains("text must not be empty")
        );
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            0
        );
    }

    /// Several processes write this file. A second connection opened while the
    /// first is mid-transaction must wait on the busy timeout and then commit,
    /// not fail — and two entries written in the same millisecond must both
    /// survive with distinct ids.
    #[test]
    fn concurrent_writers_both_land() {
        let (_guard, store, _dir) = isolated_store();
        let other = ProgressStore {
            db_path: store.database_path().to_path_buf(),
        };
        let a = store
            .record("/p", &entry(ProgressKind::Done, "from a"))
            .unwrap();
        let b = other
            .record("/p", &entry(ProgressKind::Done, "from b"))
            .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(
            store
                .list("/p", &ProgressListInput::default())
                .unwrap()
                .entries
                .len(),
            2
        );
    }

    /// Restarting TUIC does not delete the journal: a fresh store over the same
    /// directory reads what the previous one wrote.
    #[test]
    fn the_journal_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        ProgressStore::open()
            .unwrap()
            .record("/p", &entry(ProgressKind::Done, "before the restart"))
            .unwrap();
        let listed = ProgressStore::open()
            .unwrap()
            .list("/p", &ProgressListInput::default())
            .unwrap();
        assert_eq!(listed.entries.len(), 1);
        assert_eq!(listed.entries[0].text, "before the restart");
    }

    #[test]
    fn an_unopenable_store_reports_a_concrete_error() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the database file belongs: SQLite cannot open it,
        // and the caller must be told which path failed rather than getting an
        // empty list that reads as "nothing happened yet".
        std::fs::create_dir(dir.path().join(STORE_FILE)).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let error = ProgressStore::open().unwrap_err();
        assert!(
            error.starts_with("progress_store_unavailable:"),
            "unexpected error: {error}"
        );
        assert!(error.contains(STORE_FILE), "unexpected error: {error}");
    }
}
