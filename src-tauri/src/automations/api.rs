//! Shared HTTP, IPC, MCP and CLI boundary; all state belongs to the addressed backend.
use super::{
    actions::{self, DefinitionAction},
    definitions::DefinitionStore,
    model::AutomationDefinition,
    run::SummaryWindow,
    runtime::AutomationRuntime,
    schedule::{self, SchedulePreset},
    store::RunStore,
};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AutomationAction {
    List {},
    Templates {},
    Template {
        template_id: String,
        repository: String,
        run_config: String,
        timezone: Option<String>,
    },
    Get {
        id: String,
    },
    Create {
        definition: AutomationDefinition,
    },
    Update {
        id: Option<String>,
        definition: Option<AutomationDefinition>,
        enabled: Option<bool>,
    },
    Pause {
        id: String,
    },
    Resume {
        id: String,
    },
    Delete {
        id: String,
    },
    RunNow {
        id: String,
    },
    Run {
        id: String,
    },
    ListRuns {
        id: Option<String>,
        #[serde(default = "history_limit")]
        limit: u32,
        #[serde(default)]
        offset: u32,
    },
    WorktreePreview {
        id: String,
    },
    CleanupWorktree {
        id: String,
    },
    Summary {
        window: SummaryWindow,
    },
    Preview {
        cron: String,
        timezone: Option<String>,
        #[serde(default = "preview_count")]
        count: usize,
    },
    PreviewDefinition {
        definition: AutomationDefinition,
        #[serde(default = "preview_count")]
        count: usize,
    },
    Preset {
        preset: SchedulePreset,
    },
}
fn history_limit() -> u32 {
    50
}
fn preview_count() -> usize {
    4
}

/// Parse the same input and return the same errors on either transport.
pub(crate) async fn execute(input: Value, state: Arc<AppState>) -> Result<Value, String> {
    execute_as(input, state, Caller::Client).await
}

pub(crate) enum Caller {
    Client,
    Agent(Option<String>),
}

impl Caller {
    fn creator(&self, action: &AutomationAction) -> Result<Option<&str>, String> {
        match self {
            Self::Agent(None) if matches!(action, AutomationAction::Create { .. }) => {
                Err("Automation creation requires a bound agent session".into())
            }
            Self::Agent(session) => Ok(session.as_deref()),
            Self::Client => Ok(None),
        }
    }
}

pub(crate) async fn execute_as(
    input: Value,
    state: Arc<AppState>,
    caller: Caller,
) -> Result<Value, String> {
    let action = parse(input)?;
    caller.creator(&action)?;
    if let AutomationAction::RunNow { id } = action {
        return value(AutomationRuntime::run_now(&state, &id).await?);
    }
    if matches!(
        action,
        AutomationAction::WorktreePreview { .. } | AutomationAction::CleanupWorktree { .. }
    ) {
        return tokio::task::spawn_blocking(move || {
            let (id, remove) = match action {
                AutomationAction::WorktreePreview { id } => (id, false),
                AutomationAction::CleanupWorktree { id } => (id, true),
                _ => unreachable!(),
            };
            super::worktrees::execute(&state, &RunStore::open()?, &id, remove)
        })
        .await
        .map_err(|error| format!("Automation cleanup task failed: {error}"))?;
    }
    tokio::task::spawn_blocking(move || {
        execute_stored_as(
            &DefinitionStore::new(),
            &RunStore::open()?,
            action,
            Utc::now(),
            caller,
        )
    })
    .await
    .map_err(|error| format!("Automation task failed: {error}"))?
}

pub(crate) fn parse(input: Value) -> Result<AutomationAction, String> {
    serde_json::from_value(input).map_err(|error| format!("Invalid automation action: {error}"))
}

#[cfg(test)]
pub(crate) fn execute_stored(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
) -> Result<Value, String> {
    execute_stored_as(definitions, runs, action, now, Caller::Client)
}

