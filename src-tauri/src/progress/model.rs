use serde::{Deserialize, Serialize};

pub const MAX_SUMMARY_CHARS: usize = 500;
pub const MAX_WORKSTREAM_CHARS: usize = 80;
pub const DEFAULT_PAGE_LIMIT: usize = 100;
pub const MAX_PAGE_LIMIT: usize = 250;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Started,
    Milestone,
    Blocked,
    Done,
}

impl ProgressKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Milestone => "milestone",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "started" => Ok(Self::Started),
            "milestone" => Ok(Self::Milestone),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Done),
            other => Err(format!("unknown progress event type '{other}'")),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamState {
    Started,
    Progressing,
    Blocked,
    Done,
}

impl WorkstreamState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Progressing => "progressing",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "started" => Ok(Self::Started),
            "progressing" => Ok(Self::Progressing),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Done),
            other => Err(format!("unknown workstream state '{other}'")),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporter_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporter_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewProgressEvent {
    #[serde(rename = "type")]
    pub kind: ProgressKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(default)]
    pub provenance: ProgressProvenance,
}

impl NewProgressEvent {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("summary", &self.summary, MAX_SUMMARY_CHARS)?;
        if let Some(workstream) = &self.workstream {
            validate_text("workstream", workstream, MAX_WORKSTREAM_CHARS)?;
        }
        Ok(())
    }

    pub(crate) fn trimmed_summary(&self) -> String {
        self.summary.trim().to_string()
    }

    pub(crate) fn trimmed_workstream(&self) -> Option<String> {
        self.workstream
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }
}

pub(crate) fn validate_workstream_name(value: &str) -> Result<(), String> {
    validate_text("workstream", value, MAX_WORKSTREAM_CHARS)
}

pub(crate) fn validate_text(field: &str, value: &str, max: usize) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    let count = trimmed.chars().count();
    if count > max {
        return Err(format!(
            "{field} must be at most {max} characters (got {count})"
        ));
    }
    Ok(())
}

