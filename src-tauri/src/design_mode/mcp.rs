//! Agent-facing Design Mode lifecycle uses the same core as HTTP and IPC.
use crate::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) fn tool_definition() -> Value {
    json!({
        "name": "design_mode",
        "description": "Start or stop Design Mode on the host. Use start with session_id and url as the default path for layout work: it binds the browser to the agent that printed the link and opens that HTTP(S) URL directly. Omit url to use dev-server discovery/onboarding. Requires a live terminal agent; an ACP coordinator should name the target terminal agent. stop and reload use repo_path from start/status; reload makes the browser reload the reviewed tab once, after you change the page; status lists repository bindings. Browser extension setup is required once per repository profile.",
        "inputSchema": {"type":"object", "additionalProperties":false, "required":["action"], "properties": {
            "action":{"type":"string","enum":["start","stop","status","reload"]},
            "session_id":{"type":"string","description":"Live agent PTY id, TUIC session, alias, unique prefix or display name; required for start"},
            "repo_path":{"type":"string","description":"Repository path returned by start/status; required for stop and reload"},
            "browser":{"type":"string","enum":["chrome","edge"]},
            "url":{"type":"string","description":"Absolute HTTP or HTTPS URL, including localhost, opened directly on start"}
        }}
    })
}

pub(crate) async fn handle(state: &Arc<AppState>, args: &Value) -> Value {
    #[cfg(feature = "desktop")]
    let result = execute(state, args).await;
    #[cfg(not(feature = "desktop"))]
    let result: Result<Value, String> = {
        let _ = (state, args);
        Err("Design Mode requires the desktop host".into())
    };
    result.unwrap_or_else(|error| json!({"error":error}))
}

#[cfg(feature = "desktop")]
async fn execute(state: &Arc<AppState>, args: &Value) -> Result<Value, String> {
    let required = |key: &str| {
        args[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{key} is required"))
    };
    let optional = |key: &str| -> Result<Option<String>, String> {
        match args.get(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.clone())),
            Some(_) => Err(format!("{key} must be a string")),
        }
    };
    match args["action"].as_str() {
        Some("start") => {
            let reference = required("session_id")?;
            let (browser, url) = (optional("browser")?, optional("url")?);
            let session = state
                .resolve_session_ref_checked(&reference)?
                .ok_or("Agent session no longer exists")?;
            super::start(state, session, browser, url).await
        }
        Some("stop") => super::stop(state, &required("repo_path")?).await,
        Some("reload") => super::reload(state, &required("repo_path")?).await,
        Some("status") => Ok(json!(super::statuses(state).await)),
        _ => Err("Choose Design Mode start, stop, status or reload".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn mcp_rejects_invalid_start_fields_without_launching() {
        // Catches: omitted or incorrectly typed fields reaching native launch.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for (args, expected) in [
            (json!({"action":"start"}), "session_id is required"),
            (json!({"action":"stop"}), "repo_path is required"),
            (
                json!({"action":"unknown"}),
                "Choose Design Mode start, stop or status",
            ),
            (
                json!({"action":"start","session_id":"missing","url":4}),
                "url must be a string",
            ),
        ] {
            assert_eq!(handle(&state, &args).await["error"], expected, "{args}");
        }
    }
    #[tokio::test]
    async fn mcp_status_reads_the_shared_empty_lifecycle() {
        // Catches: status adapter inventing a different lifecycle snapshot.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        #[cfg(feature = "desktop")]
        assert_eq!(handle(&state, &json!({"action":"status"})).await, json!([]));
        #[cfg(not(feature = "desktop"))]
        assert!(
            handle(&state, &json!({"action":"status"}))
                .await
                .get("error")
                .is_some()
        );
    }
}
