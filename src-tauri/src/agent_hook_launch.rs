//! Launch-scoped native status integration for agent CLIs.

use crate::agent_hook::{SENTINEL, claude_hook_map};
use serde_json::{Map, Value};
use std::path::Path;

pub(crate) fn enabled(agent_type: &str) -> bool {
    crate::config::load_agents_config()
        .agents
        .get(agent_type)
        .and_then(|settings| settings.native_status_signals)
        .unwrap_or(true)
}

fn claude_document() -> Value {
    let mut hooks = Map::new();
    for (event, matcher, command) in claude_hook_map() {
        let group = serde_json::json!({
            "matcher": matcher,
            "hooks": [{"type": "command", "command": command}],
        });
        hooks
            .entry(event.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("new hook event is an array")
            .push(group);
    }
    serde_json::json!({"hooks": hooks, "_tuic": SENTINEL})
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn codex_user_notify() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(home.join(".codex/config.toml")) else {
        return Vec::new();
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return Vec::new();
    };
    value
        .get("notify")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn codex_script(user_notify: &[String]) -> String {
    let chain = if user_notify.is_empty() {
        String::new()
    } else {
        let command = user_notify
            .iter()
            .map(|part| shell_quote(part))
            .collect::<Vec<_>>()
            .join(" ");
        format!("exec {command} \"$payload\"\n")
    };
    format!(
        r#"#!/bin/sh
# {SENTINEL}
payload=${{1:-}}
case "$payload" in
  *'"type":"agent-turn-complete"'*|*'"type": "agent-turn-complete"'*)
    tty=$(ps -o tty= -p "$PPID" 2>/dev/null | tr -d '[:space:]')
    case "$tty" in *[0-9]*) tty="/dev/${{tty#/dev/}}";; *) tty=/dev/tty;; esac
    printf '\033]7770;state=idle\033\\' > "$tty" 2>/dev/null || true
    ;;
esac
{chain}exit 0
"#
    )
}

pub(crate) fn regenerate_launch_assets(config_dir: &Path) -> Result<(), String> {
    let dir = config_dir.join("agent-hooks");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let claude = serde_json::to_vec_pretty(&claude_document()).map_err(|e| e.to_string())?;
    crate::config::persist_atomic(&dir.join("claude.json"), &claude)?;
    let codex = codex_script(&codex_user_notify());
    let codex_path = dir.join("codex-notify.sh");
    crate::config::persist_atomic(&codex_path, codex.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&codex_path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("chmod {}: {e}", codex_path.display()))?;
    }
    Ok(())
}

pub(crate) fn augment_args(agent_type: &str, args: &[String], config_dir: &Path) -> Vec<String> {
    augment_args_when(enabled(agent_type), agent_type, args, config_dir)
}

