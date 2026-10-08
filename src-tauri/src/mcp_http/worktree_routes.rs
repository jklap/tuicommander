use crate::AppState;
use axum::Extension;
use axum::Json;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;
use std::sync::Arc;

use super::guards::{Authenticated, require_local_or_auth};
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
    /// dirtiness and finalize all take an id.
    pub workspace_id: String,
    pub branch: String,
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
                    "warm_artifacts": crate::worktree::warm_status(&wt.path),
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

    // No setup_script/setup_script_error here: create_worktree_shared hands
    // the Setup Script to the background chain (warm -> sync -> script,
    // spawn_worktree_setup_chain); its outcome is the dual-emitted
    // `worktree-setup-script-completed` event, not this response.
    let response = serde_json::json!({
        "name": created.worktree.name,
        "path": &created.path,
        // The instruction payload rides on both transports identically: the
        // model reading it over MCP and the client reading it over HTTP need
        // the same isolation semantics (#734-ca73).
        "instructions": &created.instructions,
        // How the caller addresses this workspace from here on. `branch` remains
        // explicit display data even though linked worktree ids currently match it.
        "workspace_id": &created.workspace_id,
        "branch": created.worktree.branch,
        "base_repo": created.worktree.base_repo.to_string_lossy(),
    });

    (StatusCode::CREATED, Json(response))
}

/// `POST /worktrees/run-script` — HTTP counterpart of the `run_setup_script`
/// Tauri command. The script runs a real process, so it goes to a blocking
/// pool rather than stalling the axum worker for its whole duration.
///
/// This runs an arbitrary shell script, so `require_local_or_auth` is
/// load-bearing, not boilerplate — it must never become LAN-reachable
/// without authentication. Response shape is exactly
/// `{exit_code, stdout, stderr}`, identical to the Tauri command, so the
/// existing `transport.ts` mapping needs no `transform`.
pub(super) async fn run_setup_script_http(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<Authenticated>>,
    Json(body): Json<RunSetupScriptRequest>,
) -> Response {
    if let Err(resp) = require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    // `~` stays accepted (the documented contract); the expanded path is what
    // gets validated, so `..`/relative paths are still refused.
    let cwd = crate::cli::expand_tilde(&body.cwd);
    if let Err(e) = validate_repo_path(&cwd) {
        return e.into_response();
    }
    let result =
        tokio::task::spawn_blocking(move || crate::worktree::run_setup_script(body.script, cwd))
            .await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

/// Poll the current state of a worktree's background setup chain
/// (`worktree::spawn_worktree_setup_chain`) — closes the observability gap
/// left by that chain no longer returning `setup_script`/`setup_script_error`
/// synchronously: an MCP client has no SSE/event stream to receive
/// `worktree-setup-script-completed` on, so this lets it poll instead.
/// Read-only status lookup, no `require_local_or_auth` gate needed (unlike
/// `run_setup_script_http`, this never executes anything).
pub(super) async fn get_worktree_setup_status_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WorktreeSetupStatusQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    match crate::worktree::get_worktree_setup_status(&state, &q.repo_path, &q.branch) {
        Some(status) => Json(status).into_response(),
        None => Json(serde_json::json!({"state": "unknown"})).into_response(),
    }
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
    // `create_workspace` picks the mechanism and degrades rather than failing;
    // for the worktree path it still goes through the stale-recovery wrapper, so
    // MCP clients keep healing automatically when an orphaned directory is
    // sitting where the new one should land. Off-loaded onto spawn_blocking
    // because a clone or a `git worktree add` can take seconds.
    let config_bg = config.clone();
    let worktrees_dir_bg = worktrees_dir.clone();
    let result = match tokio::task::spawn_blocking(move || {
        crate::worktree::create_workspace_unwarmed(
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
        Ok(workspace) => {
            let wt_path = workspace.path.to_string_lossy().to_string();
            let branch_name = workspace.branch.clone();
            let workspace_id = workspace.workspace_id.clone();
            let warm_token = crate::worktree::begin_warm(&workspace.path);
            // Built before the chain runs: the payload describes what the
            // workspace ARRIVED with (warm still pending), and a script that
            // installs something does not change what was already warm.
            let instructions = workspace.instruction_payload_pending();
            state.notify_worktree_created(crate::state::WorktreeCreatedPayload {
                repo_path: base_repo.clone(),
                workspace_id: workspace_id.clone(),
                branch: branch_name.clone(),
                worktree_path: wt_path.clone(),
                kind: workspace.kind,
            });
            // CoW warm -> file sync -> Setup Script, in the background. The
            // chain awaits the warm before the sync touches the destination,
            // and keeps warm_artifacts.status `pending` until the script is
            // done; this response returns before any of it finishes, so it
            // carries no setup_script/setup_script_error (the outcome arrives
            // as the dual-emitted worktree-setup-script-completed event).
            crate::worktree::spawn_worktree_setup_chain(
                state,
                base_repo.clone(),
                branch_name.clone(),
                workspace.path.clone(),
                Some(warm_token),
            );
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
    let force = q.force.unwrap_or(false);
    let delete_branch = q.delete_branch.unwrap_or(!force);
    let override_lock = q.override_lock.unwrap_or(false);
    let expected_fingerprint = q.expected_fingerprint.clone();
    let confirm_missing_checkout = q.confirm_missing_checkout.unwrap_or(false);
    let override_busy = q.override_busy.unwrap_or(false);
    if force && expected_fingerprint.is_none() && !confirm_missing_checkout {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"})),
        )
            .into_response();
    }
    let id_for_event = workspace_id.clone();
    let preview_state = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        if let Err(busy) = crate::worktree::workspace_removal_guard(
            &preview_state,
            &repo_path,
            &workspace_id,
            override_busy,
        ) {
            return Err(RemovalError::Busy(busy));
        }
        let warnings = crate::worktree::inspect_worktree_removal(
            &preview_state,
            std::path::Path::new(&repo_path),
            &workspace_id,
        )
        .warnings;
        let archive = crate::worktree::resolve_archive_script(&repo_path);
        let outcome = crate::worktree::remove_worktree_with_presence_confirmation(
            &repo_path,
            &workspace_id,
            delete_branch,
            archive.as_deref(),
            force,
            override_lock,
            expected_fingerprint.as_deref(),
            confirm_missing_checkout,
        )
        .map_err(RemovalError::Failed)?;
        Ok::<_, RemovalError>((outcome, warnings))
    })
    .await;
    // The branch comes off the outcome: it was read from the record before the
    // checkout was removed, and nothing can resolve the id afterwards.
    if let Ok(Ok((ref outcome, _))) = result {
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: q.repo_path.clone(),
            workspace_id: id_for_event,
            branch: outcome.branch.clone(),
        });
    }
    match result {
        Ok(Ok((outcome, warnings))) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "branch_delete_warning": outcome.branch_delete_warning,
                "removal_rule": outcome.removal_rule,
                "warnings": warnings,
            })),
        )
            .into_response(),
        // 409: the request is valid but conflicts with live sessions; the
        // caller retries with `overrideBusy=true` once the user has seen them.
        Ok(Err(RemovalError::Busy(busy))) => {
            (StatusCode::CONFLICT, Json(busy.to_json())).into_response()
        }
        Ok(Err(RemovalError::Failed(e))) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

