use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewPlan {
    pub project: String,
    pub title: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub project: String,
    pub title: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    Draft,
    Active,
    Done,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoryOrigin {
    Native,
    PlanStep { step: String },
    GitHubIssue { number: u64 },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NewStory {
    pub plan_id: String,
    pub title: String,
    pub criteria: Vec<String>,
    pub priority: u8,
    pub origin: StoryOrigin,
    pub file_scope: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryStatus {
    Backlog,
    Ready,
    InProgress,
    Review,
    Done,
    Blocked,
    WontFix,
}

impl StoryStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::WontFix => "wontfix",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "backlog" => Ok(Self::Backlog),
            "ready" => Ok(Self::Ready),
            "in_progress" => Ok(Self::InProgress),
            "review" => Ok(Self::Review),
            "done" => Ok(Self::Done),
            "blocked" => Ok(Self::Blocked),
            "wontfix" => Ok(Self::WontFix),
            _ => Err(format!("unknown story status: {value}")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: String,
    pub plan_id: String,
    pub title: String,
    pub criteria: Vec<String>,
    pub checked: Vec<bool>,
    pub dependencies: Vec<String>,
    pub priority: u8,
    pub origin: StoryOrigin,
    pub file_scope: Vec<String>,
    pub status: StoryStatus,
    pub revision: i64,
    pub claim_session: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoryCommand {
    CheckCriterion(usize),
    UncheckCriterion(usize),
    SubmitReview,
    Approve,
    RejectReview,
    Block,
    Unblock,
    WontFix,
}
