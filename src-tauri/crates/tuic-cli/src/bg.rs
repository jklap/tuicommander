//! Detached command runner for managed CLI callers.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::{ipc, mcp};

const WAKE_ATTEMPTS: u64 = 6;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(200);

struct WakeError {
    message: String,
    retryable: bool,
}

impl WakeError {
    fn permanent(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }

    fn from_io(error: std::io::Error) -> Self {
        let retryable = matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::NotFound
        );
        Self {
            message: error.to_string(),
            retryable,
        }
    }

    fn from_http(operation: &str, status: u16, body: &str) -> Self {
        Self {
            message: format!("{operation} answered HTTP {status}: {body}"),
            retryable: status >= 500,
        }
    }
}

fn write_wake_status(path: &str, status: &Value) -> Result<(), String> {
    let temp = format!("{path}.tmp");
    fs::write(&temp, format!("{status}\n"))
        .and_then(|()| fs::rename(&temp, path))
        .map_err(|error| format!("Cannot write wake status file {path}: {error}"))
}

pub fn launch(log: &str, command: &[String]) -> Result<(), String> {
    let caller = std::env::var("TUIC_SESSION").unwrap_or_default();
    if caller.is_empty() {
        eprintln!("tuic: bg requires TUIC_SESSION; nobody would be woken");
        std::process::exit(2);
    }
    if command.is_empty() {
        eprintln!("tuic: bg requires a command after --");
        std::process::exit(2);
    }
    if let Some(parent) = Path::new(log)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Cannot create background log directory {}: {e}",
                parent.display()
            )
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("Cannot open background log {log}: {e}"))?;
    let exit_file = format!("{log}.exit");
    if let Err(e) = fs::remove_file(&exit_file)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("Cannot remove stale exit file {exit_file}: {e}"));
    }
    let wake_file = format!("{log}.wake");
    if let Err(e) = fs::remove_file(&wake_file)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("Cannot remove stale wake file {wake_file}: {e}"));
    }

    let mut runner = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    runner
        .arg("__bg-runner")
        .arg(log)
        .arg(&caller)
        .arg("--")
        .args(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        runner.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        runner.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let child = runner
        .spawn()
        .map_err(|e| format!("Cannot start background runner: {e}"))?;
    println!("tuic bg: pid={} log={log} wake={wake_file}", child.id());
    Ok(())
}

pub fn run(log: &str, caller: &str, command: &[String]) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("Cannot open background log {log}: {e}"))?;
    let stderr = output.try_clone().map_err(|e| e.to_string())?;
    let code = match Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(stderr))
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            let _ = writeln!(output, "tuic bg: command failed: {e}");
            127
        }
    };
    let exit_file = format!("{log}.exit");
    fs::write(&exit_file, format!("{code}\n"))
        .map_err(|e| format!("Cannot write exit file {exit_file}: {e}"))?;
    let wake = format!("BG DONE exit={code} log={log} cmd={}", command.join(" "));
    let wake_file = format!("{log}.wake");
    for attempt in 1..=WAKE_ATTEMPTS {
        let wake_status = match queue_wake(caller, &wake) {
            Ok(()) => {
                serde_json::json!({"status": "queued", "tuic_session": caller, "attempts": attempt})
            }
            Err(queue_error) => {
                let _ = writeln!(
                    output,
                    "tuic bg: queue wake attempt {attempt} failed: {}",
                    queue_error.message
                );
                let message = format!(
                    "{wake}\ntuic bg: queue wake failed: {}",
                    queue_error.message
                );
                let mail_result = mcp::McpClient::connect()
                    .and_then(|client| mcp::agent_send(&client, caller, &message))
                    .and_then(|report| {
                        if report["delivered"] == true {
                            Ok(())
                        } else {
                            Err(format!(
                                "Mail stayed inbox-only ({})",
                                report["delivery_path"]
                            ))
                        }
                    });
                match mail_result {
                    Ok(()) => {
                        serde_json::json!({"status": "mailed", "tuic_session": caller, "attempts": attempt, "queue_error": queue_error.message})
                    }
                    Err(mail_error) => {
                        let _ = writeln!(
                            output,
                            "tuic bg: mail wake attempt {attempt} failed: {mail_error}"
                        );
                        let error = format!("queue: {}; mail: {mail_error}", queue_error.message);
                        if queue_error.retryable && attempt < WAKE_ATTEMPTS {
                            let status = serde_json::json!({"status": "retrying", "tuic_session": caller, "attempts": attempt, "error": error});
                            write_wake_status(&wake_file, &status)?;
                            std::thread::sleep(INITIAL_RETRY_DELAY * (1 << (attempt - 1)));
                            continue;
                        }
                        serde_json::json!({"status": "failed", "tuic_session": caller, "attempts": attempt, "error": error})
                    }
                }
            }
        };
        write_wake_status(&wake_file, &wake_status).map_err(|message| {
            let _ = writeln!(output, "tuic bg: {message}");
            message
        })?;
        break;
    }
    Ok(())
}

fn queue_wake(caller: &str, wake: &str) -> Result<(), WakeError> {
    let sessions = ipc::get("/sessions").map_err(WakeError::from_io)?;
    if !sessions.is_success() {
        return Err(WakeError::from_http(
            "Session lookup",
            sessions.status,
            &sessions.body,
        ));
    }
    let rows: Value = sessions
        .json()
        .map_err(|e| WakeError::permanent(e.to_string()))?;
    let matches: Vec<&str> = rows
        .as_array()
        .ok_or_else(|| WakeError::permanent("Session lookup did not return a list"))?
        .iter()
        .filter(|row| row["tuic_session"].as_str() == Some(caller))
        .filter_map(|row| row["session_id"].as_str())
        .collect();
    let [session_id] = matches.as_slice() else {
        return Err(WakeError::permanent(format!(
            "Expected one live session for {caller}, found {}",
            matches.len()
        )));
    };
    let body = serde_json::json!({"text": wake}).to_string();
    let response =
        ipc::post(&format!("/sessions/{session_id}/queue"), &body).map_err(WakeError::from_io)?;
    if !response.is_success() {
        return Err(WakeError::from_http(
            "Queue",
            response.status,
            &response.body,
        ));
    }
    let receipt: Value = response
        .json()
        .map_err(|e| WakeError::permanent(e.to_string()))?;
    if receipt["typed"] != true && receipt["queued"].as_u64().is_none_or(|count| count == 0) {
        return Err(WakeError::permanent(format!(
            "Queue did not accept wake: {}",
            response.body
        )));
    }
    Ok(())
}
