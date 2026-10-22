//! MCP permission domains. Catalogue names are not the persisted policy contract.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct McpCapabilities {
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates MCP capability denials for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub denied: Vec<String>,
    /// Retained until the user resolves them; never silently discard old policy.
    #[schemars(extend("x-not-setting" = "Unresolved legacy MCP policy migration diagnostics"))]
    pub migration_unknown_tools: Vec<String>,
}

impl Default for McpCapabilities {
    fn default() -> Self {
        Self {
            denied: vec!["settings".into(), "diagnostics".into()],
            migration_unknown_tools: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for McpCapabilities {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Current {
                denied: Vec<String>,
                #[serde(default)]
                migration_unknown_tools: Vec<String>,
            },
            Legacy(Vec<String>),
        }
        Ok(match Input::deserialize(deserializer)? {
            Input::Current {
                denied,
                migration_unknown_tools,
            } => Self {
                denied,
                migration_unknown_tools,
            },
            Input::Legacy(names) => Self::from_legacy(&names),
        })
    }
}

impl McpCapabilities {
    pub(crate) fn from_legacy(names: &[String]) -> Self {
        let mut result = Self {
            denied: Vec::new(),
            migration_unknown_tools: Vec::new(),
        };
        for name in names {
            if let Some(capability) = legacy_capability(name) {
                if !result.denied.iter().any(|denied| denied == capability) {
                    result.denied.push(capability.into());
                }
            } else if !result.migration_unknown_tools.contains(name) {
                tracing::warn!(tool = %name, "Unknown legacy MCP tool denial; retained in migration diagnostics");
                result.migration_unknown_tools.push(name.clone());
            }
        }
        result
    }

    pub(crate) fn allows(&self, tool: &str, action: Option<&str>) -> bool {
        let Some(parent) = tool_capability(tool) else {
            return true;
        };
        !self.denied.iter().any(|denied| {
            denied == parent
                || action
                    .is_some_and(|action| action_capability(tool, action) == Some(denied.as_str()))
        })
    }
}

fn legacy_capability(name: &str) -> Option<&'static str> {
    match name {
        "task" => Some("agent_tasks"),
        "story" => Some("workflow.story"),
        "workflow_run" => Some("workflow.run"),
        "workflow_story_create" => Some("workflow.proposals"),
        "workflow_report" => Some("workflow.reports"),
        "workflow_launch" => Some("workflow.launch"),
        "design_mode" => Some("design"),
        other => tool_capability(other),
    }
}

fn tool_capability(tool: &str) -> Option<&'static str> {
    match tool {
        "session" => Some("session"),
        "agent" => Some("agent"),
        "repo" => Some("repo"),
        "workflow" => Some("workflow"),
        "automations" => Some("automations"),
        "ui" => Some("ui"),
        "secret" => Some("secret"),
        "settings" => Some("settings"),
        "progress" => Some("progress"),
        "debug" => Some("diagnostics"),
        "plugin_dev_guide" => Some("plugin_reference"),
        "remote" => Some("remote_deployment"),
        "voice" => Some("voice"),
        "telegram" => Some("telegram"),
        _ => None,
    }
}

fn action_capability(tool: &str, action: &str) -> Option<&'static str> {
    match (tool, action) {
        ("agent", "task_get" | "task_cancel") => Some("agent_tasks"),
        ("workflow", "story") => Some("workflow.story"),
        ("workflow", "run") => Some("workflow.run"),
        ("workflow", "proposal_create") => Some("workflow.proposals"),
        ("workflow", "report") => Some("workflow.reports"),
        ("workflow", "launch") => Some("workflow.launch"),
        ("ui", action) if action.starts_with("design_") => Some("design"),
        _ => None,
    }
}

