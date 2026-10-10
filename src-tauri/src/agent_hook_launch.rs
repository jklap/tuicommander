//! Launch-scoped native status integration for agent CLIs.

use crate::agent_hook::{SENTINEL, claude_hook_map};
use serde_json::{Map, Value};
use std::path::Path;

/// Recorded installed Claude help supplies advertised verbs when a probe is unavailable.
pub(crate) const RECORDED_CLAUDE_HELP: &str =
    include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt");

fn claude_command_names(help: &str) -> impl Iterator<Item = &str> {
    help.lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
}

pub(crate) fn claude_help_is_usable(help: &str) -> bool {
    claude_command_names(help).next().is_some()
}

pub(crate) fn enabled(agent_type: &str) -> bool {
    crate::config::load_agents_config()
        .agents
        .get(agent_type)
        .and_then(|settings| settings.native_status_signals)
        .unwrap_or(true)
}

pub(crate) static SCREEN_POLICIES: &[(&str, &str, Option<&str>)] = &[
    ("codex", "--no-alt-screen", Some("exec")),
    ("grok", "--no-alt-screen", None),
    ("opencode", "--mini", Some("run")),
];

pub(crate) fn screen_policy(agent_type: &str) -> Option<(&'static str, Option<&'static str>)> {
    SCREEN_POLICIES
        .iter()
        .find(|(agent, _, _)| *agent == agent_type)
        .map(|(_, flag, skip)| (*flag, *skip))
}

pub(crate) fn prevents_alt_screen_in(
    config: &crate::config::AgentsConfig,
    agent_type: &str,
) -> bool {
    config
        .agents
        .get(agent_type)
        .and_then(|settings| settings.prevent_alt_screen)
        .unwrap_or(true)
}

pub(crate) fn prevents_alt_screen(agent_type: &str) -> bool {
    prevents_alt_screen_in(&crate::config::load_agents_config(), agent_type)
}

/// `None` means "ask when detected" — deliberately no `.unwrap_or(..)` here,
/// unlike `enabled()` above: every caller must branch on all three states
/// (`None`/`Some(true)`/`Some(false)`), not collapse to a bool default.
pub(crate) fn wrap_user_function(agent_type: &str) -> Option<bool> {
    crate::config::load_agents_config()
        .agents
        .get(agent_type)
        .and_then(|settings| settings.wrap_user_function)
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

/// Codex's `notify` script. The tty comes from `$TUIC_PTY_TTY` — the device
/// TUIC stamps on every PTY child (`spawn_pty_pair_with_retry`), inherited by
/// the agent and by the notify it runs — exactly as `tuic-hook` resolves it.
/// `ps -o tty=` is only the fallback for an agent TUIC did not spawn: inside a
/// sandboxed agent the setuid `ps` cannot be exec'd, which used to drop the
/// tty to `/dev/tty` (none for a detached notify) and lose `state=idle`.
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
    tty=${{TUIC_PTY_TTY:-}}
    if [ -z "$tty" ]; then
      tty=$(ps -o tty= -p "$PPID" 2>/dev/null | tr -d '[:space:]')
      case "$tty" in *[0-9]*) tty="/dev/${{tty#/dev/}}";; *) tty=/dev/tty;; esac
    fi
    printf '\033]7770;state=idle\033\\' > "$tty" 2>/dev/null || true
    ;;
esac
{chain}exit 0
"#
    )
}

/// Boot-time entry shared by the desktop and both headless binaries: a
/// `claude` spawn points at `agent-hooks/claude.json`, so every process that can
/// spawn agents must write it before serving. A failure is logged, not fatal.
pub(crate) fn regenerate_launch_assets_at_boot(config_dir: &Path) {
    if let Err(error) = regenerate_launch_assets(config_dir) {
        tracing::error!(
            source = "agent_hooks",
            "Failed to generate launch-scoped agent status assets: {error}"
        );
    }
}

