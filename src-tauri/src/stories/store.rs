use super::model::{NewPlan, NewStory, Plan, PlanState, Story, StoryCommand, StoryStatus};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

mod records;
mod transitions;

use records::*;

const STORE_FILE: &str = "stories.sqlite3";
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug)]
pub struct StoryStore {
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
        Self::open_at(&path)?.release_session_claims(session)
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create story store directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("open story store: {e}"))?;
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
        validate_text("story title", &input.title, 200)?;
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
        self.get_plan(&input.plan_id)?;
        let story = Story {
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
        };
        self.connect()?
            .execute(
                "INSERT INTO stories(id,plan_id,document,status,revision,claim_session)
             VALUES (?1,?2,?3,?4,?5,NULL)",
                params![
                    story.id,
                    story.plan_id,
                    encode(&story)?,
                    story.status.as_str(),
                    story.revision
                ],
            )
            .map_err(|e| format!("create story: {e}"))?;
        Ok(story)
    }

    pub fn get_story(&self, id: &str) -> Result<Story, String> {
        read_story(&self.connect()?, id)
    }

    pub fn list_stories(&self, plan_id: &str) -> Result<Vec<Story>, String> {
        self.get_plan(plan_id)?;
        read_plan_stories(&self.connect()?, plan_id)
    }

    pub fn plan_state(&self, plan_id: &str) -> Result<PlanState, String> {
        let stories = self.list_stories(plan_id)?;
        if stories.is_empty() {
            return Ok(PlanState::Draft);
        }
        if stories.iter().all(|s| s.status == StoryStatus::Done) {
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
        story.status = StoryStatus::InProgress;
        story.claim_session = Some(session.into());
        save_story(&tx, &mut story, expected_revision)?;
        tx.commit().map_err(|e| format!("commit claim: {e}"))?;
        Ok(story)
    }

    /// A manual claim belongs to a live tab and is released at tab teardown.
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