pub(crate) fn execute_stored_as(
    definitions: &DefinitionStore,
    runs: &RunStore,
    action: AutomationAction,
    now: DateTime<Utc>,
    caller: Caller,
) -> Result<Value, String> {
    let creator = caller.creator(&action)?;
    match action {
        AutomationAction::List {} => {
            let mut items = Vec::new();
            for definition in definitions.load()?.definitions {
                let cursor = cursor(runs, &definition.id)?;
                let schedule = definition.schedule()?;
                let next = if definition.enabled && !schedule.is_completed(cursor) {
                    schedule.next_after(now.max(cursor.unwrap_or(now)))?
                } else {
                    None
                };
                let last = runs
                    .history(Some(&definition.id), 1, 0)?
                    .first()
                    .map(|run| run.status);
                items.push(json!({"definition": definition, "next_run_ms": next.map(|t| t.timestamp_millis()), "last_status": last}));
            }
            Ok(Value::Array(items))
        }
        AutomationAction::Templates {} => value(super::templates::TEMPLATES),
        AutomationAction::Template {
            template_id,
            repository,
            run_config,
            timezone,
        } => value(super::templates::draft(
            &template_id,
            repository,
            run_config,
            timezone.as_deref(),
        )?),
        AutomationAction::Get { id } => value(definitions.get(&id)?),
        AutomationAction::Create { definition } => {
            let id = definition.id.clone();
            actions::execute(
                definitions,
                DefinitionAction::Create { definition },
                creator,
            )?;
            value(definitions.get(&id)?)
        }
        AutomationAction::Update {
            id,
            definition,
            enabled,
        } => match (definition, enabled) {
            (Some(definition), None) => {
                let id = id.unwrap_or_else(|| definition.id.clone());
                actions::execute(
                    definitions,
                    DefinitionAction::Update {
                        id: id.clone(),
                        definition,
                    },
                    None,
                )?;
                value(definitions.get(&id)?)
            }
            (None, Some(enabled)) => {
                let id = id.ok_or("Automation update requires id")?;
                definitions.set_enabled(&id, enabled)?;
                value(definitions.get(&id)?)
            }
            _ => Err("Automation update requires exactly one of definition or enabled".into()),
        },
        AutomationAction::Pause { id } => {
            actions::execute(definitions, DefinitionAction::Pause { id }, None)
        }
        AutomationAction::Resume { id } => {
            actions::execute(definitions, DefinitionAction::Resume { id }, None)
        }
        AutomationAction::Delete { id } => {
            actions::execute(definitions, DefinitionAction::Delete { id }, None)
        }
        AutomationAction::Run { id } => value(runs.get(&id)?),
        AutomationAction::ListRuns { id, limit, offset } => {
            value(runs.history(id.as_deref(), limit, offset)?)
        }
        AutomationAction::Summary { window } => {
            value(runs.summary(window, now.timestamp_millis())?)
        }
        AutomationAction::Preview {
            cron,
            timezone,
            count,
        } => {
            let zone = schedule::timezone_for_creation(timezone.as_deref())?;
            value(schedule::preview(&cron, &zone, now, count)?)
        }
        AutomationAction::PreviewDefinition {
            mut definition,
            count,
        } => {
            if definition.timezone.is_empty() {
                definition.timezone = schedule::timezone_for_creation(None)?;
            }
            value(schedule::once::preview_definition(
                &definition,
                now,
                count,
                cursor(runs, &definition.id)?,
            )?)
        }
        AutomationAction::Preset { preset } => Ok(json!({"cron": preset.cron()?})),
        AutomationAction::WorktreePreview { .. } | AutomationAction::CleanupWorktree { .. } => {
            Err("Worktree cleanup requires local session state".into())
        }
        AutomationAction::RunNow { .. } => Err("Run Now requires the runtime owner".into()),
    }
}
fn cursor(runs: &RunStore, id: &str) -> Result<Option<DateTime<Utc>>, String> {
    runs.scheduled_cursor(id)?
        .map(|millis| {
            DateTime::from_timestamp_millis(millis)
                .ok_or_else(|| "Invalid automation cursor timestamp".into())
        })
        .transpose()
}
fn value(input: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(input).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
