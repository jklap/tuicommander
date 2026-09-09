//! Conversation persistence types for AI Chat (L1) and Agent loop (L2+).
//!
//! Extracted from `ai_chat.rs` so L2 tool-call extensions live next to the
//! agent code while L1 code can keep importing the same names via
//! `crate::ai_chat::{ChatMessage, Conversation, ConversationMeta}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current persisted schema version.
///
/// - 1 — L1 chat: role/content/timestamp messages only.
/// - 2 — tool-call extensions on messages (`tool_calls`, `tool_use_id`, …).
/// - 3 — agent run snapshot (`agent`): loop state, iteration, tool-call log.
///
/// `save_conversation` stamps this on every write, so the number is the
/// backend's and a client never has to know it. Older documents keep loading:
/// every field added since v1 has a serde default, and [`Conversation::migrate`]
/// re-stamps them on read.
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 3;

fn default_schema_version() -> u32 {
    1
}

/// One tool call emitted by an assistant message in an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ToolCallRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// A single message in a saved conversation.
///
/// L1 chat uses `role` + `content` only. L2 agent turns add tool-call
/// metadata: assistant messages may carry `tool_calls`, and a `tool` role
/// message carries `tool_use_id` + `tool_result` (+ `is_error`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system" | "tool"
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: u64, // unix millis

    // -- L2 tool-call extensions (all optional + serde default) --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ChatMessage {
    /// Plain L1 text message (user/assistant/system).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn text(role: impl Into<String>, content: impl Into<String>, timestamp: u64) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp,
            tool_calls: None,
            tool_use_id: None,
            tool_result: None,
            is_error: None,
        }
    }
}

/// Metadata for a saved conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConversationMeta {
    pub id: String,
    pub title: String,
    /// Session ID of the attached terminal (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created: u64, // unix millis
    pub updated: u64, // unix millis
    pub message_count: usize,
    /// Provider + model used
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

/// State of the agent loop that produced this conversation.
///
/// Mirrors the panel's `AgentState` union: the strings are the wire contract,
/// so an unknown one is a client bug and must not be silently accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentRunState {
    Running,
    Paused,
    Completed,
    Cancelled,
    Error,
    #[default]
    Idle,
}

/// Whether a logged tool call has come back yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolCallStatus {
    Pending,
    Done,
}

/// Outcome of a finished tool call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ToolCallResult {
    pub success: bool,
    #[serde(default)]
    pub output: String,
}

/// One entry of the agent's tool-call log, as the panel renders it.
///
/// This is deliberately NOT [`ToolCallRecord`]: that one is the LLM protocol
/// shape (id + name + arguments, paired with a `tool` role message), while this
/// one is the run log the UI shows — timing and outcome included. Mapping
/// between them would lose `status`, `startedAt` and `duration`, which are the
/// whole point of restoring a run that a reload interrupted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentToolCall {
    pub status: ToolCallStatus,
    pub tool_name: String,
    #[serde(default)]
    pub args: Value,
    pub started_at: u64,
    /// Present exactly when `status` is `Done`, matching the TS union.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolCallResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
}

/// The agent run as it stood at the last save.
///
/// Field names are camelCase because they mirror the panel's in-memory state
/// one for one: the store hands its signals over unchanged and reads them back
/// unchanged, so no reshaping code exists on either side to drift.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSnapshot {
    #[serde(default)]
    pub state: AgentRunState,
    #[serde(default)]
    pub current_iteration: u32,
    #[serde(default)]
    pub tool_calls: Vec<AgentToolCall>,
}

impl AgentSnapshot {
    /// Nothing an agent run would want back. Kept out of the JSON so a plain
    /// L1 chat file still looks like one.
    pub fn is_empty(&self) -> bool {
        self.state == AgentRunState::Idle
            && self.current_iteration == 0
            && self.tool_calls.is_empty()
    }
}

