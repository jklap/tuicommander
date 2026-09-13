use std::path::PathBuf;

use super::model::*;
use super::ownership::resolve_owning_project;
use super::store::ProgressStore;

pub struct SubmittedProgressReport {
    pub project_root: PathBuf,
    pub receipt: ProgressReceipt,
    pub event: Option<super::model::ProgressEvent>,
}

/// Shared reporting core for MCP, HTTP, and Tauri IPC.
pub fn submit_progress_report(
    project_hint: Option<&str>,
    input: ProgressReportInput,
    provenance: ProgressProvenance,
) -> Result<SubmittedProgressReport, String> {
    let event = input.into_event(provenance);
    event.validate()?;
    let project_root = resolve_owning_project(project_hint)?;
    let outcome = ProgressStore::open(&project_root)?.report(&event)?;
    Ok(SubmittedProgressReport {
        project_root,
        receipt: outcome.receipt,
        event: outcome.event,
    })
}

fn store(project: &str) -> Result<ProgressStore, String> {
    ProgressStore::open(resolve_owning_project(Some(project))?)
}

pub fn progress_status(project: &str) -> Result<ProgressStatus, String> {
    store(project)?.status()
}
pub fn progress_list(project: &str, input: ProgressListInput) -> Result<ProgressPage, String> {
    store(project)?.list_filtered(&input)
}
pub fn progress_pause(project: &str) -> Result<ProgressMutationReceipt, String> {
    store(project)?.set_collection_enabled(false)
}
pub fn progress_resume(project: &str) -> Result<ProgressMutationReceipt, String> {
    store(project)?.set_collection_enabled(true)
}
pub fn progress_delete(
    project: &str,
    input: ProgressDeleteInput,
) -> Result<ProgressMutationReceipt, String> {
    store(project)?.delete_events(&input.event_ids)
}
pub fn progress_clear(
    project: &str,
    input: ProgressClearInput,
) -> Result<ProgressMutationReceipt, String> {
    store(project)?.clear(input.expected_revision)
}
pub fn progress_update(
    project: &str,
    input: ProgressUpdateInput,
) -> Result<ProgressMutationReceipt, String> {
    store(project)?.update(&input)
}
pub fn progress_read(
    project: &str,
    input: ProgressReadInput,
) -> Result<ProgressMutationReceipt, String> {
    store(project)?.acknowledge_read(input.snapshot_cursor)
}
