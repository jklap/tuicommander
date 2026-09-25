use super::model::*;
use super::reducer::apply_event;
use crate::stories::{NewStory, Story, StoryOrigin, StoryStatus, StoryStore};
use crate::workflows::{NodeKind, WorkflowKind, WorkflowStore};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RunStore {
    db_path: PathBuf,
}

impl RunStore {
    pub fn open() -> Result<Self, String> {
        Self::open_at(&crate::config::config_dir().join("workflow_runs.sqlite3"))
    }

    pub(crate) fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create workflow run directory: {e}"))?;
        }
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.connect()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let conn =
            Connection::open(&self.db_path).map_err(|e| format!("open workflow run store: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| format!("workflow run busy timeout: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("workflow run WAL: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("workflow run foreign keys: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_runs (
                id TEXT PRIMARY KEY, project TEXT NOT NULL, plan_id TEXT NOT NULL,
                status TEXT NOT NULL, snapshot_json TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS one_active_workflow_per_plan
                ON workflow_runs(project,plan_id) WHERE status IN ('running','paused');
            CREATE TABLE IF NOT EXISTS workflow_events (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), sequence INTEGER NOT NULL,
                command_id TEXT NOT NULL, event_json TEXT NOT NULL, receipt_json TEXT NOT NULL,
                PRIMARY KEY(run_id,sequence), UNIQUE(run_id,command_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_story_executions (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), story_id TEXT NOT NULL,
                document_json TEXT NOT NULL, PRIMARY KEY(run_id,story_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_node_attempts (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), attempt_id TEXT NOT NULL,
                document_json TEXT NOT NULL, PRIMARY KEY(run_id,attempt_id)
            );
            CREATE TABLE IF NOT EXISTS workflow_effects (
                run_id TEXT NOT NULL REFERENCES workflow_runs(id), effect_id TEXT NOT NULL,
                effect_key TEXT NOT NULL, document_json TEXT NOT NULL,
                PRIMARY KEY(run_id,effect_id), UNIQUE(run_id,effect_key)
            );",
        )
        .map_err(|e| format!("prepare workflow run schema: {e}"))?;
        Ok(conn)
    }

    pub fn start_plan(
        &self,
        project: &str,
        plan_id: &str,
        definition_id: &str,
        definition_revision: i64,
        limits: RunLimits,
    ) -> Result<RunSnapshot, String> {
        limits.validate()?;
        let owner = crate::progress::resolve_owning_project(Some(project))?
            .to_string_lossy()
            .to_string();
        let plan = StoryStore::open()?.get_plan(plan_id)?;
        if plan.project != owner {
            return Err("plan does not belong to project".into());
        }
        let published = WorkflowStore::open()?.get_published(definition_id, definition_revision)?;
        if published.project != owner || published.kind != WorkflowKind::Plan {
            return Err("published plan workflow does not belong to project".into());
        }
        let dispatches: Vec<_> = published
            .graph
            .nodes
            .iter()
            .filter_map(|node| {
                if let NodeKind::StoryDispatch {
                    story_template_id,
                    story_revision,
                } = &node.kind
                {
                    Some((story_template_id.clone(), *story_revision))
                } else {
                    None
                }
            })
            .collect();
        if dispatches.len() != 1 {
            return Err("plan workflow requires one Story Dispatch node".into());
        }
        let (story_definition_id, story_definition_revision) = dispatches[0].clone();
        let story_definition = WorkflowStore::open()?
            .get_published(&story_definition_id, story_definition_revision)?;
        if story_definition.kind != WorkflowKind::Story || story_definition.project != owner {
            return Err("pinned story workflow does not belong to project".into());
        }
        let initial = RunSnapshot {
            id: Uuid::now_v7().to_string(),
            project: owner,
            plan_id: plan_id.into(),
            definition_id: definition_id.into(),
            definition_revision,
            story_definition_id,
            story_definition_revision,
            status: RunStatus::Running,
            sequence: 0,
            started_ms: now_ms(),
            limits,
            loops: 0,
            story_creations: 0,
            spawns: 0,
            planning_fingerprint: None,
            verification_fingerprint: None,
            stories: vec![],
            attempts: vec![],
            effects: vec![],
        };
        let event = RunEvent {
            sequence: 1,
            command_id: format!("start:{}", initial.id),
            command_hash: None,
            at_ms: initial.started_ms,
            kind: RunEventKind::Started {
                initial: Box::new(initial.clone()),
            },
        };
        let snapshot = apply_event(None, &event)?;
        let receipt = RunReceipt {
            sequence: 1,
            event: event.clone(),
            snapshot: snapshot.clone(),
        };
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin run start: {e}"))?;
        tx.execute("INSERT INTO workflow_runs(id,project,plan_id,status,snapshot_json) VALUES (?1,?2,?3,?4,?5)",
            params![snapshot.id, snapshot.project, snapshot.plan_id, snapshot.status.as_str(), encode(&snapshot)?])
            .map_err(|e| format!("start plan run: {e}"))?;
        insert_event(&tx, &snapshot.id, &receipt)?;
        tx.commit()
            .map_err(|e| format!("commit plan run start: {e}"))?;
        Ok(snapshot)
    }

    pub fn snapshot(&self, run_id: &str) -> Result<RunSnapshot, String> {
        read_snapshot(&self.connect()?, run_id)
    }

    pub fn events_after(
        &self,
        run_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<RunEvent>, String> {
        self.snapshot(run_id)?;
        if after_sequence < 0 || limit == 0 || limit > 500 {
            return Err("invalid workflow event cursor or limit".into());
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT event_json FROM workflow_events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")
            .map_err(|e| format!("prepare workflow event replay: {e}"))?;
        stmt.query_map(params![run_id, after_sequence, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("read workflow events: {e}"))?
        .map(|row| decode(&row.map_err(|e| format!("read workflow event: {e}"))?))
        .collect()
    }

    pub fn replay(&self, run_id: &str) -> Result<RunSnapshot, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT event_json FROM workflow_events WHERE run_id=?1 ORDER BY sequence")
            .map_err(|e| format!("prepare workflow replay: {e}"))?;
        let rows = stmt
            .query_map([run_id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("read workflow replay: {e}"))?;
        let mut snapshot = None;
        for row in rows {
            let event: RunEvent = decode(&row.map_err(|e| format!("read replay event: {e}"))?)?;
            snapshot = Some(apply_event(snapshot, &event)?);
        }
        snapshot.ok_or("workflow run has no events".into())
    }

    pub fn command(
        &self,
        run_id: &str,
        command_id: &str,
        command: RunCommand,
    ) -> Result<RunReceipt, String> {
        self.command_at(run_id, command_id, command, now_ms())
    }

    /// Complete a previously reserved spawn effect in the same event that binds
    /// the live managed session to its attempt.
    pub fn bind_agent(
        &self,
        run_id: &str,
        attempt_id: &str,
        binding: AgentBinding,
    ) -> Result<RunReceipt, String> {
        self.command(
            run_id,
            &format!("bind-agent:{attempt_id}"),
            RunCommand::BindAgent {
                attempt_id: attempt_id.into(),
                binding,
            },
        )
    }

    /// The caller session is resolved from the MCP connection, never from an
    /// agent-supplied field. A stable command ID makes retries idempotent.
    pub fn report_bound_agent(
        &self,
        report: AttemptReport,
        caller_session: &str,
    ) -> Result<RunReceipt, String> {
        let run_id = report.run_id.clone();
        let command_id = format!("agent-report:{}", report.attempt_id);
        self.command(
            &run_id,
            &command_id,
            RunCommand::ReportBoundAttempt {
                caller_session: caller_session.into(),
                report,
            },
        )
    }

    /// Create plan work only for the run's bound coordinator. The reservation
    /// and the story database each have a durable key, so a lost tool response
    /// can be retried without creating another story.
    pub fn create_story_from_coordinator(
        &self,
        run_id: &str,
        caller_session: &str,
        proposal_key: &str,
        input: NewStory,
    ) -> Result<Story, String> {
        validate_key("agent session", caller_session)?;
        if proposal_key.trim().is_empty() || proposal_key.len() > 64 || proposal_key.contains('\0')
        {
            return Err("invalid story proposal key".into());
        }
        if !matches!(&input.origin, StoryOrigin::PlanStep { .. }) {
            return Err("workflow stories require a plan-step origin".into());
        }
        let snapshot = self.snapshot(run_id)?;
        if input.plan_id != snapshot.plan_id
            || !snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == snapshot.plan_id
                    && attempt.state == AttemptState::Running
                    && attempt
                        .agent
                        .as_ref()
                        .is_some_and(|agent| agent.session_id == caller_session)
            })
        {
            return Err("only the active run coordinator may create plan stories".into());
        }
        let effect_key = format!("create-story:{proposal_key}");
        let effect = if let Some(effect) = snapshot
            .effects
            .iter()
            .find(|effect| effect.key == effect_key)
        {
            if effect.kind != EffectKind::CreateStory || effect.state == EffectState::Failed {
                return Err("story proposal effect cannot be retried".into());
            }
            effect.clone()
        } else {
            if snapshot.status != RunStatus::Running {
                return Err("workflow run is not running".into());
            }
            let reserved = self.command(
                run_id,
                &format!("create-story-intent:{proposal_key}"),
                RunCommand::ReserveEffect {
                    key: effect_key,
                    kind: EffectKind::CreateStory,
                },
            )?;
            let RunEventKind::EffectReserved { effect } = reserved.event.kind else {
                return Err("workflow state changed before story proposal".into());
            };
            effect
        };
        let story_store = StoryStore::open()?;
        let creation = match effect.state {
            EffectState::Intended => {
                // Hold the run writer lock across the other database's story
                // transaction. Cancel must either win first (no story is
                // created) or observe a story committed before cancellation.
                let mut conn = self.connect()?;
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(|error| format!("begin workflow story creation: {error}"))?;
                let current = read_snapshot(&tx, run_id)?;
                if current.status != RunStatus::Running
                    || !current
                        .effects
                        .iter()
                        .any(|entry| entry.id == effect.id && entry.state == EffectState::Intended)
                    || !current.attempts.iter().any(|attempt| {
                        attempt.story_id == current.plan_id
                            && attempt.state == AttemptState::Running
                            && attempt
                                .agent
                                .as_ref()
                                .is_some_and(|agent| agent.session_id == caller_session)
                    })
                {
                    return Err("workflow stopped before story creation".into());
                }
                let created = story_store.create_story_once(run_id, proposal_key, input);
                tx.commit()
                    .map_err(|error| format!("commit workflow story reservation: {error}"))?;
                created
            }
            EffectState::Uncertain => {
                return Err(
                    "story proposal outcome is uncertain; operator reconciliation required".into(),
                );
            }
            EffectState::Succeeded => story_store
                .existing_story_for_proposal(run_id, proposal_key, &input)?
                .ok_or("completed story proposal has no story receipt".into()),
            EffectState::Failed => unreachable!(),
        };
        let story = match creation {
            Ok(story) => story,
            Err(error) => {
                if effect.state == EffectState::Intended {
                    let _ = self.command(
                        run_id,
                        &format!("create-story-failed:{proposal_key}"),
                        RunCommand::MarkEffect {
                            effect_id: effect.id,
                            succeeded: false,
                        },
                    );
                }
                return Err(error);
            }
        };
        let completed = match effect.state {
            EffectState::Intended => Some(RunCommand::MarkEffect {
                effect_id: effect.id,
                succeeded: true,
            }),
            EffectState::Succeeded => None,
            EffectState::Uncertain | EffectState::Failed => unreachable!(),
        };
        if let Some(command) = completed {
            self.command(
                run_id,
                &format!("create-story-complete:{proposal_key}"),
                command,
            )?;
        }
        Ok(story)
    }

    /// Process exit is an observation, never an outcome report. Fence it in the
    /// same transaction as the event so a racing semantic report wins or loses
    /// cleanly without converting an exit code into success.
    pub fn interrupt_agent_session(&self, session_id: &str) -> Result<Vec<RunSnapshot>, String> {
        validate_key("agent session", session_id)?;
        let mut conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id FROM workflow_runs WHERE status IN ('running','paused')")
            .map_err(|error| format!("prepare agent exit lookup: {error}"))?;
        let run_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|error| format!("read agent exit runs: {error}"))?
            .collect::<Result<_, _>>()
            .map_err(|error| format!("read agent exit run: {error}"))?;
        drop(stmt);
        let mut changed = Vec::new();
        for run_id in run_ids {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| format!("begin agent exit: {error}"))?;
            let snapshot = read_snapshot(&tx, &run_id)?;
            if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
                continue;
            }
            let attempts: Vec<String> = snapshot
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.state == AttemptState::Running
                        && attempt
                            .agent
                            .as_ref()
                            .is_some_and(|agent| agent.session_id == session_id)
                })
                .map(|attempt| attempt.id.clone())
                .collect();
            let had_pending = !attempts.is_empty();
            let mut current = snapshot;
            for attempt_id in attempts {
                let command_id = format!("agent-exit:{attempt_id}");
                current = persist_event(
                    &tx,
                    current,
                    &command_id,
                    None,
                    now_ms(),
                    RunEventKind::AttemptInterrupted { attempt_id },
                )?
                .snapshot;
            }
            tx.commit()
                .map_err(|error| format!("commit agent exit: {error}"))?;
            if had_pending {
                changed.push(current);
            }
        }
        Ok(changed)
    }

    pub fn command_expected(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: i64,
        command: RunCommand,
    ) -> Result<RunReceipt, String> {
        if expected_sequence < 1 {
            return Err("invalid expected workflow sequence".into());
        }
        self.command_at_checked(
            run_id,
            command_id,
            Some(expected_sequence),
            command,
            now_ms(),
        )
    }

    pub(crate) fn command_at(
        &self,
        run_id: &str,
        command_id: &str,
        command: RunCommand,
        at_ms: i64,
    ) -> Result<RunReceipt, String> {
        self.command_at_checked(run_id, command_id, None, command, at_ms)
    }

    fn command_at_checked(
        &self,
        run_id: &str,
        command_id: &str,
        expected_sequence: Option<i64>,
        command: RunCommand,
        at_ms: i64,
    ) -> Result<RunReceipt, String> {
        validate_key("command id", command_id)?;
        if command_id.starts_with("start:") || command_id.starts_with("reconcile-") {
            return Err("reserved workflow command id".into());
        }
        let command_hash = hex::encode(Sha256::digest(
            encode(&(expected_sequence, &command))?.as_bytes(),
        ));
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow command: {e}"))?;
        if let Some(receipt) = read_command_receipt(&tx, run_id, command_id)? {
            if receipt.event.command_hash.as_deref() != Some(command_hash.as_str()) {
                return Err("workflow command id was reused with a different payload".into());
            }
            return Ok(receipt);
        }
        let snapshot = read_snapshot(&tx, run_id)?;
        if expected_sequence.is_some_and(|expected| expected != snapshot.sequence) {
            return Err("stale workflow sequence".into());
        }
        let kind = choose_event(&snapshot, command, at_ms)?;
        let receipt = persist_event(&tx, snapshot, command_id, Some(command_hash), at_ms, kind)?;
        tx.commit()
            .map_err(|e| format!("commit workflow command: {e}"))?;
        Ok(receipt)
    }

    /// Mark uncertain external effects and interrupted attempts; never replay them.
    pub fn reconcile(&self, run_id: &str) -> Result<RunSnapshot, String> {
        let snapshot = self.snapshot(run_id)?;
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            return Ok(snapshot);
        }
        for effect in snapshot
            .effects
            .iter()
            .filter(|effect| effect.state == EffectState::Intended)
        {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-effect:{}", effect.id),
                RunEventKind::EffectChanged {
                    effect_id: effect.id.clone(),
                    state: EffectState::Uncertain,
                },
            )?;
        }
        for attempt in snapshot
            .attempts
            .iter()
            .filter(|attempt| attempt.state == AttemptState::Running)
        {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-attempt:{}", attempt.id),
                RunEventKind::AttemptInterrupted {
                    attempt_id: attempt.id.clone(),
                },
            )?;
        }
        if self.snapshot(run_id)?.status == RunStatus::Running {
            self.append_reconcile_event(
                run_id,
                &format!("reconcile-run:{run_id}"),
                RunEventKind::Paused,
            )?;
        }
        self.snapshot(run_id)
    }

    /// Called once at process startup, before new workflow work is accepted.
    pub fn reconcile_active(&self) -> Result<usize, String> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT id FROM workflow_runs WHERE status IN ('running','paused')")
            .map_err(|e| format!("prepare active workflow runs: {e}"))?;
        let run_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("read active workflow runs: {e}"))?
            .map(|row| row.map_err(|e| format!("read active workflow run: {e}")))
            .collect::<Result<_, _>>()?;
        drop(stmt);
        drop(conn);
        for run_id in &run_ids {
            self.reconcile(run_id)?;
        }
        Ok(run_ids.len())
    }

    fn append_reconcile_event(
        &self,
        run_id: &str,
        command_id: &str,
        kind: RunEventKind,
    ) -> Result<(), String> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| format!("begin workflow reconcile: {e}"))?;
        if read_command_receipt(&tx, run_id, command_id)?.is_none() {
            let snapshot = read_snapshot(&tx, run_id)?;
            persist_event(&tx, snapshot, command_id, None, now_ms(), kind)?;
        }
        tx.commit()
            .map_err(|e| format!("commit workflow reconcile: {e}"))?;
        Ok(())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn validate_key(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 128 || value.contains('\0') {
        return Err(format!("invalid {label}"));
    }
    Ok(())
}

fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("encode workflow run: {e}"))
}
fn decode<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|e| format!("decode workflow run: {e}"))
}

fn read_snapshot(conn: &Connection, run_id: &str) -> Result<RunSnapshot, String> {
    let raw: String = conn
        .query_row(
            "SELECT snapshot_json FROM workflow_runs WHERE id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read workflow run: {e}"))?
        .ok_or("workflow run not found")?;
    decode(&raw)
}

fn read_command_receipt(
    conn: &Connection,
    run_id: &str,
    command_id: &str,
) -> Result<Option<RunReceipt>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT receipt_json FROM workflow_events WHERE run_id=?1 AND command_id=?2",
            params![run_id, command_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read workflow command receipt: {e}"))?;
    raw.map(|raw| decode(&raw)).transpose()
}

fn insert_event(conn: &Connection, run_id: &str, receipt: &RunReceipt) -> Result<(), String> {
    conn.execute("INSERT INTO workflow_events(run_id,sequence,command_id,event_json,receipt_json) VALUES (?1,?2,?3,?4,?5)",
        params![run_id, receipt.sequence, receipt.event.command_id, encode(&receipt.event)?, encode(receipt)?])
        .map_err(|e| format!("append workflow event: {e}"))?;
    Ok(())
}

fn persist_event(
    conn: &Connection,
    previous: RunSnapshot,
    command_id: &str,
    command_hash: Option<String>,
    at_ms: i64,
    kind: RunEventKind,
) -> Result<RunReceipt, String> {
    let event = RunEvent {
        sequence: previous.sequence + 1,
        command_id: command_id.into(),
        command_hash,
        at_ms,
        kind,
    };
    let snapshot = apply_event(Some(previous.clone()), &event)?;
    let receipt = RunReceipt {
        sequence: event.sequence,
        event,
        snapshot: snapshot.clone(),
    };
    insert_event(conn, &snapshot.id, &receipt)?;
    let changed = conn
        .execute(
            "UPDATE workflow_runs SET status=?1,snapshot_json=?2 WHERE id=?3 AND snapshot_json=?4",
            params![
                snapshot.status.as_str(),
                encode(&snapshot)?,
                snapshot.id,
                encode(&previous)?
            ],
        )
        .map_err(|e| format!("update workflow projection: {e}"))?;
    if changed != 1 {
        return Err("workflow projection changed concurrently".into());
    }
    replace_projections(conn, &snapshot)?;
    Ok(receipt)
}

