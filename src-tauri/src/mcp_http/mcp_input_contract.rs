//! The advertised schema is also the native pre-dispatch validation contract.
use std::{collections::HashMap, sync::LazyLock};

use serde_json::{Value, json};

use super::mcp_transport_catalogue::*;

/// Add the action-dependent requirements that prose alone cannot enforce.
pub(super) fn complete_schema(tool: &mut Value) {
    let name = tool["name"].as_str().unwrap_or("").to_owned();
    let schema = &mut tool["inputSchema"];
    let actions = match name.as_str() {
        "session" => Some(SESSION_ACTIONS),
        "agent" => Some(AGENT_ACTIONS),
        "repo" => Some(REPO_ACTIONS),
        "task" => Some(TASK_ACTIONS),
        "ui" => Some(UI_ACTIONS),
        "voice" => Some(VOICE_ACTIONS),
        "debug" => Some(DEBUG_ACTIONS),
        _ => None,
    };
    if let Some(actions) = actions {
        schema["properties"]["action"]["enum"] = json!(actions.split(", ").collect::<Vec<_>>());
    }
    let mut conditions = Vec::new();
    let mut required = |actions: &[&str], fields: &[&str]| {
        conditions.push(json!({
            "if": {"properties":{"action":{"enum":actions}}, "required":["action"]},
            "then": {"required":fields}
        }));
    };
    match name.as_str() {
        "session" => {
            required(
                &[
                    "submit",
                    "force_retry",
                    "input",
                    "output",
                    "status",
                    "wait",
                    "resize",
                    "rename",
                    "keep_open",
                    "suspend",
                    "close",
                    "kill",
                    "pause",
                    "resume",
                ],
                &["session_id"],
            );
            required(&["submit"], &["input"]);
            required(&["rename"], &["name"]);
            required(&["keep_open"], &["enabled"]);
            required(&["declare_worktree"], &["worktree_path"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"input"}},"required":["action"]},"then":{"anyOf":[{"required":["input"]},{"required":["special_key"]},{"required":["pty_description"]}]}}));
        }
        "agent" => {
            required(&["send"], &["to", "message"]);
            required(&["subscribe", "ack", "unsubscribe"], &["subscription_id"]);
            required(&["ack"], &["cursor"]);
        }
        "repo" => {
            required(
                &[
                    "add",
                    "branch_integrations",
                    "branch_integration",
                    "worktree_list",
                    "worktree_lifecycle",
                    "worktree_create",
                    "worktree_remove",
                    "orphan_cleanup_answer",
                    "branch_delete",
                    "progress_list",
                ],
                &["path"],
            );
            required(
                &["branch_integration", "worktree_lifecycle", "branch_delete"],
                &["branch"],
            );
            required(&["orphan_cleanup_answer"], &["decision"]);
            required(&["work_item_act"], &["operation"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"worktree_remove"}},"required":["action"]},"then":{"anyOf":[{"required":["branch"]},{"required":["worktree_path"]}]}}));
        }
        "ui" => {
            required(&["tab"], &["id", "title"]);
            required(&["toast"], &["title"]);
            required(&["screenshot"], &["id"]);
            conditions.push(json!({"if":{"properties":{"action":{"const":"tab"}},"required":["action"]},"then":{"oneOf":[{"required":["html"]},{"required":["url"]}]}}));
            conditions.push(json!({"if":{"properties":{"action":{"const":"confirm"}},"required":["action"]},"then":{"anyOf":[{"required":["title"]},{"required":["proposalId"]}]}}));
        }
        "voice" => required(&["speak"], &["text"]),
        "debug" => required(&["invoke_js"], &["script"]),
        "remote" => required(&["update"], &["confirmed_sessions", "expected_sha256"]),
        _ => {}
    }
    if !conditions.is_empty() {
        schema["allOf"] = json!(conditions);
    }
    if matches!(name.as_str(), "session" | "agent") {
        for field in ["rows", "cols"] {
            schema["properties"][field]["minimum"] = json!(1);
            schema["properties"][field]["maximum"] = json!(super::MAX_TERMINAL_DIMENSION);
        }
    }
    if name == "agent" {
        // Launch has two disjoint forms. Validate both before the journal or PTY
        // handler runs, and publish the same contract through lazy discovery.
        let spawn_properties: serde_json::Map<String, Value> = [
            "prompt",
            "pty_description",
            "model",
            "env",
            "print_mode",
            "output_format",
            "agent_type",
            "binary_path",
            "args",
            "rows",
            "cols",
        ]
        .into_iter()
        .map(|field| (field.to_owned(), schema["properties"][field].clone()))
        .collect();
        schema["properties"]["spawn"]["properties"] = json!(spawn_properties);
        schema["properties"]["spawn"]["required"] = json!(["prompt"]);
        schema["properties"]["spawn"]["not"] = json!({"anyOf": [
            {"required":["action"]}, {"required":["name"]}, {"required":["keep_open"]},
            {"required":["cwd"]}, {"required":["parent_session_id"]}, {"required":["idempotency_key"]}
        ]});
        schema["properties"]["workspace"]["oneOf"] = json!([
            {"type":"object", "properties":{"existing_path":{"type":"string"}},
             "required":["existing_path"], "additionalProperties":false},
            {"type":"object", "properties":{"repo_path":{"type":"string"}, "branch":{"type":"string"}, "base_ref":{"type":["string","null"]}},
             "required":["repo_path","branch"], "additionalProperties":false}
        ]);
        schema["allOf"].as_array_mut().unwrap().push(json!({
            "if":{"properties":{"action":{"const":"launch"}},"required":["action"]},
            "then":{"oneOf":[
                {"properties":{"action":{},"launch_id":{}}, "required":["launch_id"], "additionalProperties":false},
                {"properties":{"action":{},"idempotency_key":{"pattern":"\\S"},"parent_session_id":{},"name":{"pattern":"^\\S(?:[\\s\\S]*\\S)?$"},"keep_open":{},"workspace":{},"spawn":{}},
                 "required":["idempotency_key","parent_session_id","name","keep_open","workspace","spawn"], "additionalProperties":false}
            ]}
        }));
    }
    if name == "progress" {
        schema["additionalProperties"] = json!(false);
        schema["properties"]["text"]["maxLength"] = json!(crate::progress::MAX_TEXT_CHARS);
        schema["properties"]["text"]["minLength"] = json!(1);
        schema["properties"]["text"]["pattern"] = json!("\\S");
        schema["properties"]["step"]["maxLength"] = json!(crate::progress::MAX_STEP_CHARS);
        schema["properties"]["text"]["description"] = json!(
            "Outcome, not implementation. At most 500 Unicode scalar values, before trimming; not UTF-8 bytes, UTF-16 units or grapheme clusters."
        );
    }
}