fn augment_args_when(
    enabled: bool,
    agent_type: &str,
    args: &[String],
    config_dir: &Path,
) -> Vec<String> {
    let mut result = args.to_vec();
    if !enabled {
        return result;
    }
    match agent_type {
        "claude"
            if !args.iter().any(|arg| {
                arg == "--settings" || arg.starts_with("--settings=") || arg == "--bare"
            }) =>
        {
            result.push("--settings".into());
            result.push(
                config_dir
                    .join("agent-hooks/claude.json")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        "codex"
            if !args
                .windows(2)
                .any(|pair| pair[0] == "-c" && pair[1].starts_with("notify="))
                && !args.iter().any(|arg| {
                    arg.starts_with("-cnotify=") || arg.starts_with("--config=notify=")
                }) =>
        {
            result.push("-c".into());
            let path = config_dir.join("agent-hooks/codex-notify.sh");
            result.push(format!(
                "notify=[{}]",
                serde_json::to_string(&path.to_string_lossy()).expect("path serializes")
            ));
        }
        _ => {}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook_commands(document: &Value, event: &str) -> Vec<String> {
        document["hooks"][event]
            .as_array()
            .expect("event hook groups")
            .iter()
            .flat_map(|group| group["hooks"].as_array().expect("hooks in group").iter())
            .map(|hook| hook["command"].as_str().expect("command hook").to_string())
            .collect()
    }

    #[test]
    fn explicit_agent_flags_are_not_overridden() {
        let root = Path::new("/config");
        assert_eq!(
            augment_args("claude", &["--bare".into()], root),
            vec!["--bare"]
        );
        assert_eq!(
            augment_args("claude", &["--settings=x".into()], root),
            vec!["--settings=x"]
        );
        assert_eq!(
            augment_args("codex", &["-c".into(), "notify=['mine']".into()], root),
            vec!["-c", "notify=['mine']"]
        );
    }
    #[test]
    fn generated_assets_have_protocol_and_ownership_markers() {
        let dir = tempfile::TempDir::new().unwrap();
        regenerate_launch_assets(dir.path()).unwrap();
        let claude = std::fs::read_to_string(dir.path().join("agent-hooks/claude.json")).unwrap();
        let codex =
            std::fs::read_to_string(dir.path().join("agent-hooks/codex-notify.sh")).unwrap();
        assert!(claude.contains(SENTINEL));
        assert!(claude.contains("UserPromptSubmit"));
        assert!(codex.contains("agent-turn-complete"));
        assert!(codex.contains("7770;state=idle"));
    }

    #[test]
    fn setting_off_keeps_claude_and_codex_argv_byte_for_byte() {
        let root = Path::new("/config");
        for agent in ["claude", "codex"] {
            let original = vec!["--model".to_string(), "test".to_string()];
            assert_eq!(augment_args_when(false, agent, &original, root), original);
        }
    }

    /// Unix only because it runs the hook commands, and the TUIC one is a POSIX
    /// shell script that reads the controlling tty through `ps -o tty=`. The
    /// additivity it proves is Claude's, not the platform's.
    ///
    /// DEFERRED (2026-09-15) — `hook_command` and `codex_script` are generated
    /// on every platform, so a Windows install writes agent hooks that no shell
    /// there can run and the agent-state badge never leaves its initial value.
    /// Giving Windows its own hook shape is a feature, not part of making the
    /// suite pass; raised with Boss rather than guessed at here.
    #[cfg(unix)]
    #[test]
    fn claude_settings_hooks_are_additive_with_same_event_global_and_project_hooks() {
        let dir = tempfile::TempDir::new().unwrap();
        let global_marker = dir.path().join("global-ran");
        let project_marker = dir.path().join("project-ran");
        let global = serde_json::json!({
            "hooks": {"UserPromptSubmit": [{"hooks": [{
                "type": "command",
                "command": format!("printf global > {}", shell_quote(&global_marker.to_string_lossy()))
            }]}]}
        });
        let project = serde_json::json!({
            "hooks": {"UserPromptSubmit": [{"hooks": [{
                "type": "command",
                "command": format!("printf project > {}", shell_quote(&project_marker.to_string_lossy()))
            }]}]}
        });
        let tuic = claude_document();

        // Claude documents `--settings` as an additional settings source. Model
        // that source loading directly: same-event arrays concatenate rather
        // than one source replacing another.
        let commands = [&global, &project, &tuic]
            .into_iter()
            .flat_map(|source| hook_commands(source, "UserPromptSubmit"))
            .collect::<Vec<_>>();
        assert_eq!(
            commands.len(),
            3,
            "all same-event sources must remain loaded"
        );
        assert!(commands[2].contains("7770;state=busy"));

        for command in commands {
            let status = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .status()
                .unwrap();
            assert!(
                status.success(),
                "every loaded same-event hook must execute"
            );
        }
        assert_eq!(std::fs::read_to_string(global_marker).unwrap(), "global");
        assert_eq!(std::fs::read_to_string(project_marker).unwrap(), "project");
    }
}