fn replace_projections(conn: &Connection, snapshot: &RunSnapshot) -> Result<(), String> {
    conn.execute(
        "DELETE FROM workflow_story_executions WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh story executions: {e}"))?;
    conn.execute(
        "DELETE FROM workflow_node_attempts WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh node attempts: {e}"))?;
    conn.execute(
        "DELETE FROM workflow_effects WHERE run_id=?1",
        [&snapshot.id],
    )
    .map_err(|e| format!("refresh effect intents: {e}"))?;
    for story in &snapshot.stories {
        conn.execute("INSERT INTO workflow_story_executions(run_id,story_id,document_json) VALUES (?1,?2,?3)",
            params![snapshot.id, story.story_id, encode(story)?]).map_err(|e| format!("project story execution: {e}"))?;
    }
    for attempt in &snapshot.attempts {
        conn.execute(
            "INSERT INTO workflow_node_attempts(run_id,attempt_id,document_json) VALUES (?1,?2,?3)",
            params![snapshot.id, attempt.id, encode(attempt)?],
        )
        .map_err(|e| format!("project node attempt: {e}"))?;
    }
    for effect in &snapshot.effects {
        conn.execute("INSERT INTO workflow_effects(run_id,effect_id,effect_key,document_json) VALUES (?1,?2,?3,?4)",
            params![snapshot.id, effect.id, effect.key, encode(effect)?]).map_err(|e| format!("project effect intent: {e}"))?;
    }
    Ok(())
}

