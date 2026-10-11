//! MCP schema and adapter; definition decisions stay in the shared action module.

use super::api::{self, Caller};
use crate::state::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

/// Advertise the native automation definition tool.
pub fn tool_definition() -> Value {
    json!({
        "name": "automations",
        "description": "Manage automations through the shared HTTP/IPC API. list returns definition, next_run_ms and last_status. create/update return the saved definition; update accepts definition or enabled, exactly one. run_now starts a run; run reads a saved run; list_runs and summary read history. preview/preview_definition and preset resolve schedules. templates lists built-in recipes; template returns a disabled unsaved draft using template_id, repository, run_config and optional timezone (host local by default). Creation records the bound caller session; delete retains history. worktree_preview inspects saved run provenance and safety; cleanup_worktree explicitly removes only a safe final run worktree, retaining history.",
        "inputSchema": {
            "type": "object", "additionalProperties": false,
            "required": ["action"],
            "properties": {
                "action": {"type":"string", "enum":["list","get","create","update","pause","resume","delete","run_now","run","list_runs","summary","preview","preview_definition","preset","templates","template","worktree_preview","cleanup_worktree"]},
                "id": {"type":"string", "description":"Definition id, or saved run id for run/worktree_preview/cleanup_worktree"},
                "template_id":{"type":"string","description":"Built-in template id from templates"},
                "repository":{"type":"string"}, "run_config":{"type":"string"},
                "enabled":{"type":"boolean"},
                "limit":{"type":"integer","minimum":1,"maximum":100,"default":50},
                "offset":{"type":"integer","minimum":0,"default":0},
                "window":{"type":"string","enum":["24h","7d"]},
                "cron":{"type":"string"}, "timezone":{"type":["string","null"]},
                "count":{"type":"integer","minimum":1,"maximum":20,"default":4},
                "preset":{"oneOf":[
                    {"type":"object","additionalProperties":false,"required":["kind","minute"],"properties":{"kind":{"const":"hourly"},"minute":{"type":"integer","minimum":0,"maximum":59}}},
                    {"type":"object","additionalProperties":false,"required":["kind","hour","minute"],"properties":{"kind":{"enum":["daily","weekdays"]},"hour":{"type":"integer","minimum":0,"maximum":23},"minute":{"type":"integer","minimum":0,"maximum":59}}},
                    {"type":"object","additionalProperties":false,"required":["kind","weekday","hour","minute"],"properties":{"kind":{"const":"weekly"},"weekday":{"type":"integer","minimum":0,"maximum":7},"hour":{"type":"integer","minimum":0,"maximum":23},"minute":{"type":"integer","minimum":0,"maximum":59}}}
                ]},
                "definition": {
                    "type":"object", "additionalProperties":false,
                    "required":["id","name","prompt","run_config","repository","workspace","cron","enabled","grace_secs","overlap","max_duration_secs"],
                    "properties": {
                        "id":{"type":"string"}, "name":{"type":"string"}, "prompt":{"type":"string"},
                        "run_config":{"type":"string"}, "repository":{"type":"string"},
                        "workspace":{"oneOf":[
                            {"type":"object","additionalProperties":false,"required":["mode"],"properties":{"mode":{"const":"existing"}}},
                            {"type":"object","additionalProperties":false,"required":["mode","base_branch"],"properties":{"mode":{"const":"new_per_run"},"base_branch":{"type":"string"}}}
                        ]},
                        "once_local":{"type":["string","null"],"description":"Local YYYY-MM-DDTHH:MM:SS; cron must be empty"},
                        "created_by_session":{"type":["string","null"],"description":"Ignored on create; host owns provenance"},
                        "cron":{"type":"string"}, "timezone":{"type":"string","description":"IANA zone; defaults to local on creation"},
                        "enabled":{"type":"boolean"}, "grace_secs":{"type":"integer","minimum":1},
                        "overlap":{"type":"string","enum":["skip"]}, "max_duration_secs":{"type":"integer","minimum":1},
                        "precheck":{"anyOf":[{"type":"null"},{"type":"object","additionalProperties":false,"required":["command","timeout_secs"],"properties":{"command":{"type":"string"},"timeout_secs":{"type":"integer","minimum":1},"inject_output":{"type":"boolean","default":false,"description":"Prefix the prompt with up to 16 KiB of escaped, untrusted precheck stdout. stderr is never injected."}}}]}
                    }
                }
            }
        }
    })
}

/// Use the same async runtime and validation boundary as HTTP and IPC.
pub async fn handle(args: &Value, creator_session: Option<String>, state: Arc<AppState>) -> Value {
    reply(api::execute_as(args.clone(), state, Caller::Agent(creator_session)).await)
}

fn reply(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|error| json!({"error": error}))
}

#[cfg(test)]
pub(super) fn execute(
    store: &super::definitions::DefinitionStore,
    args: Value,
    creator_session: Option<&str>,
) -> Value {
    reply(api::parse(args).and_then(|action| {
        api::execute_stored_as(
            store,
            &super::store::RunStore::open()?,
            action,
            chrono::Utc::now(),
            Caller::Agent(creator_session.map(str::to_owned)),
        )
    }))
}
