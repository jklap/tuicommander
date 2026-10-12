//! Launch-time TUIC output-marker instructions (story 1810-c28c).
//!
//! The MCP `instructions` field reaches an agent once per session at best (Claude
//! does not refresh it on reconnect, Codex never surfaces it). Every agent below
//! also takes extra system instructions at launch, and the marker matrix of
//! 2026-10-12 proved that text survives compact, clear and resume. So TUIC writes
//! the effective text to one file per agent and passes it through the agent's own
//! flag on structured launches (`agent_hook_launch::launch_args`).
//!
//! Agents typed in a TUIC shell (also through an alias or a wrapper script that
//! runs the agent by name) get the same text from the PATH shim in
//! `shell_integration`, or from `OPENCODE_CONFIG_CONTENT` for OpenCode, the only
//! agent with a native env var that appends instructions (verified live,
//! 2026-10-12). The MCP field stays as the fallback for launches neither path
//! reaches, such as an absolute-path launch of a shim-served agent.
use crate::config::{AgentsConfig, AppConfig};
use std::path::{Path, PathBuf};

/// Agents with a verified launch-time instruction channel (installed `--help`,
/// 2026-10-12: claude 2.1.286, codex 0.162.1, grok 1.0.50, opencode 1.18.30,
/// goose 1.49.0, pi 0.84.2, ego 0.1.0).
pub(crate) const AGENTS: [&str; 7] = ["claude", "codex", "grok", "opencode", "goose", "pi", "ego"];

/// Tokens the TUIC parser anchors on. Settings warns when an edited text drops one.
pub(crate) const REQUIRED_MARKERS: [&str; 3] = ["intent:", "suggest:", "💬 "];

const HEADER: &str = "## TUIC output markers\n\n\
Required even under concision rules; omission breaks the UI. Each marker is the first text on its own line, outside lists, quotes and code fences.\n\n";
const CONNECTION: &str = "- Connection: once per MCP connection or reconnect, your first message starts with `TUICommander v{VER} is connected.`";
const CONNECTION_INTENT: &str = " and the next line is the `intent:` line";
const INTENT: &str = "- `intent: <desc> (<title>)` — the first text of your turn, before your first tool call, when the user sends a new request, changes direction, or you resume after an interruption or compaction. On any other turn, do not emit it. `<desc>`: the work in progress, present tense. `<title>`: ≤3 words, spaces not hyphens, the last parenthesis on the line.\n  Example: `intent: Fixing the mobile tab bar height (Mobile tab bar)`\n";
const SUGGEST: &str = "- `suggest: [ A | B | C ]` — once, as the last line of the turn that completes the task or asks the user to choose. Never while work continues. EXACTLY 3 items separated by ` | `, each ≤40 chars.\n";
const ANSWER: &str = "- `💬 ` — prefix every sentence that directly answers the user's question with `💬 ` at row start (marker + space). Exclude status, tool and hook/task notices.\n";

/// The shipped text. A marker turned off for the agent is left out, so the
/// launch text never demands what the MCP instructions withhold.
pub(crate) fn default_text(intent: bool, suggest: bool) -> String {
    let mut out = String::from(HEADER);
    out.push_str(CONNECTION);
    if intent {
        out.push_str(CONNECTION_INTENT);
    }
    out.push_str(".\n");
    if intent {
        out.push_str(INTENT);
    }
    if suggest {
        out.push_str(SUGGEST);
    }
    out.push_str(ANSWER);
    out
}

/// `global AND (per_agent ?? true)` — the one rule for both instruction surfaces.
pub(crate) fn marker_flags(
    app: &AppConfig,
    agents: &AgentsConfig,
    agent: Option<&str>,
) -> (bool, bool) {
    let settings = agent.and_then(|agent| agents.agents.get(agent));
    (
        app.intent_tab_title && settings.and_then(|s| s.intent_tab_title).unwrap_or(true),
        app.suggest_followups && settings.and_then(|s| s.suggest_followups).unwrap_or(true),
    )
}

/// Per-agent override, else the global override, else the default. An empty
/// text turns injection off; `{VER}` becomes the running version.
pub(crate) fn effective_text(app: &AppConfig, agents: &AgentsConfig, agent: &str) -> String {
    let custom = agents
        .agents
        .get(agent)
        .and_then(|settings| settings.launch_instructions.clone())
        .or_else(|| agents.launch_instructions.clone());
    let text = custom.unwrap_or_else(|| {
        let (intent, suggest) = marker_flags(app, agents, Some(agent));
        default_text(intent, suggest)
    });
    text.replace("{VER}", env!("CARGO_PKG_VERSION"))
}

/// Codex has no file flag: its file holds the whole `-c` value, a TOML string.
fn file_content(agent: &str, text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else if agent == "codex" {
        format!(
            "developer_instructions={}",
            toml::Value::String(text.to_owned())
        )
    } else {
        text.to_owned()
    }
}

pub(crate) fn file_path(config_dir: &Path, agent: &str) -> PathBuf {
    let extension = if agent == "codex" { "toml" } else { "md" };
    config_dir.join(format!("agent-hooks/instructions-{agent}.{extension}"))
}

/// Write every agent's file. A disabled agent gets an empty file, which a
/// launch treats as "inject nothing".
pub(crate) fn write_files(config_dir: &Path) -> Result<(), String> {
    let app = crate::config::load_app_config();
    let agents = crate::config::load_agents_config();
    write_files_with(config_dir, &app, &agents)
}

