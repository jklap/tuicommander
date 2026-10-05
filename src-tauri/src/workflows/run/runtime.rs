//! Daemon-owned serial scheduling. Mailboxes wake actors; SQLite owns the work.
use super::graph::{ActivationState, DecisionEvidence, GraphTransition};
use super::store::{GraphStartRequest, now_ms};
use super::{RunCommand, RunSnapshot, RunStatus, RunStore, emit_run_changed};
use crate::state::AppState;
use crate::workflows::NodeKind;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Notify;

/// Bound synchronous graph transitions before giving other actors a turn.
const TRANSITIONS_PER_TURN: usize = 32;

#[derive(Default)]
pub(crate) struct WorkflowRuntime {
    started: AtomicBool,
    state: parking_lot::Mutex<Weak<AppState>>,
    owner: parking_lot::Mutex<Option<Arc<RuntimeOwner>>>,
}

pub(super) struct RuntimeOwner {
    // Never unlink this file: another process must lock the same inode.
    _lock: File,
    pub(super) store: RunStore,
    actors: parking_lot::Mutex<HashMap<String, Arc<Notify>>>,
}

impl RuntimeOwner {
    pub(super) fn acquire(path: &Path) -> Result<Arc<Self>, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create executor directory: {e}"))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("owner.lock"))
            .map_err(|e| format!("open executor owner lock: {e}"))?;
        lock.try_lock()
            .map_err(|e| format!("workflow executor unavailable: {e}"))?;
        // Ownership precedes even opening/recovering the store. Reads never recover.
        let store = RunStore::open_at(path)?;
        store.reconcile_runs_after_restart(&store.active_run_ids()?);
        Ok(Arc::new(Self {
            _lock: lock,
            store,
            actors: Default::default(),
        }))
    }
}

impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        for notify in self.actors.get_mut().values() {
            notify.notify_one();
        }
    }
}

impl WorkflowRuntime {
    /// Start once on both daemon boot paths, outside the WebView lifecycle.
    pub(crate) fn spawn(state: &Arc<AppState>) {
        if state.workflow_runtime.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(state);
        *state.workflow_runtime.state.lock() = weak.clone();
        let path = crate::config::config_dir().join("workflow_runs.sqlite3");
        tokio::spawn(async move {
            let owner = match tokio::task::spawn_blocking(move || RuntimeOwner::acquire(&path))
                .await
            {
                Ok(Ok(owner)) => owner,
                Ok(Err(error)) => {
                    tracing::warn!(source = "workflows", %error, "Workflow executor unavailable");
                    return;
                }
                Err(error) => {
                    tracing::error!(source = "workflows", %error, "Workflow executor startup failed");
                    return;
                }
            };
            let Some(state) = weak.upgrade() else {
                return;
            };
            let ids = match owner.store.active_run_ids() {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::error!(source = "workflows", %error, "Workflow recovery scan failed");
                    return;
                }
            };
            *state.workflow_runtime.owner.lock() = Some(owner);
            for id in ids {
                state.workflow_runtime.wake(&id);
            }
        });
    }

    pub(crate) fn require_owner(&self) -> Result<(), String> {
        self.owner.lock().as_ref().map(|_| ()).ok_or_else(|| {
            "workflow executor unavailable: this daemon does not own the run database".into()
        })
    }

    /// Internal root-start boundary; public start controls arrive in slice F.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "graph start transport is delivered in slice F")
    )]
    pub(crate) fn start_graph(
        &self,
        state: &Arc<AppState>,
        request: &GraphStartRequest,
    ) -> Result<RunSnapshot, String> {
        let owner = self
            .owner
            .lock()
            .clone()
            .ok_or("workflow executor unavailable")?;
        let definition = crate::workflows::WorkflowStore::open()?
            .get_published(&request.definition_id, request.definition_revision)?;
        require_supported_nodes(&definition)?;
        let snapshot = owner.store.start_graph_run(request)?;
        emit_run_changed(state, &snapshot.project, &snapshot.id, snapshot.sequence);
        Ok(snapshot)
    }

    pub(crate) fn wake(&self, run_id: &str) {
        let Some(owner) = self.owner.lock().clone() else {
            return;
        };
        let mut actors = owner.actors.lock();
        if let Some(notify) = actors.get(run_id) {
            notify.notify_one();
            return;
        }
        let notify = Arc::new(Notify::new());
        actors.insert(run_id.to_owned(), notify.clone());
        tokio::spawn(run_actor(
            self.state.lock().clone(),
            Arc::downgrade(&owner),
            run_id.to_owned(),
            notify,
        ));
    }
}

