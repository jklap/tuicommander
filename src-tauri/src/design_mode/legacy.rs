use super::manager as mode_manager;

use crate::AppState;
use std::sync::Arc;

pub(crate) async fn manager(state: &Arc<AppState>) -> &mode_manager::DesignModeManager {
    state
        .design_mode
        .get_or_init(|| async {
            let prefill_state = Arc::downgrade(state);
            let notify_state = Arc::downgrade(state);
            let manager = mode_manager::DesignModeManager::new(
                state.data_dir.join("design-grabs"),
                Arc::new(move |session_id, text| {
                    let state = prefill_state
                        .upgrade()
                        .ok_or("TUICommander is shutting down")?;
                    crate::pty::prefill_agent_input(&state, session_id, text)
                }),
                Arc::new(move |status| {
                    if let Some(state) = notify_state.upgrade() {
                        emit_changed(&state, &status.repo_path, &status.session_id, status.status);
                    }
                }),
            );
            let mut events = state.event_bus.subscribe();
            let closing = manager.clone();
            tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(crate::state::AppEvent::SessionClosed { session_id, .. }) => {
                            closing.session_closed(&session_id).await;
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            let sweeper = manager.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(super::liveness::SWEEP_INTERVAL);
                loop {
                    tick.tick().await;
                    sweeper.stop_expired().await;
                }
            });
            manager
        })
        .await
}

pub(crate) fn status_json(status: &mode_manager::ModeStatus) -> serde_json::Value {
    serde_json::json!({
        "repoPath": status.repo_path,
        "sessionId": status.session_id,
        "status": status.status,
    })
}

pub(crate) async fn start(
    state: &Arc<AppState>,
    session_id: String,
    browser: Option<String>,
    url: Option<String>,
) -> Result<serde_json::Value, String> {
    let agent = state
        .session_maps
        .session_states
        .get(&session_id)
        .is_some_and(|session| session.agent_type.is_some());
    if !agent {
        return Err("Session is not running an agent".into());
    }
    let repo = {
        let session_entry = state
            .session_maps
            .sessions
            .get(&session_id)
            .ok_or("Agent session no longer exists")?;
        let session = session_entry.lock();
        if let Some(worktree) = &session.worktree {
            crate::git::canonical_repo_root(&worktree.base_repo)
        } else {
            let cwd = session
                .cwd
                .as_deref()
                .ok_or("Agent session has no repository path")?;
            let root = std::path::Path::new(cwd)
                .ancestors()
                .find(|path| crate::git::resolve_git_dir(path).is_some())
                .ok_or("Agent session is not in a repository")?;
            crate::git::canonical_repo_root(root)
        }
    };
    let repo_text = repo.to_string_lossy();
    let repo_path = if repo.parent().is_none() {
        repo_text.into_owned()
    } else {
        repo_text
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .to_owned()
    };
    let _ = repo_path;
    super::launcher::launch(
        state,
        &repo,
        &session_id,
        browser.as_deref(),
        url.as_deref(),
    )
    .await
}

/// Repo settings are keyed by the path the frontend registered, which may be a
/// symlink (or `/var` against `/private/var`) of the canonical root Design Mode
/// uses. Match on the canonical root of each key, not on the text.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Retained CDP path (design-mode-v2 sections 14/15)"
    )
)]
fn dev_server_url(
    settings: &crate::config::RepoSettingsMap,
    repo: &std::path::Path,
) -> Option<String> {
    let entry = settings.repos.iter().find_map(|(key, entry)| {
        (crate::git::canonical_repo_root(std::path::Path::new(key)) == repo).then_some(entry)
    });
    if entry.is_none() {
        tracing::debug!(repo = %repo.display(), "Design Mode found no repo settings for this repository");
    }
    entry?
        .dev_server_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .map(str::to_owned)
}

pub(crate) async fn stop(
    state: &Arc<AppState>,
    repo_path: &str,
) -> Result<serde_json::Value, String> {
    let status = manager(state)
        .await
        .stop(repo_path)
        .await?
        .ok_or("Design Mode is not running for this repository")?;
    Ok(status_json(&status))
}

/// Asks the extension to reload the reviewed tab; it sees the new sequence on its next beat.
pub(crate) async fn reload(
    state: &Arc<AppState>,
    repo_path: &str,
) -> Result<serde_json::Value, String> {
    let seq = manager(state)
        .await
        .request_reload(repo_path)
        .await
        .ok_or("Design Mode is not running for this repository")?;
    Ok(serde_json::json!({"repoPath": repo_path, "reloadSeq": seq}))
}

pub(crate) async fn statuses(state: &Arc<AppState>) -> Vec<serde_json::Value> {
    let Some(manager) = state.design_mode.get() else {
        return Vec::new();
    };
    manager.statuses().await.iter().map(status_json).collect()
}

pub(crate) fn emit_changed(
    state: &crate::AppState,
    repo_path: &str,
    session_id: &str,
    status: &str,
) {
    let payload = serde_json::json!({
        "repo_path": repo_path,
        "session_id": session_id,
        "status": status,
    });
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::DesignModeChanged {
            repo_path: repo_path.to_owned(),
            session_id: session_id.to_owned(),
            status: status.to_owned(),
        });
    use tauri::Emitter;
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit("design-mode-changed", payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_server_url_is_found_under_a_symlinked_repo_path() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real, &link).unwrap();
        let mut settings = crate::config::RepoSettingsMap::default();
        settings.repos.insert(
            link.to_string_lossy().into_owned(),
            crate::config::RepoSettingsEntry {
                dev_server_url: Some("http://localhost:5173".into()),
                ..Default::default()
            },
        );
        let canonical = crate::git::canonical_repo_root(&real);
        assert_eq!(
            dev_server_url(&settings, &canonical).as_deref(),
            Some("http://localhost:5173")
        );
        assert_eq!(dev_server_url(&settings, &dir.path().join("other")), None);
    }
}
