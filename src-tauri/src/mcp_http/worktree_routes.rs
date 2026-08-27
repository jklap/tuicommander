use crate::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::Emitter;

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

pub(super) struct CreatedWorktree {
    pub worktree: crate::state::WorktreeInfo,
    pub path: String,
    pub branch: String,
    pub setup_script: Option<serde_json::Value>,
    pub setup_script_error: Option<serde_json::Value>,
}

pub(super) async fn list_worktrees_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let worktrees: Vec<serde_json::Value> = state
        .sessions
        .iter()
        .filter_map(|entry| {
            let session = entry.value().lock();
            session.worktree.as_ref().map(|wt| {
                serde_json::json!({
                    "session_id": entry.key(),
                    "name": wt.name,
                    "path": wt.path.to_string_lossy(),
                    "branch": wt.branch,
                    "base_repo": wt.base_repo.to_string_lossy(),
                })
            })
        })
        .collect();
    Json(worktrees)
}

pub(super) async fn get_worktrees_dir_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OptionalRepoQuery>,
) -> impl IntoResponse {
    let dir = match q.repo_path {
        Some(rp) => crate::worktree::resolve_worktree_dir_for_repo(
            std::path::Path::new(&rp),
            &state.worktrees_dir,
        )
        .to_string_lossy()
        .to_string(),
        None => state.worktrees_dir.to_string_lossy().to_string(),
    };
    Json(serde_json::json!({"dir": dir}))
}

pub(super) async fn get_worktree_paths_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let result =
        tokio::task::spawn_blocking(move || crate::worktree::get_worktree_paths(path)).await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn create_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorktreeRequest>,
) -> impl IntoResponse {
    let created = match create_worktree_shared(
        &state,
        body.base_repo.clone(),
        body.branch_name.clone(),
        body.base_ref.clone(),
    )
    .await
    {
        Ok(created) => created,
        Err(response) => return response,
    };

    let mut response = serde_json::json!({
        "name": created.worktree.name,
        "path": &created.path,
        "branch": created.worktree.branch,
        "base_repo": created.worktree.base_repo.to_string_lossy(),
    });
    if let Some(setup_script) = created.setup_script {
        response["setup_script"] = setup_script;
    }
    if let Some(setup_script_error) = created.setup_script_error {
        response["setup_script_error"] = setup_script_error;
    }

    (StatusCode::CREATED, Json(response))
}

pub(super) async fn create_worktree_shared(
    state: &Arc<AppState>,
    base_repo: String,
    branch_name: String,
    base_ref: Option<String>,
) -> Result<CreatedWorktree, (StatusCode, Json<serde_json::Value>)> {
    validate_repo_path(&base_repo)?;
    // Model provides only branch_name and optionally base_ref (start point).
    // Storage path and strategy come entirely from user config via resolve_worktree_dir_for_repo.
    let config = crate::worktree::WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo: base_repo.clone(),
        branch: Some(branch_name),
        create_branch: true, // Always create a new branch — model must not control this
    };
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&config.base_repo),
        &state.worktrees_dir,
    );
    // Use the stale-recovery wrapper so MCP clients heal automatically when an
    // orphaned worktree directory is sitting where the new one should land.
    // Off-loaded onto spawn_blocking because git worktree add can take seconds.
    let config_bg = config.clone();
    let worktrees_dir_bg = worktrees_dir.clone();
    let result = match tokio::task::spawn_blocking(move || {
        crate::worktree::create_worktree_with_stale_recovery(
            &worktrees_dir_bg,
            &config_bg,
            base_ref.as_deref(),
        )
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("task panic: {e}")})),
            ));
        }
    };
    match result {
        Ok(wt) => {
            state.invalidate_repo_caches(&base_repo);
            let wt_path = wt.path.to_string_lossy().to_string();
            let branch_name = wt.branch.clone().unwrap_or_default();
            let _ = state
                .event_bus
                .send(crate::state::AppEvent::WorktreeCreated {
                    repo_path: base_repo.clone(),
                    branch: branch_name.clone(),
                    worktree_path: wt_path.clone(),
                });
            #[cfg(feature = "desktop")]
            if let Some(handle) = state.app_handle.read().as_ref() {
                let _ = handle.emit(
                    "worktree-created",
                    serde_json::json!({
                        "repo_path": &base_repo,
                        "branch": &branch_name,
                        "worktree_path": &wt_path,
                    }),
                );
            }
            let mut setup_script = None;
            let mut setup_script_error = None;
            let repo_for_script = base_repo.clone();
            let cwd_for_script = wt_path.clone();
            if let Some(script) = tokio::task::spawn_blocking(move || {
                crate::config::resolve_effective_setup_script(&repo_for_script)
            })
            .await
            .ok()
            .flatten()
            {
                match tokio::task::spawn_blocking(move || {
                    crate::worktree::run_setup_script(script, cwd_for_script)
                })
                .await
                {
                    Ok(Ok(result)) => {
                        setup_script = Some(result);
                    }
                    Ok(Err(e)) => {
                        setup_script_error = Some(serde_json::json!(e));
                    }
                    Err(e) => {
                        setup_script_error = Some(serde_json::json!(format!("task panic: {e}")));
                    }
                }
            }
            Ok(CreatedWorktree {
                worktree: wt,
                path: wt_path,
                branch: branch_name,
                setup_script,
                setup_script_error,
            })
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )),
    }
}