/// Metadata shared by Settings clients. Core/recovery restrictions are advanced;
/// optional effects have prominent switches independent of tool implementation.
pub(crate) fn catalog() -> Vec<serde_json::Value> {
    [
        ("secret", "Secrets", "Collect and use secrets through private forms.", true),
        ("telegram", "Telegram", "Send external messages; requires Telegram configuration.", true),
        ("settings", "Settings access", "Read and propose settings; trusted UI opt-in is also required.", true),
        ("diagnostics", "Diagnostics / JS inspection", "Inspect diagnostics and execute frontend inspection scripts.", true),
        ("remote_deployment", "Remote daemon deployment", "Preview and update connected remote daemons with deployment consent.", true),
        ("progress", "Progress tracking", "Record outcomes; global Progress tracking must also be enabled.", true),
        ("session", "Terminal sessions", "Create and control terminal sessions.", false),
        ("agent", "Peers and mail", "Launch peers and exchange durable messages, including task actions.", false),
        ("agent_tasks", "Background task outcomes", "Read and cancel task outcomes, without killing terminals.", false),
        ("repo", "Repositories", "Manage repositories and worktrees.", false),
        ("workflow", "Project workflows", "All story, run, proposal, report and launch actions.", false),
        ("workflow.story", "Stories and plans", "Read and update native stories and plans.", false),
        ("workflow.run", "Workflow runs", "Start and inspect published workflow graphs.", false),
        ("workflow.proposals", "Workflow story proposals", "Create idempotent coordinator proposals.", false),
        ("workflow.reports", "Workflow reports", "Submit caller-bound attempt outcomes.", false),
        ("workflow.launch", "Workflow agent launch", "Launch pinned workflow attempts.", false),
        ("automations", "Scheduled automations", "Manage scheduled definitions and runs.", false),
        ("ui", "User interaction", "Tabs, notices and confirmation, including Design Mode actions.", false),
        ("design", "Design Mode", "Browser inspection; existing extension host grants remain required.", false),
        ("voice", "Spoken replies", "Caller-bound speech; voice configuration remains required.", false),
        ("plugin_reference", "Plugin authoring reference", "Read the plugin development guide.", false),
    ].into_iter().map(|(id, title, description, optional)| serde_json::json!({"name":id,"title":title,"description":description,"optional":optional})).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: merging a former tool silently widens a saved denial, or drops unknown names.
    #[test]
    fn legacy_denials_migrate_once_without_authority_expansion() {
        for (old, tool, denied_action, sibling) in [
            ("task", "agent", "task_get", "list_peers"),
            ("story", "workflow", "story", "run"),
            ("workflow_run", "workflow", "run", "story"),
            (
                "workflow_story_create",
                "workflow",
                "proposal_create",
                "run",
            ),
            ("workflow_report", "workflow", "report", "run"),
            ("workflow_launch", "workflow", "launch", "run"),
            ("design_mode", "ui", "design_status", "toast"),
        ] {
            let mut legacy = serde_json::to_value(crate::config::AppConfig::default()).unwrap();
            legacy.as_object_mut().unwrap().remove("mcp_capabilities");
            legacy["disabled_native_tools"] = serde_json::json!([old, "unknown_tool"]);
            let config: crate::config::AppConfig = serde_json::from_value(legacy).unwrap();
            assert!(
                !config.mcp_capabilities.allows(tool, Some(denied_action)),
                "{old}"
            );
            assert!(config.mcp_capabilities.allows(tool, Some(sibling)), "{old}");
            assert_eq!(
                config.mcp_capabilities.migration_unknown_tools,
                ["unknown_tool"]
            );
            let persisted = serde_json::to_value(&config).unwrap();
            assert!(persisted.get("disabled_native_tools").is_none());
            let again: crate::config::AppConfig = serde_json::from_value(persisted).unwrap();
            assert_eq!(again.mcp_capabilities, config.mcp_capabilities);
        }
        for (old, tool, action) in [
            ("agent", "agent", "task_cancel"),
            ("workflow", "workflow", "report"),
            ("ui", "ui", "design_start"),
            ("debug", "debug", "invoke_js"),
            ("remote", "remote", "update"),
        ] {
            let policy = McpCapabilities::from_legacy(&[old.into()]);
            assert!(!policy.allows(tool, Some(action)), "{old}");
        }
    }
}
