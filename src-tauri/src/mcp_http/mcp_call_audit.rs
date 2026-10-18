//! Bounded, body-free observations at the shared HTTP/ACP tools/call boundary.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, LazyLock},
};

use axum::{Json, extract::State};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;

const CAPACITY: usize = 1024;
const PROJECTION_LIMIT: usize = 100;

static OPERATIONS: LazyLock<HashMap<String, Vec<String>>> = LazyLock::new(|| {
    super::mcp_transport_catalogue::native_tool_definitions()
        .as_array()
        .expect("native catalogue is an array")
        .iter()
        .map(|tool| {
            let actions = tool["inputSchema"]["properties"]["action"]["enum"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            (
                tool["name"].as_str().unwrap_or_default().to_owned(),
                actions,
            )
        })
        .collect()
});

#[derive(Clone, Serialize)]
pub(crate) struct Record {
    call_id: Uuid,
    client_call_id: Option<Uuid>,
    parent_operation_id: Option<Uuid>,
    operation: String,
    target_digest: Option<String>,
    started_ms: u64,
    response_text_bytes: Option<usize>,
    application_error: Option<bool>,
    transport_error: &'static str,
    effect: &'static str,
}

#[derive(Default)]
pub(crate) struct Audit {
    rows: VecDeque<Record>,
    evicted: u64,
}

impl Audit {
    fn push(&mut self, row: Record) {
        if self.rows.len() == CAPACITY {
            self.rows.pop_front();
            self.evicted = self.evicted.saturating_add(1);
        }
        self.rows.push_back(row);
    }

    fn projection(&self) -> Value {
        json!({
            "records": self.rows.iter().rev().take(PROJECTION_LIMIT).collect::<Vec<_>>(),
            "retained": self.rows.len(), "evicted": self.evicted,
            "capacity": CAPACITY, "limit": PROJECTION_LIMIT,
            "scope": "runtime_mcp_tools_call_ingress",
            "delivery_observed": false
        })
    }
}

pub(super) async fn endpoint(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(state.mcp.call_audit.lock().projection())
}

/// Owned across dispatch. Dropping an unfinished call must not assert rollback:
/// a blocking handler or upstream may still complete after its future is gone.
pub(super) struct Call<'a> {
    audit: &'a Mutex<Audit>,
    row: Option<Record>,
}

fn metadata_uuid(params: &Value, key: &str) -> Option<Uuid> {
    params["_meta"][key]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
}

impl<'a> Call<'a> {
    pub(super) fn start(audit: &'a Mutex<Audit>, params: &Value) -> Self {
        let name = params["name"].as_str().unwrap_or("");
        let args = if name == "call_tool" {
            &params["arguments"]["arguments"]
        } else {
            &params["arguments"]
        };
        let name = if name == "call_tool" {
            params["arguments"]["tool_name"].as_str().unwrap_or("")
        } else {
            name
        };
        // Only catalogue-defined native names/actions enter the projection.
        // Unknown/upstream names and all target values are digested, never copied.
        let operation = match OPERATIONS.get(name) {
            Some(actions) => {
                let action = args["action"]
                    .as_str()
                    .filter(|action| actions.iter().any(|value| value == action));
                action.map_or_else(|| name.to_owned(), |action| format!("{name}.{action}"))
            }
            None => format!("opaque:{}", hex::encode(Sha256::digest(name.as_bytes()))),
        };
        let target_digest = ["to", "session_id", "path", "worktree_path", "id"]
            .into_iter()
            .find_map(|key| {
                args[key].as_str().map(|value| {
                    let mut digest = Sha256::new();
                    digest.update(key.as_bytes());
                    digest.update([0]);
                    digest.update(value.as_bytes());
                    hex::encode(digest.finalize())
                })
            });
        Self {
            audit,
            row: Some(Record {
                call_id: Uuid::new_v4(),
                client_call_id: metadata_uuid(params, "tuic/call_id"),
                parent_operation_id: metadata_uuid(params, "tuic/parent_operation_id"),
                operation,
                target_digest,
                started_ms: super::mcp_transport_peer::now_unix_ms(),
                response_text_bytes: None,
                application_error: None,
                transport_error: "cancelled_before_reply",
                effect: "unknown",
            }),
        }
    }