pub(super) async fn remove_worktree_http(
    State(state): State<Arc<AppState>>,
    Path(branch): Path<String>,
    Query(q): Query<RemoveWorktreeQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let repo_path = q.repo_path.clone();
    let delete_branch = q.delete_branch.unwrap_or(true);
    let force = q.force.unwrap_or(false);
    let override_busy = q.override_busy.unwrap_or(false);
    let mode = if force {
        crate::worktree::RemovalMode::Forced
    } else {
        crate::worktree::RemovalMode::Safe
    };
    let branch_for_event = branch.clone();
    let state_arc = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        // `resolve_archive_script` — the Tauri command and MCP transport both
        // run the pre-removal archive script; this HTTP route previously
        // skipped it (passed `None`), a transport-parity drift.
        let script = crate::worktree::resolve_archive_script(&repo_path);
        crate::worktree::remove_worktree_by_branch(
            &repo_path,
            &branch,
            delete_branch,
            script.as_deref(),
            mode,
            Some(&state_arc),
            override_busy,
        )
    })
    .await;
    if matches!(result, Ok(Ok(_))) {
        state.notify_worktree_removed(&q.repo_path, &branch_for_event);
    }
    match result {
        Ok(Ok(outcome)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "branch_delete_warning": outcome.branch_delete_warning,
            })),
        )
            .into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn detect_orphan_worktrees_http(Query(q): Query<OptionalRepoQuery>) -> Response {
    let repo_path = match q.repo_path {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "repoPath required"})),
            )
                .into_response();
        }
    };
    if let Err(e) = validate_repo_path(&repo_path) {
        return e.into_response();
    }
    json_result(crate::worktree::detect_orphan_worktrees(repo_path).await)
}