pub(crate) fn regenerate_launch_assets(config_dir: &Path) -> Result<(), String> {
    let dir = config_dir.join("agent-hooks");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let claude = serde_json::to_vec_pretty(&claude_document()).map_err(|e| e.to_string())?;
    crate::config::persist_atomic(&dir.join("claude.json"), &claude)?;
    let codex = codex_script(&codex_user_notify());
    let codex_path = dir.join("codex-notify.sh");
    crate::config::persist_atomic_with_mode(&codex_path, codex.as_bytes(), 0o700)?;
    Ok(())
}

pub(crate) fn augment_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
    config_dir: &Path,
) -> Vec<String> {
    let enabled = enabled(agent_type);
    if enabled && agent_type == "claude" && args.first().is_some_and(|arg| !arg.starts_with('-')) {
        let help = crate::agent::cli_help(binary_path).unwrap_or_default();
        if claude_is_subcommand(args, &help) {
            return args.to_vec();
        }
    }
    let result = augment_args_when(enabled, agent_type, args, config_dir);
    if screen_flag_candidate(agent_type, args).is_none() {
        return result;
    }
    add_screen_flag(
        agent_type,
        args,
        result,
        crate::agent::supports_no_alt_screen(agent_type, binary_path),
    )
}

/// Use the same screen policy for commands typed into an existing terminal.
/// The frontend owns shell quoting, while this function owns the agent-specific
/// flag and its capability check.
pub(crate) fn build_agent_launch_args(
    agent_type: &str,
    binary_path: &str,
    args: &[String],
) -> Vec<String> {
    if screen_flag_candidate(agent_type, args).is_none() {
        return args.to_vec();
    }
    add_screen_flag(
        agent_type,
        args,
        args.to_vec(),
        crate::agent::supports_no_alt_screen(agent_type, binary_path),
    )
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn prepare_agent_launch_args(
    agent_type: String,
    binary_path: String,
    args: Vec<String>,
) -> Vec<String> {
    let fallback = args.clone();
    tokio::task::spawn_blocking(move || build_agent_launch_args(&agent_type, &binary_path, &args))
        .await
        .unwrap_or(fallback)
}

fn screen_flag_candidate(agent_type: &str, args: &[String]) -> Option<&'static str> {
    if !prevents_alt_screen(agent_type) {
        return None;
    }
    let (flag, excluded_first_arg) = screen_policy(agent_type)?;
    if excluded_first_arg.is_some_and(|excluded| args.first().is_some_and(|arg| arg == excluded))
        || args.iter().any(|arg| arg == flag)
    {
        return None;
    }
    Some(flag)
}

fn add_screen_flag(
    agent_type: &str,
    args: &[String],
    mut result: Vec<String>,
    supported: bool,
) -> Vec<String> {
    if let Some(screen_flag) = screen_flag_candidate(agent_type, args)
        && supported
    {
        result.insert(0, screen_flag.to_string());
    }
    result
}

