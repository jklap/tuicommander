use super::model::{
    DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, MAX_SUMMARY_CHARS, NewProgressEvent, ProgressCorrection,
    ProgressEvent, ProgressKind, ProgressListInput, ProgressMutationReceipt, ProgressPage,
    ProgressProvenance, ProgressReceipt, ProgressReportOutcome, ProgressStatus,
    ProgressUpdateInput, ProjectSnapshot, WorkstreamSnapshot, WorkstreamState,
    normalize_workstream, validate_workstream_name,
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 2;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DEDUP_WINDOW_MS: u64 = 60_000;
const STORE_DIR: &str = ".tuic";
const STORE_FILE: &str = "progress.sqlite3";
const RECOVERY_LOCK_FILE: &str = "progress.sqlite3.recovery.lock";
const EXCLUDE_ENTRIES: [&str; 5] = [
    ".tuic/progress.sqlite3",
    ".tuic/progress.sqlite3-wal",
    ".tuic/progress.sqlite3-shm",
    ".tuic/progress.sqlite3.recovery.lock",
    ".tuic/progress.sqlite3*.corrupt-*",
];

#[derive(Clone, Debug)]
pub struct ProgressStore {
    project_root: PathBuf,
    db_path: PathBuf,
}

impl ProgressStore {
    /// Prepare and validate a project-local Progress store.
    ///
    /// Connections are deliberately not retained: separate TUIC processes can
    /// write the same project, and SQLite is the serialization boundary.
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, String> {
        let project_root = fs::canonicalize(project_root.as_ref()).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot access project root '{}': {error}",
                project_root.as_ref().display()
            )
        })?;
        if !project_root.is_dir() {
            return Err(format!(
                "progress_store_unavailable: project root '{}' is not a directory",
                project_root.display()
            ));
        }

        let store_dir = project_root.join(STORE_DIR);
        fs::create_dir_all(&store_dir).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot create '{}': {error}",
                store_dir.display()
            )
        })?;
        ensure_local_git_excludes(&project_root)?;

        let store = Self {
            db_path: store_dir.join(STORE_FILE),
            project_root,
        };
        let lock_path = store.db_path.with_file_name(RECOVERY_LOCK_FILE);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| {
                format!(
                    "progress_store_recovery_failed: cannot open recovery lock '{}': {error}",
                    lock_path.display()
                )
            })?;
        lock.lock().map_err(|error| {
            format!(
                "progress_store_recovery_failed: cannot lock recovery state '{}': {error}",
                lock_path.display()
            )
        })?;
        let original_shm = store.read_existing_shm();
        let result = match original_shm {
            Err(error) => Err(error),
            Ok(original_shm) => match store
                .validate_existing_database_read_only()
                .and_then(|()| store.connect())
            {
                Ok(_) => Ok(store),
                Err(error) if error.starts_with("progress_store_corrupt:") => {
                    match store.recover_corrupt_database(&error, original_shm.as_deref()) {
                        Ok(preserved) => Err(format!(
                            "progress_store_recovered: corrupt progress history was preserved as {}; a new empty database is ready, retry the operation",
                            preserved
                                .iter()
                                .map(|path| format!("'{}'", path.display()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                        Err(recovery_error) => Err(recovery_error),
                    }
                }
                Err(error) => Err(error),
            },
        };
        lock.unlock().map_err(|error| {
            format!(
                "progress_store_recovery_failed: cannot unlock recovery state '{}': {error}",
                lock_path.display()
            )
        })?;
        result
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn record(&self, new_event: &NewProgressEvent) -> Result<ProgressEvent, String> {
        new_event.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress transaction"))?;
        let event = insert_event(&tx, new_event)?;
        tx.commit().map_err(db_error("commit progress event"))?;
        Ok(event)
    }

    /// Atomically enforces collection state and exact caller-scoped retry deduplication.
    pub(crate) fn report(
        &self,
        new_event: &NewProgressEvent,
    ) -> Result<ProgressReportOutcome, String> {
        new_event.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress report transaction"))?;
        let enabled = tx
            .query_row(
                "SELECT collection_enabled FROM project_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error("read progress collection state"))?
            != 0;
        if !enabled {
            let revision = current_revision(&tx)?;
            tx.commit()
                .map_err(db_error("commit paused progress report"))?;
            return Ok(ProgressReportOutcome {
                receipt: ProgressReceipt::paused(revision),
                event: None,
            });
        }
        if let Some(event) = find_recent_duplicate(&tx, new_event)? {
            tx.commit()
                .map_err(db_error("commit duplicate progress report"))?;
            return Ok(ProgressReportOutcome {
                receipt: ProgressReceipt::duplicate(&event),
                event: Some(event),
            });
        }
        let event = insert_event(&tx, new_event)?;
        tx.commit().map_err(db_error("commit progress report"))?;
        Ok(ProgressReportOutcome {
            receipt: ProgressReceipt::recorded(&event),
            event: Some(event),
        })
    }

    pub fn list(
        &self,
        before_sequence: Option<u64>,
        limit: Option<usize>,
    ) -> Result<ProgressPage, String> {
        let conn = self.connect()?;
        let revision = current_revision(&conn)?;
        let snapshot_cursor = max_sequence(&conn)?;
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let before = before_sequence
            .map(i64_from_u64)
            .transpose()?
            .unwrap_or(i64::MAX);
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                        e.summary, e.workstream_id, w.name, e.reporter_id,
                        e.reporter_name, e.session_id, e.workspace_path
                 FROM events e
                 LEFT JOIN workstreams w ON w.id = e.workstream_id
                 WHERE e.sequence < ?1
                 ORDER BY e.sequence DESC
                 LIMIT ?2",
            )
            .map_err(db_error("prepare progress history query"))?;
        let rows = stmt
            .query_map(params![before, limit as i64 + 1], row_to_event)
            .map_err(db_error("query progress history"))?;
        let mut events = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read progress history"))?;
        let has_more = events.len() > limit;
        events.truncate(limit);
        let next_before_sequence = has_more
            .then(|| events.last().map(|event| event.sequence))
            .flatten();
        Ok(ProgressPage {
            revision,
            snapshot_cursor,
            events,
            next_before_sequence,
        })
    }

    pub fn list_filtered(&self, input: &ProgressListInput) -> Result<ProgressPage, String> {
        let conn = self.connect()?;
        let revision = current_revision(&conn)?;
        let snapshot_cursor = max_sequence(&conn)?;
        let read_cursor = read_cursor(&conn)?;
        let limit = input
            .limit
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT);
        let before = input.before_sequence.unwrap_or(u64::MAX);
        let after_ms = input.created_after_ms.unwrap_or(0);
        let before_ms = input.created_before_ms.unwrap_or(u64::MAX);
        let kind = input.kind.map(ProgressKind::as_str);
        let mut stmt = conn.prepare(
            "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                    e.summary, e.workstream_id, w.name, e.reporter_id,
                    e.reporter_name, e.session_id, e.workspace_path
             FROM events e LEFT JOIN workstreams w ON w.id = e.workstream_id
             WHERE e.sequence < ?1 AND e.created_at_ms >= ?2 AND e.created_at_ms <= ?3
               AND (?4 IS NULL OR e.workstream_id = ?4)
               AND (?5 IS NULL OR e.kind = ?5)
               AND (?6 = 0 OR e.sequence > ?7)
               AND (?8 = 0 OR EXISTS (SELECT 1 FROM blockers b WHERE b.event_id=e.id AND b.active=1))
             ORDER BY e.sequence DESC LIMIT ?9"
        ).map_err(db_error("prepare filtered progress query"))?;
        let rows = stmt
            .query_map(
                params![
                    i64_from_u64(before)?,
                    i64_from_u64(after_ms)?,
                    i64_from_u64(before_ms)?,
                    input.workstream_id,
                    kind,
                    input.unread_only.unwrap_or(false) as i64,
                    i64_from_u64(read_cursor)?,
                    input.blocker_only.unwrap_or(false) as i64,
                    limit as i64 + 1
                ],
                row_to_event,
            )
            .map_err(db_error("query filtered progress history"))?;
        let mut events = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read filtered progress history"))?;
        let has_more = events.len() > limit;
        events.truncate(limit);
        let next_before_sequence = has_more
            .then(|| events.last().map(|e| e.sequence))
            .flatten();
        Ok(ProgressPage {
            revision,
            snapshot_cursor,
            events,
            next_before_sequence,
        })
    }

    pub fn status(&self) -> Result<ProgressStatus, String> {
        let snapshot = self.snapshot()?;
        let conn = self.connect()?;
        let snapshot_cursor = max_sequence(&conn)?;
        let read_cursor = read_cursor(&conn)?;
        let unread_count = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE sequence > ?1",
                [i64_from_u64(read_cursor)?],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error("count unread progress events"))?;
        Ok(ProgressStatus {
            project_root: snapshot.project_root,
            revision: snapshot.revision,
            snapshot_cursor,
            read_cursor,
            unread_count: u64_from_i64(unread_count, "unread count")?,
            collection_enabled: snapshot.collection_enabled,
            workstreams: snapshot.workstreams,
            project_blockers: snapshot.project_blockers,
        })
    }

    pub fn set_collection_enabled(&self, enabled: bool) -> Result<ProgressMutationReceipt, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress collection transaction"))?;
        let current = tx
            .query_row(
                "SELECT collection_enabled FROM project_meta WHERE id=1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(db_error("read progress collection state"))?
            != 0;
        let revision = if current == enabled {
            current_revision(&tx)?
        } else {
            tx.execute(
                "UPDATE project_meta SET collection_enabled=?1 WHERE id=1",
                [enabled as i64],
            )
            .map_err(db_error("update progress collection state"))?;
            bump_revision(&tx)?
        };
        tx.commit()
            .map_err(db_error("commit progress collection transaction"))?;
        Ok(ProgressMutationReceipt {
            revision,
            affected: usize::from(current != enabled),
        })
    }

    pub fn delete_events(&self, event_ids: &[String]) -> Result<ProgressMutationReceipt, String> {
        if event_ids.is_empty() {
            return Err("progress_invalid_request: eventIds must not be empty".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress delete transaction"))?;
        for id in event_ids {
            let exists = tx
                .query_row("SELECT 1 FROM events WHERE id=?1", [id], |_| Ok(()))
                .optional()
                .map_err(db_error("validate progress event id"))?
                .is_some();
            if !exists {
                return Err(format!(
                    "progress_event_not_found: event '{id}' does not belong to this project"
                ));
            }
        }
        let mut affected = 0;
        for id in event_ids {
            affected += tx
                .execute("DELETE FROM events WHERE id=?1", [id])
                .map_err(db_error("delete progress event"))?;
        }
        recompute_all_workstreams(&tx)?;
        let revision = bump_revision(&tx)?;
        tx.commit()
            .map_err(db_error("commit progress delete transaction"))?;
        Ok(ProgressMutationReceipt { revision, affected })
    }

    pub fn clear(&self, expected_revision: u64) -> Result<ProgressMutationReceipt, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress clear transaction"))?;
        require_revision(&tx, expected_revision)?;
        let affected = tx
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .map_err(db_error("count progress events"))?;
        tx.execute_batch("DELETE FROM events; DELETE FROM workstreams; UPDATE project_meta SET read_cursor=0, collection_enabled=0 WHERE id=1;")
            .map_err(db_error("clear project progress"))?;
        let revision = bump_revision(&tx)?;
        tx.commit()
            .map_err(db_error("commit progress clear transaction"))?;
        Ok(ProgressMutationReceipt {
            revision,
            affected: usize::try_from(affected).unwrap_or(usize::MAX),
        })
    }

    pub fn acknowledge_read(&self, cursor: u64) -> Result<ProgressMutationReceipt, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress read transaction"))?;
        let maximum = max_sequence(&tx)?;
        if cursor > maximum {
            return Err(format!(
                "progress_invalid_cursor: snapshot cursor {cursor} exceeds project cursor {maximum}"
            ));
        }
        let old = read_cursor(&tx)?;
        let next = old.max(cursor);
        tx.execute(
            "UPDATE project_meta SET read_cursor=?1 WHERE id=1",
            [i64_from_u64(next)?],
        )
        .map_err(db_error("acknowledge progress snapshot"))?;
        tx.commit()
            .map_err(db_error("commit progress read transaction"))?;
        Ok(ProgressMutationReceipt {
            revision: current_revision(&conn)?,
            affected: usize::from(next != old),
        })
    }

    pub fn update(&self, input: &ProgressUpdateInput) -> Result<ProgressMutationReceipt, String> {
        if input.corrections.is_empty() {
            return Err("progress_invalid_request: corrections must not be empty".into());
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin progress update transaction"))?;
        require_revision(&tx, input.expected_revision)?;
        for correction in &input.corrections {
            apply_correction(&tx, correction)?;
        }
        recompute_all_workstreams(&tx)?;
        // Explicit state is intentionally applied after derived recomputation.
        for correction in &input.corrections {
            if let ProgressCorrection::SetWorkstreamState {
                workstream_id,
                state,
            } = correction
            {
                set_workstream_state(&tx, workstream_id, *state)?;
            }
        }
        let revision = bump_revision(&tx)?;
        tx.commit()
            .map_err(db_error("commit progress update transaction"))?;
        Ok(ProgressMutationReceipt {
            revision,
            affected: input.corrections.len(),
        })
    }

    pub fn snapshot(&self) -> Result<ProjectSnapshot, String> {
        let conn = self.connect()?;
        let revision = current_revision(&conn)?;
        let collection_enabled = conn
            .query_row(
                "SELECT collection_enabled FROM project_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error("read progress collection state"))?
            != 0;

        let mut stmt = conn
            .prepare(
                "SELECT w.id, w.name, w.state, w.updated_sequence,
                        COUNT(CASE WHEN b.active = 1 THEN 1 END)
                 FROM workstreams w
                 LEFT JOIN blockers b ON b.workstream_id = w.id
                 GROUP BY w.id
                 ORDER BY w.updated_sequence DESC, w.name COLLATE NOCASE",
            )
            .map_err(db_error("prepare progress workstream query"))?;
        let workstreams = stmt
            .query_map([], |row| {
                let state: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    state,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(db_error("query progress workstreams"))?
            .map(|row| {
                let (id, name, state, updated_sequence, active_blockers) =
                    row.map_err(db_error("read progress workstream"))?;
                Ok(WorkstreamSnapshot {
                    id,
                    name,
                    state: WorkstreamState::parse(&state)?,
                    active_blockers: u64_from_i64(active_blockers, "active blocker count")?,
                    updated_sequence: u64_from_i64(updated_sequence, "updated sequence")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut blockers = conn
            .prepare(
                "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                        e.summary, NULL, NULL, e.reporter_id, e.reporter_name,
                        e.session_id, e.workspace_path
                 FROM blockers b
                 JOIN events e ON e.id = b.event_id
                 WHERE b.active = 1 AND b.workstream_id IS NULL
                 ORDER BY e.sequence DESC",
            )
            .map_err(db_error("prepare project blocker query"))?;
        let project_blockers = blockers
            .query_map([], row_to_event)
            .map_err(db_error("query project blockers"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read project blockers"))?;

        Ok(ProjectSnapshot {
            project_root: self.project_root.to_string_lossy().to_string(),
            revision,
            collection_enabled,
            workstreams,
            project_blockers,
        })
    }

    pub(crate) fn export_data(&self) -> Result<super::export::ExportData, String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(db_error("begin progress export snapshot"))?;
        let revision = current_revision(&tx)?;
        let collection_enabled = tx
            .query_row(
                "SELECT collection_enabled FROM project_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(db_error("read progress export collection state"))?
            != 0;
        let mut workstream_stmt = tx
            .prepare(
                "SELECT id, name, state FROM workstreams
                 ORDER BY name COLLATE NOCASE, id",
            )
            .map_err(db_error("prepare progress export workstreams"))?;
        let workstreams = workstream_stmt
            .query_map([], |row| {
                Ok(super::export::ExportWorkstream {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    state: row.get(2)?,
                })
            })
            .map_err(db_error("query progress export workstreams"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read progress export workstreams"))?;
        drop(workstream_stmt);

        let mut blocker_stmt = tx
            .prepare(
                "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                        e.summary, e.workstream_id, w.name, e.reporter_id,
                        e.reporter_name, e.session_id, e.workspace_path
                 FROM blockers b JOIN events e ON e.id=b.event_id
                 LEFT JOIN workstreams w ON w.id=e.workstream_id
                 WHERE b.active=1 ORDER BY e.sequence",
            )
            .map_err(db_error("prepare progress export blockers"))?;
        let blockers = blocker_stmt
            .query_map([], row_to_event)
            .map_err(db_error("query progress export blockers"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read progress export blockers"))?;
        drop(blocker_stmt);

        let mut event_stmt = tx
            .prepare(
                "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                        e.summary, e.workstream_id, w.name, e.reporter_id,
                        e.reporter_name, e.session_id, e.workspace_path
                 FROM events e LEFT JOIN workstreams w ON w.id=e.workstream_id
                 ORDER BY e.created_at_ms, e.sequence",
            )
            .map_err(db_error("prepare progress export history"))?;
        let events = event_stmt
            .query_map([], row_to_event)
            .map_err(db_error("query progress export history"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error("read progress export history"))?;
        drop(event_stmt);
        tx.commit()
            .map_err(db_error("finish progress export snapshot"))?;
        Ok(super::export::ExportData {
            revision,
            collection_enabled,
            workstreams,
            blockers,
            events,
        })
    }

    /// Rename a workstream while retaining every prior normalized name as an
    /// alias. Transport-level correction commands add revision preconditions;
    /// this storage primitive keeps the identity and grouping invariant atomic.
    pub fn rename_workstream(&self, workstream_id: &str, new_name: &str) -> Result<u64, String> {
        validate_workstream_name(new_name)?;
        let name = new_name.trim();
        let normalized = normalize_workstream(name);
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error("begin workstream rename transaction"))?;
        let workstream = load_workstream(&tx, workstream_id)?;
        if let Some(owner) = tx
            .query_row(
                "SELECT workstream_id FROM workstream_aliases WHERE normalized_alias = ?1",
                [&normalized],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error("check progress workstream alias"))?
            && owner != workstream.id
        {
            return Err(format!(
                "progress_workstream_conflict: '{name}' already identifies another workstream"
            ));
        }
        tx.execute(
            "UPDATE workstreams SET name = ?1, normalized_name = ?2 WHERE id = ?3",
            params![name, normalized, workstream.id],
        )
        .map_err(db_error("rename progress workstream"))?;
        tx.execute(
            "INSERT INTO workstream_aliases (normalized_alias, workstream_id)
             VALUES (?1, ?2)
             ON CONFLICT(normalized_alias) DO NOTHING",
            params![normalized, workstream.id],
        )
        .map_err(db_error("retain progress workstream alias"))?;
        let revision = bump_revision(&tx)?;
        tx.commit()
            .map_err(db_error("commit workstream rename transaction"))?;
        Ok(revision)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error("open progress database"))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(db_error("configure progress busy timeout"))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(db_error("configure progress database"))?;
        migrate(&conn)?;
        validate_schema(&conn)?;
        Ok(conn)
    }

    /// Check an existing database before the normal read-write configuration and
    /// migration path. SQLite may update the transient SHM index even on a
    /// read-only WAL connection, but this avoids granting write access to the
    /// database and WAL that contain the recoverable history.
    fn validate_existing_database_read_only(&self) -> Result<(), String> {
        if !self.db_path.exists() {
            return Ok(());
        }
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error("open progress database for validation"))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(db_error("configure progress validation timeout"))?;
        let quick_check = conn
            .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
            .map_err(db_error("validate progress database"))?;
        if quick_check != "ok" {
            return Err(format!(
                "progress_store_corrupt: SQLite quick_check failed: {quick_check}"
            ));
        }
        let version = conn
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map_err(db_error("read progress schema version"))?;
        if version > SCHEMA_VERSION {
            return Err(format!(
                "progress_store_incompatible: schema version {version} is newer than supported version {SCHEMA_VERSION}"
            ));
        }
        if version == SCHEMA_VERSION {
            validate_schema(&conn)?;
        } else if version == 0 {
            let application_tables = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(db_error("inspect unversioned progress database"))?;
            if application_tables != 0 {
                return Err(format!(
                    "progress_store_corrupt: unversioned database contains {application_tables} unexpected table(s)"
                ));
            }
        } else {
            // A version number alone is not proof of a valid legacy schema.
            // Reject malformed v1 databases before the migration path can turn
            // their missing-table error into an ordinary storage failure.
            for query in [
                "SELECT revision, collection_enabled FROM project_meta LIMIT 0",
                "SELECT id FROM workstreams LIMIT 0",
                "SELECT id FROM events LIMIT 0",
                "SELECT event_id FROM blockers LIMIT 0",
            ] {
                conn.prepare(query).map_err(|error| {
                    format!(
                        "progress_store_corrupt: legacy schema version {version} has an invalid table shape: {error}"
                    )
                })?;
            }
        } // Valid older schemas are migrated by the normal read-write connection.
        Ok(())
    }

    fn read_existing_shm(&self) -> Result<Option<Vec<u8>>, String> {
        let path = append_to_path(&self.db_path, "-shm");
        if !path.exists() {
            return Ok(None);
        }
        fs::read(&path).map(Some).map_err(|error| {
            format!(
                "progress_store_unavailable: cannot preserve SQLite SHM state '{}': {error}",
                path.display()
            )
        })
    }

    /// Preserve a corrupt database and establish a validated empty replacement.
    ///
    /// [`Self::open`] serializes its first validation and this recovery across
    /// TUIC processes. That placement matters: probing a corrupt SQLite database
    /// twice can itself delete an invalid journal before it can be preserved. The
    /// main database is moved last, after any WAL/SHM sidecars, so a failed sidecar
    /// rename cannot leave an apparently fresh main path backed by old journal
    /// files. The caller still reports recovery as an error: otherwise its
    /// operation could appear successful while reading a newly empty history.
    fn recover_corrupt_database(
        &self,
        original_error: &str,
        original_shm: Option<&[u8]>,
    ) -> Result<Vec<PathBuf>, String> {
        let recovery_id = uuid::Uuid::new_v4();
        let mut preserved = Vec::new();
        let shm = append_to_path(&self.db_path, "-shm");
        if let Some(bytes) = original_shm {
            let destination = append_to_path(&shm, &format!(".corrupt-{recovery_id}"));
            let current_matches = fs::read(&shm).is_ok_and(|current| current == bytes);
            if current_matches {
                fs::rename(&shm, &destination).map_err(|error| {
                    format!(
                        "progress_store_recovery_failed: cannot preserve '{}' as '{}': {error}",
                        shm.display(),
                        destination.display()
                    )
                })?;
            } else {
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&destination)
                    .map_err(|error| {
                        format!(
                            "progress_store_recovery_failed: cannot preserve original SHM state as '{}': {error}",
                            destination.display()
                        )
                    })?;
                file.write_all(bytes).map_err(|error| {
                    format!(
                        "progress_store_recovery_failed: cannot write preserved SHM state '{}': {error}",
                        destination.display()
                    )
                })?;
                file.sync_all().map_err(|error| {
                    format!(
                        "progress_store_recovery_failed: cannot sync preserved SHM state '{}': {error}",
                        destination.display()
                    )
                })?;
                if shm.exists() {
                    fs::remove_file(&shm).map_err(|error| {
                        format!(
                            "progress_store_recovery_failed: cannot remove probed SHM state '{}': {error}",
                            shm.display()
                        )
                    })?;
                }
            }
            preserved.push(destination);
        } else if shm.exists() {
            fs::remove_file(&shm).map_err(|error| {
                format!(
                    "progress_store_recovery_failed: cannot remove transient SHM state '{}': {error}",
                    shm.display()
                )
            })?;
        }

        for source in [append_to_path(&self.db_path, "-wal"), self.db_path.clone()] {
            if !source.exists() {
                continue;
            }
            let destination = append_to_path(&source, &format!(".corrupt-{recovery_id}"));
            fs::rename(&source, &destination).map_err(|error| {
                format!(
                    "progress_store_recovery_failed: cannot preserve '{}' as '{}': {error}",
                    source.display(),
                    destination.display()
                )
            })?;
            preserved.push(destination);
        }
        if preserved.is_empty() {
            return Err(format!(
                "progress_store_recovery_failed: corrupt database '{}' disappeared before it could be preserved",
                self.db_path.display()
            ));
        }

        self.connect().map_err(|error| {
            format!(
                "progress_store_recovery_failed: preserved corrupt history but could not create replacement: {error}"
            )
        })?;
        tracing::warn!(
            path = %self.db_path.display(),
            preserved_as = ?preserved,
            error = %original_error,
            "Recovered corrupt project progress database"
        );
        Ok(preserved)
    }
}

fn append_to_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

#[derive(Debug)]
struct WorkstreamRow {
    id: String,
    name: String,
    state: WorkstreamState,
    last_nonblocked_state: WorkstreamState,
}

fn migrate(conn: &Connection) -> Result<(), String> {
    let version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(db_error("read progress schema version"))?;
    if version > SCHEMA_VERSION {
        return Err(format!(
            "progress_store_incompatible: schema version {version} is newer than supported version {SCHEMA_VERSION}"
        ));
    }
    if version == 0 {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE project_meta (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 revision INTEGER NOT NULL CHECK (revision >= 0),
                 collection_enabled INTEGER NOT NULL CHECK (collection_enabled IN (0, 1)),
                 read_cursor INTEGER NOT NULL DEFAULT 0 CHECK (read_cursor >= 0)
             );
             INSERT INTO project_meta (id, revision, collection_enabled, read_cursor) VALUES (1, 0, 1, 0);
             CREATE TABLE workstreams (
                 id TEXT PRIMARY KEY NOT NULL,
                 name TEXT NOT NULL,
                 normalized_name TEXT NOT NULL UNIQUE,
                 state TEXT NOT NULL CHECK (state IN ('started','progressing','blocked','done')),
                 last_nonblocked_state TEXT NOT NULL CHECK (last_nonblocked_state IN ('started','progressing','done')),
                 updated_sequence INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE workstream_aliases (
                 normalized_alias TEXT PRIMARY KEY NOT NULL,
                 workstream_id TEXT NOT NULL REFERENCES workstreams(id) ON DELETE CASCADE
             );
             CREATE TABLE events (
                 sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                 id TEXT NOT NULL UNIQUE,
                 revision INTEGER NOT NULL,
                 created_at_ms INTEGER NOT NULL,
                 kind TEXT NOT NULL CHECK (kind IN ('started','milestone','blocked','done')),
                 summary TEXT NOT NULL,
                 workstream_id TEXT REFERENCES workstreams(id) ON DELETE SET NULL,
                 reporter_id TEXT,
                 reporter_name TEXT,
                 session_id TEXT,
                 workspace_path TEXT
             );
             CREATE INDEX events_created_at ON events(created_at_ms DESC, sequence DESC);
             CREATE INDEX events_workstream_sequence ON events(workstream_id, sequence DESC);
             CREATE TABLE blockers (
                 event_id TEXT PRIMARY KEY NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                 workstream_id TEXT REFERENCES workstreams(id) ON DELETE CASCADE,
                 active INTEGER NOT NULL CHECK (active IN (0, 1)),
                 manually_resolved INTEGER NOT NULL DEFAULT 0 CHECK (manually_resolved IN (0, 1))
             );
             CREATE INDEX blockers_active_workstream ON blockers(active, workstream_id);
             CREATE TABLE merged_event_sources (
                 target_event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                 source_event_id TEXT NOT NULL,
                 source_event_json TEXT NOT NULL,
                 PRIMARY KEY(target_event_id, source_event_id)
             );
             PRAGMA user_version = 2;
             COMMIT;",
        )
        .map_err(db_error("create progress schema"))?;
    } else if version == 1 {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             ALTER TABLE project_meta ADD COLUMN read_cursor INTEGER NOT NULL DEFAULT 0 CHECK (read_cursor >= 0);
             ALTER TABLE blockers ADD COLUMN manually_resolved INTEGER NOT NULL DEFAULT 0 CHECK (manually_resolved IN (0, 1));
             CREATE TABLE merged_event_sources (
                 target_event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                 source_event_id TEXT NOT NULL,
                 source_event_json TEXT NOT NULL,
                 PRIMARY KEY(target_event_id, source_event_id)
             );
             PRAGMA user_version = 2;
             COMMIT;"
        ).map_err(db_error("migrate progress schema to version 2"))?;
    }
    Ok(())
}

fn validate_schema(conn: &Connection) -> Result<(), String> {
    let quick_check = conn
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(db_error("validate progress database"))?;
    if quick_check != "ok" {
        return Err(format!(
            "progress_store_corrupt: SQLite quick_check failed: {quick_check}"
        ));
    }
    let version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(db_error("read progress schema version"))?;
    if version != SCHEMA_VERSION {
        return Err(format!(
            "progress_store_corrupt: expected schema version {SCHEMA_VERSION}, found {version}"
        ));
    }
    for table in [
        "project_meta",
        "workstreams",
        "workstream_aliases",
        "events",
        "blockers",
        "merged_event_sources",
    ] {
        let present = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |_| Ok(()),
            )
            .optional()
            .map_err(db_error("validate progress schema"))?
            .is_some();
        if !present {
            return Err(format!(
                "progress_store_corrupt: schema version {SCHEMA_VERSION} is missing table '{table}'"
            ));
        }
    }
    for query in [
        "SELECT revision, collection_enabled, read_cursor FROM project_meta LIMIT 0",
        "SELECT id, name, normalized_name, state, last_nonblocked_state, updated_sequence FROM workstreams LIMIT 0",
        "SELECT normalized_alias, workstream_id FROM workstream_aliases LIMIT 0",
        "SELECT sequence, id, revision, created_at_ms, kind, summary, workstream_id, reporter_id, reporter_name, session_id, workspace_path FROM events LIMIT 0",
        "SELECT event_id, workstream_id, active, manually_resolved FROM blockers LIMIT 0",
        "SELECT target_event_id, source_event_id, source_event_json FROM merged_event_sources LIMIT 0",
    ] {
        conn.prepare(query).map_err(|error| {
            format!("progress_store_corrupt: schema version {SCHEMA_VERSION} has an invalid table shape: {error}")
        })?;
    }
    conn.query_row(
        "SELECT revision, collection_enabled FROM project_meta WHERE id = 1",
        [],
        |_| Ok(()),
    )
    .map_err(|error| {
        format!(
            "progress_store_corrupt: schema version {SCHEMA_VERSION} has invalid metadata: {error}"
        )
    })?;
    Ok(())
}

fn resolve_or_create_workstream(tx: &Transaction<'_>, name: &str) -> Result<WorkstreamRow, String> {
    let normalized = normalize_workstream(name);
    if let Some(id) = tx
        .query_row(
            "SELECT workstream_id FROM workstream_aliases WHERE normalized_alias = ?1",
            [&normalized],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error("resolve workstream alias"))?
    {
        return load_workstream(tx, &id);
    }

    let id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO workstreams (
            id, name, normalized_name, state, last_nonblocked_state, updated_sequence
         ) VALUES (?1, ?2, ?3, 'started', 'started', 0)",
        params![id, name, normalized],
    )
    .map_err(db_error("create progress workstream"))?;
    tx.execute(
        "INSERT INTO workstream_aliases (normalized_alias, workstream_id) VALUES (?1, ?2)",
        params![normalized, id],
    )
    .map_err(db_error("create progress workstream alias"))?;
    load_workstream(tx, &id)
}

fn load_workstream(tx: &Transaction<'_>, id: &str) -> Result<WorkstreamRow, String> {
    let (id, name, state, last_nonblocked_state) = tx
        .query_row(
            "SELECT id, name, state, last_nonblocked_state FROM workstreams WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(db_error("load progress workstream"))?;
    Ok(WorkstreamRow {
        id,
        name,
        state: WorkstreamState::parse(&state)?,
        last_nonblocked_state: WorkstreamState::parse(&last_nonblocked_state)?,
    })
}

fn apply_workstream_transition(
    tx: &Transaction<'_>,
    workstream: &WorkstreamRow,
    kind: ProgressKind,
    sequence: u64,
) -> Result<(), String> {
    let active_blockers = tx
        .query_row(
            "SELECT COUNT(*) FROM blockers WHERE workstream_id = ?1 AND active = 1",
            [&workstream.id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error("count active progress blockers"))?;
    let (state, last_nonblocked_state) = match kind {
        ProgressKind::Started if active_blockers > 0 => {
            (WorkstreamState::Blocked, workstream.last_nonblocked_state)
        }
        ProgressKind::Started => (WorkstreamState::Started, WorkstreamState::Started),
        ProgressKind::Milestone if workstream.state == WorkstreamState::Blocked => {
            (WorkstreamState::Blocked, workstream.last_nonblocked_state)
        }
        ProgressKind::Milestone if workstream.state == WorkstreamState::Done => {
            (WorkstreamState::Done, WorkstreamState::Done)
        }
        ProgressKind::Milestone => (WorkstreamState::Progressing, WorkstreamState::Progressing),
        ProgressKind::Blocked => (WorkstreamState::Blocked, workstream.last_nonblocked_state),
        ProgressKind::Done => {
            tx.execute(
                "UPDATE blockers SET active = 0 WHERE workstream_id = ?1 AND active = 1",
                [&workstream.id],
            )
            .map_err(db_error("resolve completed workstream blockers"))?;
            (WorkstreamState::Done, WorkstreamState::Done)
        }
    };
    tx.execute(
        "UPDATE workstreams
         SET state = ?1, last_nonblocked_state = ?2, updated_sequence = ?3
         WHERE id = ?4",
        params![
            state.as_str(),
            last_nonblocked_state.as_str(),
            i64_from_u64(sequence)?,
            workstream.id,
        ],
    )
    .map_err(db_error("update progress workstream state"))?;
    Ok(())
}

fn insert_event(
    tx: &Transaction<'_>,
    new_event: &NewProgressEvent,
) -> Result<ProgressEvent, String> {
    let workstream = match new_event.trimmed_workstream() {
        Some(name) => Some(resolve_or_create_workstream(tx, &name)?),
        None => None,
    };
    let revision = bump_revision(tx)?;
    let id = uuid::Uuid::now_v7().to_string();
    let created_at_ms = now_ms()?;
    let summary = new_event.trimmed_summary();
    tx.execute(
        "INSERT INTO events (
            id, revision, created_at_ms, kind, summary, workstream_id,
            reporter_id, reporter_name, session_id, workspace_path
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            i64_from_u64(revision)?,
            i64_from_u64(created_at_ms)?,
            new_event.kind.as_str(),
            summary,
            workstream.as_ref().map(|value| value.id.as_str()),
            new_event.provenance.reporter_id,
            new_event.provenance.reporter_name,
            new_event.provenance.session_id,
            new_event.provenance.workspace_path,
        ],
    )
    .map_err(db_error("insert progress event"))?;
    let sequence = u64_from_i64(tx.last_insert_rowid(), "event sequence")?;
    if let Some(workstream) = &workstream {
        apply_workstream_transition(tx, workstream, new_event.kind, sequence)?;
    }
    if new_event.kind == ProgressKind::Blocked {
        tx.execute(
            "INSERT INTO blockers (event_id, workstream_id, active) VALUES (?1, ?2, 1)",
            params![id, workstream.as_ref().map(|value| value.id.as_str())],
        )
        .map_err(db_error("insert progress blocker"))?;
    }
    Ok(ProgressEvent {
        id,
        sequence,
        revision,
        created_at_ms,
        kind: new_event.kind,
        summary,
        workstream_id: workstream.as_ref().map(|value| value.id.clone()),
        workstream: workstream.map(|value| value.name),
        provenance: new_event.provenance.clone(),
    })
}

fn find_recent_duplicate(
    tx: &Transaction<'_>,
    new_event: &NewProgressEvent,
) -> Result<Option<ProgressEvent>, String> {
    let workstream_id = match new_event.trimmed_workstream() {
        Some(name) => {
            let normalized = normalize_workstream(&name);
            match tx
                .query_row(
                    "SELECT workstream_id FROM workstream_aliases WHERE normalized_alias = ?1",
                    [&normalized],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(db_error("resolve duplicate workstream"))?
            {
                Some(id) => Some(id),
                None => return Ok(None),
            }
        }
        None => None,
    };
    let cutoff = i64_from_u64(now_ms()?.saturating_sub(RETRY_DEDUP_WINDOW_MS))?;
    let latest = tx
        .query_row(
            "SELECT e.id, e.sequence, e.revision, e.created_at_ms, e.kind,
                e.summary, e.workstream_id, w.name, e.reporter_id,
                e.reporter_name, e.session_id, e.workspace_path
         FROM events e LEFT JOIN workstreams w ON w.id = e.workstream_id
         WHERE e.reporter_id IS ?1 AND e.session_id IS ?2
         ORDER BY e.sequence DESC LIMIT 1",
            params![
                new_event.provenance.reporter_id,
                new_event.provenance.session_id,
            ],
            row_to_event,
        )
        .optional()
        .map_err(db_error("query recent progress duplicate"))?;
    Ok(latest.filter(|event| {
        event.created_at_ms >= cutoff as u64
            && event.kind == new_event.kind
            && event.summary == new_event.trimmed_summary()
            && event.workstream_id == workstream_id
    }))
}

fn bump_revision(tx: &Transaction<'_>) -> Result<u64, String> {
    let revision = tx
        .query_row(
            "UPDATE project_meta SET revision = revision + 1 WHERE id = 1 RETURNING revision",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error("advance progress revision"))?;
    u64_from_i64(revision, "project revision")
}

fn current_revision(conn: &Connection) -> Result<u64, String> {
    let revision = conn
        .query_row(
            "SELECT revision FROM project_meta WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error("read progress revision"))?;
    u64_from_i64(revision, "project revision")
}

fn max_sequence(conn: &Connection) -> Result<u64, String> {
    let value = conn
        .query_row("SELECT COALESCE(MAX(sequence), 0) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(db_error("read progress snapshot cursor"))?;
    u64_from_i64(value, "snapshot cursor")
}

fn read_cursor(conn: &Connection) -> Result<u64, String> {
    let value = conn
        .query_row(
            "SELECT read_cursor FROM project_meta WHERE id=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error("read progress acknowledgement cursor"))?;
    u64_from_i64(value, "read cursor")
}

fn require_revision(tx: &Transaction<'_>, expected: u64) -> Result<(), String> {
    let actual = current_revision(tx)?;
    if actual != expected {
        return Err(format!(
            "progress_revision_conflict: expected revision {expected}, current revision is {actual}"
        ));
    }
    Ok(())
}

fn require_event(tx: &Transaction<'_>, id: &str) -> Result<ProgressEvent, String> {
    tx.query_row(
        "SELECT e.id,e.sequence,e.revision,e.created_at_ms,e.kind,e.summary,e.workstream_id,w.name,e.reporter_id,e.reporter_name,e.session_id,e.workspace_path FROM events e LEFT JOIN workstreams w ON w.id=e.workstream_id WHERE e.id=?1",
        [id], row_to_event,
    ).optional().map_err(db_error("load progress event"))?
        .ok_or_else(|| format!("progress_event_not_found: event '{id}' does not belong to this project"))
}

fn apply_correction(tx: &Transaction<'_>, correction: &ProgressCorrection) -> Result<(), String> {
    match correction {
        ProgressCorrection::EditSummary { event_id, summary } => {
            super::model::validate_text("summary", summary, MAX_SUMMARY_CHARS)?;
            require_event(tx, event_id)?;
            tx.execute(
                "UPDATE events SET summary=?1 WHERE id=?2",
                params![summary.trim(), event_id],
            )
            .map_err(db_error("edit progress summary"))?;
        }
        ProgressCorrection::MoveEvent {
            event_id,
            workstream_id,
        } => {
            require_event(tx, event_id)?;
            if let Some(id) = workstream_id {
                load_workstream(tx, id)?;
            }
            tx.execute(
                "UPDATE events SET workstream_id=?1 WHERE id=?2",
                params![workstream_id, event_id],
            )
            .map_err(db_error("move progress event"))?;
            tx.execute(
                "UPDATE blockers SET workstream_id=?1 WHERE event_id=?2",
                params![workstream_id, event_id],
            )
            .map_err(db_error("move progress blocker"))?;
        }
        ProgressCorrection::RenameWorkstream {
            workstream_id,
            name,
        } => rename_workstream_tx(tx, workstream_id, name)?,
        ProgressCorrection::MergeWorkstreams {
            source_workstream_ids,
            target_workstream_id,
        } => {
            if source_workstream_ids.is_empty() {
                return Err(
                    "progress_invalid_request: sourceWorkstreamIds must not be empty".into(),
                );
            }
            load_workstream(tx, target_workstream_id)?;
            for source in source_workstream_ids {
                if source == target_workstream_id {
                    return Err(
                        "progress_invalid_request: a workstream cannot merge into itself".into(),
                    );
                }
                let row = load_workstream(tx, source)?;
                tx.execute(
                    "UPDATE events SET workstream_id=?1 WHERE workstream_id=?2",
                    params![target_workstream_id, source],
                )
                .map_err(db_error("merge progress events"))?;
                tx.execute(
                    "UPDATE blockers SET workstream_id=?1 WHERE workstream_id=?2",
                    params![target_workstream_id, source],
                )
                .map_err(db_error("merge progress blockers"))?;
                tx.execute(
                    "UPDATE workstream_aliases SET workstream_id=?1 WHERE workstream_id=?2",
                    params![target_workstream_id, source],
                )
                .map_err(db_error("merge workstream aliases"))?;
                tx.execute("DELETE FROM workstreams WHERE id=?1", [source])
                    .map_err(db_error("delete merged workstream"))?;
                let _ = row;
            }
        }
        ProgressCorrection::MergeEvents {
            source_event_ids,
            target_event_id,
        } => {
            if source_event_ids.is_empty() {
                return Err("progress_invalid_request: sourceEventIds must not be empty".into());
            }
            require_event(tx, target_event_id)?;
            for source in source_event_ids {
                if source == target_event_id {
                    return Err(
                        "progress_invalid_request: an event cannot merge into itself".into(),
                    );
                }
                let event = require_event(tx, source)?;
                let json = serde_json::to_string(&event)
                    .map_err(|e| format!("progress_store_error: serialize merged source: {e}"))?;
                tx.execute("INSERT INTO merged_event_sources(target_event_id,source_event_id,source_event_json) VALUES(?1,?2,?3)", params![target_event_id, source, json]).map_err(db_error("preserve merged progress source"))?;
                tx.execute("DELETE FROM events WHERE id=?1", [source])
                    .map_err(db_error("delete merged progress event"))?;
            }
        }
        ProgressCorrection::ResolveBlocker { event_id } => {
            let event = require_event(tx, event_id)?;
            if event.kind != ProgressKind::Blocked {
                return Err(format!(
                    "progress_invalid_request: event '{event_id}' is not a blocker"
                ));
            }
            let changed = tx
                .execute(
                    "UPDATE blockers SET active=0, manually_resolved=1 WHERE event_id=?1 AND active=1",
                    [event_id],
                )
                .map_err(db_error("resolve progress blocker"))?;
            if changed == 0 {
                return Err(format!(
                    "progress_invalid_request: blocker '{event_id}' is already resolved"
                ));
            }
        }
        ProgressCorrection::SetWorkstreamState { workstream_id, .. } => {
            load_workstream(tx, workstream_id)?;
        }
    }
    Ok(())
}

fn rename_workstream_tx(tx: &Transaction<'_>, id: &str, new_name: &str) -> Result<(), String> {
    validate_workstream_name(new_name)?;
    let row = load_workstream(tx, id)?;
    let normalized = normalize_workstream(new_name.trim());
    if let Some(owner) = tx
        .query_row(
            "SELECT workstream_id FROM workstream_aliases WHERE normalized_alias=?1",
            [&normalized],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(db_error("check workstream alias"))?
        && owner != id
    {
        return Err(format!(
            "progress_workstream_conflict: '{}' already identifies another workstream",
            new_name.trim()
        ));
    }
    tx.execute(
        "UPDATE workstreams SET name=?1,normalized_name=?2 WHERE id=?3",
        params![new_name.trim(), normalized, id],
    )
    .map_err(db_error("rename progress workstream"))?;
    tx.execute("INSERT INTO workstream_aliases(normalized_alias,workstream_id) VALUES(?1,?2) ON CONFLICT(normalized_alias) DO NOTHING", params![normalized,row.id]).map_err(db_error("retain progress workstream alias"))?;
    Ok(())
}

fn recompute_all_workstreams(tx: &Transaction<'_>) -> Result<(), String> {
    tx.execute(
        "UPDATE blockers SET active = CASE WHEN manually_resolved=1 THEN 0 ELSE 1 END",
        [],
    )
    .map_err(db_error("reset derived progress blockers"))?;
    let ids = tx
        .prepare("SELECT id FROM workstreams")
        .map_err(db_error("prepare workstream recompute"))?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(db_error("query workstreams for recompute"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error("read workstreams for recompute"))?;
    for id in ids {
        recompute_workstream(tx, &id)?;
    }
    Ok(())
}

fn recompute_workstream(tx: &Transaction<'_>, id: &str) -> Result<(), String> {
    let mut state = WorkstreamState::Started;
    let mut last = WorkstreamState::Started;
    let mut updated = 0u64;
    let mut blockers = 0u64;
    let mut stmt = tx.prepare("SELECT e.sequence,e.kind,COALESCE(b.active,0) FROM events e LEFT JOIN blockers b ON b.event_id=e.id WHERE e.workstream_id=?1 ORDER BY e.sequence").map_err(db_error("prepare projection recompute"))?;
    let rows = stmt
        .query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(db_error("query projection events"))?;
    for row in rows {
        let (sequence, kind, active) = row.map_err(db_error("read projection event"))?;
        updated = u64_from_i64(sequence, "event sequence")?;
        match ProgressKind::parse(&kind)? {
            ProgressKind::Blocked if active != 0 => {
                blockers += 1;
                state = WorkstreamState::Blocked;
            }
            ProgressKind::Blocked => {}
            ProgressKind::Done => {
                state = WorkstreamState::Done;
                last = WorkstreamState::Done;
                blockers = 0;
                tx.execute("UPDATE blockers SET active=0 WHERE workstream_id=?1 AND event_id IN (SELECT id FROM events WHERE sequence <= ?2)", params![id, sequence])
                    .map_err(db_error("recompute completed blockers"))?;
            }
            ProgressKind::Started if blockers == 0 => {
                state = WorkstreamState::Started;
                last = WorkstreamState::Started;
            }
            ProgressKind::Started => state = WorkstreamState::Blocked,
            ProgressKind::Milestone if blockers > 0 => state = WorkstreamState::Blocked,
            ProgressKind::Milestone if state != WorkstreamState::Done => {
                state = WorkstreamState::Progressing;
                last = WorkstreamState::Progressing;
            }
            ProgressKind::Milestone => {}
        }
    }
    if blockers > 0 {
        state = WorkstreamState::Blocked;
    }
    tx.execute(
        "UPDATE workstreams SET state=?1,last_nonblocked_state=?2,updated_sequence=?3 WHERE id=?4",
        params![state.as_str(), last.as_str(), i64_from_u64(updated)?, id],
    )
    .map_err(db_error("store recomputed workstream"))?;
    Ok(())
}

fn set_workstream_state(
    tx: &Transaction<'_>,
    id: &str,
    state: WorkstreamState,
) -> Result<(), String> {
    load_workstream(tx, id)?;
    match state {
        WorkstreamState::Blocked => {
            return Err(
                "progress_invalid_request: blocked state comes from active blockers".into(),
            );
        }
        WorkstreamState::Done => {
            tx.execute("UPDATE blockers SET active=0 WHERE workstream_id=?1", [id])
                .map_err(db_error("resolve completed workstream blockers"))?;
        }
        _ => {}
    }
    tx.execute(
        "UPDATE workstreams SET state=?1,last_nonblocked_state=?1 WHERE id=?2",
        params![state.as_str(), id],
    )
    .map_err(db_error("set progress workstream state"))?;
    Ok(())
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProgressEvent> {
    let kind: String = row.get(4)?;
    let sequence: i64 = row.get(1)?;
    let revision: i64 = row.get(2)?;
    let created_at_ms: i64 = row.get(3)?;
    Ok(ProgressEvent {
        id: row.get(0)?,
        sequence: sequence
            .try_into()
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, sequence))?,
        revision: revision
            .try_into()
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, revision))?,
        created_at_ms: created_at_ms
            .try_into()
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(3, created_at_ms))?,
        kind: ProgressKind::parse(&kind).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        summary: row.get(5)?,
        workstream_id: row.get(6)?,
        workstream: row.get(7)?,
        provenance: ProgressProvenance {
            reporter_id: row.get(8)?,
            reporter_name: row.get(9)?,
            session_id: row.get(10)?,
            workspace_path: row.get(11)?,
        },
    })
}

fn ensure_local_git_excludes(project_root: &Path) -> Result<(), String> {
    let git_dir = crate::git_cli::git_cmd(project_root)
        .args(["rev-parse", "--git-common-dir"])
        .run()
        .map_err(|error| {
            format!(
                "progress_store_unavailable: cannot resolve Git metadata for '{}': {error}",
                project_root.display()
            )
        })?;
    let git_dir = PathBuf::from(git_dir.stdout.trim());
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        project_root.join(git_dir)
    };
    let info_dir = git_dir.join("info");
    fs::create_dir_all(&info_dir).map_err(|error| {
        format!(
            "progress_store_unavailable: cannot create Git exclude directory '{}': {error}",
            info_dir.display()
        )
    })?;
    let exclude_path = info_dir.join("exclude");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&exclude_path)
        .map_err(|error| {
            format!(
                "progress_store_unavailable: cannot open local Git excludes '{}': {error}",
                exclude_path.display()
            )
        })?;
    file.lock().map_err(|error| {
        format!(
            "progress_store_busy: cannot lock local Git excludes '{}': {error}",
            exclude_path.display()
        )
    })?;
    let result = update_excludes(&mut file);
    let unlock_result = file.unlock();
    result?;
    unlock_result.map_err(|error| {
        format!(
            "progress_store_unavailable: cannot unlock local Git excludes '{}': {error}",
            exclude_path.display()
        )
    })?;
    Ok(())
}

fn update_excludes(file: &mut File) -> Result<(), String> {
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        format!("progress_store_unavailable: cannot seek Git excludes: {error}")
    })?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|error| {
        format!("progress_store_unavailable: cannot read Git excludes: {error}")
    })?;
    let present = contents.lines().collect::<std::collections::HashSet<_>>();
    let missing = EXCLUDE_ENTRIES
        .iter()
        .filter(|entry| !present.contains(**entry))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    file.seek(SeekFrom::End(0)).map_err(|error| {
        format!("progress_store_unavailable: cannot seek Git excludes: {error}")
    })?;
    if !contents.is_empty() && !contents.ends_with('\n') {
        file.write_all(b"\n").map_err(|error| {
            format!("progress_store_unavailable: cannot update Git excludes: {error}")
        })?;
    }
    for entry in missing {
        writeln!(file, "{entry}").map_err(|error| {
            format!("progress_store_unavailable: cannot update Git excludes: {error}")
        })?;
    }
    file.sync_all()
        .map_err(|error| format!("progress_store_unavailable: cannot sync Git excludes: {error}"))
}

fn now_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("progress_clock_error: {error}"))?
        .as_millis();
    millis
        .try_into()
        .map_err(|_| "progress_clock_error: current timestamp does not fit u64".to_string())
}

fn i64_from_u64(value: u64) -> Result<i64, String> {
    value
        .try_into()
        .map_err(|_| format!("progress_value_out_of_range: {value} does not fit SQLite INTEGER"))
}

fn u64_from_i64(value: i64, field: &str) -> Result<u64, String> {
    value
        .try_into()
        .map_err(|_| format!("progress_store_corrupt: {field} is negative ({value})"))
}

fn db_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> String {
    move |error| {
        let code = match &error {
            rusqlite::Error::SqliteFailure(inner, _)
                if matches!(
                    inner.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                "progress_store_busy"
            }
            rusqlite::Error::SqliteFailure(inner, _)
                if matches!(
                    inner.code,
                    rusqlite::ErrorCode::ReadOnly
                        | rusqlite::ErrorCode::PermissionDenied
                        | rusqlite::ErrorCode::CannotOpen
                ) =>
            {
                "progress_store_unavailable"
            }
            rusqlite::Error::SqliteFailure(inner, _)
                if matches!(
                    inner.code,
                    rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                ) =>
            {
                "progress_store_corrupt"
            }
            _ => "progress_store_error",
        };
        format!("{code}: {context}: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::ProgressReceiptStatus;
    use std::process::Command;
    use std::sync::{Arc, Barrier};

    fn git_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::git_cli::git_cmd(dir.path())
            .args(["init"])
            .run()
            .unwrap();
        dir
    }

    fn event(kind: ProgressKind, summary: &str, workstream: Option<&str>) -> NewProgressEvent {
        NewProgressEvent {
            kind,
            summary: summary.to_string(),
            workstream: workstream.map(str::to_string),
            provenance: ProgressProvenance::default(),
        }
    }

    #[test]
    fn records_and_pages_events_with_monotonic_identity() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let first = store
            .record(&event(
                ProgressKind::Started,
                "Runtime detection started.",
                Some("MCP Detection"),
            ))
            .unwrap();
        let second = store
            .record(&event(
                ProgressKind::Milestone,
                "Local detection works.",
                Some("mcp   detection"),
            ))
            .unwrap();
        assert_eq!(first.sequence + 1, second.sequence);
        assert_eq!(first.revision + 1, second.revision);
        assert_eq!(first.workstream_id, second.workstream_id);
        assert_eq!(
            uuid::Uuid::parse_str(&first.id).unwrap().get_version_num(),
            7
        );
        assert!(uuid::Uuid::parse_str(first.workstream_id.as_deref().unwrap()).is_ok());

        let page = store.list(None, Some(1)).unwrap();
        assert_eq!(page.events, vec![second]);
        assert_eq!(page.next_before_sequence, Some(page.events[0].sequence));
        let older = store.list(page.next_before_sequence, Some(1)).unwrap();
        assert_eq!(older.events, vec![first]);

        let reopened = ProgressStore::open(project.path()).unwrap();
        assert_eq!(reopened.snapshot().unwrap().revision, 2);
    }

    #[test]
    fn blockers_dominate_milestones_and_done_resolves_them() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        store
            .record(&event(
                ProgressKind::Started,
                "Enforcement started.",
                Some("Enforcement"),
            ))
            .unwrap();
        store
            .record(&event(
                ProgressKind::Blocked,
                "TLS strategy needs a decision.",
                Some("Enforcement"),
            ))
            .unwrap();
        store
            .record(&event(
                ProgressKind::Blocked,
                "Certificate ownership is unresolved.",
                Some("Enforcement"),
            ))
            .unwrap();
        store
            .record(&event(
                ProgressKind::Milestone,
                "Prototype records requests.",
                Some("Enforcement"),
            ))
            .unwrap();
        let blocked = store.snapshot().unwrap().workstreams.remove(0);
        assert_eq!(blocked.state, WorkstreamState::Blocked);
        assert_eq!(blocked.active_blockers, 2);

        store
            .record(&event(
                ProgressKind::Started,
                "Enforcement restarted while dependencies remain.",
                Some("Enforcement"),
            ))
            .unwrap();
        assert_eq!(
            store.snapshot().unwrap().workstreams[0].state,
            WorkstreamState::Blocked
        );

        store
            .record(&event(
                ProgressKind::Done,
                "Enforcement is complete.",
                Some("Enforcement"),
            ))
            .unwrap();
        let done = store.snapshot().unwrap().workstreams.remove(0);
        assert_eq!(done.state, WorkstreamState::Done);
        assert_eq!(done.active_blockers, 0);
        store
            .record(&event(
                ProgressKind::Milestone,
                "A later note.",
                Some("Enforcement"),
            ))
            .unwrap();
        assert_eq!(
            store.snapshot().unwrap().workstreams[0].state,
            WorkstreamState::Done
        );
        store
            .record(&event(
                ProgressKind::Started,
                "Enforcement explicitly reopened.",
                Some("Enforcement"),
            ))
            .unwrap();
        assert_eq!(
            store.snapshot().unwrap().workstreams[0].state,
            WorkstreamState::Started
        );
    }

    #[test]
    fn rename_preserves_exact_alias_grouping_across_restart() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let first = store
            .record(&event(
                ProgressKind::Started,
                "Detection started.",
                Some("Shadow AI"),
            ))
            .unwrap();
        let revision = store
            .rename_workstream(first.workstream_id.as_deref().unwrap(), "AI Discovery")
            .unwrap();
        assert_eq!(revision, 2);

        let reopened = ProgressStore::open(project.path()).unwrap();
        let through_old_name = reopened
            .record(&event(
                ProgressKind::Milestone,
                "The old reporter name still groups correctly.",
                Some("  shadow   ai "),
            ))
            .unwrap();
        let through_new_name = reopened
            .record(&event(
                ProgressKind::Milestone,
                "The new reporter name groups correctly.",
                Some("ai discovery"),
            ))
            .unwrap();
        assert_eq!(through_old_name.workstream_id, first.workstream_id);
        assert_eq!(through_new_name.workstream_id, first.workstream_id);
        assert_eq!(through_old_name.workstream.as_deref(), Some("AI Discovery"));
        assert_eq!(reopened.snapshot().unwrap().workstreams.len(), 1);
    }

    #[test]
    fn ungrouped_blockers_stay_project_level() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        store
            .record(&event(
                ProgressKind::Blocked,
                "Release credentials are unavailable.",
                None,
            ))
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        assert!(snapshot.workstreams.is_empty());
        assert_eq!(snapshot.project_blockers.len(), 1);
    }

    #[test]
    fn separate_connections_serialize_concurrent_writes() {
        let project = git_project();
        let root = Arc::new(project.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(5));
        let handles = (0..4)
            .map(|index| {
                let root = Arc::clone(&root);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let store = ProgressStore::open(root.as_path()).unwrap();
                    barrier.wait();
                    store.record(&event(
                        ProgressKind::Milestone,
                        &format!("Capability {index} works."),
                        Some("Parallel"),
                    ))
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        let page = ProgressStore::open(root.as_path())
            .unwrap()
            .list(None, Some(10))
            .unwrap();
        assert_eq!(page.events.len(), 4);
        assert_eq!(page.revision, 4);
        let mut sequences = page
            .events
            .iter()
            .map(|item| item.sequence)
            .collect::<Vec<_>>();
        sequences.sort_unstable();
        sequences.dedup();
        assert_eq!(sequences.len(), 4);
    }

    #[test]
    fn process_writer_helper() {
        let Ok(project) = std::env::var("TUIC_PROGRESS_TEST_PROJECT") else {
            return;
        };
        let summary = std::env::var("TUIC_PROGRESS_TEST_SUMMARY").unwrap();
        ProgressStore::open(project)
            .unwrap()
            .record(&event(ProgressKind::Milestone, &summary, Some("Processes")))
            .unwrap();
    }

    #[test]
    fn separate_processes_serialize_writes_without_losing_events() {
        let project = git_project();
        ProgressStore::open(project.path()).unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut children = (0..3)
            .map(|index| {
                Command::new(&executable)
                    .args([
                        "--exact",
                        "progress::store::tests::process_writer_helper",
                        "--nocapture",
                    ])
                    .env("TUIC_PROGRESS_TEST_PROJECT", project.path())
                    .env(
                        "TUIC_PROGRESS_TEST_SUMMARY",
                        format!("Process {index} recorded an outcome."),
                    )
                    .spawn()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for child in &mut children {
            assert!(child.wait().unwrap().success());
        }
        let page = ProgressStore::open(project.path())
            .unwrap()
            .list(None, Some(10))
            .unwrap();
        assert_eq!(page.events.len(), 3);
        assert_eq!(page.revision, 3);
    }

    #[test]
    fn equal_timestamps_are_ordered_by_monotonic_sequence() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let first = store
            .record(&event(ProgressKind::Milestone, "First.", None))
            .unwrap();
        let second = store
            .record(&event(ProgressKind::Milestone, "Second.", None))
            .unwrap();
        let conn = store.connect().unwrap();
        conn.execute("UPDATE events SET created_at_ms = 7", [])
            .unwrap();
        let page = store.list(None, Some(10)).unwrap();
        assert_eq!(
            page.events
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec![second.id.as_str(), first.id.as_str()]
        );
    }

    #[test]
    fn unknown_provenance_remains_absent_after_restart() {
        let project = git_project();
        let recorded = ProgressStore::open(project.path())
            .unwrap()
            .record(&event(
                ProgressKind::Milestone,
                "An origin-free outcome remains readable.",
                None,
            ))
            .unwrap();
        assert_eq!(recorded.provenance, ProgressProvenance::default());
        let loaded = ProgressStore::open(project.path())
            .unwrap()
            .list(None, Some(1))
            .unwrap()
            .events
            .remove(0);
        assert_eq!(loaded.provenance, ProgressProvenance::default());
    }

    #[test]
    fn sequence_and_revision_are_not_reused_after_history_is_cleared() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let first = store
            .record(&event(
                ProgressKind::Milestone,
                "History existed before clear.",
                Some("Persistence"),
            ))
            .unwrap();
        let mut conn = store.connect().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("DELETE FROM events", []).unwrap();
        tx.execute("DELETE FROM workstreams", []).unwrap();
        let clear_revision = bump_revision(&tx).unwrap();
        tx.commit().unwrap();

        let reopened = ProgressStore::open(project.path()).unwrap();
        let second = reopened
            .record(&event(
                ProgressKind::Milestone,
                "History restarted after clear.",
                Some("Persistence"),
            ))
            .unwrap();
        assert!(second.sequence > first.sequence);
        assert_eq!(second.revision, clear_revision + 1);
    }

    #[test]
    fn missing_project_root_fails_without_creating_a_fallback_store() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("missing");
        let error = ProgressStore::open(&missing).unwrap_err();
        assert!(error.starts_with("progress_store_unavailable:"), "{error}");
        assert!(!missing.exists());
    }

    #[test]
    fn a_busy_database_reports_a_concrete_busy_error() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let locking_connection = store.connect().unwrap();
        locking_connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let error = store
            .record(&event(
                ProgressKind::Milestone,
                "This write must not fall back.",
                None,
            ))
            .unwrap_err();
        assert!(error.starts_with("progress_store_busy:"), "{error}");
        locking_connection.execute_batch("ROLLBACK").unwrap();
        assert!(store.list(None, Some(10)).unwrap().events.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_database_reports_a_concrete_unavailable_error() {
        use std::os::unix::fs::PermissionsExt;

        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let path = store.database_path().to_path_buf();
        let original = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let result = ProgressStore::open(project.path());
        fs::set_permissions(&path, original).unwrap();
        let error = result.unwrap_err();
        assert!(error.starts_with("progress_store_unavailable:"), "{error}");
    }

    #[test]
    fn creates_idempotent_local_git_excludes() {
        let project = git_project();
        ProgressStore::open(project.path()).unwrap();
        ProgressStore::open(project.path()).unwrap();
        let git_dir = crate::git_cli::git_cmd(project.path())
            .args(["rev-parse", "--git-common-dir"])
            .run()
            .unwrap();
        let contents = fs::read_to_string(
            project
                .path()
                .join(git_dir.stdout.trim())
                .join("info/exclude"),
        )
        .unwrap();
        for entry in EXCLUDE_ENTRIES {
            assert_eq!(contents.lines().filter(|line| *line == entry).count(), 1);
        }
        let status = crate::git_cli::git_cmd(project.path())
            .args(["status", "--porcelain", "--untracked-files=all"])
            .run()
            .unwrap();
        assert!(status.stdout.trim().is_empty(), "{}", status.stdout);
    }

    #[test]
    fn corrupt_databases_are_preserved_and_replaced_before_retry() {
        let project = git_project();
        let dir = project.path().join(STORE_DIR);
        fs::create_dir(&dir).unwrap();
        let path = dir.join(STORE_FILE);
        fs::write(&path, b"not sqlite").unwrap();
        let before = fs::read(&path).unwrap();
        let error = ProgressStore::open(project.path()).unwrap_err();
        assert!(error.starts_with("progress_store_recovered:"), "{error}");

        let backups = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| {
                candidate
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("progress.sqlite3.corrupt-"))
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), before);
        assert!(error.contains(&backups[0].display().to_string()));

        let store = ProgressStore::open(project.path()).unwrap();
        let recorded = store
            .record(&event(
                ProgressKind::Milestone,
                "Fresh history accepts events after recovery.",
                Some("Recovery"),
            ))
            .unwrap();
        assert_eq!(recorded.sequence, 1);
        assert_eq!(store.list(None, Some(10)).unwrap().events, vec![recorded]);
        assert_eq!(fs::read(&backups[0]).unwrap(), before);

        let conn = store.connect().unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        validate_schema(&conn).unwrap();
    }

    #[test]
    fn recovery_preserves_sqlite_sidecars_with_the_corrupt_database() {
        let project = git_project();
        let dir = project.path().join(STORE_DIR);
        fs::create_dir(&dir).unwrap();
        let database = dir.join(STORE_FILE);
        fs::write(&database, b"not sqlite").unwrap();
        let wal = append_to_path(&database, "-wal");
        let shm = append_to_path(&database, "-shm");
        fs::write(&wal, b"recoverable wal bytes").unwrap();
        fs::write(&shm, b"recoverable shm bytes").unwrap();

        let error = ProgressStore::open(project.path()).unwrap_err();
        assert!(error.starts_with("progress_store_recovered:"), "{error}");
        let preserved = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(".corrupt-"))
            })
            .collect::<Vec<_>>();
        assert_eq!(preserved.len(), 3);
        assert!(
            preserved
                .iter()
                .any(|path| fs::read(path).unwrap() == b"not sqlite")
        );
        assert!(
            preserved
                .iter()
                .any(|path| fs::read(path).unwrap() == b"recoverable wal bytes")
        );
        assert!(preserved.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("progress.sqlite3-shm.corrupt-"))
                && fs::read(path).unwrap() == b"recoverable shm bytes"
        }));
        assert!(
            preserved
                .iter()
                .all(|path| error.contains(&path.display().to_string()))
        );
        assert!(ProgressStore::open(project.path()).is_ok());
    }

    #[test]
    fn concurrent_open_recovers_a_corrupt_database_exactly_once() {
        let project = git_project();
        let dir = project.path().join(STORE_DIR);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join(STORE_FILE), b"not sqlite").unwrap();
        let root = Arc::new(project.path().to_path_buf());
        let barrier = Arc::new(Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let root = Arc::clone(&root);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    ProgressStore::open(root.as_path())
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| result
                    .as_ref()
                    .is_err_and(|error| error.starts_with("progress_store_recovered:")))
                .count(),
            1
        );
        assert_eq!(
            fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("progress.sqlite3.corrupt-")))
                .count(),
            1
        );
        let backup = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("progress.sqlite3.corrupt-"))
            })
            .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"not sqlite");
        let store = ProgressStore::open(project.path()).unwrap();
        let recorded = store
            .record(&event(
                ProgressKind::Milestone,
                "Concurrent recovery left a usable replacement.",
                Some("Recovery"),
            ))
            .unwrap();
        assert_eq!(recorded.sequence, 1);
        assert_eq!(store.list(None, Some(1)).unwrap().events, vec![recorded]);
        assert_eq!(fs::read(&backup).unwrap(), b"not sqlite");
    }

    #[test]
    fn malformed_schema_is_quarantined_and_replaced_with_schema_v1() {
        let project = git_project();
        let dir = project.path().join(STORE_DIR);
        fs::create_dir(&dir).unwrap();
        let path = dir.join(STORE_FILE);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE unrelated (id INTEGER); PRAGMA user_version = 1;")
            .unwrap();
        drop(conn);

        let error = ProgressStore::open(project.path()).unwrap_err();
        assert!(error.starts_with("progress_store_recovered:"), "{error}");
        let store = ProgressStore::open(project.path()).unwrap();
        validate_schema(&store.connect().unwrap()).unwrap();
    }

    #[test]
    fn missing_schema_metadata_is_recovered_instead_of_failing_later() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let conn = store.connect().unwrap();
        conn.execute("DELETE FROM project_meta", []).unwrap();
        drop(conn);

        let error = ProgressStore::open(project.path()).unwrap_err();
        assert!(error.starts_with("progress_store_recovered:"), "{error}");
        let replacement = ProgressStore::open(project.path()).unwrap();
        assert_eq!(replacement.snapshot().unwrap().revision, 0);
    }

    #[test]
    fn validates_unicode_character_limits() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let too_long = "é".repeat(super::super::model::MAX_SUMMARY_CHARS + 1);
        let error = store
            .record(&event(ProgressKind::Milestone, &too_long, None))
            .unwrap_err();
        assert!(error.contains("at most 500 characters"));
    }

    #[test]
    fn list_clamps_a_zero_limit_up_to_one_page() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        store
            .record(&event(ProgressKind::Milestone, "First.", None))
            .unwrap();
        store
            .record(&event(ProgressKind::Milestone, "Second.", None))
            .unwrap();
        let page = store.list(None, Some(0)).unwrap();
        assert_eq!(
            page.events.len(),
            1,
            "a zero limit must not return zero rows"
        );
    }

    #[test]
    fn list_never_exceeds_the_max_page_limit_regardless_of_requested_size() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        for index in 0..(MAX_PAGE_LIMIT + 5) {
            store
                .record(&event(
                    ProgressKind::Milestone,
                    &format!("Event {index}."),
                    None,
                ))
                .unwrap();
        }
        let page = store.list(None, Some(usize::MAX)).unwrap();
        assert_eq!(page.events.len(), MAX_PAGE_LIMIT);
        assert!(page.next_before_sequence.is_some());

        let default_page = store.list(None, None).unwrap();
        assert_eq!(default_page.events.len(), DEFAULT_PAGE_LIMIT);
    }

    fn reported(summary: &str, reporter: &str) -> NewProgressEvent {
        NewProgressEvent {
            kind: ProgressKind::Milestone,
            summary: summary.to_string(),
            workstream: Some("Delivery".to_string()),
            provenance: ProgressProvenance {
                reporter_id: Some(reporter.to_string()),
                session_id: Some(format!("{reporter}-session")),
                ..Default::default()
            },
        }
    }

    #[test]
    fn exact_retry_returns_the_durable_original_event() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let first = store.report(&reported("API shipped.", "one")).unwrap();
        let retry = store.report(&reported("API shipped.", "one")).unwrap();
        assert_eq!(first.receipt.status, ProgressReceiptStatus::Recorded);
        assert_eq!(retry.receipt.status, ProgressReceiptStatus::Duplicate);
        assert_eq!(retry.receipt.event_id, first.receipt.event_id);
        assert_eq!(store.list(None, Some(10)).unwrap().events.len(), 1);
    }

    #[test]
    fn different_reporters_are_not_deduplicated() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        store.report(&reported("API shipped.", "one")).unwrap();
        let second = store.report(&reported("API shipped.", "two")).unwrap();
        assert_eq!(second.receipt.status, ProgressReceiptStatus::Recorded);
        assert_eq!(store.list(None, Some(10)).unwrap().events.len(), 2);
    }

    #[test]
    fn an_intervening_transition_makes_a_repeated_report_new() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let milestone = reported("API shipped.", "one");
        store.report(&milestone).unwrap();
        let mut blocked = reported("Release approval is pending.", "one");
        blocked.kind = ProgressKind::Blocked;
        store.report(&blocked).unwrap();

        let repeated = store.report(&milestone).unwrap();

        assert_eq!(repeated.receipt.status, ProgressReceiptStatus::Recorded);
        assert_eq!(store.list(None, Some(10)).unwrap().events.len(), 3);
    }

    #[test]
    fn pause_persists_and_resume_only_accepts_future_reports() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        store.set_collection_enabled(false).unwrap();
        drop(store);
        let reopened = ProgressStore::open(project.path()).unwrap();
        let paused = reopened
            .report(&reported("Missed while paused.", "one"))
            .unwrap();
        assert_eq!(paused.receipt.status, ProgressReceiptStatus::Paused);
        assert!(reopened.list(None, Some(10)).unwrap().events.is_empty());
        reopened.set_collection_enabled(true).unwrap();
        assert_eq!(
            reopened
                .report(&reported("Future report.", "one"))
                .unwrap()
                .receipt
                .status,
            ProgressReceiptStatus::Recorded
        );
        assert_eq!(reopened.list(None, Some(10)).unwrap().events.len(), 1);
    }

    #[test]
    fn clear_preserves_monotonic_ids_pauses_and_rejects_stale_revision() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let export = project.path().join("progress.md");
        fs::write(&export, "# Existing export\n").unwrap();
        let old = store
            .record(&event(ProgressKind::Milestone, "Old.", Some("Arc")))
            .unwrap();
        assert!(
            store
                .clear(0)
                .unwrap_err()
                .starts_with("progress_revision_conflict:")
        );
        let cleared = store.clear(old.revision).unwrap();
        assert!(cleared.revision > old.revision);
        assert!(!store.status().unwrap().collection_enabled);
        store.set_collection_enabled(true).unwrap();
        let new = store
            .record(&event(ProgressKind::Milestone, "New.", Some("Arc")))
            .unwrap();
        assert!(new.sequence > old.sequence);
        assert_eq!(fs::read_to_string(export).unwrap(), "# Existing export\n");
    }

    #[test]
    fn delete_rejects_a_wrong_project_id_without_partial_mutation() {
        let first = git_project();
        let second = git_project();
        let a = ProgressStore::open(first.path()).unwrap();
        let b = ProgressStore::open(second.path()).unwrap();
        let owned = a
            .record(&event(ProgressKind::Milestone, "Owned.", Some("API")))
            .unwrap();
        let foreign = b
            .record(&event(ProgressKind::Milestone, "Foreign.", Some("API")))
            .unwrap();

        let error = a
            .delete_events(&[owned.id.clone(), foreign.id])
            .unwrap_err();

        assert!(error.starts_with("progress_event_not_found:"));
        assert_eq!(a.list(None, Some(10)).unwrap().events, vec![owned]);
    }

    #[test]
    fn corrections_merge_history_and_preserve_the_source_event() {
        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let target = store
            .record(&reported("Target summary.", "target-reporter"))
            .unwrap();
        let source = store
            .record(&NewProgressEvent {
                kind: ProgressKind::Blocked,
                summary: "Source blocker.".into(),
                workstream: Some("Source stream".into()),
                provenance: ProgressProvenance {
                    reporter_id: Some("source-reporter".into()),
                    reporter_name: Some("Source Agent".into()),
                    session_id: Some("source-session".into()),
                    workspace_path: Some(project.path().display().to_string()),
                },
            })
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        let target_stream = snapshot
            .workstreams
            .iter()
            .find(|stream| stream.name == "Delivery")
            .unwrap()
            .id
            .clone();
        let source_stream = snapshot
            .workstreams
            .iter()
            .find(|stream| stream.name == "Source stream")
            .unwrap()
            .id
            .clone();

        let first = store
            .update(&ProgressUpdateInput {
                expected_revision: snapshot.revision,
                corrections: vec![
                    ProgressCorrection::EditSummary {
                        event_id: target.id.clone(),
                        summary: "Corrected target.".into(),
                    },
                    ProgressCorrection::ResolveBlocker {
                        event_id: source.id.clone(),
                    },
                    ProgressCorrection::MergeWorkstreams {
                        source_workstream_ids: vec![source_stream],
                        target_workstream_id: target_stream.clone(),
                    },
                    ProgressCorrection::MergeEvents {
                        source_event_ids: vec![source.id.clone()],
                        target_event_id: target.id.clone(),
                    },
                    ProgressCorrection::SetWorkstreamState {
                        workstream_id: target_stream,
                        state: WorkstreamState::Done,
                    },
                ],
            })
            .unwrap();

        let page = store.list(None, Some(10)).unwrap();
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].id, target.id);
        assert_eq!(page.events[0].summary, "Corrected target.");
        assert_eq!(
            store.snapshot().unwrap().workstreams[0].state,
            WorkstreamState::Done
        );
        assert!(first.revision > snapshot.revision);

        let conn = store.connect().unwrap();
        let preserved: String = conn
            .query_row(
                "SELECT source_event_json FROM merged_event_sources WHERE source_event_id=?1",
                [&source.id],
                |row| row.get(0),
            )
            .unwrap();
        let preserved: serde_json::Value = serde_json::from_str(&preserved).unwrap();
        assert_eq!(preserved["reporterId"], "source-reporter");
        assert_eq!(preserved["sessionId"], "source-session");
        assert_eq!(preserved["summary"], "Source blocker.");
    }

    #[test]
    fn clear_and_report_serialize_without_losing_an_accepted_report() {
        use std::sync::{Arc, Barrier};

        let project = git_project();
        let store = ProgressStore::open(project.path()).unwrap();
        let existing = store
            .record(&event(ProgressKind::Milestone, "Existing.", Some("API")))
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let clear_store = store.clone();
        let clear_barrier = barrier.clone();
        let clear = std::thread::spawn(move || {
            clear_barrier.wait();
            clear_store.clear(existing.revision)
        });
        let report_store = store.clone();
        let report = std::thread::spawn(move || {
            barrier.wait();
            report_store.report(&reported("Concurrent report.", "race-reporter"))
        });

        let clear = clear.join().unwrap();
        let report = report.join().unwrap().unwrap();
        let page = store.list(None, Some(10)).unwrap();
        match clear {
            Ok(_) => {
                assert_eq!(report.receipt.status, ProgressReceiptStatus::Paused);
                assert!(page.events.is_empty());
            }
            Err(error) => {
                assert!(error.starts_with("progress_revision_conflict:"));
                assert_eq!(report.receipt.status, ProgressReceiptStatus::Recorded);
                assert!(
                    page.events
                        .iter()
                        .any(|event| event.summary == "Concurrent report.")
                );
            }
        }
    }

    #[test]
    fn viewed_cursor_converges_without_acknowledging_later_or_other_projects() {
        let first = git_project();
        let second = git_project();
        let a = ProgressStore::open(first.path()).unwrap();
        let b = ProgressStore::open(second.path()).unwrap();
        a.record(&event(ProgressKind::Milestone, "Viewed.", None))
            .unwrap();
        let cursor = a.list(None, Some(10)).unwrap().snapshot_cursor;
        a.record(&event(ProgressKind::Milestone, "Later.", None))
            .unwrap();
        b.record(&event(ProgressKind::Milestone, "Other project.", None))
            .unwrap();
        a.acknowledge_read(cursor).unwrap();
        a.acknowledge_read(cursor).unwrap();
        assert_eq!(a.status().unwrap().unread_count, 1);
        assert_eq!(b.status().unwrap().unread_count, 1);
    }
}