/// Why `DELETE /worktrees/{id}` did not remove the checkout.
enum RemovalError {
    Busy(crate::worktree::WorktreeBusy),
    Failed(String),
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

pub(super) async fn assess_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OptionalRepoQuery>,
) -> Response {
    let repo_path = match q.repo_path {
        Some(path) if !path.is_empty() => path,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "repoPath required"})),
            )
                .into_response();
        }
    };
    if let Err(error) = validate_repo_path(&repo_path) {
        return error.into_response();
    }
    json_result(crate::worktree::assess_orphan_cleanup_internal(state, repo_path).await)
}

pub(super) async fn begin_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BeginOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    let repo_path = body.repo_path;
    let paths = body.paths;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::begin_orphan_cleanup_internal(&state, &repo_path, paths)
    })
    .await
    {
        Ok(Ok(())) => Json(serde_json::Value::Null).into_response(),
        Ok(Err(error)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
        Err(error) => err_500(&format!("orphan cleanup registration task failed: {error}")),
    }
}

pub(super) async fn pending_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OptionalRepoQuery>,
) -> Response {
    let Some(repo_path) = q.repo_path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "repoPath required"})),
        )
            .into_response();
    };
    if let Err(error) = validate_repo_path(&repo_path) {
        return error.into_response();
    }
    let answer = state
        .pending_orphan_cleanup
        .get(&repo_path)
        .and_then(|entry| entry.answer);
    Json(answer).into_response()
}

pub(super) async fn answer_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnswerOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    let remove = match body.decision.as_str() {
        "remove" => true,
        "keep" => false,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "decision must be remove or keep"})),
            )
                .into_response();
        }
    };
    let repo_path = body.repo_path;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::answer_orphan_cleanup_internal(&state, &repo_path, remove)
    })
    .await
    {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(error)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
        Err(error) => err_500(&format!("orphan cleanup answer task failed: {error}")),
    }
}