pub(crate) fn write_files_with(
    config_dir: &Path,
    app: &AppConfig,
    agents: &AgentsConfig,
) -> Result<(), String> {
    std::fs::create_dir_all(config_dir.join("agent-hooks")).map_err(|e| e.to_string())?;
    for agent in AGENTS {
        let content = file_content(agent, &effective_text(app, agents, agent));
        let path = file_path(config_dir, agent);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
            crate::config::persist_atomic(&path, content.as_bytes())?;
        }
    }
    Ok(())
}

/// The agent's file after a refresh, or `None` when its text is empty.
fn current_file(agent: &str, config_dir: &Path) -> Option<(PathBuf, String)> {
    if let Err(error) = write_files(config_dir) {
        tracing::error!(
            source = "agent_hooks",
            "Failed to write launch instructions: {error}"
        );
    }
    let path = file_path(config_dir, agent);
    let content = std::fs::read_to_string(&path).ok()?;
    (!content.trim().is_empty()).then_some((path, content))
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| {
            flags
                .iter()
                .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
        })
}

fn first_is(args: &[String], verbs: &[&str]) -> bool {
    args.first()
        .is_some_and(|first| verbs.contains(&first.as_str()))
}

/// Where the flag goes, or `None` when this launch must not carry it: a
/// headless one-shot (its output is consumed by a program), a non-session
/// subcommand, or a user who already passes the same instruction flag.
/// Claude's flag goes last, where `--settings` goes: a run config may wrap
/// the agent (`/bin/sh -c ... {prompt}`) and read its first argument.
fn insertion(agent: &str, args: &[String]) -> Option<usize> {
    match agent {
        "claude" => (!crate::agent_hook_launch::claude_is_subcommand(args, "")
            && !has_flag(
                args,
                &[
                    "-p",
                    "--print",
                    "--system-prompt",
                    "--system-prompt-file",
                    "--append-system-prompt",
                    "--append-system-prompt-file",
                ],
            ))
        .then(|| {
            args.iter()
                .position(|arg| arg == "--")
                .unwrap_or(args.len())
        }),
        "codex" => (!first_is(args, &["exec", "e"])
            && !args
                .windows(2)
                .any(|pair| pair[0] == "-c" && pair[1].starts_with("developer_instructions="))
            && !args.iter().any(|arg| {
                arg.starts_with("-cdeveloper_instructions=")
                    || arg.starts_with("--config=developer_instructions=")
            }))
        .then_some(0),
        "grok" => (!has_flag(args, &["-p", "--single", "--rules"])).then_some(0),
        "goose" => {
            (first_is(args, &["session", "s"]) && !has_flag(args, &["--system"])).then_some(1)
        }
        "pi" => (!first_is(
            args,
            &[
                "install",
                "remove",
                "uninstall",
                "update",
                "list",
                "config",
                "auth",
            ],
        ) && !has_flag(args, &["-p", "--print", "--append-system-prompt"]))
        .then_some(0),
        "ego" => {
            if has_flag(args, &["--append-system"]) {
                None
            } else if first_is(args, &["resume"]) {
                Some(1)
            } else if args.first().is_some_and(|first| {
                first == "run"
                    || crate::agent_hook_launch::EGO_NON_RUN_COMMANDS.contains(&first.as_str())
            }) {
                None
            } else {
                Some(0)
            }
        }
        _ => None,
    }
}

/// Add the agent's instruction flag to a structured launch's argv.
pub(crate) fn args(agent: &str, args: Vec<String>, config_dir: &Path) -> Vec<String> {
    if agent == "opencode" {
        return args;
    }
    let Some(at) = insertion(agent, &args) else {
        return args;
    };
    let Some((path, content)) = current_file(agent, config_dir) else {
        return args;
    };
    let path = path.to_string_lossy().into_owned();
    let flag: Vec<String> = match agent {
        "claude" => vec!["--append-system-prompt-file".into(), path],
        "codex" => vec!["-c".into(), content],
        // Glued with `=`: a text that starts with `-` is not read as a flag.
        "grok" => vec![format!("--rules={content}")],
        "goose" => vec![format!("--system={content}")],
        "pi" => vec!["--append-system-prompt".into(), path],
        "ego" => vec!["--append-system".into(), format!("@{path}")],
        _ => return args,
    };
    let mut result = args;
    result.splice(at..at, flag);
    result
}

/// OpenCode has no flag; it merges `OPENCODE_CONFIG_CONTENT` into its config.
/// Callers apply this before the caller's own environment, so an explicit
/// `OPENCODE_CONFIG_CONTENT` keeps precedence.
pub(crate) fn env(agent: &str, args: &[String], config_dir: &Path) -> Option<(String, String)> {
    if agent != "opencode" || first_is(args, &["run"]) {
        return None;
    }
    let (path, _) = current_file(agent, config_dir)?;
    Some(("OPENCODE_CONFIG_CONTENT".into(), opencode_config(&path)))
}

/// Apply `env` to a structured spawn unless the caller already set the key.
pub(crate) fn apply_env(
    cmd: &mut portable_pty::CommandBuilder,
    agent: &str,
    args: &[String],
    config_dir: &Path,
) {
    if let Some((key, value)) = env(agent, args, config_dir)
        && cmd.get_env(&key).is_none()
    {
        cmd.env(key, value);
    }
}

pub(crate) fn opencode_config(path: &Path) -> String {
    serde_json::json!({"instructions": [path.to_string_lossy()]}).to_string()
}

#[cfg(test)]
#[path = "launch_instructions_tests.rs"]
mod tests;
