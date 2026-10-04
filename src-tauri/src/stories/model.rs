use schemars::JsonSchema;
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlanSource {
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

/// Where a story came from: `{"type":"native"}`, `{"type":"plan_step","step":"..."}` or `{"type":"git_hub_issue","number":123}`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoryOrigin {
    Native,
    PlanStep {
        /// The plan step this story implements.
        step: String,
    },
    GitHubIssue {
        /// The GitHub issue number.
        number: u64,
    },
}

/// A new story. Field names are camelCase.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewStory {
    /// Id of the plan that owns the story (from create_plan or list_plans).
    pub plan_id: String,
    pub title: String,
    /// Acceptance criteria, 1 to 100 entries.
    pub criteria: Vec<String>,
    /// 1 (highest), 2 or 3.
    #[schemars(range(min = 1, max = 3))]
    pub priority: u8,
    pub origin: StoryOrigin,
    /// Repo-relative paths the story may touch; may be empty.
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryRead {
    #[serde(flatten)]
    pub story: Story,
    pub abandoned: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub stories: Vec<StoryRead>,
    pub state: PlanState,
    pub wont_fix_count: usize,
    pub all_cancelled: bool,
}

/// A status command. Unit commands are plain strings (`"submit_review"`); the criterion commands
/// carry a zero-based index (`{"check_criterion":0}`).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryCommand {
    /// Ready -> in_progress without a session claim.
    StartManual,
    /// Only while in_progress.
    CheckCriterion(usize),
    /// Only while in_progress.
    UncheckCriterion(usize),
    /// Requires every criterion checked. Moves it to review.
    SubmitReview,
    /// Review -> done; dependants whose dependencies are all done become ready.
    Approve,
    /// Review -> in_progress (claimed) or ready.
    RejectReview,
    /// Moves an active story to blocked.
    Block,
    /// Blocked -> ready, or backlog while a dependency is unfinished.
    Unblock,
    /// Cancels the story.
    WontFix,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StoryTransitionActor {
    Human,
    LocalApi,
    ManagedSession { session_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryTransition {
    pub story_id: String,
    pub revision: i64,
    pub command: StoryCommand,
    pub actor: StoryTransitionActor,
}