pub(super) async fn remove_orphan_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::RemoveOrphanRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    // Shares `remove_orphan_worktree_impl` with the Tauri command: the archive-not-delete
    // behavior lives there, once, for both transports.
    let repo_path = body.repo_path.clone();
    let worktree_path = body.worktree_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::remove_orphan_worktree_impl(&state, repo_path, worktree_path)
    })
    .await;
    match result {
        Ok(Ok(archive_path)) => (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "archivePath": archive_path})),
        )
            .into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn delete_orphan_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::RemoveOrphanRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    // Shares `delete_orphan_worktree_impl` with the Tauri command — see above.
    let repo_path = body.repo_path.clone();
    let worktree_path = body.worktree_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::delete_orphan_worktree_impl(&state, repo_path, worktree_path)
    })
    .await;
    match result {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn generate_worktree_name_http(
    Json(body): Json<GenerateWorktreeNameRequest>,
) -> impl IntoResponse {
    Json(crate::worktree::generate_worktree_name_cmd(
        body.existing_names,
    ))
}

pub(super) async fn list_local_branches_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let result =
        tokio::task::spawn_blocking(move || crate::worktree::list_local_branches(path)).await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn checkout_remote_branch_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CheckoutRemoteRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let repo = std::path::PathBuf::from(&body.repo_path);
    let branch_name = body.branch_name.clone();
    let remote_ref = format!("origin/{}", body.branch_name);
    let result = tokio::task::spawn_blocking(move || {
        crate::git_cli::git_cmd(&repo)
            .args(["checkout", "-b", &branch_name, &remote_ref])
            .run()
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(_)) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!(null))).into_response()
        }
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn merge_pr_via_github_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::github::merge_pr_github_impl(
        &body.repo_path,
        body.pr_number,
        &body.merge_method,
        &state,
    )
    .await
    {
        Ok(sha) => (StatusCode::OK, Json(serde_json::json!({"sha": sha}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn finalize_merged_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FinalizeMergeRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let FinalizeMergeRequest {
        repo_path,
        branch_name,
        action,
        force,
    } = body;
    // Shares `finalize_merged_worktree_impl` with the Tauri command: the dirty-worktree
    // gate and the "worktree removed" notification live there, once, for both transports.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::finalize_merged_worktree_impl(
            &state,
            repo_path,
            branch_name,
            action,
            force.unwrap_or(false),
        )
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn switch_branch_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SwitchBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let SwitchBranchRequest {
        repo_path,
        branch_name,
        force,
        stash,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::switch_branch_impl(&state, repo_path, branch_name, force, stash)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn merge_and_archive_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergeArchiveRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let MergeArchiveRequest {
        repo_path,
        branch_name,
        target_branch,
        after_merge,
        force,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::merge_and_archive_worktree_impl(
            &state,
            repo_path,
            branch_name,
            target_branch,
            after_merge,
            force.unwrap_or(false),
        )
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests_support::create_temp_git_repo;
    use axum::response::IntoResponse;

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn remove_worktree_http_removes_a_real_worktree_and_notifies() {
        // `remove_worktree_http` had zero tests before this — including the
        // transport-parity bug this closes: it passed `archive_script: None`
        // unconditionally instead of `resolve_archive_script`, unlike the Tauri
        // command and the MCP transport (both of which do resolve it).
        let repo = create_temp_git_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "http-remove".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("http-remove".to_string()),
            create_branch: true,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = remove_worktree_http(
            State(state),
            Path("http-remove".to_string()),
            Query(RemoveWorktreeQuery {
                repo_path: repo.path().to_string_lossy().to_string(),
                delete_branch: Some(true),
                force: None,
                override_busy: None,
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["ok"], true, "response: {body}");
        assert!(!wt.path.exists(), "worktree directory should be removed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn remove_worktree_http_refuses_a_worktree_with_a_live_session() {
        let repo = create_temp_git_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "http-busy".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("http-busy".to_string()),
            create_branch: true,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session_attached_to(&state, "s1", wt.clone());

        let response = remove_worktree_http(
            State(state.clone()),
            Path("http-busy".to_string()),
            Query(RemoveWorktreeQuery {
                repo_path: repo.path().to_string_lossy().to_string(),
                delete_branch: Some(true),
                force: None,
                override_busy: None,
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = response_json(response).await;
        let error = body["error"].as_str().expect("error field");
        assert!(
            error.starts_with("worktree_busy:"),
            "expected worktree_busy: prefix, got: {error}"
        );
        assert!(wt.path.exists(), "worktree must survive a refused removal");

        // The `force` query param threads through to override the live-session
        // gate too — the same escape hatch the desktop UI's "Delete anyway"
        // dialog uses via the Tauri command's `override_busy` param.
        let response = remove_worktree_http(
            State(state),
            Path("http-busy".to_string()),
            Query(RemoveWorktreeQuery {
                repo_path: repo.path().to_string_lossy().to_string(),
                delete_branch: Some(true),
                force: None,
                override_busy: Some(true),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            !wt.path.exists(),
            "override_busy should let the removal through"
        );
    }

    #[tokio::test]
    async fn remove_worktree_http_runs_the_archive_script() {
        // Regression test for the transport-parity bug found while adding the
        // liveness gate: this route used to call `remove_worktree_by_branch`
        // with `archive_script: None` unconditionally, silently skipping the
        // pre-removal archive script that the Tauri command and MCP transport
        // both run.
        let repo = create_temp_git_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "http-archive-script".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("http-archive-script".to_string()),
            create_branch: true,
        };
        crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let marker = repo.path().join("script-ran.txt");
        let _guard = crate::config::set_config_dir_override(repo.path().join("tuic-config"));
        crate::config::save_repo_defaults(crate::config::RepoDefaultsConfig {
            archive_script: format!("touch {}", marker.display()),
            ..crate::config::load_repo_defaults()
        })
        .expect("save repo defaults");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = remove_worktree_http(
            State(state),
            Path("http-archive-script".to_string()),
            Query(RemoveWorktreeQuery {
                repo_path: repo.path().to_string_lossy().to_string(),
                delete_branch: Some(true),
                force: None,
                override_busy: None,
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            marker.exists(),
            "the configured archive script should have run"
        );
    }
}