fn require_supported_nodes(definition: &crate::workflows::PublishedWorkflow) -> Result<(), String> {
    if let Some(node) = definition.graph.nodes.iter().find(|node| {
        !matches!(
            node.kind,
            NodeKind::Start | NodeKind::Pause { .. } | NodeKind::End
        )
    }) {
        return Err(format!(
            "workflow node '{}' is not executable in daemon slice B",
            node.id
        ));
    }
    Ok(())
}

async fn run_actor(
    state: Weak<AppState>,
    owner: Weak<RuntimeOwner>,
    run_id: String,
    notify: Arc<Notify>,
) {
    loop {
        let Some(current) = owner.upgrade() else {
            return;
        };
        let store = current.store.clone();
        let id = run_id.clone();
        // Keep ownership alive until the blocking transaction completes.
        let result = tokio::task::spawn_blocking(move || {
            let _owner = current;
            drive_turn(&store, &id)
        })
        .await;
        let snapshot = match result {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(error)) => {
                // Sequence races are repaired by the committing producer's wake.
                tracing::warn!(source = "workflows", %run_id, %error, "Workflow scheduling turn failed");
                notify.notified().await;
                continue;
            }
            Err(error) => {
                tracing::error!(source = "workflows", %run_id, %error, "Workflow actor failed");
                break;
            }
        };
        if let Some(state) = state.upgrade() {
            // Do not wake our own actor for an unchanged snapshot.
            super::api::emit_run_cursor(&state, &snapshot.project, &snapshot.id, snapshot.sequence);
        } else {
            break;
        }
        if matches!(snapshot.status, RunStatus::Completed | RunStatus::Cancelled) {
            break;
        }
        if snapshot.status == RunStatus::Running {
            let remaining = deadline_ms(&snapshot).saturating_sub(now_ms()).max(0) as u64;
            tokio::select! {
                _ = notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(remaining)) => {},
            }
        } else {
            notify.notified().await;
        }
    }
    if let Some(owner) = owner.upgrade() {
        owner.actors.lock().remove(&run_id);
    }
}

pub(super) fn deadline_ms(snapshot: &RunSnapshot) -> i64 {
    snapshot
        .started_ms
        .saturating_add(i64::from(snapshot.limits.max_duration_secs) * 1000)
}

/// Re-read after each expected-sequence commit; no external effects run here.
// DEFERRED (2026-10-05) — self-wake after TRANSITIONS_PER_TURN: the turn yields after 32
// transitions and the actor then waits for the deadline instead of re-waking itself;
// unreachable with Start/Pause/End only, must wake itself before slice C adds Agent nodes.
pub(super) fn drive_turn(store: &RunStore, run_id: &str) -> Result<RunSnapshot, String> {
    for _ in 0..TRANSITIONS_PER_TURN {
        let snapshot = store.snapshot(run_id)?;
        if snapshot.status != RunStatus::Running {
            return Ok(snapshot);
        }
        if now_ms() >= deadline_ms(&snapshot) {
            return Ok(store
                .command_expected(
                    run_id,
                    &format!("daemon:deadline:{}", snapshot.sequence),
                    snapshot.sequence,
                    RunCommand::ExpireDeadline,
                )?
                .snapshot);
        }
        let next = snapshot
            .graph_executions
            .iter()
            .filter(|graph| !graph.completed)
            .find_map(|graph| {
                graph
                    .activations
                    .iter()
                    .find(|activation| {
                        matches!(
                            activation.state,
                            ActivationState::Ready | ActivationState::Running
                        )
                    })
                    .map(|activation| (graph, activation))
            });
        let Some((graph, activation)) = next else {
            return Ok(snapshot);
        };
        let node = graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|node| node.id == activation.node_id)
            .ok_or("activation node missing")?;
        // DEFERRED (2026-10-05): C supplies Agent effects; D/E supply policy and
        // terminal delivery gates. Reached work is retained, never called successful.
        if !matches!(node.kind, NodeKind::Start | NodeKind::Pause { .. }) {
            return Ok(snapshot);
        }
        let transition = if activation.state == ActivationState::Ready {
            GraphTransition::Activate {
                execution_id: graph.id.clone(),
                activation_id: activation.id.clone(),
            }
        } else {
            GraphTransition::Complete {
                execution_id: graph.id.clone(),
                activation_id: activation.id.clone(),
                outcome: None,
                evidence: matches!(node.kind, NodeKind::Pause { .. }).then(|| DecisionEvidence {
                    actor: "daemon".into(),
                    reason: "Published graph pause requires an explicit resolution".into(),
                    references: vec![format!("activation:{}:{}", graph.id, activation.id)],
                }),
            }
        };
        let key = format!(
            "daemon:{}:{}:{:?}",
            graph.id, activation.id, activation.state
        );
        store.command_expected(
            run_id,
            &key,
            snapshot.sequence,
            RunCommand::Graph { transition },
        )?;
    }
    // Yield with a durable pending activation; the next turn is explicitly woken.
    store.snapshot(run_id)
}