/// A full conversation with messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Conversation {
    pub meta: ConversationMeta,
    pub messages: Vec<ChatMessage>,
    /// Schema version — see [`CURRENT_SCHEMA_VERSION`]. Older files without
    /// this field load as 1 via serde default.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Agent loop state, iteration and tool-call log. Absent in v1/v2 files,
    /// and omitted again whenever there is no run to restore.
    #[serde(default, skip_serializing_if = "AgentSnapshot::is_empty")]
    pub agent: AgentSnapshot,
}

const TOOL_RESULT_MAX_BYTES: usize = 8192;

/// Redact secrets and cap the size of one piece of captured tool output.
///
/// `String::truncate` panics off a char boundary, and tool output is full of
/// box drawing and emoji — so the cap walks back to the nearest boundary.
fn sanitize_tool_output(text: &mut String) {
    *text = crate::ai_agent::tools::redact_secrets(text);
    if text.len() > TOOL_RESULT_MAX_BYTES {
        let mut cut = TOOL_RESULT_MAX_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n[truncated]");
    }
}

impl Conversation {
    pub fn sanitize_for_persist(&mut self) {
        for msg in &mut self.messages {
            if let Some(ref mut result) = msg.tool_result {
                sanitize_tool_output(result);
            }
        }
        for call in &mut self.agent.tool_calls {
            match call.result {
                Some(ref mut result) => sanitize_tool_output(&mut result.output),
                // The panel reads `result` unconditionally on a done card, so a
                // done entry without one would crash the render. The store never
                // writes that pair, but a hand-edited file or another local
                // client can; it is a call that never came back, so say so.
                None => call.status = ToolCallStatus::Pending,
            }
        }
    }

    /// Bring a document read off disk up to [`CURRENT_SCHEMA_VERSION`].
    ///
    /// Every field added since v1 carries a serde default, so v1→v2→v3 upgrades
    /// themselves as the file is parsed and this only has to stamp the version.
    /// It stays an explicit step so a version that needs real work has a place
    /// to go, and so the caller knows whether to write the file back.
    ///
    /// Returns whether anything changed.
    pub fn migrate(&mut self) -> bool {
        if self.schema_version >= CURRENT_SCHEMA_VERSION {
            return false;
        }
        self.schema_version = CURRENT_SCHEMA_VERSION;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v1_json_loads_with_default_schema_version() {
        let json_v1 = r#"{
            "meta": {
                "id": "old", "title": "Old", "created": 1, "updated": 2,
                "message_count": 1
            },
            "messages": [{"role":"user","content":"hi"}]
        }"#;
        let conv: Conversation = serde_json::from_str(json_v1).unwrap();
        assert_eq!(conv.schema_version, 1);
        assert_eq!(conv.messages.len(), 1);
        assert!(conv.messages[0].tool_calls.is_none());
        assert!(conv.messages[0].tool_use_id.is_none());
    }

