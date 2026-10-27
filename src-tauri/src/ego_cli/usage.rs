//! Terminal usage from ego's versioned local report, bound by the live pid lease.
//! ACP usage belongs to AI Chat, not a terminal. The transcript carries cost
//! deltas, not corrected totals, so it cannot substitute for this report.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::AppState;

#[derive(Debug, Serialize)]
pub(crate) struct EgoUsage {
    pub text: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    workspace_root: PathBuf,
}

fn binding(state: &AppState, terminal: &str) -> Option<(PathBuf, String)> {
    if state
        .session_maps
        .session_states
        .get(terminal)?
        .agent_type
        .as_deref()
        != Some("ego")
    {
        return None;
    }
    let pid = crate::pty::session_leaf_pid(state, terminal)?;
    crate::agent_session::ego_chat_session_for_pid(pid)
}

/// Preserve exact decimal money supplied by ego. A missing price, subscription
/// equivalent or partial total must never be presented as zero monetary spend.
fn project(report: &Value, session: &str) -> Result<EgoUsage, String> {
    if report["v"] != 3 {
        return Err("ego usage requires report version 3".into());
    }
    let row = report["sessions"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| row["session"].as_str() == Some(session))
    });
    let Some(row) = row else {
        return Ok(EgoUsage { text: None });
    };
    let mut parts = Vec::new();
    if let Some(used) = row["context"]["used_tokens"].as_u64() {
        parts.push(format!("ctx: {used}"));
    }
    let cost = &row["lines"]["total"]["cost"];
    let complete = cost["unknown"].as_array().is_some_and(Vec::is_empty)
        && cost["subscription_equivalent"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && cost["subscription_unknown"]
            .as_array()
            .is_some_and(Vec::is_empty);
    let equivalent_complete = cost["subscription_unknown"]
        .as_array()
        .is_some_and(Vec::is_empty);
    let figures = if complete {
        cost["known"].as_array().map(|known| (known, ""))
    } else if equivalent_complete {
        cost["subscription_equivalent"]
            .as_array()
            .map(|known| (known, "API eq: "))
    } else {
        None
    };
    if let Some((known, label)) = figures {
        for money in known {
            if let (Some(amount), Some(currency)) =
                (money["amount"].as_str(), money["currency"].as_str())
                && !amount.is_empty()
                && amount.chars().all(|c| c.is_ascii_digit() || c == '.')
                && amount
                    .parse::<f64>()
                    .is_ok_and(|n| n.is_finite() && n >= 0.0)
                && currency.len() == 3
                && currency.chars().all(|c| c.is_ascii_uppercase())
            {
                parts.push(format!("{label}{currency} {amount}"));
            }
        }
    }
    // DEFERRED (2026-10-27) — ego usage v3 has no provider quota balance or
    // context ceiling. Do not borrow Claude/Codex credentials or invent limits.
    Ok(EgoUsage {
        text: (!parts.is_empty()).then(|| parts.join(" · ")),
    })
}

pub(crate) async fn read(state: &AppState, terminal: &str) -> Result<EgoUsage, String> {
    let Some((home, session)) = binding(state, terminal) else {
        return Ok(EgoUsage { text: None });
    };
    let program = PathBuf::from(&state.config.read().ego_executable);
    if !program.is_absolute() || !program.is_file() {
        return Err("configure an absolute ego executable in Settings".into());
    }
    let manifest = tokio::fs::read(home.join("sessions").join(&session).join("manifest.json"))
        .await
        .map_err(|e| format!("cannot read bound ego manifest: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest).map_err(|e| e.to_string())?;
    let mut command = tokio::process::Command::new(program);
    command
        .arg("usage")
        .arg("--json")
        .arg("--session-dir")
        .arg(home.join("sessions"))
        .arg("-C")
        .arg(&manifest.workspace_root)
        .env("EGO_HOME", &home)
        .env_remove("EGO_PROFILE")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| "ego usage timed out after thirty seconds".to_string())?
        .map_err(|e| format!("could not run ego usage: {e}"))?;
    if !output.status.success() {
        return Err(format!("ego usage failed ({})", output.status));
    }
    // A compaction/resume/exit during the read invalidates this report.
    if binding(state, terminal) != Some((home, session.clone())) {
        return Ok(EgoUsage { text: None });
    }
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    project(&report, &session)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_ego_usage(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    session_id: String,
) -> Result<EgoUsage, String> {
    read(&state, &session_id).await
}

#[cfg(test)]
mod tests;