fn fingerprint(stories: &[Story], include_execution: bool) -> Result<String, String> {
    let mut sorted = stories.to_vec();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let shape: Vec<_> = sorted
        .iter()
        .map(|story| {
            if include_execution {
                serde_json::json!([story.id, story.revision, story.status, story.checked])
            } else {
                serde_json::json!([
                    story.id,
                    story.title,
                    story.criteria,
                    story.dependencies,
                    story.priority,
                    story.origin,
                    story.file_scope
                ])
            }
        })
        .collect();
    let bytes = serde_json::to_vec(&shape).map_err(|e| format!("encode plan fingerprint: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn plan_stories(snapshot: &RunSnapshot) -> Result<Vec<Story>, String> {
    StoryStore::open()?.list_stories(&snapshot.plan_id)
}

pub(super) fn ready_to_verify(snapshot: &RunSnapshot, stories: &[Story]) -> Result<(), String> {
    if snapshot.planning_fingerprint.is_none() {
        return Err("planning is still open".into());
    }
    if snapshot
        .attempts
        .iter()
        .any(|attempt| attempt.state == AttemptState::Running)
        || snapshot.effects.iter().any(|effect| {
            effect.state == EffectState::Intended || effect.state == EffectState::Uncertain
        })
    {
        return Err("workflow has active or uncertain work".into());
    }
    if !stories.iter().all(|story| {
        story.status == StoryStatus::Done
            && snapshot.stories.iter().any(|execution| {
                execution.story_id == story.id
                    && execution.accepted
                    && execution.accepted_revision == Some(story.revision)
            })
    }) {
        return Err("not all plan stories have accepted terminal outcomes".into());
    }
    Ok(())
}

fn choose_event(
    snapshot: &RunSnapshot,
    command: RunCommand,
    at_ms: i64,
) -> Result<RunEventKind, String> {
    if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
        return Err("terminal workflow cannot advance".into());
    }
    let expired = at_ms > snapshot.started_ms + i64::from(snapshot.limits.max_duration_secs) * 1000;
    if expired
        && !matches!(
            command,
            RunCommand::Cancel
                | RunCommand::Pause
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
        )
    {
        return Ok(RunEventKind::Paused);
    }
    let stories = plan_stories(snapshot)?;
    let current_plan = fingerprint(&stories, false)?;
    if snapshot
        .planning_fingerprint
        .as_deref()
        .is_some_and(|closed| closed != current_plan)
        && !matches!(
            command,
            RunCommand::Cancel
                | RunCommand::Pause
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
        )
    {
        return Ok(RunEventKind::PlanningReopened);
    }
    if snapshot.status == RunStatus::Paused
        && !matches!(
            command,
            RunCommand::Resume
                | RunCommand::Cancel
                | RunCommand::ResolveUncertainEffect { .. }
                | RunCommand::BindAgent { .. }
                | RunCommand::MarkEffect { .. }
                | RunCommand::ReportAttempt { .. }
                | RunCommand::ReportBoundAttempt { .. }
        )
    {
        return Err("workflow is paused".into());
    }
    match command {
        RunCommand::ClosePlanning => Ok(RunEventKind::PlanningClosed {
            fingerprint: current_plan,
        }),
        RunCommand::StartPlanAgent { node_id } => {
            if snapshot.attempts.len() >= 512 {
                return Err("workflow attempt budget exhausted".into());
            }
            let definition = WorkflowStore::open()?
                .get_published(&snapshot.definition_id, snapshot.definition_revision)?;
            if !definition.graph.nodes.iter().any(|node| {
                node.id == node_id
                    && matches!(
                        node.kind,
                        NodeKind::Agent {
                            role: crate::workflows::AgentRole::Coordinator
                                | crate::workflows::AgentRole::Planner,
                            ..
                        }
                    )
            }) {
                return Err(
                    "node is not a coordinator or planner in the pinned plan template".into(),
                );
            }
            if snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == snapshot.plan_id
                    && attempt.node_id == node_id
                    && attempt.state == AttemptState::Running
            }) {
                return Err("plan node already has a running attempt".into());
            }
            let generation = snapshot
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.story_id == snapshot.plan_id && attempt.node_id == node_id
                })
                .map(|attempt| attempt.generation)
                .max()
                .unwrap_or(0)
                + 1;
            Ok(RunEventKind::AttemptStarted {
                attempt: NodeAttempt {
                    id: Uuid::now_v7().to_string(),
                    story_id: snapshot.plan_id.clone(),
                    node_id,
                    generation,
                    state: AttemptState::Running,
                    outcome: None,
                    agent: None,
                    report: None,
                },
            })
        }
        RunCommand::StartAttempt { story_id, node_id } => {
            let story = stories
                .iter()
                .find(|story| story.id == story_id)
                .ok_or("story is not in plan")?;
            if !matches!(
                story.status,
                StoryStatus::Ready | StoryStatus::InProgress | StoryStatus::Review
            ) {
                return Err("story is not ready for a workflow attempt".into());
            }
            if snapshot.attempts.len() >= 512 {
                return Err("workflow attempt budget exhausted".into());
            }
            let definition = WorkflowStore::open()?.get_published(
                &snapshot.story_definition_id,
                snapshot.story_definition_revision,
            )?;
            if !definition
                .graph
                .nodes
                .iter()
                .any(|node| node.id == node_id && matches!(node.kind, NodeKind::Agent { .. }))
            {
                return Err("node is not an agent in the pinned story template".into());
            }
            if snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == story_id
                    && attempt.node_id == node_id
                    && attempt.state == AttemptState::Running
            }) {
                return Err("story node already has a running attempt".into());
            }
            let generation = snapshot
                .attempts
                .iter()
                .filter(|attempt| attempt.story_id == story_id && attempt.node_id == node_id)
                .map(|attempt| attempt.generation)
                .max()
                .unwrap_or(0)
                + 1;
            Ok(RunEventKind::AttemptStarted {
                attempt: NodeAttempt {
                    id: Uuid::now_v7().to_string(),
                    story_id,
                    node_id,
                    generation,
                    state: AttemptState::Running,
                    outcome: None,
                    agent: None,
                    report: None,
                },
            })
        }
        RunCommand::ReportAttempt {
            attempt_id,
            generation,
            outcome,
        } => {
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.agent.is_some() {
                return Err("bound agent must use a typed attempt report".into());
            }
            if expired
                || snapshot.status == RunStatus::Paused
                || attempt.generation != generation
                || attempt.state != AttemptState::Running
            {
                Ok(RunEventKind::LateReportIgnored {
                    attempt_id,
                    generation,
                })
            } else {
                Ok(RunEventKind::AttemptReported {
                    attempt_id,
                    generation,
                    outcome,
                    report: None,
                })
            }
        }
        RunCommand::BindAgent {
            attempt_id,
            binding,
        } => {
            validate_key("agent session", &binding.session_id)?;
            validate_key("spawn effect", &binding.effect_id)?;
            if binding.prompt_contract_version != crate::workflows::PROMPT_CONTRACT_VERSION
                || binding.prompt_sha256.len() != 64
                || !binding
                    .prompt_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || binding.audit_preview.chars().count() > 2_000
                || binding
                    .task_id
                    .as_ref()
                    .is_some_and(|id| validate_key("task id", id).is_err())
            {
                return Err("invalid agent binding".into());
            }
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.state != AttemptState::Running || attempt.agent.is_some() {
                return Err("node attempt cannot bind an agent".into());
            }
            if snapshot.attempts.iter().any(|existing| {
                existing
                    .agent
                    .as_ref()
                    .is_some_and(|agent| agent.session_id == binding.session_id)
            }) {
                return Err("agent session is already bound to a workflow attempt".into());
            }
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == binding.effect_id)
                .ok_or("spawn effect not found")?;
            if effect.kind != EffectKind::SpawnAgent
                || effect.state != EffectState::Intended
                || effect.key != format!("spawn:{attempt_id}")
            {
                return Err("spawn effect is not intended".into());
            }
            Ok(RunEventKind::AgentBound {
                attempt_id,
                binding,
            })
        }
        RunCommand::ReportBoundAttempt {
            caller_session,
            report,
        } => {
            validate_key("agent session", &caller_session)?;
            validate_key("run id", &report.run_id)?;
            validate_key("story id", &report.story_id)?;
            validate_key("attempt id", &report.attempt_id)?;
            if report.contract_version != crate::workflows::PROMPT_CONTRACT_VERSION
                || report.run_id != snapshot.id
                || report.generation == 0
                || report.story_revision < 0
                || report.summary.trim().is_empty()
                || report.summary.len() > 4_000
                || report.evidence.len() > 32
                || report
                    .evidence
                    .iter()
                    .any(|item| item.trim().is_empty() || item.len() > 2_000)
                || report.criterion_results.len() > 100
                || report
                    .criterion_results
                    .iter()
                    .any(|item| item.evidence.trim().is_empty() || item.evidence.len() > 2_000)
            {
                return Err("invalid typed attempt report".into());
            }
            let attempt = snapshot
                .attempts
                .iter()
                .find(|attempt| attempt.id == report.attempt_id)
                .ok_or("node attempt not found")?;
            if attempt.story_id != report.story_id
                || attempt
                    .agent
                    .as_ref()
                    .map(|agent| agent.session_id.as_str())
                    != Some(caller_session.as_str())
            {
                return Err("agent does not own the attempt".into());
            }
            let mut seen = std::collections::HashSet::new();
            if report.story_id == snapshot.plan_id {
                if report.story_revision != 0 || !report.criterion_results.is_empty() {
                    return Err("plan agent report cannot contain story criteria".into());
                }
            } else {
                let story = stories
                    .iter()
                    .find(|story| story.id == report.story_id)
                    .ok_or("story is not in plan")?;
                if story.revision != report.story_revision {
                    return Err("attempt report targets a stale story revision".into());
                }
                if report
                    .criterion_results
                    .iter()
                    .any(|item| item.index >= story.criteria.len() || !seen.insert(item.index))
                {
                    return Err("invalid criterion result index".into());
                }
            }
            if expired
                || snapshot.status == RunStatus::Paused
                || attempt.generation != report.generation
                || attempt.state != AttemptState::Running
            {
                return Ok(RunEventKind::LateReportIgnored {
                    attempt_id: report.attempt_id,
                    generation: report.generation,
                });
            }
            Ok(RunEventKind::AttemptReported {
                attempt_id: report.attempt_id.clone(),
                generation: report.generation,
                outcome: report.outcome,
                report: Some(report),
            })
        }
        RunCommand::ReserveEffect { key, kind } => {
            validate_key("effect key", &key)?;
            if snapshot.effects.iter().any(|effect| effect.key == key) {
                return Err("effect key already reserved".into());
            }
            if kind == EffectKind::SpawnAgent && snapshot.spawns >= snapshot.limits.max_spawns
                || kind == EffectKind::CreateStory
                    && snapshot.story_creations >= snapshot.limits.max_story_creations
            {
                return Err("workflow effect budget exhausted".into());
            }
            Ok(RunEventKind::EffectReserved {
                effect: EffectIntent {
                    id: Uuid::now_v7().to_string(),
                    key,
                    kind,
                    state: EffectState::Intended,
                },
            })
        }
        RunCommand::MarkEffect {
            effect_id,
            succeeded,
        } => {
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == effect_id)
                .ok_or("effect intent not found")?;
            if effect.state != EffectState::Intended {
                return Err("effect is no longer intended".into());
            }
            Ok(RunEventKind::EffectChanged {
                effect_id,
                state: if succeeded {
                    EffectState::Succeeded
                } else {
                    EffectState::Failed
                },
            })
        }
        RunCommand::ResolveUncertainEffect {
            effect_id,
            succeeded,
        } => {
            let effect = snapshot
                .effects
                .iter()
                .find(|effect| effect.id == effect_id)
                .ok_or("effect intent not found")?;
            if effect.state != EffectState::Uncertain {
                return Err("effect is not uncertain".into());
            }
            Ok(RunEventKind::EffectChanged {
                effect_id,
                state: if succeeded {
                    EffectState::Succeeded
                } else {
                    EffectState::Failed
                },
            })
        }
        RunCommand::AdvanceLoop => {
            if snapshot.loops >= snapshot.limits.max_loops {
                return Err("workflow loop budget exhausted".into());
            }
            Ok(RunEventKind::LoopAdvanced)
        }
        RunCommand::AcceptStory { story_id } => {
            let story = stories
                .iter()
                .find(|story| story.id == story_id)
                .ok_or("story is not in plan")?;
            if story.status != StoryStatus::Done {
                return Err("story has not been approved".into());
            }
            if snapshot.attempts.iter().any(|attempt| {
                attempt.story_id == story_id && attempt.state == AttemptState::Running
            }) {
                return Err("story still has a running attempt".into());
            }
            Ok(RunEventKind::StoryAccepted {
                story_id,
                revision: story.revision,
            })
        }
        RunCommand::FinalVerificationPassed => {
            ready_to_verify(snapshot, &stories)?;
            Ok(RunEventKind::VerificationPassed {
                fingerprint: fingerprint(&stories, true)?,
            })
        }
        RunCommand::Complete => {
            ready_to_verify(snapshot, &stories)?;
            if snapshot.verification_fingerprint.as_deref()
                != Some(fingerprint(&stories, true)?.as_str())
            {
                return Err("final verification is missing or stale".into());
            }
            Ok(RunEventKind::Completed)
        }
        RunCommand::Pause => Ok(RunEventKind::Paused),
        RunCommand::Resume => {
            if snapshot.status != RunStatus::Paused {
                return Err("workflow is not paused".into());
            }
            if snapshot
                .effects
                .iter()
                .any(|effect| effect.state == EffectState::Uncertain)
            {
                return Err("uncertain effects need an explicit resolution".into());
            }
            Ok(RunEventKind::Resumed)
        }
        RunCommand::Cancel => Ok(RunEventKind::Cancelled),
    }
}
