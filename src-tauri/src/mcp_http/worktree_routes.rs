use crate::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

pub(super) struct CreatedWorktree {
    pub worktree: crate::state::WorktreeInfo,
    pub path: String,
    /// What the caller needs in order to USE this workspace: the dirty policy
    /// applied, the warm artifacts, and the isolation semantics of the
    /// mechanism it actually got. The only instruction channel there is
    /// (#734-ca73) — there is no enforcement layer behind it.
    pub instructions: serde_json::Value,
    /// The id the caller must use to address this workspace afterwards — removal,
    /// dirtiness and finalize all take an id. Reported rather than left implicit:
    /// a caller cannot re-derive it, because for a COW clone it is not the branch.
    pub workspace_id: String,
    pub branch: String,
    pub setup_script: Option<serde_json::Value>,
    pub setup_script_error: Option<serde_json::Value>,
}

pub(super) async fn list_worktrees_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let worktrees: Vec<serde_json::Value> = state
        .session_maps
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
        body.mode,
        body.dirty,
    )
    .await
    {
        Ok(created) => created,
        Err(response) => return response,
    };

    let mut response = serde_json::json!({
        "name": created.worktree.name,
        "path": &created.path,
        // The instruction payload rides on both transports identically: the
        // model reading it over MCP and the client reading it over HTTP need
        // the same isolation semantics (#734-ca73).
        "instructions": &created.instructions,
        // How the caller addresses this workspace from here on. `branch` is what
        // is checked out; the two match for a linked worktree and will not for a
        // COW clone, so both are reported.
        "workspace_id": &created.workspace_id,
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
    mode: crate::cow::WorkspaceMode,
    dirty: crate::cow::DirtyPolicy,
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
    // `create_workspace` picks the mechanism and degrades rather than failing;
    // for the worktree path it still goes through the stale-recovery wrapper, so
    // MCP clients keep healing automatically when an orphaned directory is
    // sitting where the new one should land. Off-loaded onto spawn_blocking
    // because a clone or a `git worktree add` can take seconds.
    let config_bg = config.clone();
    let worktrees_dir_bg = worktrees_dir.clone();
    let result = match tokio::task::spawn_blocking(move || {
        crate::worktree::create_workspace(
            &worktrees_dir_bg,
            &config_bg,
            base_ref.as_deref(),
            mode,
            dirty,
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
        Ok(workspace) => {
            let wt_path = workspace.path.to_string_lossy().to_string();
            let branch_name = workspace.branch.clone();
            let workspace_id = workspace.workspace_id.clone();
            // Built before the setup script runs: the payload describes what the
            // workspace ARRIVED with, and a script that installs something does
            // not change what was already warm.
            let instructions = workspace.instruction_payload();
            state.notify_worktree_created(crate::state::WorktreeCreatedPayload {
                repo_path: base_repo.clone(),
                workspace_id: workspace_id.clone(),
                branch: branch_name.clone(),
                worktree_path: wt_path.clone(),
            });
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
                worktree: crate::state::WorktreeInfo {
                    name: workspace
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| branch_name.clone()),
                    path: workspace.path,
                    branch: Some(branch_name.clone()),
                    base_repo: std::path::PathBuf::from(&base_repo),
                },
                path: wt_path,
                instructions,
                workspace_id,
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
    Path(workspace_id): Path<String>,
    Query(q): Query<RemoveWorktreeQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let repo_path = q.repo_path.clone();
    let delete_branch = q.delete_branch.unwrap_or(true);
    let force = q.force.unwrap_or(false);
    let id_for_event = workspace_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::remove_worktree_by_workspace_id(
            &repo_path,
            &workspace_id,
            delete_branch,
            None,
            force,
        )
    })
    .await;
    // The branch comes off the outcome: it was read from the record before the
    // checkout was removed, and nothing can resolve the id afterwards.
    if let Ok(Ok(ref outcome)) = result {
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: q.repo_path.clone(),
            workspace_id: id_for_event,
            branch: outcome.branch.clone(),
        });
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
    let repo_path = body.repo_path.clone();
    let worktree_path = body.worktree_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::validate_worktree_path(&repo_path, &worktree_path)?;
        let worktree = crate::state::WorktreeInfo {
            name: std::path::Path::new(&worktree_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| worktree_path.clone()),
            path: std::path::PathBuf::from(&worktree_path),
            branch: None,
            base_repo: std::path::PathBuf::from(&repo_path),
        };
        crate::worktree::remove_worktree_internal(&worktree, false)
    })
    .await;
    match result {
        Ok(Ok(())) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
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

/// `GET /worktrees/unpublished?repoPath=&workspaceId=` — the HTTP half of
/// `count_unpublished_commits`.
pub(super) async fn unpublished_commits_http(Query(q): Query<WorkspaceIdQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    // Blocking: counting refreshes the parent mirror first.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::unpublished_commits_impl(&q.repo_path, &q.workspace_id)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

/// `POST /worktrees/publish` — the HTTP half of `publish_workspace`.
///
/// Shares `publish_workspace_impl` with the Tauri command, so the two-step
/// outcome (parent, then origin) has one implementation and one shape.
pub(super) async fn publish_workspace_http(Json(body): Json<PublishWorkspaceRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let PublishWorkspaceRequest {
        repo_path,
        workspace_id,
    } = body;
    // Blocking: a publish runs a fetch and a push.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::publish_workspace_impl(&repo_path, &workspace_id)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
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
        workspace_id,
        action,
        force,
    } = body;
    // Shares `finalize_merged_worktree_impl` with the Tauri command: the dirty-worktree
    // gate and the "worktree removed" notification live there, once, for both transports.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::finalize_merged_worktree_impl(
            &state,
            repo_path,
            workspace_id,
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
        workspace_id,
        target_branch,
        after_merge,
        force,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::merge_and_archive_worktree_impl(
            &state,
            repo_path,
            branch_name,
            workspace_id,
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

/// Body of `POST /worktrees/run-script`.
#[derive(serde::Deserialize)]
pub(super) struct RunSetupScriptRequest {
    pub script: String,
    pub cwd: String,
}

/// `POST /worktrees/run-script` — mirror of the `run_setup_script` command.
/// The script runs a real process, so it goes to a blocking pool rather than
/// stalling the axum worker for its whole duration.
pub(super) async fn run_setup_script_http(
    Json(body): Json<RunSetupScriptRequest>,
) -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::run_setup_script(body.script, body.cwd)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}
