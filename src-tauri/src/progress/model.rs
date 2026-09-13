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

fn validate_text(field: &str, value: &str, max: usize) -> Result<(), String> {
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
    pub events: Vec<ProgressEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before_sequence: Option<u64>,
}
