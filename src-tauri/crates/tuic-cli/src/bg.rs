//! Detached command runner for managed CLI callers.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{ipc, mcp};

const WAKE_RETRY_BUDGET: Duration = Duration::from_secs(5 * 60);

fn marker_path(caller: &str) -> Result<std::path::PathBuf, String> {
    if caller.is_empty()
        || caller.len() > 128
        || !caller
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("Invalid TUIC_SESSION for background wake marker".into());
    }
    let directory = std::env::var_os("TUIC_BG_WAKE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut config = ipc::config_dir();
            if let Ok(id) = std::env::var("TUIC_APP_INSTANCE")
                && !id.is_empty()
                && id != "default"
                && id.len() <= 63
                && id
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && id
                    .bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                config = config.join("instances").join(id);
            }
            config.join("bg-wakes")
        });
    Ok(directory.join(format!("{caller}.json")))
}

fn write_marker(caller: &str, status: &Value) -> Result<(), String> {
    let path = marker_path(caller)?;
    fs::create_dir_all(path.parent().expect("marker has parent")).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, format!("{status}\n"))
        .and_then(|()| fs::rename(&temp, &path))
        .map_err(|e| {
            format!(
                "Cannot write background wake marker {}: {e}",
                path.display()
            )
        })
}

fn write_wake_file(log: &str, status: &Value) -> Result<(), String> {
    let path = format!("{log}.wake");
    let temp = format!("{path}.tmp");
    fs::write(&temp, format!("{status}\n"))
        .and_then(|()| fs::rename(&temp, &path))
        .map_err(|e| format!("Cannot write wake status file {path}: {e}"))
}

fn transient_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if [
        "resource temporarily unavailable",
        "connection refused",
        "no such file or directory",
        "timed out",
        "would block",
        "os error 11",
        "os error 35",
        "os error 10035",
        "os error 10061",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return true;
    }
    lower
        .split("http ")
        .nth(1)
        .and_then(|suffix| suffix.get(..3))
        .and_then(|status| status.parse::<u16>().ok())
        .is_some_and(|status| status == 408 || status == 429 || status >= 500)
}

fn retry_delay(attempts: u32, remaining: Duration) -> Duration {
    let seconds = 1u64
        .checked_shl(attempts.saturating_sub(1).min(5))
        .unwrap_or(30)
        .min(30);
    Duration::from_secs(seconds).min(remaining)
}

fn retry_after(attempts: u32, elapsed: Duration, queue_error: &str) -> Option<Duration> {
    let remaining = WAKE_RETRY_BUDGET.saturating_sub(elapsed);
    (!remaining.is_zero() && transient_error(queue_error)).then(|| retry_delay(attempts, remaining))
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
    write_marker(
        &caller,
        &serde_json::json!({"session_id": caller, "status": "retrying", "attempts": 0}),
    )?;

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
    let child = runner.spawn().map_err(|e| {
        let message = format!("Cannot start background runner: {e}");
        let _ = write_marker(&caller, &serde_json::json!({"session_id": caller, "status": "failed", "attempts": 0, "error": message}));
        message
    })?;
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
    let started = Instant::now();
    let mut attempts = 0u32;
    let wake_status = loop {
        attempts += 1;
        match queue_wake(caller, &wake) {
            Ok(()) => {
                break serde_json::json!({"session_id": caller, "status": "queued", "attempts": attempts});
            }
            Err(queue_error) => {
                let _ = writeln!(output, "tuic bg: queue wake failed: {queue_error}");
                let message = format!("{wake}\ntuic bg: queue wake failed: {queue_error}");
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
                        break serde_json::json!({"session_id": caller, "status": "mailed", "attempts": attempts, "queue_error": queue_error});
                    }
                    Err(mail_error) => {
                        let _ = writeln!(output, "tuic bg: mail wake failed: {mail_error}");
                        let reason = format!("queue: {queue_error}; mail: {mail_error}");
                        if let Some(delay) = retry_after(attempts, started.elapsed(), &queue_error)
                        {
                            write_marker(
                                caller,
                                &serde_json::json!({"session_id": caller, "status": "retrying", "attempts": attempts, "error": reason}),
                            )?;
                            std::thread::sleep(delay);
                            continue;
                        }
                        break serde_json::json!({"session_id": caller, "status": "failed", "attempts": attempts, "error": reason});
                    }
                }
            }
        }
    };
    write_marker(caller, &wake_status)?;
    write_wake_file(log, &wake_status).map_err(|message| {
        let _ = writeln!(output, "tuic bg: {message}");
        message
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_wakes_retry_only_inside_the_five_minute_budget() {
        assert_eq!(
            retry_after(
                1,
                Duration::ZERO,
                "Resource temporarily unavailable (os error 35)"
            ),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            retry_after(9, Duration::from_secs(299), "Connection refused"),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            retry_after(9, Duration::from_secs(300), "Connection refused"),
            None
        );
        assert_eq!(
            retry_after(1, Duration::ZERO, "Queue answered HTTP 400"),
            None
        );
    }
}

fn queue_wake(caller: &str, wake: &str) -> Result<(), String> {
    let sessions = ipc::get("/sessions").map_err(|e| e.to_string())?;
    if !sessions.is_success() {
        return Err(format!("Session lookup answered HTTP {}", sessions.status));
    }
    let rows: Value = sessions.json().map_err(|e| e.to_string())?;
    let matches: Vec<&str> = rows
        .as_array()
        .ok_or("Session lookup did not return a list")?
        .iter()
        .filter(|row| row["tuic_session"].as_str() == Some(caller))
        .filter_map(|row| row["session_id"].as_str())
        .collect();
    let [session_id] = matches.as_slice() else {
        return Err(format!(
            "Expected one live session for {caller}, found {}",
            matches.len()
        ));
    };
    let body = serde_json::json!({"text": wake}).to_string();
    let response =
        ipc::post(&format!("/sessions/{session_id}/queue"), &body).map_err(|e| e.to_string())?;
    if !response.is_success() {
        return Err(format!(
            "Queue answered HTTP {}: {}",
            response.status, response.body
        ));
    }
    let receipt: Value = response.json().map_err(|e| e.to_string())?;
    if receipt["typed"] != true && receipt["queued"].as_u64().is_none_or(|count| count == 0) {
        return Err(format!("Queue did not accept wake: {}", response.body));
    }
    Ok(())
}