struct Contract {
    schema: Value,
    validator: Result<jsonschema::Validator, String>,
}

static CONTRACTS: LazyLock<HashMap<String, Contract>> = LazyLock::new(|| {
    native_tool_definitions()
        .as_array()
        .expect("native tool array")
        .iter()
        .map(|tool| {
            let schema = tool["inputSchema"].clone();
            let validator = jsonschema::validator_for(&schema)
                .map_err(|_| "invalid native input schema".to_owned());
            (
                tool["name"].as_str().expect("native name").to_owned(),
                Contract { schema, validator },
            )
        })
        .collect()
});

pub(super) fn validate(name: &str, args: &Value) -> Option<Value> {
    let contract = CONTRACTS.get(name)?;
    let validator = match &contract.validator {
        Ok(validator) => validator,
        Err(error) => {
            return Some(
                json!({"error":error,"code":"invalid_contract","effect":"not_started","retryable":false}),
            );
        }
    };
    let violations: Vec<Value> = validator
        .iter_errors(args)
        .take(16)
        .map(|error| {
            let schema_path = error.schema_path().to_string();
            json!({
                "path":error.instance_path().to_string(),
                "schema_path":schema_path,
                "constraint":contract.schema.pointer(&schema_path),
                "message":error.masked().to_string()
            })
        })
        .collect();
    (!violations.is_empty()).then(|| json!({
        "error": format!("{name}: {}: {}", violations[0]["path"].as_str().unwrap_or(""), violations[0]["message"].as_str().unwrap_or("invalid arguments")),
        "code":"invalid_arguments", "effect":"not_started", "outcome":"rejected",
        "retryable":false, "repair":{"retry_after_correction":true}, "violations":violations
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches: newly advertised subscription actions omit their repairable required fields.
    #[test]
    fn actionable_subscription_schema_requires_identity_and_ack_cursor() {
        for action in ["subscribe", "ack", "unsubscribe"] {
            let error = validate("agent", &json!({"action":action})).unwrap();
            assert_eq!(error["code"], "invalid_arguments");
            let args = json!({"action":action,"subscription_id":"pump"});
            assert_eq!(validate("agent", &args).is_some(), action == "ack");
        }
        assert!(
            validate(
                "agent",
                &json!({"action":"ack","subscription_id":"pump","cursor":1})
            )
            .is_none()
        );
    }

    /// Catches: a client and server disagree on historical bad inputs or Unicode boundaries.
    #[test]
    fn action_validation_consumer_fixture_matches_advertised_contract() {
        let cases: Value =
            serde_json::from_str(include_str!("fixtures/action-input-contract-2007.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            let valid = case["valid"].as_bool().unwrap();
            assert_eq!(
                validate(tool, &case["arguments"]).is_none(),
                valid,
                "{}",
                case["name"]
            );
            if tool == "progress" && case["arguments"].get("text").is_some() {
                let input: crate::progress::ProgressReportInput =
                    serde_json::from_value(case["arguments"].clone()).unwrap();
                assert_eq!(
                    input.into_entry(None).unwrap().validate().is_ok(),
                    valid,
                    "shared HTTP/IPC core: {}",
                    case["name"]
                );
            }
        }
    }

    /// Catches: repair errors echo secret input values or recommend an unchanged retry.
    #[test]
    fn action_validation_repairs_exclude_argument_values() {
        let error = validate("agent", &json!({"action":"secret-token-not-an-action"})).unwrap();
        assert!(!error.to_string().contains("secret-token-not-an-action"));
        assert_eq!(error["violations"][0]["path"], "/action");
        assert!(
            error["violations"][0]["constraint"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == "list_peers")
        );
        assert_eq!(error["effect"], "not_started");
        assert_eq!(error["retryable"], false);
    }
}