    #[test]
    fn agent_conversation_with_tool_calls_roundtrips() {
        let conv = Conversation {
            meta: ConversationMeta {
                id: "a1".into(),
                title: "Agent".into(),
                session_id: Some("s1".into()),
                created: 1,
                updated: 2,
                message_count: 3,
                provider: "anthropic".into(),
                model: "claude-sonnet-4-5".into(),
            },
            messages: vec![
                ChatMessage::text("user", "list files", 1),
                ChatMessage {
                    role: "assistant".into(),
                    content: "I'll list them.".into(),
                    timestamp: 2,
                    tool_calls: Some(vec![ToolCallRecord {
                        id: "call_1".into(),
                        name: "ai_terminal_send_input".into(),
                        arguments: json!({"text": "ls\n"}),
                    }]),
                    tool_use_id: None,
                    tool_result: None,
                    is_error: None,
                },
                ChatMessage {
                    role: "tool".into(),
                    content: String::new(),
                    timestamp: 3,
                    tool_calls: None,
                    tool_use_id: Some("call_1".into()),
                    tool_result: Some("Cargo.toml\nsrc/".into()),
                    is_error: Some(false),
                },
            ],
            schema_version: CURRENT_SCHEMA_VERSION,
            agent: AgentSnapshot::default(),
        };

        let json = serde_json::to_string(&conv).unwrap();
        let loaded: Conversation = serde_json::from_str(&json).unwrap();
        assert!(
            !json.contains("\"agent\""),
            "an L1 chat gains no agent block: {json}"
        );

        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.messages.len(), 3);
        let tool_calls = loaded.messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].name, "ai_terminal_send_input");
        assert_eq!(tool_calls[0].arguments["text"], "ls\n");
        assert_eq!(loaded.messages[2].tool_use_id.as_deref(), Some("call_1"));
        assert_eq!(
            loaded.messages[2].tool_result.as_deref(),
            Some("Cargo.toml\nsrc/")
        );
        assert_eq!(loaded.messages[2].is_error, Some(false));
    }

    /// The agent run snapshot is what a reload restores: without it the panel
    /// comes back with prose and an idle badge in the middle of a live loop.
    #[test]
    fn agent_snapshot_survives_a_round_trip() {
        let json_v3 = r#"{
            "meta": {
                "id": "run", "title": "Run", "created": 1, "updated": 2,
                "message_count": 0
            },
            "messages": [],
            "schema_version": 3,
            "agent": {
                "state": "running",
                "currentIteration": 4,
                "toolCalls": [
                    {
                        "status": "done",
                        "toolName": "ai_terminal_send_input",
                        "args": {"text": "ls\n"},
                        "startedAt": 100,
                        "result": {"success": true, "output": "Cargo.toml"},
                        "duration": 25
                    },
                    {
                        "status": "pending",
                        "toolName": "ai_terminal_read",
                        "args": {},
                        "startedAt": 200
                    }
                ]
            }
        }"#;
        let conv: Conversation = serde_json::from_str(json_v3).unwrap();
        assert_eq!(conv.agent.state, AgentRunState::Running);
        assert_eq!(conv.agent.current_iteration, 4);
        assert_eq!(conv.agent.tool_calls.len(), 2);

        // Field names must match the panel's in-memory shape byte for byte —
        // nothing reshapes this on either side of the wire.
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&conv).unwrap()).unwrap();
        let calls = &value["agent"]["toolCalls"];
        assert_eq!(value["agent"]["currentIteration"], 4);
        assert_eq!(calls[0]["toolName"], "ai_terminal_send_input");
        assert_eq!(calls[0]["startedAt"], 100);
        assert_eq!(calls[0]["result"]["success"], true);
        assert_eq!(calls[0]["duration"], 25);
        // A pending call carries neither, exactly like the TS union variant.
        assert_eq!(calls[1]["status"], "pending");
        assert!(calls[1].get("result").is_none());
        assert!(calls[1].get("duration").is_none());
    }

    /// A v1/v2 document predates the snapshot. It must load as an idle agent,
    /// not fail to parse.
    #[test]
    fn v1_document_loads_with_an_empty_agent_snapshot() {
        let json_v1 = r#"{
            "meta": {"id": "old", "title": "Old", "created": 1, "updated": 2, "message_count": 1},
            "messages": [{"role":"user","content":"hi"}]
        }"#;
        let conv: Conversation = serde_json::from_str(json_v1).unwrap();
        assert_eq!(conv.agent, AgentSnapshot::default());
        assert!(conv.agent.is_empty());
    }

    #[test]
    fn migrate_stamps_the_current_version_and_reports_the_change() {
        let mut conv: Conversation = serde_json::from_str(
            r#"{"meta":{"id":"m","title":"M","created":1,"updated":2,"message_count":0},"messages":[]}"#,
        )
        .unwrap();
        assert_eq!(conv.schema_version, 1);
        assert!(conv.migrate(), "a v1 document is a migration");
        assert_eq!(conv.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!conv.migrate(), "a current document is not migrated twice");
    }

    /// The tool-call log holds raw command output, same as `tool_result` on a
    /// message — so it gets the same redaction and the same size cap.
    #[test]
    fn sanitize_redacts_and_truncates_the_agent_tool_log() {
        let mut conv = Conversation {
            meta: ConversationMeta {
                id: "s".into(),
                title: "S".into(),
                session_id: None,
                created: 1,
                updated: 2,
                message_count: 0,
                provider: String::new(),
                model: String::new(),
            },
            messages: vec![],
            schema_version: CURRENT_SCHEMA_VERSION,
            agent: AgentSnapshot {
                state: AgentRunState::Running,
                current_iteration: 1,
                tool_calls: vec![AgentToolCall {
                    status: ToolCallStatus::Done,
                    tool_name: "ai_terminal_read".into(),
                    args: json!({}),
                    started_at: 1,
                    result: Some(ToolCallResult {
                        success: true,
                        output: format!(
                            "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMIabcdEFGHiJKLMNOpQRSTuvWXYZ1\n{}",
                            "x".repeat(TOOL_RESULT_MAX_BYTES)
                        ),
                    }),
                    duration: Some(3),
                }],
            },
        };
        conv.sanitize_for_persist();
        let output = conv.agent.tool_calls[0]
            .result
            .as_ref()
            .map(|r| r.output.clone())
            .unwrap();
        assert!(
            !output.contains("wJalrXUtnFEMIabcdEFGHiJKLMNOpQRSTuvWXYZ1"),
            "secret survived redaction: {output}"
        );
        assert!(output.ends_with("[truncated]"));
    }

    /// A done card renders its result without checking, so an entry that claims
    /// to be done and carries none must not reach the panel.
    #[test]
    fn sanitize_downgrades_a_done_call_with_no_result() {
        let mut conv: Conversation = serde_json::from_str(
            r#"{
                "meta": {"id":"d","title":"D","created":1,"updated":2,"message_count":0},
                "messages": [],
                "agent": {"state":"running","currentIteration":1,
                          "toolCalls":[{"status":"done","toolName":"t","args":{},"startedAt":1}]}
            }"#,
        )
        .unwrap();
        conv.sanitize_for_persist();
        assert_eq!(conv.agent.tool_calls[0].status, ToolCallStatus::Pending);
    }

    /// `String::truncate` panics off a char boundary, and tool output is full of
    /// box drawing and emoji. The cap has to land on a boundary.
    #[test]
    fn truncation_does_not_split_a_multibyte_char() {
        let mut conv = Conversation {
            meta: ConversationMeta {
                id: "u".into(),
                title: "U".into(),
                session_id: None,
                created: 1,
                updated: 2,
                message_count: 1,
                provider: String::new(),
                model: String::new(),
            },
            // "─" is 3 bytes, so byte TOOL_RESULT_MAX_BYTES falls inside one.
            messages: vec![ChatMessage {
                role: "tool".into(),
                content: String::new(),
                timestamp: 1,
                tool_calls: None,
                tool_use_id: Some("c1".into()),
                tool_result: Some("─".repeat(TOOL_RESULT_MAX_BYTES)),
                is_error: Some(false),
            }],
            schema_version: CURRENT_SCHEMA_VERSION,
            agent: AgentSnapshot::default(),
        };
        conv.sanitize_for_persist();
        let result = conv.messages[0].tool_result.as_deref().unwrap();
        assert!(result.ends_with("[truncated]"));
        assert!(result.len() <= TOOL_RESULT_MAX_BYTES + "\n[truncated]".len());
    }

    #[test]
    fn l1_message_omits_tool_fields_in_json() {
        let msg = ChatMessage::text("user", "hello", 1);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("tool_calls"));
        assert!(!json.contains("tool_use_id"));
        assert!(!json.contains("tool_result"));
        assert!(!json.contains("is_error"));
    }
}
