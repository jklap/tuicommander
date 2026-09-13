use std::path::PathBuf;

use super::model::{ProgressProvenance, ProgressReceipt, ProgressReportInput};
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