/// Installed help owns advertised verbs. On probe failure, retain the verbs
/// verified in the recorded help. The reported remote-control refusal proves
/// that exact hidden command also rejects root launch settings.
fn claude_is_subcommand(args: &[String], help: &str) -> bool {
    let Some(first) = args
        .first()
        .filter(|arg| !arg.is_empty() && !arg.starts_with('-'))
    else {
        return false;
    };
    let help = if !claude_help_is_usable(help) {
        RECORDED_CLAUDE_HELP
    } else {
        help
    };
    first == "remote-control"
        || claude_command_names(help).any(|names| names.split('|').any(|name| name == first))
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
            if !claude_is_subcommand(args, "")
                && !args.iter().any(|arg| {
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

    /// Catches: a successful probe publishes unusable stdout/stderr as Claude help.
    #[test]
    fn successful_probe_without_command_rows_is_unavailable() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let truncated = RECORDED_CLAUDE_HELP.split("Commands:").next().unwrap();
        for (case, help) in [
            ("whitespace", " \n\t"),
            ("truncated", truncated),
            ("header-only", "Commands:\n  \n"),
            ("recorded", RECORDED_CLAUDE_HELP),
        ] {
            let help_path = dir.path().join(format!("{case}.txt"));
            std::fs::write(&help_path, help).unwrap();
            for (stream, redirect) in [("stdout", ""), ("stderr", " >&2")] {
                let binary = crate::test_support::fake_ssh_script(
                    &format!("claude-help-{case}-{stream}"),
                    &format!(
                        "cat {}{redirect}",
                        shell_quote(&help_path.to_string_lossy())
                    ),
                    &format!("type \"{}\"{redirect}", help_path.display()),
                );
                let actual = crate::agent::cli_help(&binary.to_string_lossy());
                assert_eq!(actual.is_some(), case == "recorded", "{case} on {stream}");
            }
        }
    }

    /// Catches: a hyphen in an ordinary prompt disables native status hooks.
    #[test]
    fn prompt_with_hyphen_retains_native_settings() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        for prompt in ["fix-bug", "Explain the remote-control failure"] {
            let args = vec![prompt.to_string()];
            let actual = augment_args("claude", &binary.to_string_lossy(), &args, dir.path());
            assert!(
                actual.iter().any(|arg| arg == "--settings"),
                "prompt {prompt:?} lost native status settings: {actual:?}"
            );
        }
    }

    /// Catches: a failed help probe adds launch settings to the mcp subcommand.
    #[test]
    fn unavailable_help_does_not_corrupt_subcommand_arguments() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        let args = vec!["mcp".into(), "list".into()];
        assert_eq!(
            augment_args("claude", &binary.to_string_lossy(), &args, dir.path()),
            args,
            "a failed help probe must not add settings to a subcommand"
        );
    }

    /// Catches: failed help injects root settings into the recorded auth subcommand.
    #[test]
    fn unavailable_help_preserves_recorded_auth_subcommand() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(
            dir.path()
                .starts_with(crate::test_support::test_temp_root())
        );
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let binary = dir.path().join("missing-claude");
        let args = vec!["auth".into(), "status".into()];
        assert_eq!(
            augment_args("claude", &binary.to_string_lossy(), &args, dir.path()),
            args,
            "recorded auth command must retain argv when help is unavailable"
        );
    }

    /// Catches: launch-scoped settings corrupt a verb or its alias, while a
    /// positional prompt gets misclassified as a command.
    #[test]
    fn claude_subcommands_use_captured_help_and_preserve_prompt_launches() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let help = include_str!("../tests/fixtures/agent-help/claude-2026-10-04.txt");
        let help_path = dir.path().join("claude-help.txt");
        std::fs::write(&help_path, help).unwrap();
        let script = crate::test_support::fake_ssh_script(
            "claude-recorded-help-launch",
            &format!("cat {}", shell_quote(&help_path.to_string_lossy())),
            &format!("type \"{}\"", help_path.display()),
        );
        let binary = script.to_string_lossy();
        for verb in [
            "auth",
            "doctor",
            "mcp",
            "plugin",
            "plugins",
            "update",
            "upgrade",
            "remote-control",
        ] {
            let args = vec![verb.into()];
            assert_eq!(
                augment_args("claude", &binary, &args, dir.path()),
                args,
                "{verb}"
            );
        }
        for args in [
            vec![],
            vec!["prompt".into()],
            vec!["fix-bug".into()],
            vec!["--resume".into(), "x".into()],
        ] {
            let mut expected = args.clone();
            expected.extend([
                "--settings".into(),
                dir.path()
                    .join("agent-hooks/claude.json")
                    .to_string_lossy()
                    .into_owned(),
            ]);
            assert_eq!(augment_args("claude", &binary, &args, dir.path()), expected);
        }
        let args = vec!["remote-control".into(), "--resume".into(), "x".into()];
        assert_eq!(augment_args("claude", &binary, &args, dir.path()), args);
    }

    #[cfg(unix)]
    #[test]
    fn screen_probe_is_skipped_when_its_answer_cannot_change_launch_args() {
        let script = crate::test_support::fake_ssh_script(
            "screen-lazy-probe",
            "printf x >> \"${0%/*}/screen-lazy-probe.invoked\"; printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        let marker = script.with_file_name("screen-lazy-probe.invoked");
        let _ = std::fs::remove_file(&marker);
        let binary = script.to_string_lossy();
        for (agent, args) in [
            ("codex", vec!["--no-alt-screen".into()]),
            ("codex", vec!["exec".into(), "echo".into()]),
        ] {
            let _ = augment_args(agent, &binary, &args, Path::new("/unused"));
            assert!(!marker.exists(), "{agent} {args:?} needlessly ran --help");
        }
    }

    #[cfg(unix)]
    #[test]
    fn terminal_launch_args_share_the_structured_spawn_screen_policy() {
        let script = crate::test_support::fake_ssh_script(
            "screen-terminal-launch-args",
            "printf '%s\\n' '--no-alt-screen --mini'",
            "echo --no-alt-screen --mini",
        );
        let binary = script.to_string_lossy();
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["resume".into()]),
            ["--no-alt-screen", "resume"]
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["exec".into()]),
            ["exec"]
        );
        assert_eq!(
            build_agent_launch_args("opencode", &binary, &["run".into()]),
            ["run"]
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary, &["--no-alt-screen".into()]),
            ["--no-alt-screen"]
        );
        assert_eq!(
            build_agent_launch_args("grok", &binary, &["--fullscreen".into()]),
            ["--no-alt-screen", "--fullscreen"]
        );
        assert_eq!(
            build_agent_launch_args("gemini", &binary, &[]),
            Vec::<String>::new()
        );

        let older = crate::test_support::fake_ssh_script(
            "screen-terminal-older-codex",
            "printf '%s\\n' 'old help'",
            "echo old help",
        );
        assert_eq!(
            build_agent_launch_args("codex", &older.to_string_lossy(), &["resume".into()]),
            ["resume"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn per_agent_screen_setting_disables_the_structured_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = tuic_core::config_dir::set_override(dir.path().to_path_buf());
        let mut config = crate::config::AgentsConfig::default();
        config.agents.insert(
            "codex".into(),
            crate::config::AgentSettings {
                prevent_alt_screen: Some(false),
                ..Default::default()
            },
        );
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
        let binary = crate::test_support::fake_ssh_script(
            "screen-setting-codex",
            "printf '%s\\n' '--no-alt-screen'",
            "echo --no-alt-screen",
        );
        assert_eq!(
            build_agent_launch_args("codex", &binary.to_string_lossy(), &["resume".into()]),
            ["resume"]
        );
    }

    #[test]
    fn actual_agent_argument_builder_defaults_to_native_scrollback() {
        for (agent, flag) in [
            ("codex", "--no-alt-screen"),
            ("grok", "--no-alt-screen"),
            ("opencode", "--mini"),
        ] {
            let script = crate::test_support::fake_ssh_script(
                &format!("screen-help-{agent}"),
                &format!("printf '%s\\n' '{flag}'"),
                &format!("echo {flag}"),
            );
            let binary = script.to_string_lossy();
            let args = vec!["resume".to_string()];
            let built = augment_args(agent, &binary, &args, Path::new("/unused"));
            assert_eq!(built.first().map(String::as_str), Some(flag), "{agent}");
            assert_eq!(built.get(1).map(String::as_str), Some("resume"), "{agent}");
        }
    }

    #[test]
    fn screen_flags_are_placed_before_subcommands_only_when_supported() {
        let args = vec!["resume".to_string(), "abc".to_string()];
        assert_eq!(
            add_screen_flag("codex", &args, args.clone(), true),
            ["--no-alt-screen", "resume", "abc"]
        );
        assert_eq!(add_screen_flag("codex", &args, args.clone(), false), args);
        let args = vec!["--model".to_string(), "fast".to_string()];
        assert_eq!(
            add_screen_flag("grok", &args, args.clone(), true),
            ["--no-alt-screen", "--model", "fast"]
        );
        assert_eq!(
            add_screen_flag("opencode", &args, args.clone(), true),
            ["--mini", "--model", "fast"]
        );
    }

    #[test]
    fn screen_flags_respect_existing_flags_and_subcommands() {
        let args = vec!["--no-alt-screen".to_string()];
        assert_eq!(add_screen_flag("codex", &args, args.clone(), true), args);
        let args = vec!["run".to_string(), "task".to_string()];
        assert_eq!(add_screen_flag("opencode", &args, args.clone(), true), args);
    }

    /// Gated with its only caller, which is unix-only because the hooks it
    /// builds are POSIX shell. Without the gate this is dead code on Windows,
    /// and the Windows job warns about it where nothing reads the warning.
    #[cfg(unix)]
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
            augment_args_when(true, "claude", &["--bare".into()], root),
            vec!["--bare"]
        );
        assert_eq!(
            augment_args_when(true, "claude", &["--settings=x".into()], root),
            vec!["--settings=x"]
        );
        assert_eq!(
            augment_args_when(
                true,
                "codex",
                &["-c".into(), "notify=['mine']".into()],
                root
            ),
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
        let pty_tty = codex.find("TUIC_PTY_TTY").expect("reads TUIC_PTY_TTY");
        let ps = codex.find("ps -o tty=").expect("keeps the ps fallback");
        assert!(pty_tty < ps, "TUIC_PTY_TTY is tried before ps: {codex}");
    }

    /// The notify must reach the PTY TUIC stamped without exec'ing `ps`:
    /// `PATH` is emptied, so any `ps`/`tr` call would fail. Red before the
    /// `$TUIC_PTY_TTY` lookup (the tty fell back to `/dev/tty`).
    #[cfg(unix)]
    #[test]
    fn codex_notify_writes_to_tuic_pty_tty_without_ps() {
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("notify.sh");
        std::fs::write(&script, codex_script(&[])).unwrap();
        let tty = dir.path().join("fake-tty");
        std::fs::write(&tty, "").unwrap();
        let status = std::process::Command::new("/bin/sh")
            .arg(&script)
            .arg(r#"{"type":"agent-turn-complete"}"#)
            .env_clear()
            .env("PATH", dir.path().join("no-such-dir"))
            .env("TUIC_PTY_TTY", &tty)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read(&tty).unwrap(), b"\x1b]7770;state=idle\x1b\\");
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
    /// shell script that resolves the controlling tty (`$TUIC_PTY_TTY`, else `ps -o tty=`). The
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
        // TUIC's entry is the submit-prompt hook from `claude_hook_map`, whose
        // `state=prompt` wire output is pinned against the real binary by
        // `agent_hook::tests::golden_wire_output::every_spec_emits_exactly_its_wire_contract`.
        let (_, _, submit) = claude_hook_map()
            .into_iter()
            .find(|(event, _, _)| *event == "UserPromptSubmit")
            .expect("claude map has a UserPromptSubmit entry");
        assert_eq!(commands[2], submit);

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

#[cfg(test)]
mod critic_1302_tests {
    use super::*;

    /// Catches: two boots on one config dir (desktop + `tuic-remote`) tear
    /// `claude.json`, so a `claude --settings` spawn reads half a document.
    #[test]
    fn concurrent_boots_never_expose_a_torn_claude_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("agent-hooks/claude.json");
        regenerate_launch_assets_at_boot(dir.path());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let root = dir.path().to_path_buf();
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        regenerate_launch_assets_at_boot(&root);
                    }
                })
            })
            .collect();
        for _ in 0..300 {
            let bytes = std::fs::read(&path).expect("claude.json always present");
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("never torn");
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for writer in writers {
            writer.join().unwrap();
        }
    }

    /// Catches: `codex-notify.sh` is renamed into place as 0600 and chmod'ed
    /// afterwards, so a codex spawn racing a second boot finds it not executable.
    #[cfg(unix)]
    #[test]
    fn codex_notify_script_is_never_visible_without_the_execute_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("agent-hooks/codex-notify.sh");
        regenerate_launch_assets_at_boot(dir.path());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let root = dir.path().to_path_buf();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    regenerate_launch_assets_at_boot(&root);
                }
            })
        };
        let mut bad = 0;
        for _ in 0..2000 {
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            if mode & 0o100 == 0 {
                bad += 1;
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
        assert_eq!(bad, 0, "observed the script without its execute bit {bad}x");
    }
}
