use super::model::{
    NewPlan, NewStory, Plan, PlanState, PlanView, Story, StoryCommand, StoryOrigin, StoryRead,
    StoryStatus, StoryTransition, StoryTransitionActor,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use uuid::Uuid;

mod records;
mod transitions;

use records::*;

const STORE_FILE: &str = "stories.sqlite3";
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const SCHEMA_VERSION: i64 = 1;

#[derive(Debug)]
pub struct StoryStore {
    connection: Mutex<Connection>,
    db_path: PathBuf,
}

impl StoryStore {
    /// The owning host stores story data beside its configuration, never in a repository.
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join(STORE_FILE))
    }

    /// Session teardown is common even when no story has ever been created.
    pub fn release_closed_session(session: &str) -> Result<usize, String> {
        let path = crate::config::config_dir().join(STORE_FILE);
        if !path.exists() {
            return Ok(0);
        }
        // The probe only proves "no claim". When it cannot answer (schema missing or
        // unreadable), the full path creates or rejects the schema as it would on open.
        if let Ok(false) = Self::session_has_claim(&path, session) {
            return Ok(0);
        }
        Self::open_at(&path)?.release_session_claims(session)
    }

    /// Check for a claim without opening the schema or acquiring a write transaction.
    fn session_has_claim(path: &Path, session: &str) -> Result<bool, String> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("open story store read-only: {e}"))?;
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM stories WHERE claim_session=?1)",
            [session],
            |row| row.get(0),
        )
        .map_err(|e| format!("find session claim: {e}"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create story store directory: {e}"))?;
        }
        Ok(Self {
            connection: Mutex::new(Self::open_connection(path)?),
            db_path: path.to_path_buf(),
        })
    }

    fn connect(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "story store connection lock poisoned".into())
    }

    fn open_connection(path: &Path) -> Result<Connection, String> {
        let conn = Connection::open(path).map_err(|e| format!("open story store: {e}"))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| format!("set story store timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("enable story store WAL: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("enable story store foreign keys: {e}"))?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| format!("read story schema version: {e}"))?;
        if version > SCHEMA_VERSION {
            return Err(format!(
                "story schema version {version} is newer than supported version {SCHEMA_VERSION}"
            ));
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plans (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                title TEXT NOT NULL,
                source TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS plans_by_project ON plans(project);
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                plan_id TEXT NOT NULL REFERENCES plans(id),
                document TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN
                  ('backlog','ready','in_progress','review','done','blocked','wontfix')),
                revision INTEGER NOT NULL,
                claim_session TEXT
            );
            CREATE INDEX IF NOT EXISTS stories_by_plan ON stories(plan_id);
            CREATE UNIQUE INDEX IF NOT EXISTS one_story_per_session
              ON stories(claim_session) WHERE claim_session IS NOT NULL;
            CREATE TABLE IF NOT EXISTS workflow_story_proposals (
                run_id TEXT NOT NULL,
                proposal_key TEXT NOT NULL,
                story_id TEXT NOT NULL REFERENCES stories(id),
                input_sha256 TEXT NOT NULL,
                PRIMARY KEY(run_id, proposal_key)
            );
            CREATE TABLE IF NOT EXISTS story_transitions (
                story_id TEXT NOT NULL REFERENCES stories(id),
                revision INTEGER NOT NULL,
                command_json TEXT NOT NULL,
                actor_json TEXT NOT NULL,
                PRIMARY KEY(story_id, revision)
            );
            PRAGMA user_version = 1;",
        )
        .map_err(|e| format!("prepare story schema: {e}"))?;
        Ok(conn)
    }

    pub fn create_plan(&self, input: NewPlan) -> Result<Plan, String> {
        validate_text("project", &input.project, 4096)?;
        validate_text("plan title", &input.title, 200)?;
        validate_text("plan source", &input.source, 4096)?;
        if !crate::fs::is_absolute_on_any_platform(&input.project) {
            return Err("project must be an absolute path".into());
        }
        let plan = Plan {
            id: Uuid::now_v7().to_string(),
            project: input.project.trim().into(),
            title: input.title.trim().into(),
            source: input.source.trim().into(),
        };
        self.connect()?
            .execute(
                "INSERT INTO plans(id,project,title,source) VALUES (?1,?2,?3,?4)",
                params![plan.id, plan.project, plan.title, plan.source],
            )
            .map_err(|e| format!("create plan: {e}"))?;
        Ok(plan)
    }

    pub fn get_plan(&self, id: &str) -> Result<Plan, String> {
        self.connect()?
            .query_row(
                "SELECT id,project,title,source FROM plans WHERE id=?1",
                [id],
                |row| {
                    Ok(Plan {
                        id: row.get(0)?,
                        project: row.get(1)?,
                        title: row.get(2)?,
                        source: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("read plan: {e}"))?
            .ok_or_else(|| format!("plan not found: {id}"))
    }

    pub fn list_plans(&self, project: &str) -> Result<Vec<Plan>, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id,project,title,source FROM plans WHERE project=?1 ORDER BY rowid")
            .map_err(|e| format!("prepare plan list: {e}"))?;
        stmt.query_map([project], |row| {
            Ok(Plan {
                id: row.get(0)?,
                project: row.get(1)?,
                title: row.get(2)?,
                source: row.get(3)?,
            })
        })
        .map_err(|e| format!("list plans: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read plans: {e}"))
    }

    pub fn create_story(&self, input: NewStory) -> Result<Story, String> {
        validate_new_story(&input)?;
        self.get_plan(&input.plan_id)?;
        let story = build_story(input);
        let conn = self.connect()?;
        insert_story(&conn, &story)?;
        Ok(story)
    }

    /// The coordinator's proposal key is persisted in the same transaction as
    /// the story. A retry after a lost MCP response returns the original story;
    /// reusing the key for different work is rejected.
    pub fn create_story_once(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String> {
        validate_text("workflow run id", run_id, 128)?;
        validate_text("story proposal key", proposal_key, 128)?;
        validate_new_story(&input)?;
        let hash = proposal_hash(&input)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        if let Some((story_id, prior_hash)) = tx
            .query_row(
                "SELECT story_id,input_sha256 FROM workflow_story_proposals WHERE run_id=?1 AND proposal_key=?2",
                params![run_id, proposal_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| format!("read story proposal: {error}"))?
        {
            if prior_hash != hash {
                return Err("story proposal key was reused with a different payload".into());
            }
            return read_story(&tx, &story_id);
        }
        let exists: bool = tx
            .query_row("SELECT 1 FROM plans WHERE id=?1", [&input.plan_id], |_| {
                Ok(true)
            })
            .optional()
            .map_err(|error| format!("read proposal plan: {error}"))?
            .unwrap_or(false);
        if !exists {
            return Err("story proposal plan not found".into());
        }
        let story = build_story(input);
        insert_story(&tx, &story)?;
        tx.execute(
            "INSERT INTO workflow_story_proposals(run_id,proposal_key,story_id,input_sha256) VALUES (?1,?2,?3,?4)",
            params![run_id, proposal_key, story.id, hash],
        )
        .map_err(|error| format!("record story proposal: {error}"))?;
        tx.commit()
            .map_err(|error| format!("commit story proposal: {error}"))?;
        Ok(story)
    }

    /// Observe an uncertain effect without replaying an external creation.
    pub fn existing_story_for_proposal(
        &self,
        run_id: &str,
        proposal_key: &str,
        input: &NewStory,
    ) -> Result<Option<Story>, String> {
        validate_text("workflow run id", run_id, 128)?;
        validate_text("story proposal key", proposal_key, 128)?;
        let hash = proposal_hash(input)?;
        let conn = self.connect()?;
        let prior: Option<(String, String)> = conn
            .query_row(
                "SELECT story_id,input_sha256 FROM workflow_story_proposals WHERE run_id=?1 AND proposal_key=?2",
                params![run_id, proposal_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| format!("read story proposal: {error}"))?;
        match prior {
            Some((story_id, prior_hash)) if prior_hash == hash => {
                read_story(&conn, &story_id).map(Some)
            }
            Some(_) => Err("story proposal key was reused with a different payload".into()),
            None => Ok(None),
        }
    }

    pub fn get_story(&self, id: &str) -> Result<Story, String> {
        read_story(&*self.connect()?, id)
    }

    pub fn list_stories(&self, plan_id: &str) -> Result<Vec<Story>, String> {
        self.get_plan(plan_id)?;
        read_plan_stories(&*self.connect()?, plan_id)
    }

    pub fn plan_view(&self, plan_id: &str) -> Result<PlanView, String> {
        let stories = self.list_stories(plan_id)?;
        let by_id: HashMap<&str, &Story> = stories
            .iter()
            .map(|story| (story.id.as_str(), story))
            .collect();
        let wont_fix_count = stories
            .iter()
            .filter(|story| story.status == StoryStatus::WontFix)
            .count();
        let state = if stories.is_empty() {
            PlanState::Draft
        } else if stories
            .iter()
            .all(|story| matches!(story.status, StoryStatus::Done | StoryStatus::WontFix))
        {
            PlanState::Done
        } else {
            PlanState::Active
        };
        let reads = stories
            .iter()
            .map(|story| {
                let mut stack = vec![story.id.as_str()];
                let mut visited = HashSet::new();
                let mut abandoned = false;
                while let Some(id) = stack.pop() {
                    if !visited.insert(id) {
                        continue;
                    }
                    if let Some(found) = by_id.get(id) {
                        if found.status == StoryStatus::WontFix {
                            abandoned = true;
                            break;
                        }
                        stack.extend(found.dependencies.iter().map(String::as_str));
                    }
                }
                StoryRead {
                    story: story.clone(),
                    abandoned,
                }
            })
            .collect();
        Ok(PlanView {
            stories: reads,
            state,
            wont_fix_count,
            all_cancelled: !stories.is_empty() && wont_fix_count == stories.len(),
        })
    }

    pub fn plan_state(&self, plan_id: &str) -> Result<PlanState, String> {
        self.get_plan(plan_id)?;
        let (total, unfinished): (i64, i64) = self
            .connect()?
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status IN ('done', 'wontfix') THEN 0 ELSE 1 END), 0) FROM stories WHERE plan_id=?1",
                [plan_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| format!("read plan state: {e}"))?;
        if total == 0 {
            return Ok(PlanState::Draft);
        }
        if unfinished == 0 {
            return Ok(PlanState::Done);
        }
        Ok(PlanState::Active)
    }

    pub fn claim(
        &self,
        story_id: &str,
        session: &str,
        expected_revision: i64,
    ) -> Result<Story, String> {
        validate_text("session", session, 200)?;
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let mut story = read_story(&tx, story_id)?;
        check_revision(&story, expected_revision)?;
        if story.status != StoryStatus::Ready {
            return Err("story is not ready".into());
        }
        if !dependencies_integrated(&tx, &story, &self.db_path)? {
            return Err("story dependency lacks a current integration receipt".into());
        }
        story.status = StoryStatus::InProgress;
        story.claim_session = Some(session.into());
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit().map_err(|e| format!("commit claim: {e}"))?;
        Ok(story)
    }
    /// A manual claim belongs to a live tab, not to a durable workflow reservation.
    pub fn release_session_claims(&self, session: &str) -> Result<usize, String> {
        let mut conn = self.connect()?;
        let tx = immediate(&mut conn)?;
        let ids = {
            let mut stmt = tx
                .prepare("SELECT id FROM stories WHERE claim_session=?1")
                .map_err(|e| format!("find session claims: {e}"))?;
            stmt.query_map([session], |row| row.get::<_, String>(0))
                .map_err(|e| format!("find session claims: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("read session claims: {e}"))?
        };
        for id in &ids {
            let mut story = read_story(&tx, id)?;
            let revision = story.revision;
            story.claim_session = None;
            if story.status == StoryStatus::InProgress {
                story.status = StoryStatus::Ready;
            }
            save_story(&tx, &mut story, revision)?;
        }
        tx.commit()
            .map_err(|e| format!("commit claim release: {e}"))?;
        Ok(ids.len())
    }
}

#[cfg(test)]
mod connection_tests {
    use super::*;

    #[test]
    fn story_store_reuses_its_sqlite_connection() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).expect("store");
        store
            .connect()
            .expect("connection")
            .execute_batch("CREATE TEMP TABLE story_connection_sentinel(value INTEGER); INSERT INTO story_connection_sentinel VALUES (1);")
            .expect("create connection-local table");

        let count: i64 = store
            .connect()
            .expect("same connection")
            .query_row(
                "SELECT count(*) FROM story_connection_sentinel",
                [],
                |row| row.get(0),
            )
            .expect("connection-local table survives method calls");
        assert_eq!(count, 1);
    }
}

fn validate_new_story(input: &NewStory) -> Result<(), String> {
    validate_text("story title", &input.title, 200)?;
    if let StoryOrigin::PlanStep { step } = &input.origin {
        validate_text("plan step", step, 200)?;
    }
    if input.criteria.is_empty() || input.criteria.len() > 100 {
        return Err("story must have between 1 and 100 criteria".into());
    }
    for criterion in &input.criteria {
        validate_text("criterion", criterion, 2000)?;
    }
    if !(1..=3).contains(&input.priority) {
        return Err("priority must be 1, 2 or 3".into());
    }
    if input.file_scope.len() > 100 {
        return Err("file scope has too many paths".into());
    }
    for path in &input.file_scope {
        validate_scope_path(path)?;
    }
    Ok(())
}

fn proposal_hash(input: &NewStory) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(input).map_err(|error| format!("encode story proposal: {error}"))?,
    )))
}

fn build_story(input: NewStory) -> Story {
    Story {
        id: Uuid::now_v7().to_string(),
        plan_id: input.plan_id,
        title: input.title.trim().into(),
        checked: vec![false; input.criteria.len()],
        criteria: input.criteria,
        dependencies: Vec::new(),
        priority: input.priority,
        origin: input.origin,
        file_scope: input.file_scope,
        status: StoryStatus::Ready,
        revision: 1,
        claim_session: None,
    }
}

fn insert_story(conn: &Connection, story: &Story) -> Result<(), String> {
    conn.execute(
        "INSERT INTO stories(id,plan_id,document,status,revision,claim_session)
         VALUES (?1,?2,?3,?4,?5,NULL)",
        params![
            story.id,
            story.plan_id,
            encode(story)?,
            story.status.as_str(),
            story.revision
        ],
    )
    .map_err(|error| format!("create story: {error}"))?;
    Ok(())
}