pub(crate) fn normalize_workstream(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub id: String,
    pub sequence: u64,
    pub revision: u64,
    pub created_at_ms: u64,
    #[serde(rename = "type")]
    pub kind: ProgressKind,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstream_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(flatten)]
    pub provenance: ProgressProvenance,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamSnapshot {
    pub id: String,
    pub name: String,
    pub state: WorkstreamState,
    pub active_blockers: u64,
    pub updated_sequence: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub project_root: String,
    pub revision: u64,
    pub collection_enabled: bool,
    pub workstreams: Vec<WorkstreamSnapshot>,
    pub project_blockers: Vec<ProgressEvent>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPage {
    pub revision: u64,
    pub snapshot_cursor: u64,
    pub events: Vec<ProgressEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before_sequence: Option<u64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStatus {
    pub project_root: String,
    pub revision: u64,
    pub snapshot_cursor: u64,
    pub read_cursor: u64,
    pub unread_count: u64,
    pub collection_enabled: bool,
    pub workstreams: Vec<WorkstreamSnapshot>,
    pub project_blockers: Vec<ProgressEvent>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressListInput {
    pub before_sequence: Option<u64>,
    pub limit: Option<usize>,
    pub workstream_id: Option<String>,
    pub kind: Option<ProgressKind>,
    pub unread_only: Option<bool>,
    pub blocker_only: Option<bool>,
    pub created_after_ms: Option<u64>,
    pub created_before_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressDeleteInput {
    pub event_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressClearInput {
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressReadInput {
    pub snapshot_cursor: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProgressCorrection {
    EditSummary {
        event_id: String,
        summary: String,
    },
    MoveEvent {
        event_id: String,
        workstream_id: Option<String>,
    },
    RenameWorkstream {
        workstream_id: String,
        name: String,
    },
    MergeWorkstreams {
        source_workstream_ids: Vec<String>,
        target_workstream_id: String,
    },
    MergeEvents {
        source_event_ids: Vec<String>,
        target_event_id: String,
    },
    ResolveBlocker {
        event_id: String,
    },
    SetWorkstreamState {
        workstream_id: String,
        state: WorkstreamState,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressUpdateInput {
    pub expected_revision: u64,
    pub corrections: Vec<ProgressCorrection>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressMutationReceipt {
    pub revision: u64,
    pub affected: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressExportOptions {
    #[serde(default)]
    pub include_provenance: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProgressExportInput {
    Preview {
        #[serde(default)]
        options: ProgressExportOptions,
    },
    Write {
        options: ProgressExportOptions,
        snapshot_id: String,
        snapshot_time_ms: u64,
        replace: bool,
        expected_content: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressExportReceipt {
    pub project_root: String,
    pub path: String,
    pub snapshot_id: String,
    pub snapshot_revision: u64,
    pub snapshot_time_ms: u64,
    pub markdown: String,
    pub file_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_content: Option<String>,
    pub written: bool,
}

/// Caller-supplied report fields. Provenance is derived by the transport.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReportInput {
    #[serde(rename = "type")]
    pub kind: ProgressKind,
    pub summary: String,
    #[serde(default)]
    pub workstream: Option<String>,
}

impl ProgressReportInput {
    pub fn into_event(self, provenance: ProgressProvenance) -> NewProgressEvent {
        NewProgressEvent {
            kind: self.kind,
            summary: self.summary,
            workstream: self.workstream,
            provenance,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressReceiptStatus {
    Recorded,
    Duplicate,
    Paused,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReceipt {
    pub status: ProgressReceiptStatus,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
}

impl ProgressReceipt {
    pub(crate) fn recorded(event: &ProgressEvent) -> Self {
        Self {
            status: ProgressReceiptStatus::Recorded,
            revision: event.revision,
            event_id: Some(event.id.clone()),
        }
    }

    pub(crate) fn duplicate(event: &ProgressEvent) -> Self {
        Self {
            status: ProgressReceiptStatus::Duplicate,
            revision: event.revision,
            event_id: Some(event.id.clone()),
        }
    }

    pub(crate) fn paused(revision: u64) -> Self {
        Self {
            status: ProgressReceiptStatus::Paused,
            revision,
            event_id: None,
        }
    }
}

pub(crate) struct ProgressReportOutcome {
    pub receipt: ProgressReceipt,
    pub event: Option<ProgressEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deserialise a valid body, then the same body with one extra field, and
    /// require the second to fail with an "unknown field" error.
    ///
    /// The valid case is asserted first on purpose. A rejection alone proves
    /// nothing — a fixture that is malformed for an unrelated reason is
    /// rejected too, and the test would then pass while `deny_unknown_fields`
    /// was gone.
    macro_rules! assert_rejects_unknown_field {
        ($ty:ty, $json:tt) => {{
            let name = stringify!($ty);
            let mut value = serde_json::json!($json);
            serde_json::from_value::<$ty>(value.clone())
                .unwrap_or_else(|e| panic!("{name} must accept its own valid body: {e}"));

            value
                .as_object_mut()
                .expect("fixture is a JSON object")
                .insert("nopeNotAField".to_string(), serde_json::json!(1));
            let err = serde_json::from_value::<$ty>(value).expect_err(&format!(
                "{name} accepted an unknown field — its deny_unknown_fields is gone"
            ));
            assert!(
                err.to_string().contains("unknown field"),
                "{name} rejected the body for the wrong reason: {err}"
            );
        }};
    }

    /// Every progress input type carries `serde(deny_unknown_fields)`, so a
    /// caller that misspells a field is refused instead of silently getting a
    /// default. Nothing else in the tree asserts that: delete an attribute and
    /// the valid bodies still parse, the routes still answer, and the whole
    /// suite stays green.
    ///
    /// This lives at the serde layer rather than in an HTTP test on purpose.
    /// Over HTTP a rejected body answers 422 from the axum extractor — the
    /// same 422 a caller gets when an auth guard is missing and the body never
    /// reaches it, which is the confusion
    /// `every_progress_route_runs_its_handler_for_a_loopback_caller` is built
    /// to avoid. Keep the two apart.
    #[test]
    fn every_progress_input_type_refuses_an_unknown_field() {
        assert_rejects_unknown_field!(ProgressListInput, {"beforeSequence": 4, "limit": 50});
        assert_rejects_unknown_field!(ProgressDeleteInput, {"eventIds": ["event-1"]});
        assert_rejects_unknown_field!(ProgressClearInput, {"expectedRevision": 2});
        assert_rejects_unknown_field!(ProgressReadInput, {"snapshotCursor": 7});
        assert_rejects_unknown_field!(ProgressCorrection, {
            "operation": "edit_summary",
            "eventId": "event-1",
            "summary": "a corrected summary"
        });
        assert_rejects_unknown_field!(ProgressUpdateInput, {
            "expectedRevision": 2,
            "corrections": []
        });
        assert_rejects_unknown_field!(ProgressExportOptions, {"includeProvenance": true});
        assert_rejects_unknown_field!(ProgressExportInput, {"operation": "preview"});
    }
}
