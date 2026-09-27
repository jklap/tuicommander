//! Detached command runner for managed CLI callers.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::ipc;

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
    println!("tuic bg: pid={} log={log}", child.id());
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
    if let Err(e) = queue_wake(caller, &wake) {
        let _ = writeln!(output, "tuic bg: wake failed: {e}");
    }
    Ok(())
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
