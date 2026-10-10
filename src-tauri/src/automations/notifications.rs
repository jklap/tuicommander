//! Live-only delivery, claimed durably before any external effect.
use super::{
    run::{AutomationRun, DeliveryStatus, RunStatus},
    store::RunStore,
};
use crate::state::AppState;
use std::sync::Arc;

pub(crate) fn transition(run: &AutomationRun) -> Option<&'static str> {
    match run.status {
        RunStatus::NeedsYou => Some("needs_you"),
        RunStatus::Failed => Some("failed"),
        _ => None,
    }
}

pub(crate) fn claim(store: &RunStore, run: &AutomationRun, channel: &str) -> Result<bool, String> {
    let Some(transition) = transition(run) else {
        return Ok(false);
    };
    store.claim_notification(&run.id, transition, channel, run.updated_ms)
}

pub(crate) fn settle(store: &RunStore, run: &AutomationRun, channel: &str, confirmed: bool) {
    let Some(transition) = transition(run) else {
        return;
    };
    if store
        .settle_notification(
            &run.id,
            transition,
            channel,
            if confirmed {
                DeliveryStatus::Confirmed
            } else {
                DeliveryStatus::Unknown
            },
            chrono::Utc::now().timestamp_millis().max(run.updated_ms),
        )
        .is_err()
    {
        tracing::warn!(
            source = "automations",
            "Automation notification settlement failed"
        );
    }
}

fn enabled(config: &crate::config::NotificationConfig, run: &AutomationRun) -> bool {
    config.enabled
        && match run.status {
            RunStatus::NeedsYou => config.sounds.question,
            RunStatus::Failed => config.sounds.error,
            _ => false,
        }
}

/// Bell delivery means the live event was offered, not that a client displayed it.
pub(crate) fn publish(state: &Arc<AppState>, run: &AutomationRun) -> bool {
    let Some(store) = state.automation_runtime.notification_store() else {
        return false;
    };
    let bell = claim(&store, run, "bell").unwrap_or(false);
    if transition(run).is_none() {
        return false;
    }
    let config = crate::config::load_notification_config();
    let allowed = enabled(&config, run);
    let title = format!("{} · {}", run.definition.name, transition(run).unwrap());
    let body = format!(
        "Run {}{}",
        run.id,
        run.session_id
            .as_ref()
            .map(|id| format!(" · Session {id}"))
            .unwrap_or_default()
    );
    #[cfg(feature = "desktop")]
    if allowed
        && let Some(app) = state.app_handle.read().clone()
        && claim(&store, run, "native").unwrap_or(false)
    {
        let store = store.clone();
        let run = run.clone();
        let title = title.clone();
        let body = body.clone();
        tokio::spawn(async move {
            let result = crate::native_notification::send_automation_notice(
                app,
                title,
                body,
                crate::native_notification::NativeNoticeTarget::Automation { id: run.id.clone() },
            )
            .await;
            settle(&store, &run, "native", result.is_ok());
        });
    }
    let push_config = state.config.read().clone();
    if allowed
        && push_config.services.push.enabled
        && !state.push_store.is_empty()
        && claim(&store, run, "push").unwrap_or(false)
    {
        let state = state.clone();
        let store = store.clone();
        let run = run.clone();
        let title = title.clone();
        let body = body.clone();
        tokio::spawn(async move {
            let url = run
                .session_id
                .as_ref()
                .map(|id| format!("/mobile/session/{id}"))
                .unwrap_or_else(|| "/mobile".into());
            let result = crate::push::send_push_batch(
                state.push_store.list(),
                &push_config,
                &state.http_client,
                &title,
                &body,
                &url,
            )
            .await;
            for endpoint in result.stale_endpoints {
                state.push_store.remove(&endpoint);
            }
            settle(&store, &run, "push", result.sent > 0 && result.failed == 0);
        });
    }
    // Telegram's existing opt-in is per registered agent, not a global destination.
    if allowed && run.session_id.is_some() {
        crate::telegram::automation_notice(store, run.clone(), format!("{title}\n{body}"));
    }
    bell
}

#[cfg(test)]
mod tests;
