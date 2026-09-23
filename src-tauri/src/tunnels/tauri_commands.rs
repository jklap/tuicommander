use std::sync::Arc;

use serde::Serialize;

use super::profile::TunnelProfile;
use super::storage::ProfileStore;
use crate::AppState;

#[tauri::command]
pub(crate) fn list_tunnel_profiles(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<TunnelProfile>, String> {
    ProfileStore::load_all(&state.data_dir, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn save_tunnel_profile(
    state: tauri::State<'_, Arc<AppState>>,
    mut profile: serde_json::Value,
) -> Result<String, String> {
    if profile
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .is_empty()
    {
        profile["id"] = serde_json::Value::String(uuid::Uuid::new_v4().to_string());
    }
    let mut profile: TunnelProfile = serde_json::from_value(profile).map_err(|e| e.to_string())?;
    profile.validate()?;
    let id = profile.id.clone();
    ProfileStore::save(&state.data_dir, &profile).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub(crate) fn delete_tunnel_profile(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<bool, String> {
    state.tunnel_manager.stop_if_running(&id);
    ProfileStore::delete(&state.data_dir, None, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn start_tunnel(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<String, String> {
    let profiles = ProfileStore::load_all(&state.data_dir, None).map_err(|e| e.to_string())?;
    let profile = profiles
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "profile not found".to_string())?;
    state.tunnel_manager.start(profile).await
}

#[tauri::command]
pub(crate) fn stop_tunnel(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    state.tunnel_manager.stop(&id)
}

#[tauri::command]
pub(crate) fn list_active_tunnels(
    state: tauri::State<'_, Arc<AppState>>,
) -> Vec<serde_json::Value> {
    state
        .tunnel_manager
        .list()
        .into_iter()
        .map(|(id, status)| {
            serde_json::json!({
                "id": id,
                "status": status_to_frontend(&status),
                "started_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) fn get_tunnel_status(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<serde_json::Value, String> {
    state
        .tunnel_manager
        .get_status(&id)
        .map(|status| {
            serde_json::json!({
                "id": id,
                "status": status_to_frontend(&status),
                "started_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .ok_or_else(|| "tunnel not found".to_string())
}

#[tauri::command]
pub(crate) fn list_ssh_config_hosts() -> Vec<String> {
    let config_path = match dirs::home_dir() {
        Some(h) => h.join(".ssh").join("config"),
        None => return Vec::new(),
    };

    let file = match std::fs::File::open(&config_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut reader = std::io::BufReader::new(file);
    let config = match ssh2_config::SshConfig::default()
        .parse(&mut reader, ssh2_config::ParseRule::ALLOW_UNKNOWN_FIELDS)
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    config
        .get_hosts()
        .iter()
        .flat_map(|host| {
            host.pattern.iter().filter_map(|clause| {
                if clause.negated || clause.pattern == "*" {
                    None
                } else {
                    Some(clause.pattern.clone())
                }
            })
        })
        .collect()
}

#[tauri::command]
pub(crate) fn get_tunnel_audit(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(20);
    let events = state
        .tunnel_audit
        .lock()
        .query_by_tunnel(&id, limit)
        .map_err(|e| e.to_string())?;

    Ok(events
        .into_iter()
        .map(|e| {
            let message = extract_audit_message(&e.detail);
            serde_json::json!({
                "tunnel_id": e.tunnel_id,
                "timestamp": e.timestamp.to_rfc3339(),
                "kind": e.kind,
                "message": message,
            })
        })
        .collect())
}

fn extract_audit_message(detail: &serde_json::Value) -> Option<String> {
    // Try structured fields first
    if let Some(msg) = detail.get("message").and_then(|v| v.as_str()) {
        return Some(msg.to_string());
    }
    if let Some(msg) = detail.get("reason").and_then(|v| v.as_str()) {
        return Some(msg.to_string());
    }
    // The status callback stores Debug repr: {"status": "Error { message: \"...\" }"}
    if let Some(status_str) = detail.get("status").and_then(|v| v.as_str()) {
        if let Some(start) = status_str.find("message: \"") {
            let rest = &status_str[start + 10..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        if let Some(start) = status_str.find("reason: \"") {
            let rest = &status_str[start + 9..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        return Some(status_str.to_string());
    }
    // Fallback: stringify non-empty detail
    if !detail.is_null() && detail != &serde_json::json!({}) {
        return Some(detail.to_string());
    }
    None
}

fn status_to_frontend(status: &super::supervisor::TunnelStatus) -> serde_json::Value {
    use super::supervisor::TunnelStatus;
    match status {
        TunnelStatus::Starting => serde_json::json!({"type": "starting"}),
        TunnelStatus::Connected => serde_json::json!({"type": "connected"}),
        TunnelStatus::Reconnecting { attempt, reason } => {
            serde_json::json!({"type": "reconnecting", "attempt": attempt, "reason": reason})
        }
        TunnelStatus::Stopped { reason } => {
            serde_json::json!({"type": "stopped", "reason": reason})
        }
        TunnelStatus::Error { message } => {
            serde_json::json!({"type": "error", "message": message})
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentKey {
    pub fingerprint: String,
    pub comment: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SshAgentInfo {
    pub keys: Vec<AgentKey>,
    pub agent_type: String,
}

fn detect_agent_type() -> String {
    let sock = std::env::var("SSH_AUTH_SOCK").unwrap_or_default();

    if sock.contains("1password") || sock.contains("2BUA8C4S2C") {
        return "1Password".to_string();
    }
    if sock.contains("secretive") {
        return "Secretive".to_string();
    }
    if sock.contains("gpg") || sock.contains("gnupg") {
        return "GPG Agent".to_string();
    }

    // 1Password socket exists but isn't the active SSH_AUTH_SOCK
    if let Some(home) = dirs::home_dir() {
        let op_sock = home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
        if op_sock.exists() {
            return "SSH Agent (1Password available)".to_string();
        }
    }

    if sock.is_empty() {
        "Not available".to_string()
    } else {
        "SSH Agent".to_string()
    }
}

#[tauri::command]
pub(crate) async fn list_ssh_agent_keys() -> Result<SshAgentInfo, String> {
    let agent_type = detect_agent_type();

    let output = tokio::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .await
        .map_err(|e| format!("failed to run ssh-add: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no identities") || output.status.code() == Some(1) {
            return Ok(SshAgentInfo {
                keys: Vec::new(),
                agent_type,
            });
        }
        return Err(format!("ssh-add failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let keys = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() >= 3 {
                Some(AgentKey {
                    fingerprint: parts[1].to_string(),
                    comment: parts[2].to_string(),
                    key_type: parts
                        .get(3)
                        .map(|s| s.trim_matches(|c| c == '(' || c == ')').to_string())
                        .unwrap_or_default(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(SshAgentInfo { keys, agent_type })
}

// ── Tests ───────────────────────────────────────────────────
//
// These `#[tauri::command]` fns take `tauri::State<'_, Arc<AppState>>`
// directly with no plain-`&AppState` `_impl` twin to call instead (unlike
// e.g. `pty.rs`'s `get_session_foreground_process`/`list_active_sessions`).
// `tauri::State<'r, T>` wraps a private `&'r T` with no public constructor
// outside a running Tauri app (confirmed by reading `tauri::state::State`'s
// definition — a tuple struct with a private field, only ever built by
// `StateManager::get`), and no test anywhere in this codebase constructs one
// (see `pty.rs`'s doc comment on `list_active_sessions_impl`, which
// documents this exact gap for a different command). Extracting a testable
// `_impl` twin is a production-code change, out of scope for this test-only
// pass — so these tests cover the free functions that ARE directly callable
// (`status_to_frontend`, `extract_audit_message`, `detect_agent_type`), which
// is as close to full command coverage as this pass can safely get. The
// three IPC-side halves of the known parity bugs (see plan Phase 1) are
// documented in `commands.rs`'s test module instead, next to their HTTP-side
// pinning tests, citing the exact source lines that diverge.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunnels::supervisor::TunnelStatus;

    // ── status_to_frontend ──────────────────────────────────

    #[test]
    fn status_to_frontend_starting() {
        assert_eq!(
            status_to_frontend(&TunnelStatus::Starting),
            serde_json::json!({"type": "starting"})
        );
    }

    #[test]
    fn status_to_frontend_connected() {
        assert_eq!(
            status_to_frontend(&TunnelStatus::Connected),
            serde_json::json!({"type": "connected"})
        );
    }

    #[test]
    fn status_to_frontend_reconnecting_carries_attempt_and_reason() {
        let status = TunnelStatus::Reconnecting {
            attempt: 3,
            reason: "ConnectionRefused".to_string(),
        };
        assert_eq!(
            status_to_frontend(&status),
            serde_json::json!({"type": "reconnecting", "attempt": 3, "reason": "ConnectionRefused"})
        );
    }

    #[test]
    fn status_to_frontend_stopped_carries_reason() {
        let status = TunnelStatus::Stopped {
            reason: "max retries exceeded".to_string(),
        };
        assert_eq!(
            status_to_frontend(&status),
            serde_json::json!({"type": "stopped", "reason": "max retries exceeded"})
        );
    }

    #[test]
    fn status_to_frontend_error_carries_message() {
        let status = TunnelStatus::Error {
            message: "boom".to_string(),
        };
        assert_eq!(
            status_to_frontend(&status),
            serde_json::json!({"type": "error", "message": "boom"})
        );
    }

    // ── extract_audit_message ───────────────────────────────

    #[test]
    fn extract_audit_message_prefers_a_structured_message_field() {
        let detail = serde_json::json!({"message": "explicit message"});
        assert_eq!(
            extract_audit_message(&detail),
            Some("explicit message".to_string())
        );
    }

    #[test]
    fn extract_audit_message_falls_back_to_a_structured_reason_field() {
        let detail = serde_json::json!({"reason": "explicit reason"});
        assert_eq!(
            extract_audit_message(&detail),
            Some("explicit reason".to_string())
        );
    }

    #[test]
    fn extract_audit_message_message_field_wins_over_reason_field() {
        let detail = serde_json::json!({"message": "m", "reason": "r"});
        assert_eq!(extract_audit_message(&detail), Some("m".to_string()));
    }

    #[test]
    fn extract_audit_message_parses_message_out_of_a_debug_repr_status() {
        // The status callback stores the Debug repr of an Error/Stopped status
        // under a "status" key, e.g. {"status": "Error { message: \"...\" }"}.
        let detail = serde_json::json!({
            "status": "Error { message: \"connection refused\" }"
        });
        assert_eq!(
            extract_audit_message(&detail),
            Some("connection refused".to_string())
        );
    }

    #[test]
    fn extract_audit_message_parses_reason_out_of_a_debug_repr_status() {
        let detail = serde_json::json!({
            "status": "Stopped { reason: \"AuthFailed\" }"
        });
        assert_eq!(
            extract_audit_message(&detail),
            Some("AuthFailed".to_string())
        );
    }

    #[test]
    fn extract_audit_message_falls_back_to_the_raw_status_string() {
        // A status Debug repr with neither "message: " nor "reason: " inside
        // it (e.g. a unit variant) is returned as-is.
        let detail = serde_json::json!({"status": "Connected"});
        assert_eq!(
            extract_audit_message(&detail),
            Some("Connected".to_string())
        );
    }

    #[test]
    fn extract_audit_message_stringifies_an_unrecognized_nonempty_detail() {
        let detail = serde_json::json!({"other": 1});
        assert_eq!(extract_audit_message(&detail), Some(detail.to_string()));
    }

    #[test]
    fn extract_audit_message_returns_none_for_null() {
        assert_eq!(extract_audit_message(&serde_json::Value::Null), None);
    }

    #[test]
    fn extract_audit_message_returns_none_for_an_empty_object() {
        assert_eq!(extract_audit_message(&serde_json::json!({})), None);
    }

    // ── detect_agent_type ───────────────────────────────────
    //
    // `SSH_AUTH_SOCK` is process-global env state — serialize with the same
    // `#[serial_test::serial]` + `unsafe { std::env::set_var }`/restore
    // pattern already used for `resolve_github_token`/
    // `finding_confidence_threshold` in this codebase. Any sock value that
    // doesn't match one of the `contains(...)` branches (including empty)
    // falls through to an unconditional, un-mockable filesystem check for a
    // real 1Password socket file — confirmed present on this machine, which
    // is why the fallback test below accepts either of its two valid
    // outputs rather than asserting one specific string.
    #[test]
    #[serial_test::serial]
    fn detect_agent_type_recognizes_1password_socket_path() {
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/com.1password.agent.sock");
        assert_eq!(detect_agent_type(), "1Password");
    }

    #[test]
    #[serial_test::serial]
    fn detect_agent_type_recognizes_1password_legacy_socket_marker() {
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/2BUA8C4S2C.ssh.sock");
        assert_eq!(detect_agent_type(), "1Password");
    }

    #[test]
    #[serial_test::serial]
    fn detect_agent_type_recognizes_secretive_socket_path() {
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/secretive.sock");
        assert_eq!(detect_agent_type(), "Secretive");
    }

    #[test]
    #[serial_test::serial]
    fn detect_agent_type_recognizes_gpg_agent_socket_path() {
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/gpg-agent.ssh");
        assert_eq!(detect_agent_type(), "GPG Agent");
    }

    #[test]
    #[serial_test::serial]
    fn detect_agent_type_recognizes_gnupg_socket_path() {
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/gnupg/S.gpg-agent.ssh");
        assert_eq!(detect_agent_type(), "GPG Agent");
    }

    #[test]
    #[serial_test::serial]
    fn detect_agent_type_falls_back_to_generic_ssh_agent_for_an_unrecognized_socket() {
        // A sock value that doesn't match any of the content checks still
        // passes through the unconditional "is the real 1Password socket
        // file present on disk" check below them before reaching the final
        // is_empty()/non-empty fallback — on a machine where 1Password is
        // actually installed (confirmed: this one), that check wins and
        // returns "SSH Agent (1Password available)" instead of "SSH Agent".
        // Both are the function's real, intended outputs for this branch;
        // only the on-disk fixture (which this test cannot control without
        // changing production code) decides which one a given run observes.
        let _guard = EnvVarGuard::set("SSH_AUTH_SOCK", "/tmp/some-other-agent.sock");
        let result = detect_agent_type();
        assert!(
            result == "SSH Agent" || result == "SSH Agent (1Password available)",
            "unexpected agent type: {result}"
        );
    }

    /// RAII guard that sets an env var and restores its previous value (or
    /// removes it if it was unset) on drop — mirrors the manual
    /// set/restore-at-end-of-test pattern used elsewhere in this codebase,
    /// but panic-safe (an assertion failure mid-test still restores).
    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: serialized by `#[serial_test::serial]` on every test
            // that touches this env var, so no other thread reads/writes it
            // concurrently.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            // SAFETY: see `set` above.
            unsafe {
                match &self.previous {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }
}
