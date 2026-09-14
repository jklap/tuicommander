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

    // The setup script (if configured) is no longer run inline here — it's
    // chained after the file sync in the background by create_worktree_shared
    // (spawn_worktree_setup_chain), reported via the dual-emitted
    // `worktree-setup-script-completed` event rather than this response, so
    // ordering against the sync is guaranteed regardless of how long either
    // step takes.
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

/// HTTP counterpart of the `run_setup_script` Tauri command — closes a real
/// IPC/HTTP parity gap: `transport.ts` already mapped this command to
/// `POST /worktrees/run-script`, but no such route existed
/// (`transport.test.ts`'s `KNOWN_HTTP_MAPPING_GAPS` listed it).
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
    if let Err(e) = validate_repo_path(&body.cwd) {
        return e.into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::run_setup_script(body.script, body.cwd)
    })
    .await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
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
        crate::worktree::create_workspace(&worktrees_dir_bg, &config_bg, base_ref.as_deref())
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
                kind: workspace.kind,
            });
            // File sync, then (only once it's done) the setup script — see
            // spawn_worktree_setup_chain's doc comment for why the two must
            // be sequenced, and why this response no longer carries
            // setup_script/setup_script_error inline (reported later via the
            // dual-emitted worktree-setup-script-completed event instead).
            crate::worktree::spawn_worktree_setup_chain(
                state,
                base_repo.clone(),
                branch_name.clone(),
                workspace.path.clone(),
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
    let delete_branch = q.delete_branch.unwrap_or(true);
    let force = q.force.unwrap_or(false);
    let override_busy = q.override_busy.unwrap_or(false);
    let mode = if force {
        crate::worktree::RemovalMode::Forced
    } else {
        crate::worktree::RemovalMode::Safe
    };
    let id_for_event = workspace_id.clone();
    let state_arc = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        // `resolve_archive_script` — the Tauri command and MCP transport both
        // run the pre-removal archive script; this HTTP route previously
        // skipped it (passed `None`), a transport-parity drift.
        let script = crate::worktree::resolve_archive_script(&repo_path);
        crate::worktree::remove_worktree_by_workspace_id(
            &repo_path,
            &workspace_id,
            delete_branch,
            script.as_deref(),
            mode,
            Some(&state_arc),
            override_busy,
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

/// Fresh linked-worktree removal preflight.
pub(super) async fn workspace_lifecycle_http(Query(q): Query<WorkspaceIdQuery>) -> Response {
    if let Err(error) = validate_repo_path(&q.repo_path) {
        return error.into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::inspect_workspace_lifecycle(
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

    #[tokio::test]
    #[serial_test::serial]
    async fn create_worktree_shared_runs_the_file_sync_before_the_setup_script() {
        // Inverted version of the pre-fix pin (see git history for what it
        // asserted): worktree.rs's spawn_worktree_setup_chain now awaits the
        // file sync before resolving/running the setup script, so a script
        // depending on a copy_ignored_files-synced file always sees it.
        // Both steps run in the background after create_worktree_shared has
        // already returned — hence the polling loop below instead of a
        // synchronous assertion on the response.
        let repo = create_temp_git_repo();
        // An ignored file in the source repo — copy_ignored_files is what the
        // sync would carry into the new worktree.
        std::fs::write(repo.path().join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        std::fs::write(repo.path().join("ignored.txt"), "secret-config").expect("write ignored");
        std::process::Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(repo.path())
            .output()
            .expect("git add .gitignore");
        std::process::Command::new("git")
            .args(["commit", "-m", "add gitignore"])
            .current_dir(repo.path())
            .output()
            .expect("git commit");

        let marker = repo.path().join("order-check.txt");
        let _guard = crate::config::set_config_dir_override(repo.path().join("tuic-config"));
        crate::config::save_repo_settings(crate::config::RepoSettingsMap {
            repos: [(
                repo.path().to_string_lossy().to_string(),
                crate::config::RepoSettingsEntry {
                    path: repo.path().to_string_lossy().to_string(),
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
        })
        .expect("save repo settings");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        create_worktree_shared(
            &state,
            repo.path().to_string_lossy().to_string(),
            "order-test-branch".to_string(),
            None,
        )
        .await
        .expect("worktree should be created");

        // The sync + setup script now run in a background chain, after
        // create_worktree_shared has already returned — poll for the marker
        // rather than asserting on it synchronously.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
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
    async fn run_setup_script_http_loopback_passes_guard() {
        let dir = tempfile::TempDir::new().expect("temp dir");
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
        let dir = tempfile::TempDir::new().expect("temp dir");
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
        let dir = tempfile::TempDir::new().expect("temp dir");
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
        let body = response_json(resp).await;
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

        let dir = tempfile::TempDir::new().expect("temp dir");
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
}