pub(super) async fn clear_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ClearOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    crate::worktree::clear_orphan_cleanup_internal(&state, &body.repo_path, body.kept);
    Json(serde_json::Value::Null).into_response()
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
    let safe_only = body.safe_only;
    let confirmed_sessions = body.confirmed_sessions.clone();
    let guard_state = state.clone();
    // A refused guard is the caller's to act on (400); a removal that fails
    // after the guard keeps its old mapping.
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::check_orphan_removal(
            &guard_state,
            &repo_path,
            &worktree_path,
            safe_only,
            &confirmed_sessions,
        )
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
        let worktree = crate::state::WorktreeInfo {
            name: std::path::Path::new(&worktree_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| worktree_path.clone()),
            path: std::path::PathBuf::from(&worktree_path),
            branch: None,
            base_repo: std::path::PathBuf::from(&repo_path),
        };
        tuic_git::worktree::remove_orphan_worktree_internal(&worktree).map_err(|error| {
            let status = if safe_only {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, error)
        })
    })
    .await;
    match result {
        Ok(Ok(())) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Ok(Err((status, error))) => {
            (status, Json(serde_json::json!({"error": error}))).into_response()
        }
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
        &body.expected_head_sha,
        &state,
    )
    .await
    {
        Ok(sha) => (StatusCode::OK, Json(serde_json::json!({"sha": sha}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Fresh linked-worktree removal preflight.
pub(super) async fn workspace_lifecycle_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WorkspaceIdQuery>,
) -> Response {
    if let Err(error) = validate_repo_path(&q.repo_path) {
        return error.into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::inspect_worktree_removal(
            &state,
            std::path::Path::new(&q.repo_path),
            &q.workspace_id,
        )
    })
    .await;
    match result {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => err_500(&format!("task panic: {error}")),
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
        expected_fingerprint,
    } = body;
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"}))).into_response();
    }
    // Shares `finalize_merged_worktree_impl` with the Tauri command: the dirty-worktree
    // gate and the "worktree removed" notification live there, once, for both transports.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path,
            workspace_id,
            action,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
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
        expected_fingerprint,
    } = body;
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"}))).into_response();
    }
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::merge_and_archive_worktree_impl_with_confirmation(
            &state,
            repo_path,
            branch_name,
            workspace_id,
            target_branch,
            after_merge,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
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

#[cfg(test)]
mod warm_tests {
    use super::*;

    #[cfg(unix)]
    pub(super) fn setup_repo(root: &std::path::Path) -> std::path::PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let git = crate::git_cli::git_cmd(&repo);
        git.args(["init"]).run().unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        std::fs::write(repo.join("README.md"), "base\n").unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["add", "."])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["commit", "-m", "base"])
            .run()
            .unwrap();
        repo
    }

    #[cfg(unix)]
    fn set_gated_setup_script(started: &std::path::Path, gate: &std::path::Path) {
        let mut defaults = crate::config::RepoDefaultsConfig::default();
        let finished = gate.with_extension("finished");
        let pid = gate.with_extension("pid");
        defaults.setup_script = format!(
            "echo $$ > '{}'; echo started > '{}'; while [ ! -f '{}' ]; do sleep 0.02; done; echo finished > '{}'",
            pid.display(),
            started.display(),
            gate.display(),
            finished.display()
        );
        crate::config::save_repo_defaults(crate::config::RepoDefaultsConfig::default(), defaults)
            .unwrap();
    }

    #[cfg(unix)]
    async fn wait_for_file(path: &std::path::Path, failure: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while !path.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{failure}"));
    }

    #[cfg(unix)]
    async fn wait_for_setup_exit(pid_file: &std::path::Path) {
        // `echo $$ > pid_file` truncates the file before writing the PID, so
        // polling on existence alone can read it mid-truncate.
        let content = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                let content = std::fs::read_to_string(pid_file).unwrap_or_default();
                if !content.trim().is_empty() {
                    return content;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("setup script did not record its PID"));
        let pid: i32 = content.trim().parse().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while unsafe { libc::kill(pid, 0) } == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("detached setup shell did not exit");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_worktree_shared_reports_pending_while_the_setup_script_runs() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let started = temp.path().join("setup.started");
        let gate = temp.path().join("setup.release");
        set_gated_setup_script(&started, &gate);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().into_owned();
        let state_for_create = Arc::clone(&state);
        let repo_for_create = repo_path.clone();
        let create = tokio::spawn(async move {
            create_worktree_shared(
                &state_for_create,
                repo_for_create,
                "pending-setup".into(),
                None,
            )
            .await
        });

        wait_for_file(&started, "setup script did not start").await;
        let paths = crate::worktree::get_worktree_paths(repo_path).unwrap();
        assert_eq!(
            paths["pending-setup"].warm_artifacts.as_ref().unwrap()["status"],
            "pending"
        );
        std::fs::write(&gate, "release").unwrap();
        let created = create
            .await
            .unwrap()
            .unwrap_or_else(|(status, body)| panic!("{status}: {:?}", body.0));
        assert_eq!(created.instructions["warm_artifacts"]["status"], "pending");
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while crate::worktree::warm_status(&created.worktree.path)["status"] == "pending" {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            crate::worktree::warm_status(&created.worktree.path)["status"],
            "done"
        );
        crate::worktree::clear_warm(&created.worktree.path);
    }

    /// Commit a `.gitignore` for `ignored.txt` and create that ignored file, so
    /// `copy_ignored_files` has exactly one thing to sync.
    #[cfg(unix)]
    fn add_ignored_file(repo: &std::path::Path) {
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(repo.join("ignored.txt"), "secret-config").unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["add", ".gitignore"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo)
            .args(["commit", "-m", "add gitignore"])
            .run()
            .unwrap();
    }

    #[cfg(unix)]
    fn save_repo_entry(repo: &std::path::Path, entry: crate::config::RepoSettingsEntry) {
        let key = repo.to_string_lossy().to_string();
        crate::config::save_repo_settings(
            crate::config::RepoSettingsMap::default(),
            crate::config::RepoSettingsMap {
                repos: [(
                    key.clone(),
                    crate::config::RepoSettingsEntry { path: key, ..entry },
                )]
                .into_iter()
                .collect(),
            },
        )
        .unwrap();
    }

    /// The chain's whole contract in one run: the warm finishes before the
    /// sync writes anything, the sync finishes before the setup script runs,
    /// the script's outcome is reported on the bus AFTER the sync's, and the
    /// workspace only stops reading `pending` once the chain is done.
    #[cfg(unix)]
    #[tokio::test]
    async fn setup_chain_runs_warm_then_sync_then_setup_and_reports_completion() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        add_ignored_file(&repo);
        let marker = temp.path().join("order.txt");
        save_repo_entry(
            &repo,
            crate::config::RepoSettingsEntry {
                copy_ignored_files: Some(true),
                setup_script: Some(format!(
                    "if [ -f warm.marker ] && [ -f ignored.txt ]; then echo ordered > '{0}'; else echo wrong > '{0}'; fi",
                    marker.display()
                )),
                ..Default::default()
            },
        );
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();

        crate::worktree::run_worktree_setup_chain(
            Arc::clone(&state),
            repo.to_string_lossy().into_owned(),
            "chain-order".into(),
            destination.clone(),
            Some((token, |_: &std::path::Path, dest: &std::path::Path| {
                assert!(
                    !dest.join("ignored.txt").exists(),
                    "the file sync must not write before the warm finishes"
                );
                std::fs::write(dest.join("warm.marker"), "warm").unwrap();
                crate::cow::WarmingReport::default()
            })),
        )
        .await;

        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "ordered");
        assert_eq!(crate::worktree::warm_status(&destination)["status"], "done");
        let mut order = Vec::new();
        while let Ok(event) = events.try_recv() {
            match event {
                crate::state::AppEvent::WorktreeSyncCompleted { copied, .. } => {
                    order.push(format!("sync:{copied}"));
                }
                crate::state::AppEvent::WorktreeSetupScriptCompleted {
                    exit_code, error, ..
                } => {
                    assert_eq!(exit_code, Some(0));
                    assert_eq!(error, None);
                    order.push("setup".into());
                }
                _ => {}
            }
        }
        assert_eq!(order, ["sync:1", "setup"]);
        crate::worktree::clear_warm(&destination);
    }

    /// The warm is already done when the setup script starts, yet the
    /// workspace still reads `pending`: the status is published only after
    /// the last step of the chain.
    #[cfg(unix)]
    #[tokio::test]
    async fn setup_script_observes_pending_after_the_warm_finished() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let started = temp.path().join("setup.started");
        let gate = temp.path().join("setup.release");
        set_gated_setup_script(&started, &gate);
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let chain = tokio::spawn(crate::worktree::run_worktree_setup_chain(
            state,
            repo.to_string_lossy().into_owned(),
            "pending-chain".into(),
            destination.clone(),
            Some((token, |_: &std::path::Path, dest: &std::path::Path| {
                std::fs::write(dest.join("warm.marker"), "warm").unwrap();
                crate::cow::WarmingReport::default()
            })),
        ));

        wait_for_file(&started, "setup script did not start").await;
        assert!(destination.join("warm.marker").exists());
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        std::fs::write(&gate, "release").unwrap();
        chain.await.unwrap();
        assert_eq!(crate::worktree::warm_status(&destination)["status"], "done");
        crate::worktree::clear_warm(&destination);
    }

    /// A chain that stops before publishing (task aborted, runtime shutting
    /// down) must not leave the workspace reading `pending` forever.
    #[cfg(unix)]
    #[tokio::test]
    async fn aborted_setup_chain_marks_pending_warm_failed() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let started = temp.path().join("setup.started");
        let gate = temp.path().join("setup.release");
        set_gated_setup_script(&started, &gate);
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let chain = tokio::spawn(crate::worktree::run_worktree_setup_chain(
            state,
            repo.to_string_lossy().into_owned(),
            "aborted-chain".into(),
            destination.clone(),
            Some((token, |_: &std::path::Path, _: &std::path::Path| {
                crate::cow::WarmingReport::default()
            })),
        ));

        wait_for_file(&started, "setup script did not start").await;
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        chain.abort();
        let _ = chain.await;
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "failed"
        );
        std::fs::write(&gate, "release").unwrap();
        wait_for_file(
            &gate.with_extension("finished"),
            "setup script did not finish",
        )
        .await;
        wait_for_setup_exit(&gate.with_extension("pid")).await;
        crate::worktree::clear_warm(&destination);
    }

    /// A removal that wins the race against a queued warm clears the token;
    /// the chain must then write nothing more into the removed checkout —
    /// no warm, no sync, no setup script, no completion event.
    #[cfg(unix)]
    #[tokio::test]
    async fn setup_chain_stops_when_the_workspace_was_removed_before_the_warm() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        add_ignored_file(&repo);
        let marker = temp.path().join("ran.txt");
        save_repo_entry(
            &repo,
            crate::config::RepoSettingsEntry {
                copy_ignored_files: Some(true),
                setup_script: Some(format!("echo ran > '{}'", marker.display())),
                ..Default::default()
            },
        );
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        crate::worktree::clear_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();

        crate::worktree::run_worktree_setup_chain(
            Arc::clone(&state),
            repo.to_string_lossy().into_owned(),
            "removed-chain".into(),
            destination.clone(),
            Some((
                token,
                |_: &std::path::Path, _: &std::path::Path| -> crate::cow::WarmingReport {
                    panic!("a cleared token must not warm")
                },
            )),
        )
        .await;

        assert!(!marker.exists(), "setup script ran for a removed workspace");
        assert!(!destination.join("ignored.txt").exists());
        let mut terminal_events = Vec::new();
        while let Ok(event) = events.try_recv() {
            match event {
                crate::state::AppEvent::WorktreeSyncStarted { .. } => {
                    panic!("no chain step may run after removal")
                }
                // The only event: the chain's end, so a frontend waiter for
                // this workspace resolves instead of waiting out its timeout.
                crate::state::AppEvent::WorktreeSetupScriptCompleted {
                    outcome,
                    exit_code,
                    error,
                    ..
                } => terminal_events.push((outcome, exit_code, error.is_some())),
                _ => {}
            }
        }
        assert_eq!(
            terminal_events,
            [(crate::state::SetupChainOutcome::Stopped, None, true)]
        );
        assert!(
            crate::worktree::get_worktree_setup_status(
                &state,
                &repo.to_string_lossy(),
                "removed-chain"
            )
            .is_none(),
            "a removed workspace has no setup outcome to poll"
        );
    }

    /// A stale chain for a `(repo, branch)` that was removed and created again
    /// must neither wipe nor overwrite the new chain's status, nor emit the
    /// terminal event the new creation's waiter is listening for.
    #[tokio::test]
    async fn a_superseded_chain_leaves_the_new_chains_status_and_waiter_alone() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let repo_path = repo.to_string_lossy().into_owned();
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();
        let old_token = crate::worktree::begin_warm(&destination);
        let (entered, wait_entered) = std::sync::mpsc::channel::<()>();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();

        // The old chain blocks inside its warm...
        let stale = tokio::spawn(crate::worktree::run_worktree_setup_chain(
            Arc::clone(&state),
            repo_path.clone(),
            "recreated".into(),
            destination.clone(),
            Some((
                old_token,
                move |_: &std::path::Path, _: &std::path::Path| {
                    entered.send(()).unwrap();
                    wait_release.recv().unwrap();
                    crate::cow::WarmingReport::default()
                },
            )),
        ));
        tokio::task::spawn_blocking(move || wait_entered.recv().unwrap())
            .await
            .unwrap();
        // ...while the workspace is removed and created again.
        let new_token = crate::worktree::begin_warm(&destination);
        let fresh = crate::worktree::spawn_worktree_setup_chain(
            &state,
            repo_path.clone(),
            "recreated".into(),
            destination.clone(),
            None,
        );
        fresh.await.unwrap();
        let mut outcomes = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let crate::state::AppEvent::WorktreeSetupScriptCompleted { outcome, .. } = event {
                outcomes.push(outcome);
            }
        }
        assert_eq!(outcomes, [crate::state::SetupChainOutcome::NotConfigured]);

        release.send(()).unwrap();
        stale.await.unwrap();

        assert_eq!(
            crate::worktree::get_worktree_setup_status(&state, &repo_path, "recreated"),
            Some(crate::state::WorktreeSetupStatus::NotConfigured),
            "the stale chain wiped or overwrote the new chain's status"
        );
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(
                    event,
                    crate::state::AppEvent::WorktreeSetupScriptCompleted { .. }
                ),
                "the stale chain emitted a terminal event for the new workspace"
            );
        }
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        crate::worktree::finish_warm(
            &destination,
            new_token,
            serde_json::json!({"status": "done"}),
        );
        crate::worktree::clear_warm(&destination);
    }

    /// The warm opt-out applies inside the chain, so every creation path that
    /// hands it a token honours it: the copy is skipped, the token still
    /// decides whether the chain may continue, and the status says why.
    #[tokio::test]
    async fn a_disabled_warm_skips_the_copy_but_still_finishes_the_chain() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        save_repo_entry(
            &repo,
            crate::config::RepoSettingsEntry {
                warm_ignored_directories: Some(false),
                ..Default::default()
            },
        );
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();

        crate::worktree::run_worktree_setup_chain(
            Arc::clone(&state),
            repo.to_string_lossy().into_owned(),
            "no-warm".into(),
            destination.clone(),
            Some((
                token,
                |_: &std::path::Path, _: &std::path::Path| -> crate::cow::WarmingReport {
                    panic!("warming is disabled for this repo")
                },
            )),
        )
        .await;

        let status = crate::worktree::warm_status(&destination);
        assert_eq!(status["status"], "done", "{status}");
        assert!(
            status["skipped"]
                .as_str()
                .is_some_and(|s| s.contains("disabled")),
            "{status}"
        );
        let mut saw_end = false;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, crate::state::AppEvent::WorktreeWarmStarted { .. }),
                "a disabled warm must stay silent"
            );
            if let crate::state::AppEvent::WorktreeSetupScriptCompleted { outcome, .. } = event {
                assert_eq!(outcome, crate::state::SetupChainOutcome::NotConfigured);
                saw_end = true;
            }
        }
        assert!(saw_end, "the chain must still report its end");
        crate::worktree::clear_warm(&destination);
    }

    /// `warm_with_events` turns a reporting warm into the `worktree-warm-*`
    /// events and the pending `warm_artifacts` detail a poller sees.
    #[tokio::test]
    async fn warm_progress_is_published_as_events_and_pending_detail() {
        let temp = tempfile::TempDir::new().unwrap();
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut events = state.event_bus.subscribe();
        let probe = destination.clone();

        let warm = crate::worktree::warm_with_events(
            Arc::clone(&state),
            "/repo".into(),
            "feat".into(),
            token,
            move |_, _, on_started, on_progress| {
                on_started(2);
                assert_eq!(
                    crate::worktree::warm_status(&probe),
                    serde_json::json!({"status": "pending", "phase": "warming", "copied": 0, "total": 2})
                );
                on_progress(1, 2, std::path::Path::new("node_modules"));
                on_progress(2, 2, std::path::Path::new("target"));
                assert_eq!(crate::worktree::warm_status(&probe)["copied"], 2);
                crate::cow::WarmingReport {
                    warmed: 2,
                    warnings: Vec::new(),
                }
            },
        );
        let report = warm(std::path::Path::new("/repo"), &destination);
        assert_eq!(report.warmed, 2);

        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            match event {
                crate::state::AppEvent::WorktreeWarmStarted {
                    total,
                    worktree_path,
                    ..
                } => {
                    assert_eq!(worktree_path, destination.to_string_lossy());
                    seen.push(format!("started:{total}"));
                }
                crate::state::AppEvent::WorktreeWarmProgress {
                    copied, current, ..
                } => {
                    seen.push(format!("progress:{copied}:{}", current.unwrap_or_default()));
                }
                crate::state::AppEvent::WorktreeWarmCompleted { warmed, .. } => {
                    seen.push(format!("completed:{warmed}"));
                }
                _ => {}
            }
        }
        // The first progress tick may be throttled; the last one never is.
        assert_eq!(seen.first().map(String::as_str), Some("started:2"));
        assert_eq!(seen.last().map(String::as_str), Some("completed:2"));
        assert!(seen.contains(&"progress:2:target".to_string()), "{seen:?}");
        crate::worktree::clear_warm(&destination);
    }

    #[tokio::test]
    async fn a_slow_warm_keeps_the_returned_workspace_pending_until_it_finishes() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("workspace");
        let (release, wait) = std::sync::mpsc::channel::<()>();

        let token = crate::worktree::begin_warm(&destination);
        let task = crate::worktree::spawn_background_warm(
            source,
            destination.clone(),
            token,
            move |_, _| {
                wait.recv().unwrap();
                crate::cow::WarmingReport::default()
            },
        );

        assert!(!task.is_finished());
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        release.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(crate::worktree::warm_status(&destination)["status"], "done");
        crate::worktree::clear_warm(&destination);
    }
}