    pub(super) fn finish(mut self, raw: &Value, formatted: &Value) {
        let mut row = self.row.take().expect("a call finishes once");
        row.application_error = Some(formatted["isError"].as_bool().unwrap_or(false));
        row.response_text_bytes = Some(formatted["content"].as_array().map_or(0, |content| {
            content
                .iter()
                .filter_map(|block| {
                    if block["type"] == "text" {
                        block["text"].as_str()
                    } else {
                        None
                    }
                })
                .map(str::len)
                .sum()
        }));
        // DEFERRED (2026-10-18) — proxy registry errors are untyped strings,
        // mixing refusal and transport failure. Do not infer either from prose;
        // typed upstream errors and client lost-reply correlation are still needed.
        // This is a server observation, not an acknowledgement from the client.
        row.transport_error = "unobserved_after_reply_construction";
        row.effect = match raw["effect"].as_str() {
            Some("not_started") => "not_started",
            Some("completed") => "completed",
            _ => "unknown",
        };
        self.audit.lock().push(row);
    }
}

impl Drop for Call<'_> {
    fn drop(&mut self) {
        if let Some(row) = self.row.take() {
            self.audit.lock().push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: a cancelled dispatch gets recorded as no effect, or leaks arguments.
    #[test]
    fn mcp_outcome_record_cancelled_call_keeps_unknown_effect_without_bodies() {
        let audit = Mutex::new(Audit::default());
        drop(Call::start(
            &audit,
            &json!({"name":"agent","arguments":{"action":"send","to":"PRIVATE_TARGET","message":"PRIVATE_BODY"}}),
        ));
        let projection = audit.lock().projection();
        assert_eq!(projection["records"][0]["effect"], "unknown");
        assert_eq!(projection["records"][0]["application_error"], Value::Null);
        assert_eq!(
            projection["records"][0]["transport_error"],
            "cancelled_before_reply"
        );
        assert!(!projection.to_string().contains("PRIVATE"));
    }

    // Catches: recipient batches collapse into retries or text bytes count scalars.
    #[test]
    fn mcp_outcome_record_groups_parent_but_preserves_targets_and_utf8_cost() {
        let audit = Mutex::new(Audit::default());
        let parent = Uuid::new_v4();
        for target in ["recipient-one", "recipient-two"] {
            let call = Call::start(
                &audit,
                &json!({"name":"agent","arguments":{"action":"send","to":target},"_meta":{"tuic/parent_operation_id":parent}}),
            );
            call.finish(
                &json!({"effect":"not_started"}),
                &json!({"isError":true,"content":[{"type":"text","text":"é"}]}),
            );
        }
        let projection = audit.lock().projection();
        let rows = projection["records"].as_array().unwrap();
        assert_eq!(rows[0]["parent_operation_id"], json!(parent));
        assert_eq!(
            rows[0]["parent_operation_id"],
            rows[1]["parent_operation_id"]
        );
        assert_ne!(rows[0]["call_id"], rows[1]["call_id"]);
        assert_ne!(rows[0]["target_digest"], rows[1]["target_digest"]);
        assert_eq!(rows[0]["response_text_bytes"], 2);
        assert_eq!(rows[0]["effect"], "not_started");
        assert_eq!(rows[0]["application_error"], true);
    }

    // Catches: runtime audit retention or its public projection grows without bound.
    #[test]
    fn mcp_outcome_record_projection_evicts_without_retaining_arbitrary_metadata() {
        let audit = Mutex::new(Audit::default());
        for _ in 0..=CAPACITY {
            drop(Call::start(
                &audit,
                &json!({"name":"SECRET_TOOL","_meta":{"tuic/call_id":"SECRET_ID"}}),
            ));
        }
        let projection = audit.lock().projection();
        assert_eq!(projection["retained"], CAPACITY);
        assert_eq!(projection["evicted"], 1);
        assert_eq!(
            projection["records"].as_array().unwrap().len(),
            PROJECTION_LIMIT
        );
        assert!(!projection.to_string().contains("SECRET"));
    }
}
