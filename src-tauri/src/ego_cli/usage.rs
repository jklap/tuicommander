//! Terminal usage from ego's versioned local report, bound by the live pid lease.
//! ACP usage belongs to AI Chat, not a terminal. The transcript carries cost
//! deltas, not corrected totals, so it cannot substitute for this report.

use serde::Serialize;
use serde_json::Value;

use crate::AppState;

#[derive(Debug, Serialize)]
pub(crate) struct EgoUsage {
    pub text: Option<String>,
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
    for (amounts, unknown, label) in [
        ("known", "unknown", ""),
        (
            "subscription_equivalent",
            "subscription_unknown",
            "API eq: ",
        ),
    ] {
        if !cost[unknown].as_array().is_some_and(Vec::is_empty) {
            continue;
        }
        let Some(known) = cost[amounts].as_array() else {
            continue;
        };
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
    let Some(source) = crate::session_usage::ego_terminal_source(state, terminal).await? else {
        return Ok(EgoUsage { text: None });
    };
    let report = source.read(state, terminal).await?;
    project(&report, &source.ego_session)
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