#[cfg(test)]
mod survivor_tests {
    use super::*;

    async fn json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Catches: force confirmation predicates rejecting non-force/confirmed requests or allowing unconfirmed force.
    #[tokio::test]
    async fn removal_confirmation_accepts_only_the_required_force_combinations() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for (force, fingerprint, missing, expected) in [
            (false, None, false, StatusCode::INTERNAL_SERVER_ERROR),
            (
                true,
                Some("confirmed"),
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (true, None, true, StatusCode::INTERNAL_SERVER_ERROR),
            (true, None, false, StatusCode::BAD_REQUEST),
        ] {
            let response = remove_worktree_http(
                State(state.clone()),
                Path("unknown-workspace".into()),
                Query(RemoveWorktreeQuery {
                    repo_path: temp.path().to_string_lossy().into_owned(),
                    force: Some(force),
                    delete_branch: Some(false),
                    override_lock: None,
                    expected_fingerprint: fingerprint.map(str::to_owned),
                    confirm_missing_checkout: Some(missing),
                    override_busy: None,
                }),
            )
            .await;
            assert_eq!(
                response.status(),
                expected,
                "{force} {fingerprint:?} {missing}"
            );
            assert!(json(response).await["error"].is_string());
        }
    }

    /// Catches: `DELETE /worktrees/{id}` removing a CLEAN worktree a live
    /// session still works in (wip 9586bf02c; Batch 8 review), the refusal
    /// not being a 409 carrying the sessions, and `overrideBusy` not lifting it.
    #[cfg(unix)]
    #[tokio::test]
    async fn removal_refuses_a_live_worktree_with_409_until_override_busy() {
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = tuic_git::test_fixtures::setup_test_repo();
        let worktree = tuic_git::test_fixtures::worktree_with(repo.path(), "busy", false);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session(&state, "pty-http");
        crate::state::tests_support::set_session_cwd(
            &state,
            "pty-http",
            &worktree.to_string_lossy(),
        );
        let query = |override_busy: Option<bool>| {
            Query(RemoveWorktreeQuery {
                repo_path: repo.path().to_string_lossy().into_owned(),
                force: None,
                delete_branch: Some(false),
                override_lock: None,
                expected_fingerprint: None,
                confirm_missing_checkout: None,
                override_busy,
            })
        };

        let refused =
            remove_worktree_http(State(state.clone()), Path("busy".into()), query(None)).await;
        assert_eq!(refused.status(), StatusCode::CONFLICT);
        let body = json(refused).await;
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|e| e.starts_with(crate::worktree::BUSY_WORKTREE_PREFIX)),
            "{body}"
        );
        assert_eq!(body["code"], "worktree_busy");
        assert_eq!(body["live_sessions"][0]["session_id"], "pty-http");
        assert!(worktree.exists());

        let removed =
            remove_worktree_http(State(state.clone()), Path("busy".into()), query(Some(true)))
                .await;
        assert_eq!(removed.status(), StatusCode::OK);
        assert!(!worktree.exists());
    }

    /// Catches: empty repoPath falling through to generic path validation instead of the required-field error.
    #[tokio::test]
    async fn orphan_assessment_empty_repo_reports_required_field() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = assess_orphan_cleanup_http(
            State(state),
            Query(OptionalRepoQuery {
                repo_path: Some(String::new()),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json(response).await,
            serde_json::json!({"error": "repoPath required"})
        );
    }

    /// Catches: lifecycle handler returning an empty success instead of a removal preflight document.
    #[tokio::test]
    async fn lifecycle_unknown_workspace_returns_a_preflight_document() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = workspace_lifecycle_http(
            State(state),
            Query(WorkspaceIdQuery {
                repo_path: temp.path().to_string_lossy().into_owned(),
                workspace_id: "unknown".into(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert!(body.is_object(), "{body}");
        assert!(!body.as_object().unwrap().is_empty());
    }

    /// Catches: dropped keep/remove decisions and no-op clear handlers leaving a dialog unanswered.
    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_decisions_and_clear_are_visible_to_other_clients() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = super::warm_tests::setup_repo(temp.path());
        let orphan = temp.path().join("orphan");
        crate::git_cli::git_cmd(&repo)
            .args(["worktree", "add", "--detach", orphan.to_str().unwrap()])
            .run()
            .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().into_owned();
        for (decision, expected) in [("keep", false), ("remove", true)] {
            crate::worktree::begin_orphan_cleanup_internal(
                &state,
                &repo_path,
                vec![orphan.to_string_lossy().into_owned()],
            )
            .unwrap();
            let response = answer_orphan_cleanup_http(
                State(state.clone()),
                Json(AnswerOrphanCleanupRequest {
                    repo_path: repo_path.clone(),
                    decision: decision.into(),
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(json(response).await, serde_json::json!({"ok": true}));
            let pending = pending_orphan_cleanup_http(
                State(state.clone()),
                Query(OptionalRepoQuery {
                    repo_path: Some(repo_path.clone()),
                }),
            )
            .await;
            assert_eq!(json(pending).await, serde_json::json!(expected));
            let response = clear_orphan_cleanup_http(
                State(state.clone()),
                Json(ClearOrphanCleanupRequest {
                    repo_path: repo_path.clone(),
                    kept: false,
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(json(response).await, serde_json::Value::Null);
            let pending = pending_orphan_cleanup_http(
                State(state.clone()),
                Query(OptionalRepoQuery {
                    repo_path: Some(repo_path.clone()),
                }),
            )
            .await;
            assert_eq!(json(pending).await, serde_json::Value::Null);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    #[serial_test::serial]
    async fn create_worktree_shared_runs_the_file_sync_before_the_setup_script() {
        // Inverted version of the pre-fix pin (see git history for what it
        // asserted): worktree.rs's spawn_worktree_setup_chain now awaits the
        // CoW warm and then the file sync before resolving/running the setup
        // script, so a script depending on a copy_ignored_files-synced file
        // always sees it. All steps run in the background after
        // create_worktree_shared has already returned — hence the polling loop
        // below instead of a synchronous assertion on the response.
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = super::warm_tests::setup_repo(temp.path());
        // An ignored file in the source repo — copy_ignored_files is what the
        // sync would carry into the new worktree.
        std::fs::write(repo.join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        std::fs::write(repo.join("ignored.txt"), "secret-config").expect("write ignored");
        crate::git_cli::git_cmd(&repo)
            .args(["add", ".gitignore"])
            .run()
            .expect("git add .gitignore");
        crate::git_cli::git_cmd(&repo)
            .args(["commit", "-m", "add gitignore"])
            .run()
            .expect("git commit");

        let marker = temp.path().join("order-check.txt");
        crate::config::save_repo_settings(
            crate::config::RepoSettingsMap::default(),
            crate::config::RepoSettingsMap {
                repos: [(
                    repo.to_string_lossy().to_string(),
                    crate::config::RepoSettingsEntry {
                        path: repo.to_string_lossy().to_string(),
                        copy_ignored_files: Some(true),
                        setup_script: Some(format!(
                            "if [ -f ignored.txt ]; then echo present > {}; else echo missing > {}; fi",
                            marker.display(),
                            marker.display()
                        )),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
            },
        )
        .expect("save repo settings");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        create_worktree_shared(
            &state,
            repo.to_string_lossy().to_string(),
            "order-test-branch".to_string(),
            None,
        )
        .await
        .expect("worktree should be created");

        // The warm, sync and setup script run in a background chain, after
        // create_worktree_shared has already returned — poll for the marker
        // rather than asserting on it synchronously.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut outcome = None;
        while std::time::Instant::now() < deadline {
            if let Ok(content) = std::fs::read_to_string(&marker) {
                outcome = Some(content);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let outcome =
            outcome.expect("setup script should have run in the background within the timeout");
        assert_eq!(
            outcome.trim(),
            "present",
            "the setup script must see the file the sync copied in, now that the \
             background chain awaits the sync before running the script"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_worktree_shared_tracks_setup_status_through_to_completed() {
        // Closes the observability gap left by the background chain no longer
        // returning setup_script/setup_script_error synchronously — an MCP
        // client has no event stream, so it must be able to poll instead.
        let repo = crate::state::tests_support::create_temp_git_repo();
        let _guard = crate::config::set_config_dir_override(repo.path().join("tuic-config"));
        crate::config::save_repo_settings(
            crate::config::RepoSettingsMap::default(),
            crate::config::RepoSettingsMap {
                repos: [(
                    repo.path().to_string_lossy().to_string(),
                    crate::config::RepoSettingsEntry {
                        path: repo.path().to_string_lossy().to_string(),
                        setup_script: Some("exit 0".to_string()),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
            },
        )
        .expect("save repo settings");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        create_worktree_shared(
            &state,
            repo.path().to_string_lossy().to_string(),
            "status-test-branch".to_string(),
            None,
        )
        .await
        .expect("worktree should be created");

        // spawn_worktree_setup_chain inserts a status synchronously before
        // create_worktree_shared returns — must never still be "untracked" at
        // this point, whether or not the script has finished yet.
        let immediate = crate::worktree::get_worktree_setup_status(
            &state,
            repo.path().to_string_lossy().as_ref(),
            "status-test-branch",
        );
        assert!(
            immediate.is_some(),
            "status must be tracked synchronously, before the background chain finishes"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut final_status = None;
        while std::time::Instant::now() < deadline {
            match crate::worktree::get_worktree_setup_status(
                &state,
                repo.path().to_string_lossy().as_ref(),
                "status-test-branch",
            ) {
                Some(s @ crate::state::WorktreeSetupStatus::Completed { .. }) => {
                    final_status = Some(s);
                    break;
                }
                _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        assert_eq!(
            final_status,
            Some(crate::state::WorktreeSetupStatus::Completed {
                exit_code: Some(0),
                error: None,
            })
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn create_worktree_shared_tracks_not_configured_when_no_setup_script() {
        let repo = crate::state::tests_support::create_temp_git_repo();
        let _guard = crate::config::set_config_dir_override(repo.path().join("tuic-config"));

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        create_worktree_shared(
            &state,
            repo.path().to_string_lossy().to_string(),
            "no-script-branch".to_string(),
            None,
        )
        .await
        .expect("worktree should be created");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut final_status = None;
        while std::time::Instant::now() < deadline {
            match crate::worktree::get_worktree_setup_status(
                &state,
                repo.path().to_string_lossy().as_ref(),
                "no-script-branch",
            ) {
                Some(s @ crate::state::WorktreeSetupStatus::NotConfigured) => {
                    final_status = Some(s);
                    break;
                }
                _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        assert_eq!(
            final_status,
            Some(crate::state::WorktreeSetupStatus::NotConfigured),
            "must settle on NotConfigured, never linger as Running forever, when no script is configured"
        );
    }

    #[test]
    fn get_worktree_setup_status_is_none_for_an_untracked_pair() {
        let state = crate::state::tests_support::make_test_app_state();
        assert_eq!(
            crate::worktree::get_worktree_setup_status(&state, "/never/tracked", "some-branch"),
            None
        );
    }

    // --- run_setup_script_http: the IPC/HTTP parity route ---

    fn loopback() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }
    fn lan() -> SocketAddr {
        "192.168.1.2:1".parse().unwrap()
    }
    fn authed() -> Option<Extension<Authenticated>> {
        Some(Extension(Authenticated))
    }

    #[tokio::test]
    async fn run_setup_script_http_rejects_unauthenticated_non_loopback() {
        // This route is arbitrary shell execution — must never become
        // LAN-reachable without authentication.
        let resp = run_setup_script_http(
            ConnectInfo(lan()),
            None,
            Json(RunSetupScriptRequest {
                script: "echo hi".to_string(),
                cwd: "/tmp".to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn run_setup_script_http_rejects_a_relative_or_traversing_cwd() {
        for cwd in ["relative/dir", "/tmp/../etc"] {
            let resp = run_setup_script_http(
                ConnectInfo(loopback()),
                None,
                Json(RunSetupScriptRequest {
                    script: "echo hi".to_string(),
                    cwd: cwd.to_string(),
                }),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{cwd}");
        }
    }

    #[tokio::test]
    async fn run_setup_script_http_loopback_passes_guard() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp dir");
        let resp = run_setup_script_http(
            ConnectInfo(loopback()),
            None,
            Json(RunSetupScriptRequest {
                script: "echo hi".to_string(),
                cwd: dir.path().to_string_lossy().to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn run_setup_script_http_authenticated_remote_passes_guard() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp dir");
        let resp = run_setup_script_http(
            ConnectInfo(lan()),
            authed(),
            Json(RunSetupScriptRequest {
                script: "echo hi".to_string(),
                cwd: dir.path().to_string_lossy().to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn run_setup_script_http_returns_the_same_shape_as_the_tauri_command() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp dir");
        let resp = run_setup_script_http(
            ConnectInfo(loopback()),
            None,
            Json(RunSetupScriptRequest {
                script: "echo hello".to_string(),
                cwd: dir.path().to_string_lossy().to_string(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json(resp).await;
        assert_eq!(body["exit_code"], 0);
        assert_eq!(body["stdout"].as_str().unwrap().trim(), "hello");
        assert_eq!(body["stderr"], "");
        // Exactly these three keys — no extra fields the frontend/`transport.ts`
        // mapping (which passes this response through with no `transform`)
        // wouldn't know about.
        let obj = body.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["exit_code", "stderr", "stdout"]);
    }

    #[tokio::test]
    async fn post_worktrees_run_script_does_not_match_the_branch_delete_route() {
        // Adjacency guard: /worktrees/run-script (POST, static segment) and
        // /worktrees/{branch} (DELETE, single dynamic segment) coexist in the
        // same axum router. axum 0.8's matchit prioritises static segments
        // over dynamic ones, so this should never actually collide — but the
        // two are similar enough (same prefix, one segment deep) that a
        // future refactor could get this wrong silently. A minimal router
        // registering both real handlers, not the full app (which needs
        // mod.rs's private test helpers) — routing behavior is a property of
        // the route table, not of auth/state wiring, so this is self-contained.
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mini_router = axum::Router::new()
            .route(
                "/worktrees/run-script",
                axum::routing::post(run_setup_script_http),
            )
            .route(
                "/worktrees/{branch}",
                axum::routing::delete(remove_worktree_http),
            )
            .with_state(state);

        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("temp dir");
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/worktrees/run-script")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({
                    "script": "echo hi",
                    "cwd": dir.path().to_string_lossy(),
                })
                .to_string(),
            ))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(loopback()));

        let response = mini_router.oneshot(request).await.unwrap();
        // Must not be routed as a DELETE-only /{branch} match producing 405,
        // and must not 404 — it should reach run_setup_script_http and succeed.
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn run_setup_script_http_rejects_malformed_json_body() {
        // Boundary/corrupt-data case: run_setup_script_http's Json<RunSetupScriptRequest>
        // extractor can't be exercised by calling the handler function directly with a
        // hand-built struct (the compiler would force every field to exist) — a genuinely
        // malformed/incomplete wire body only surfaces axum's own extraction rejection
        // when it goes through the real router, hence the same mini-router as the
        // adjacency test above rather than a direct handler call.
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mini_router = axum::Router::new()
            .route(
                "/worktrees/run-script",
                axum::routing::post(run_setup_script_http),
            )
            .with_state(state);

        // Missing the required "cwd" field entirely.
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/worktrees/run-script")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({"script": "echo hi"}).to_string(),
            ))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(loopback()));
        let response = mini_router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Not valid JSON at all.
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/worktrees/run-script")
            .header("content-type", "application/json")
            .body(axum::body::Body::from("not json"))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(loopback()));
        let response = mini_router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // --- get_worktree_setup_status_http: closes the MCP observability gap ---

    /// A removal that wins against the queued warm stops the chain; the
    /// pollable status must then not linger as `running` for a workspace that
    /// no longer exists — it reads `unknown`.
    #[tokio::test]
    async fn a_chain_stopped_by_removal_drops_its_setup_status() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let token = crate::worktree::begin_warm(&destination);
        crate::worktree::clear_warm(&destination);
        let repo = temp.path().to_string_lossy().into_owned();
        crate::worktree::spawn_worktree_setup_chain(
            &state,
            repo.clone(),
            "removed".into(),
            destination,
            Some(token),
        )
        .await
        .unwrap();
        assert_eq!(
            crate::worktree::get_worktree_setup_status(&state, &repo, "removed"),
            None
        );
    }

    /// An aborted chain reports the stop instead of `running` forever.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_aborted_chain_reports_completed_with_an_error() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let gate = temp.path().join("release");
        let started = temp.path().join("started");
        let finished = temp.path().join("finished");
        let mut defaults = crate::config::RepoDefaultsConfig::default();
        defaults.setup_script = format!(
            "echo s > '{}'; while [ ! -f '{}' ]; do sleep 0.02; done; echo f > '{}'",
            started.display(),
            gate.display(),
            finished.display()
        );
        crate::config::save_repo_defaults(crate::config::RepoDefaultsConfig::default(), defaults)
            .unwrap();
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo = temp.path().to_string_lossy().into_owned();
        let chain = crate::worktree::spawn_worktree_setup_chain(
            &state,
            repo.clone(),
            "aborted".into(),
            destination,
            None,
        );
        assert_eq!(
            crate::worktree::get_worktree_setup_status(&state, &repo, "aborted"),
            Some(crate::state::WorktreeSetupStatus::Running)
        );
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while !started.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("setup script did not start");
        chain.abort();
        let _ = chain.await;
        assert!(matches!(
            crate::worktree::get_worktree_setup_status(&state, &repo, "aborted"),
            Some(crate::state::WorktreeSetupStatus::Completed {
                exit_code: None,
                error: Some(_)
            })
        ));
        // Let the detached script finish before `temp` (and the gate) is
        // deleted, or it would loop forever and stall runtime shutdown.
        std::fs::write(&gate, "go").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while !finished.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("setup script did not finish");
    }

    #[tokio::test]
    async fn get_worktree_setup_status_http_returns_unknown_for_an_untracked_pair() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = get_worktree_setup_status_http(
            State(state),
            Query(WorktreeSetupStatusQuery {
                repo_path: "/never/tracked".to_string(),
                branch: "some-branch".to_string(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["state"], "unknown");
    }

    #[tokio::test]
    async fn get_worktree_setup_status_http_returns_the_tracked_status() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.worktree_setup_status.insert(
            ("/repo".to_string(), "feat-x".to_string()),
            Arc::new(crate::state::WorktreeSetupStatus::Completed {
                exit_code: Some(1),
                error: None,
            }),
        );

        let response = get_worktree_setup_status_http(
            State(state),
            Query(WorktreeSetupStatusQuery {
                repo_path: "/repo".to_string(),
                branch: "feat-x".to_string(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["state"], "completed");
        assert_eq!(body["exit_code"], 1);
        assert!(body["error"].is_null());
    }

    #[tokio::test]
    async fn get_worktree_setup_status_http_rejects_an_invalid_repo_path() {
        // Boundary/corrupt-data case: validate_repo_path must run before any
        // cache lookup, the same as every other route in this file.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = get_worktree_setup_status_http(
            State(state),
            Query(WorktreeSetupStatusQuery {
                repo_path: "not-an-absolute-path".to_string(),
                branch: "main".to_string(),
            }),
        )
        .await;
        assert_ne!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn get_worktree_setup_status_http_rejects_a_missing_query_param() {
        // Boundary/corrupt-data case, mirroring run_setup_script_http_rejects_malformed_json_body:
        // axum's own Query<WorktreeSetupStatusQuery> extraction rejection can't be
        // exercised by calling the handler directly with a hand-built struct (the
        // compiler forces every field to exist) — only a real request through the
        // router hits axum's own "missing required query param" rejection.
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mini_router = axum::Router::new()
            .route(
                "/worktrees/setup-status",
                axum::routing::get(get_worktree_setup_status_http),
            )
            .with_state(state);

        // Missing the required "branch" query param entirely.
        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/worktrees/setup-status?repoPath=%2Frepo")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = mini_router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_worktree_setup_status_http_reports_running_and_not_configured_too() {
        // The other two states aren't just theoretical — assert their literal
        // JSON shape flows through this route unchanged, not just "completed"
        // and "unknown" (already covered above). The exhaustive shape itself is
        // locked once in state.rs's own serialization test; this only proves
        // the route doesn't do anything unexpected to it in transit.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.worktree_setup_status.insert(
            ("/repo".to_string(), "running-branch".to_string()),
            Arc::new(crate::state::WorktreeSetupStatus::Running),
        );
        state.worktree_setup_status.insert(
            ("/repo".to_string(), "no-script-branch".to_string()),
            Arc::new(crate::state::WorktreeSetupStatus::NotConfigured),
        );

        let running = get_worktree_setup_status_http(
            State(state.clone()),
            Query(WorktreeSetupStatusQuery {
                repo_path: "/repo".to_string(),
                branch: "running-branch".to_string(),
            }),
        )
        .await;
        assert_eq!(json(running).await["state"], "running");

        let not_configured = get_worktree_setup_status_http(
            State(state),
            Query(WorktreeSetupStatusQuery {
                repo_path: "/repo".to_string(),
                branch: "no-script-branch".to_string(),
            }),
        )
        .await;
        assert_eq!(json(not_configured).await["state"], "not_configured");
    }

    #[tokio::test]
    async fn get_worktrees_setup_status_does_not_match_the_branch_delete_route() {
        // Adjacency guard, mirroring post_worktrees_run_script_does_not_match_the_branch_delete_route
        // above: /worktrees/setup-status (GET, static segment) and
        // /worktrees/{branch} (DELETE, single dynamic segment) coexist in the
        // same router.
        use tower::ServiceExt;

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mini_router = axum::Router::new()
            .route(
                "/worktrees/setup-status",
                axum::routing::get(get_worktree_setup_status_http),
            )
            .route(
                "/worktrees/{branch}",
                axum::routing::delete(remove_worktree_http),
            )
            .with_state(state);

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/worktrees/setup-status?repoPath=%2Frepo&branch=feat-x")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = mini_router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["state"], "unknown");
    }
}
