use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Paused,
    Completed,
    Cancelled,
}

impl RunStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunLimits {
    pub max_loops: u16,
    pub max_story_creations: u16,
    pub max_spawns: u16,
    pub max_duration_secs: u32,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_loops: 8,
            max_story_creations: 100,
            max_spawns: 24,
            max_duration_secs: 86_400,
        }
    }
}

impl RunLimits {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_loops == 0
            || self.max_loops > 100
            || self.max_story_creations == 0
            || self.max_story_creations > 100
            || self.max_spawns == 0
            || self.max_spawns > 100
            || self.max_duration_secs == 0
            || self.max_duration_secs > 604_800
        {
            return Err("run limits are outside supported bounds".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Completed,
    Failed,
    NeedsInput,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Running,
    Reported,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeAttempt {
    pub id: String,
    pub story_id: String,
    pub node_id: String,
    pub generation: u64,
    pub state: AttemptState,
    pub outcome: Option<AttemptOutcome>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    SpawnAgent,
    CreateStory,
    Notify,
    Integrate,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Intended,
    Succeeded,
    Failed,
    Uncertain,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectIntent {
    pub id: String,
    pub key: String,
    pub kind: EffectKind,
    pub state: EffectState,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoryExecution {
    pub story_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub accepted_revision: Option<i64>,
    pub attempt_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub id: String,
    pub project: String,
    pub plan_id: String,
    pub definition_id: String,
    pub definition_revision: i64,
    pub story_definition_id: String,
    pub story_definition_revision: i64,
    pub status: RunStatus,
    pub sequence: i64,
    pub started_ms: i64,
    pub limits: RunLimits,
    pub loops: u16,
    pub story_creations: u16,
    pub spawns: u16,
    pub planning_fingerprint: Option<String>,
    pub verification_fingerprint: Option<String>,
    pub stories: Vec<StoryExecution>,
    pub attempts: Vec<NodeAttempt>,
    pub effects: Vec<EffectIntent>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventKind {
    Started {
        initial: Box<RunSnapshot>,
    },
    PlanningClosed {
        fingerprint: String,
    },
    PlanningReopened,
    AttemptStarted {
        attempt: NodeAttempt,
    },
    AttemptReported {
        attempt_id: String,
        generation: u64,
        outcome: AttemptOutcome,
    },
    LateReportIgnored {
        attempt_id: String,
        generation: u64,
    },
    AttemptInterrupted {
        attempt_id: String,
    },
    EffectReserved {
        effect: EffectIntent,
    },
    EffectChanged {
        effect_id: String,
        state: EffectState,
    },
    LoopAdvanced,
    StoryAccepted {
        story_id: String,
        #[serde(default)]
        revision: i64,
    },
    VerificationPassed {
        fingerprint: String,
    },
    Paused,
    Resumed,
    Cancelled,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    pub sequence: i64,
    pub command_id: String,
    #[serde(default)]
    pub command_hash: Option<String>,
    pub at_ms: i64,
    pub kind: RunEventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunCommand {
    ClosePlanning,
    StartAttempt {
        story_id: String,
        node_id: String,
    },
    ReportAttempt {
        attempt_id: String,
        generation: u64,
        outcome: AttemptOutcome,
    },
    ReserveEffect {
        key: String,
        kind: EffectKind,
    },
    MarkEffect {
        effect_id: String,
        succeeded: bool,
    },
    ResolveUncertainEffect {
        effect_id: String,
        succeeded: bool,
    },
    AdvanceLoop,
    AcceptStory {
        story_id: String,
    },
    FinalVerificationPassed,
    Complete,
    Pause,
    Resume,
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunReceipt {
    pub sequence: i64,
    pub event: RunEvent,
    pub snapshot: RunSnapshot,
}
