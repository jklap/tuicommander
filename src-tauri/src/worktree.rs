use crate::git_cli::{FETCH_TIMEOUT, finish_failed_git_operation_after_abort, git_cmd};
use crate::state::{AppState, WorktreeInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::State;

/// Resolve the effective archive_script for a repo from the three-tier config:
/// per-repo settings → repo-local .tuic.json → global defaults.
/// Returns None if no script is configured at any level.
pub(crate) fn resolve_archive_script(repo_path: &str) -> Option<String> {
    // 1. Per-repo app settings (highest priority)
    let repo_settings = crate::config::load_repo_settings();
    if let Some(entry) = repo_settings.repos.get(repo_path)
        && let Some(ref script) = entry.archive_script
        && !script.is_empty()
    {
        return Some(script.clone());
    }
    // .tuic.json scripts intentionally skipped — executing repo-committed
    // scripts without TOFU prompt is unsafe. Re-add when trust-on-first-use
    // confirmation is implemented.
    // 2. Global repo defaults (lowest priority)
    let defaults = crate::config::load_repo_defaults();
    if !defaults.archive_script.is_empty() {
        return Some(defaults.archive_script);
    }
    None
}

/// Kick off a background copy of ignored/untracked/explicit-listed files into
/// a freshly created worktree. Resolves the repo's effective copy settings
/// from disk (`config::resolve_effective_copy_settings`) itself, so a
/// worktree created from the desktop app and one created via the MCP HTTP
/// path (no frontend in the loop) sync identically.
///
/// Fire-and-forget: returns immediately. A repo with both `copy_ignored`/
/// `copy_untracked` off and an empty `copy_paths` is a no-op with **no**
/// events at all — a plain worktree creation never shows a sync toast.
/// Progress/completion are reported via dual-emitted (event_bus + Tauri
/// window) `worktree-sync-*` events; see `state.rs`'s `AppEvent::WorktreeSync*`
/// and `sse_routes.rs` for the SSE side.
///
/// KNOWN, ACCEPTED ORDERING GAP: this is deliberately unsequenced against the
/// setup script (`resolve_effective_setup_script` / `run_setup_script`) —
/// that's what "runs in the background, doesn't block worktree creation"
/// (the explicitly requested design) means. A setup script that depends on a
/// synced file (e.g. a `copy_paths` entry symlinking `node_modules` so `npm
/// install` can skip, or a script reading a synced `.env`) can race ahead of
/// the sync and run without it. Making the two wait on each other would
/// reintroduce the blocking behavior this was built to avoid; if that
/// tradeoff ever needs revisiting, the fix is to await this function's
/// summary before running the setup script, not to make the sync
/// synchronous.
pub(crate) fn spawn_worktree_file_sync(
    state: &Arc<AppState>,
    base_repo: &str,
    branch: &str,
    dest_path: &Path,
) {
    let (copy_ignored, copy_untracked, copy_paths) =
        crate::config::resolve_effective_copy_settings(base_repo);
    if !copy_ignored && !copy_untracked && copy_paths.is_empty() {
        return;
    }

    let state = Arc::clone(state);
    let source = PathBuf::from(base_repo);
    let dest = dest_path.to_path_buf();
    let repo_path = base_repo.to_string();
    let branch = branch.to_string();
    let explicit = crate::worktree_sync::specs_from_copy_path_entries(&copy_paths);

    tokio::spawn(async move {
        emit_worktree_sync_started(&state, &repo_path, &branch);

        let repo_path_progress = repo_path.clone();
        let branch_progress = branch.clone();
        let state_progress = Arc::clone(&state);

        let summary = tokio::task::spawn_blocking(move || {
            let specs = crate::worktree_sync::build_sync_specs(
                &source,
                copy_ignored,
                copy_untracked,
                &explicit,
            );
            // Throttled: a large ignored tree (e.g. node_modules) can be
            // thousands of entries — emit at most ~once every 150ms, plus
            // always on the final entry so completion isn't preceded by a
            // stale progress count.
            let mut last_emit = std::time::Instant::now();
            crate::worktree_sync::sync_paths(&source, &dest, &specs, move |copied, total| {
                let now = std::time::Instant::now();
                if copied == total || now.duration_since(last_emit).as_millis() >= 150 {
                    last_emit = now;
                    emit_worktree_sync_progress(
                        &state_progress,
                        &repo_path_progress,
                        &branch_progress,
                        copied,
                        total,
                    );
                }
            })
        })
        .await
        .unwrap_or_else(|e| crate::worktree_sync::SyncSummary {
            copied: 0,
            total: 0,
            errors: vec![format!("sync task panicked: {e}")],
        });

        emit_worktree_sync_completed(&state, &repo_path, &branch, &summary);
    });
}

fn emit_worktree_sync_started(state: &Arc<AppState>, repo_path: &str, branch: &str) {
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::WorktreeSyncStarted {
            repo_path: repo_path.to_string(),
            branch: branch.to_string(),
        });
    #[cfg(feature = "desktop")]
    if let Some(handle) = state.app_handle.read().as_ref() {
        use tauri::Emitter as _;
        let _ = handle.emit(
            "worktree-sync-started",
            serde_json::json!({ "repoPath": repo_path, "branch": branch }),
        );
    }
}

fn emit_worktree_sync_progress(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    copied: usize,
    total: usize,
) {
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::WorktreeSyncProgress {
            repo_path: repo_path.to_string(),
            branch: branch.to_string(),
            copied,
            total,
        });
    #[cfg(feature = "desktop")]
    if let Some(handle) = state.app_handle.read().as_ref() {
        use tauri::Emitter as _;
        let _ = handle.emit(
            "worktree-sync-progress",
            serde_json::json!({ "repoPath": repo_path, "branch": branch, "copied": copied, "total": total }),
        );
    }
}

fn emit_worktree_sync_completed(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    summary: &crate::worktree_sync::SyncSummary,
) {
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::WorktreeSyncCompleted {
            repo_path: repo_path.to_string(),
            branch: branch.to_string(),
            copied: summary.copied,
            total: summary.total,
            errors: summary.errors.clone(),
        });
    #[cfg(feature = "desktop")]
    if let Some(handle) = state.app_handle.read().as_ref() {
        use tauri::Emitter as _;
        let _ = handle.emit(
            "worktree-sync-completed",
            serde_json::json!({
                "repoPath": repo_path,
                "branch": branch,
                "copied": summary.copied,
                "total": summary.total,
                "errors": summary.errors,
            }),
        );
    }
}

/// Classification of a failed `git worktree add` based on its stderr, used to
/// decide how `create_worktree_internal` should recover.
///
/// Git emits several distinct "already exists" failures from `worktree add` and
/// they require different handling — a single `contains("already exists")` guard
/// conflates them and can swallow a hard failure as success.
#[derive(Debug, PartialEq, Eq)]
enum WorktreeAddFailure {
    /// The destination PATH (or registered worktree) already exists / is already
    /// checked out / already used by another worktree. A real worktree directory
    /// may genuinely exist here → caller may treat it as idempotent, but MUST
    /// verify the directory is present before returning Ok.
    PathExists,
    /// A branch with the requested name already exists, so `-b <branch>` failed.
    /// No worktree directory was created → caller must recover (retry without
    /// `-b` to check the existing branch out into a new worktree).
    BranchExists,
    /// Any other failure → propagate as an error.
    Other,
}

/// Classify a `git worktree add` failure from its stderr. Pure function so the
/// branching logic can be unit-tested without invoking real git.
fn classify_worktree_add_failure(stderr: &str) -> WorktreeAddFailure {
    // Branch collision: git says e.g. "fatal: a branch named 'X' already exists".
    // Check this FIRST — it also contains "already exists", so the broader
    // path-exists check below would otherwise misclassify it.
    if stderr.contains("a branch named") && stderr.contains("already exists") {
        return WorktreeAddFailure::BranchExists;
    }
    // Path / worktree already present: "'<path>' already exists",
    // "is already checked out", "already used by worktree".
    if stderr.contains("already exists")
        || stderr.contains("already checked out")
        || stderr.contains("already used by worktree")
    {
        return WorktreeAddFailure::PathExists;
    }
    WorktreeAddFailure::Other
}

// `find_worktree_path_for_branch` was deleted with #726-5ac7. It returned the
// FIRST porcelain block carrying a branch, which is the whole bug: with two
// workspaces on one branch every caller silently got the wrong directory. Its
// replacement is `resolve_workspace`, keyed by workspace id. Do not reintroduce
// a branch-keyed path lookup — resolve an id and read `.branch` off the record.

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct WorktreeConfig {
    pub(crate) task_name: String,
    pub(crate) base_repo: String,
    pub(crate) branch: Option<String>,
    pub(crate) create_branch: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct WorktreeResult {
    pub(crate) session_id: String,
    pub(crate) worktree_path: String,
    pub(crate) branch: Option<String>,
}

/// Resolve the worktree base directory for a given repo + storage strategy.
///
/// - `Sibling`: `{repo_parent}/{repo_name}__wt/`
/// - `AppDir`: `{app_config_dir}/worktrees/{repo_name}/`
/// - `InsideRepo`: `{repo_path}/.worktrees/`
/// - `ClaudeCodeDefault`: `{repo_path}/.claude/worktrees/`
pub(crate) fn resolve_worktree_dir(
    repo_path: &Path,
    strategy: &crate::config::WorktreeStorage,
    app_worktrees_dir: &Path,
) -> PathBuf {
    use crate::config::WorktreeStorage;
    match strategy {
        WorktreeStorage::Sibling => {
            let repo_name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            let parent = repo_path.parent().unwrap_or(repo_path);
            parent.join(format!("{repo_name}__wt"))
        }
        WorktreeStorage::AppDir => {
            let repo_name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            app_worktrees_dir.join(repo_name)
        }
        WorktreeStorage::InsideRepo => repo_path.join(".worktrees"),
        WorktreeStorage::ClaudeCodeDefault => repo_path.join(".claude").join("worktrees"),
    }
}

/// Resolve the effective worktree directory for a repo by loading config from disk.
/// Per-repo `worktree_storage` overrides the global default from repo-defaults.
pub(crate) fn resolve_worktree_dir_for_repo(repo_path: &Path, app_worktrees_dir: &Path) -> PathBuf {
    let repo_path_str = repo_path.to_string_lossy();
    let repo_settings = crate::config::load_repo_settings();
    let strategy = repo_settings
        .repos
        .get(repo_path_str.as_ref())
        .and_then(|entry| entry.worktree_storage.clone())
        .unwrap_or_else(|| crate::config::load_repo_defaults().worktree_storage);
    resolve_worktree_dir(repo_path, &strategy, app_worktrees_dir)
}

/// Sanitize task name for use as directory name
pub(crate) fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// Create a git worktree for a task.
///
/// `base_ref` optionally specifies the starting commit/branch for new worktrees
/// (e.g., "main" or "origin/develop"). When `None`, git uses HEAD.
pub(crate) fn create_worktree_internal(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    let worktree_name = sanitize_name(&config.task_name);
    let worktree_path = worktrees_dir.join(&worktree_name);

    // Check if worktree already exists (idempotent return — stale cleanup is caller's responsibility).
    // Detached HEAD (actual_branch == None) is NOT treated as stale: it's a transient state during
    // rebase/bisect/`git checkout <sha>` on a worktree we created. Forcing cleanup there would
    // destroy an agent's in-progress work.
    if worktree_path.exists() {
        let actual_branch = crate::git::read_branch_from_head(&worktree_path);
        if let Some(ref expected) = config.branch
            && let Some(ref actual) = actual_branch
            && actual.as_str() != expected.as_str()
        {
            return Err(format!(
                "{STALE_DIR_PREFIX} directory '{}' is checked out on branch '{}', not '{}'",
                worktree_path.display(),
                actual,
                expected,
            ));
        }
        // Fall back to config.branch when actual_branch is None (detached HEAD):
        // the worktree was created for `config.branch`, the detach is transient, and
        // the JS layer's `BranchState` keys on `result.branch: string` — returning
        // `null` would corrupt the store. The branch field reflects logical
        // ownership, not the live HEAD state.
        return Ok(WorktreeInfo {
            name: worktree_name,
            path: worktree_path,
            branch: actual_branch.or_else(|| config.branch.clone()),
            base_repo: PathBuf::from(&config.base_repo),
        });
    }

    // Ensure worktrees directory exists
    std::fs::create_dir_all(worktrees_dir)
        .map_err(|e| format!("Failed to create worktrees directory: {e}"))?;

    // Build git worktree add command
    let base_repo_path = PathBuf::from(&config.base_repo);
    let wt_path_str = worktree_path.to_string_lossy().to_string();
    // --quiet suppresses git's own checkout progress lines ("Updating files: X% (N/M)")
    // that would otherwise appear in the controlling terminal. Hooks still run normally.
    let mut args: Vec<String> = vec!["worktree".into(), "add".into(), "--quiet".into()];

    if config.create_branch
        && let Some(ref branch) = config.branch
    {
        args.push("-b".into());
        args.push(branch.clone());
    }

    // End-of-options guard: the branch/start-point below is attacker-influenced
    // (e.g. a PR head_ref), so `--` forces git to treat it as a ref, not an option.
    args.push("--".into());
    args.push(wt_path_str);

    if let Some(ref branch) = config.branch
        && !config.create_branch
    {
        args.push(branch.clone());
    }

    // Append base_ref as start-point when creating a new branch
    if config.create_branch
        && let Some(start_point) = base_ref
    {
        // Auto-fetch if the base ref is a remote tracking branch
        fetch_if_remote(&config.base_repo, start_point)?;
        args.push(start_point.to_string());
    }

    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match git_cmd(&base_repo_path).args(&args_str).run() {
        Ok(_) => {}
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. }) => {
            match classify_worktree_add_failure(stderr) {
                WorktreeAddFailure::BranchExists => {
                    // `-b <branch>` failed because the branch already exists, but
                    // NO worktree was created. The user intent ("give me a worktree
                    // for this branch") is still satisfiable: retry without `-b` to
                    // check the existing branch out into a fresh worktree. Only
                    // reachable when create_branch && branch is Some (that's the
                    // only way `-b` was passed), so `branch` is guaranteed present.
                    let branch = config
                        .branch
                        .as_ref()
                        .expect("BranchExists implies -b was passed, so branch is Some");
                    let retry_args = [
                        "worktree",
                        "add",
                        "--quiet",
                        "--",
                        &worktree_path.to_string_lossy(),
                        branch.as_str(),
                    ];
                    if let Err(e) = git_cmd(&base_repo_path).args(retry_args).run() {
                        return Err(format!(
                            "Git worktree failed: branch '{branch}' already exists and could not be checked out into a new worktree: {e}"
                        ));
                    }
                    // Retry creates the dir at config.branch — fall through to the
                    // success return below.
                }
                WorktreeAddFailure::PathExists => {
                    // A path/worktree already exists. Defensive fail-loud: only treat
                    // this as idempotent if the directory is genuinely present on disk.
                    if !worktree_path.exists() {
                        return Err(format!(
                            "Git worktree failed: git reported '{}' already exists but no worktree directory is present: {stderr}",
                            worktree_path.display(),
                        ));
                    }
                    let actual_branch = crate::git::read_branch_from_head(&worktree_path);
                    // Mirror the earlier idempotent-path STALE_DIR check: only treat a
                    // KNOWN-mismatched branch as stale. Detached HEAD (None) is preserved
                    // as transient state. Use the same STALE_DIR_PREFIX so the recovery
                    // path in `create_worktree` (background cleanup + retry) handles it.
                    if let Some(ref expected) = config.branch
                        && let Some(ref actual) = actual_branch
                        && actual.as_str() != expected.as_str()
                    {
                        return Err(format!(
                            "{STALE_DIR_PREFIX} directory '{}' already exists and is checked out on branch '{}', not '{}'",
                            worktree_path.display(),
                            actual,
                            expected,
                        ));
                    }
                    return Ok(WorktreeInfo {
                        name: worktree_name,
                        path: worktree_path,
                        branch: actual_branch.or_else(|| config.branch.clone()),
                        base_repo: PathBuf::from(&config.base_repo),
                    });
                }
                WorktreeAddFailure::Other => {
                    return Err(crate::git_locks::describe_stale_lock(&base_repo_path)
                        .unwrap_or_else(|| {
                            format!("Git worktree failed: git exited with: {stderr}")
                        }));
                }
            }
        }
        Err(e) => {
            return Err(crate::git_locks::describe_stale_lock(&base_repo_path)
                .unwrap_or_else(|| format!("Git worktree failed: {e}")));
        }
    }

    // Persist the base ref in git config for "Update from base" support
    if let Some(ref branch) = config.branch
        && let Some(start_point) = base_ref
    {
        let _ = set_branch_base(&config.base_repo, branch, start_point);
    }

    Ok(WorktreeInfo {
        name: worktree_name,
        path: worktree_path,
        branch: config.branch.clone(),
        base_repo: PathBuf::from(&config.base_repo),
    })
}

/// Error prefix returned when a worktree is git-locked and `force` is false.
/// The JS layer checks for this prefix to show a confirmation dialog before retrying.
pub(crate) const LOCKED_WORKTREE_PREFIX: &str = "worktree_locked:";

/// Error prefix returned when trying to `git worktree remove` the main working tree.
/// The JS layer treats this as a non-fatal condition and does NOT remove the branch
/// from the store (to avoid resurrection on the next refresh).
pub(crate) const MAIN_WORKTREE_PREFIX: &str = "worktree_is_main:";

/// Error prefix returned when a worktree directory exists but is checked out on a
/// different branch than requested. The Tauri command's stale-recovery path matches
/// this prefix to trigger background cleanup + recreate. Centralised here so callers
/// don't drift on the literal string.
pub(crate) const STALE_DIR_PREFIX: &str = "STALE_DIR:";

/// Error prefix returned when `git worktree remove` refuses a worktree that holds
/// uncommitted (modified or untracked) work and the caller did not confirm
/// destroying it. The JS layer matches this prefix to offer an informed retry
/// ("this will discard uncommitted changes") instead of silently dropping the
/// sidebar row while the worktree is still on disk.
pub(crate) const DIRTY_WORKTREE_PREFIX: &str = "worktree_dirty:";

/// Error prefix returned when a live PTY/agent session is attached to the
/// worktree being removed (see [`crate::state::AppState::live_sessions_in_worktree`]).
/// This is a hard refusal independent of git's own dirty/lock checks — a clean,
/// unlocked worktree can still have a terminal open in it. Only an explicit
/// `override_busy` skips this gate; it is deliberately never implied by `force`,
/// so overriding a live session never also escalates dirty-file or lock handling.
pub(crate) const BUSY_WORKTREE_PREFIX: &str = "worktree_busy:";

/// How aggressively `git worktree remove` should override git's own safety checks.
///
/// A plain bool can't express this: git's `remove` has two *independent* refusals
/// (uncommitted work, and a `git worktree lock`) and a single `--force` only lifts
/// the first — lifting the second needs `--force` twice. See the incident writeup
/// at `plans/worktree-removal-incident-2026-08-26.md` for why collapsing these two
/// into one flag is exactly what let a live worktree be destroyed unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemovalMode {
    /// `git worktree remove` — refuses both a dirty and a locked worktree. The
    /// default for every removal path; nothing is destroyed without an explicit
    /// override from a caller that has already confirmed with the user.
    Safe,
    /// `git worktree remove --force` — overrides the dirty-worktree check only.
    /// Used once uncommitted work is confirmed disposable (e.g. a merge/finalize
    /// cleanup the user already confirmed), never to break a lock.
    Dirty,
    /// `git worktree remove --force --force` — also overrides a `git worktree
    /// lock`. Reserved for the explicit "force remove" confirmation after a
    /// `worktree_locked:` refusal.
    Forced,
}

impl RemovalMode {
    fn force_args(self) -> &'static [&'static str] {
        match self {
            RemovalMode::Safe => &["worktree", "remove"],
            RemovalMode::Dirty => &["worktree", "remove", "--force"],
            RemovalMode::Forced => &["worktree", "remove", "--force", "--force"],
        }
    }
}

/// Force-remove a stale worktree directory.
///
/// Runs `git worktree remove --force` (cleans the registry entry) and then verifies
/// the directory is gone, falling back to `fs::remove_dir_all` (async then blocking
/// to handle file-locks on Windows / AV scanners). Returns `Ok(())` only when the
/// path is verified absent. Synchronous wrapper used by callers that can't spawn a
/// background task (PTY creation, MCP request handlers).
pub(crate) fn cleanup_stale_worktree_dir(base_repo: &str, stale_path: &Path) -> Result<(), String> {
    if let Err(e) = git_cmd(&PathBuf::from(base_repo))
        .args([
            "worktree",
            "remove",
            "--force",
            &stale_path.to_string_lossy(),
        ])
        .run()
    {
        tracing::warn!(
            source = "worktree",
            stale = %stale_path.display(),
            "cleanup_stale_worktree_dir: git worktree remove --force failed (falling back to fs removal): {e}"
        );
    }

    if stale_path.exists()
        && let Err(e) = std::fs::remove_dir_all(stale_path)
    {
        return Err(format!(
            "stale dir cleanup failed for '{}': {e}",
            stale_path.display()
        ));
    }

    if stale_path.exists() {
        return Err(format!(
            "stale dir '{}' still present after cleanup",
            stale_path.display()
        ));
    }
    Ok(())
}

/// Synchronous create-with-STALE_DIR-recovery for non-Tauri callers (MCP HTTP routes,
/// `create_session_with_worktree`, etc.). Tries `create_worktree_internal`; on a
/// STALE_DIR error, runs `cleanup_stale_worktree_dir` and retries once. The retry's
/// result is returned as-is — a second STALE_DIR (e.g. TOCTOU with another caller)
/// surfaces to the caller rather than looping.
pub(crate) fn create_worktree_with_stale_recovery(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<WorktreeInfo, String> {
    match create_worktree_internal(worktrees_dir, config, base_ref) {
        Ok(wt) => Ok(wt),
        Err(ref e) if e.starts_with(STALE_DIR_PREFIX) => {
            let stale_path = worktrees_dir.join(sanitize_name(&config.task_name));
            tracing::warn!(
                source = "worktree",
                stale = %stale_path.display(),
                "create_worktree_with_stale_recovery: STALE_DIR detected, cleaning up + retrying"
            );
            cleanup_stale_worktree_dir(&config.base_repo, &stale_path)?;
            create_worktree_internal(worktrees_dir, config, base_ref)
        }
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceCommitStatus {
    Unmerged,
    Merged,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceRemovalSafety {
    Safe,
    RequiresForce,
    Unknown,
}

/// One backend-authored answer for every UI that explains or removes a
/// workspace. Optional fields mean inspection failed; unknown is never zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WorkspaceLifecycleStatus {
    pub(crate) dirty: Option<bool>,
    pub(crate) commit_status: WorkspaceCommitStatus,
    pub(crate) removal_safety: WorkspaceRemovalSafety,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

fn dirty_at(path: &Path) -> Result<bool, String> {
    git_cmd(path)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .run()
        .map(|out| !out.stdout.trim().is_empty())
        .map_err(|e| format!("could not check the workspace for uncommitted changes: {e}"))
}

pub(crate) fn inspect_workspace_lifecycle(
    base_repo: &Path,
    workspace_id: &str,
) -> WorkspaceLifecycleStatus {
    let inspected = (|| -> Result<WorkspaceLifecycleStatus, String> {
        let workspace = resolve_any_workspace(base_repo, workspace_id)?;
        let dirty = dirty_at(Path::new(&workspace.path))?;
        let default_branch = get_remote_default_branch(&base_repo.to_string_lossy())?;
        let ancestry = git_cmd(Path::new(&workspace.path))
            .args(["merge-base", "--is-ancestor", "HEAD", &default_branch])
            .run_raw()
            .map_err(|e| format!("could not compare the worktree with the default branch: {e}"))?;
        let merged = match ancestry.status.code() {
            Some(0) => true,
            Some(1) => false,
            code => {
                let stderr = String::from_utf8_lossy(&ancestry.stderr).trim().to_string();
                return Err(format!(
                    "could not compare the worktree with the default branch (exit {code:?}): {stderr}"
                ));
            }
        };
        Ok(WorkspaceLifecycleStatus {
            dirty: Some(dirty),
            commit_status: if merged {
                WorkspaceCommitStatus::Merged
            } else {
                WorkspaceCommitStatus::Unmerged
            },
            removal_safety: if dirty {
                WorkspaceRemovalSafety::RequiresForce
            } else {
                WorkspaceRemovalSafety::Safe
            },
            error: None,
        })
    })();

    inspected.unwrap_or_else(|error| WorkspaceLifecycleStatus {
        dirty: None,
        commit_status: WorkspaceCommitStatus::Unknown,
        removal_safety: WorkspaceRemovalSafety::Unknown,
        error: Some(error),
    })
}

/// Resolve a workspace id through git's linked-worktree list.
pub(crate) fn resolve_any_workspace(
    base_repo: &Path,
    workspace_id: &str,
) -> Result<WorkspaceWorktree, String> {
    git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .ok()
        .and_then(|out| map_worktree_workspace_paths(&out.stdout).remove(workspace_id))
        .ok_or_else(|| {
            format!(
                "No workspace found for id '{workspace_id}' in '{}'",
                base_repo.display()
            )
        })
}

/// A workspace, however it was built.
///
/// One type for both mechanisms on purpose: the caller asked for a workspace,
/// and everything downstream — the sidebar row, publish, remove — needs to know
/// which one it got rather than infer it from the directory's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceKind {
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedWorkspace {
    /// The linked worktree id. It currently equals the branch name.
    pub(crate) workspace_id: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    pub(crate) kind: WorkspaceKind,
    /// Failures encountered while warming ignored build directories.
    pub(crate) warnings: Vec<String>,
    /// Git-ignored build directories clonefiled in from the parent so a linked
    /// worktree starts warm.
    pub(crate) warmed_directories: usize,
}

impl CreatedWorkspace {
    /// What the caller needs to know to USE this workspace, at the moment it
    /// can act on it.
    ///
    /// Boss ruled out every enforcement layer — no deny hooks, no shell
    /// override, no PATH shim — so this response is the only instruction
    /// channel there is. It states the things a model would otherwise get
    /// wrong by reflex, with their consequences, rather than in a preamble read
    /// 200k tokens ago:
    ///
    /// - inherited work in progress is not the model's own bug,
    /// - the warm artifacts are already there, so setting up is not a build,
    /// - and, for a clone, that the parent cannot see this branch — the failure
    ///   is silent, because `git merge` in the parent finds a same-named ref
    ///   and merges the WRONG one.
    pub(crate) fn instruction_payload(&self) -> serde_json::Value {
        let warm = crate::cow::warm_artifacts(&self.path);
        let isolation = "This is a linked worktree: refs and objects are shared with the parent \
            repository, so your commits are visible there immediately."
            .to_string();

        let setup = if warm.is_empty() {
            "No build output came with this workspace.".to_string()
        } else {
            "These came with the workspace at near-zero cost. Do NOT run an install or a full build \
             to \"set up\" — they are already warm. Run one only if a lockfile or a dependency \
             actually changed."
                .to_string()
        };

        serde_json::json!({
            "workspace_id": self.workspace_id,
            "path": self.path.to_string_lossy(),
            "branch": self.branch,
            "kind": self.kind,
            "warnings": self.warnings,
            "state": {
                "carried_over": 0,
                "note": "Tracked changes are not carried over: this workspace starts from a clean checkout.",
            },
            "warm_artifacts": {
                "present": warm,
                "warmed_directories": self.warmed_directories,
                "note": setup,
            },
            "isolation": isolation,
        })
    }
}

/// Create a linked worktree and warm its ignored build directories.
pub(crate) fn create_workspace(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
) -> Result<CreatedWorkspace, String> {
    create_workspace_with(worktrees_dir, config, base_ref, crate::cow::warm_worktree)
}

/// `create_workspace` with warming injected for deterministic tests.
pub(crate) fn create_workspace_with(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
    warm: impl Fn(&Path, &Path) -> crate::cow::WarmingReport,
) -> Result<CreatedWorkspace, String> {
    let src = PathBuf::from(&config.base_repo);
    let branch = config
        .branch
        .clone()
        .unwrap_or_else(|| sanitize_name(&config.task_name));

    ensure_branch_has_no_workspace(&src, &branch)?;
    let worktree = create_worktree_with_stale_recovery(worktrees_dir, config, base_ref)?;
    let branch = worktree.branch.unwrap_or(branch);

    // DEFERRED (2026-09-13): carrying the parent's tracked changes was dropped
    // with independent COW workspace creation. If reinstated, pipe
    // `git diff HEAD` in the parent to `git apply` in this linked worktree.
    let warming = warm(&src, &worktree.path);
    Ok(CreatedWorkspace {
        workspace_id: workspace_id_of_worktree(&branch),
        path: worktree.path,
        branch,
        kind: WorkspaceKind::Worktree,
        warnings: warming.warnings,
        warmed_directories: warming.warmed,
    })
}
/// A branch can belong to only one linked worktree.
fn ensure_branch_has_no_workspace(base_repo: &Path, branch: &str) -> Result<(), String> {
    let listed = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    match map_worktree_workspace_paths(&listed.stdout)
        .into_iter()
        .find(|(_, workspace)| workspace.branch == branch)
    {
        Some((workspace_id, workspace)) => Err(format!(
            "branch '{branch}' already belongs to linked worktree '{workspace_id}' at '{}'; reuse that worktree",
            workspace.path
        )),
        None => Ok(()),
    }
}
pub(crate) fn remove_worktree_internal(
    worktree: &WorktreeInfo,
    mode: RemovalMode,
) -> Result<(), String> {
    let wt_path_str = worktree.path.to_string_lossy().to_string();
    tracing::info!(
        source = "worktree",
        branch = %worktree.name,
        path = %wt_path_str,
        mode = ?mode,
        "remove_worktree_internal: start"
    );

    let force_args = mode.force_args();

    match git_cmd(&worktree.base_repo)
        .args(
            force_args
                .iter()
                .chain(std::iter::once(&wt_path_str.as_str())),
        )
        .run()
    {
        Ok(_) => {
            tracing::info!(source = "worktree", branch = %worktree.name, mode = ?mode, "git worktree remove: OK");
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("not a working tree") || stderr.contains("No such file") =>
        {
            tracing::info!(
                source = "worktree",
                branch = %worktree.name,
                "git worktree remove: worktree already gone (treating as success)"
            );
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if mode != RemovalMode::Forced
                && (stderr.contains("locked working tree")
                    || stderr.contains("cannot remove a locked")) =>
        {
            // Worktree is locked and the caller did not request the double-force
            // that overrides a lock. Surface a distinctive error so the JS layer
            // can prompt the user to confirm before retrying with Forced.
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                stderr = %stderr,
                "git worktree remove: locked — returning error for JS confirmation prompt"
            );
            return Err(format!("{LOCKED_WORKTREE_PREFIX}{stderr}"));
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if mode == RemovalMode::Safe
                && stderr.contains("contains modified or untracked files") =>
        {
            // Worktree has uncommitted work and the caller asked for the Safe
            // (no-override) removal. Surface a distinctive error so the JS layer
            // can offer an informed retry instead of silently discarding it — or,
            // on the generic-error fallback path, silently dropping the sidebar
            // row while the worktree is still on disk.
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                stderr = %stderr,
                "git worktree remove: dirty — returning error for JS confirmation prompt"
            );
            return Err(format!("{DIRTY_WORKTREE_PREFIX}{stderr}"));
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("is a main working tree") =>
        {
            // The branch is checked out in the main repo directory, not a linked
            // worktree. `git worktree remove` is not the right tool here.
            // Return a distinctive prefix so the JS layer can show a clear message
            // and NOT remove the branch from the store (avoiding resurrection).
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                "git worktree remove: branch is in main worktree, cannot remove"
            );
            return Err(format!("{MAIN_WORKTREE_PREFIX}{stderr}"));
        }
        Err(crate::git_cli::GitError::NonZeroExit { ref stderr, .. })
            if stderr.contains("cannot be moved or removed") =>
        {
            // Git refuses a plain (no-force) `worktree remove` when the
            // worktree's checked-out tree contains a submodule reference — but
            // unlike the dirty/lock refusals above, a single `--force` DOES
            // lift this one (confirmed empirically on git 2.55.0: `git
            // worktree remove --force` succeeds outright here, even on an
            // otherwise-dirty worktree). That means `RemovalMode::Dirty` and
            // `::Forced` never even reach this arm — their first attempt above
            // already used `--force` and would have succeeded directly; only
            // `::Safe` (no force at all) hits it.
            //
            // It still fires BEFORE git's own dirty-worktree check, so a
            // Safe-mode caller never gets a chance to learn separately whether
            // the worktree is also dirty — replicate that check ourselves
            // before retrying with `--force`, so uncommitted/untracked work
            // isn't silently discarded just because a submodule happens to be
            // present too.
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                stderr = %stderr,
                "git worktree remove: worktree tree contains a submodule — retrying with --force"
            );

            if mode == RemovalMode::Safe {
                match git_cmd(&worktree.path)
                    .args(["status", "--porcelain", "--untracked-files=all"])
                    .run()
                {
                    Ok(out) if !out.stdout.trim().is_empty() => {
                        tracing::warn!(
                            source = "worktree",
                            branch = %worktree.name,
                            "git worktree remove: worktree (with submodule) has uncommitted/untracked changes — returning error for JS confirmation prompt"
                        );
                        return Err(format!(
                            "{DIRTY_WORKTREE_PREFIX}worktree contains modified or untracked files"
                        ));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(source = "worktree", branch = %worktree.name, "git status check (submodule fallback) failed: {e}");
                        return Err(format!(
                            "Git worktree remove failed: could not verify worktree is clean before retrying: {e}"
                        ));
                    }
                }
            }

            // Clean (or the caller already confirmed Dirty/Forced removal):
            // retry with `--force`, which lifts the submodule refusal. This
            // is a real `git worktree remove`, not a manual fallback, so it
            // leaves git's own administrative bookkeeping consistent — the
            // directory-existence check and prune below are just the same
            // safety net every other path already falls through.
            if let Err(e) = git_cmd(&worktree.base_repo)
                .args(["worktree", "remove", "--force", &wt_path_str])
                .run()
            {
                tracing::error!(source = "worktree", branch = %worktree.name, "git worktree remove --force retry (submodule) FAILED: {e}");
                return Err(format!(
                    "Git worktree remove failed after retrying with --force to clear the submodule refusal: {e}"
                ));
            }
            tracing::info!(source = "worktree", branch = %worktree.name, "git worktree remove --force (submodule retry): OK");
        }
        Err(e) => {
            tracing::error!(source = "worktree", branch = %worktree.name, "git worktree remove FAILED: {e}");
            return Err(crate::git_locks::describe_stale_lock(&worktree.base_repo)
                .unwrap_or_else(|| format!("Git worktree remove failed: {e}")));
        }
    }

    // Cleanup the directory if it still exists
    if worktree.path.exists() {
        tracing::warn!(
            source = "worktree",
            branch = %worktree.name,
            path = %wt_path_str,
            "directory still exists after git worktree remove — running rm -rf"
        );
        std::fs::remove_dir_all(&worktree.path)
            .map_err(|e| format!("Failed to remove worktree directory: {e}"))?;
        tracing::info!(source = "worktree", branch = %worktree.name, "directory removed");
    } else {
        tracing::info!(source = "worktree", branch = %worktree.name, "directory already gone after git worktree remove");
    }

    // Unlock first — `prune` skips locked worktrees and would leave a ghost
    // entry in `git worktree list` (same reasoning as `archive_worktree_dir`).
    // Best-effort: the directory is already gone at this point, so a failure
    // here just means git had nothing left to unlock.
    unlock_worktree(&worktree.base_repo, &worktree.path);

    // Prune worktrees (non-fatal: stale entries are harmless)
    if let Err(e) = git_cmd(&worktree.base_repo)
        .args(["worktree", "prune"])
        .run()
    {
        tracing::warn!(source = "worktree", "git worktree prune failed: {e}");
    } else {
        tracing::info!(source = "worktree", branch = %worktree.name, "git worktree prune: OK");
    }

    tracing::info!(source = "worktree", branch = %worktree.name, "remove_worktree_internal: done");
    Ok(())
}

/// Best-effort `git worktree lock` with a TUIC-owned reason, run when a PTY/agent
/// session attaches to a worktree. Failure is never fatal to the caller — a lock
/// is defense in depth (it also protects against a bare `git worktree remove` run
/// outside TUIC entirely), not the primary gate; the primary gate is the live
/// session check in `remove_worktree_by_branch`.
///
/// The reason is prefixed `tuic: ` so the startup sweep can distinguish a
/// TUIC-owned lock (safe to clear once its session is gone) from a lock a user
/// or another tool (e.g. Claude Code's own agent locking) set deliberately.
pub(crate) fn lock_worktree_for_session(base_repo: &Path, worktree_path: &Path, session_id: &str) {
    let wt_path_str = worktree_path.to_string_lossy().to_string();
    if let Err(e) = git_cmd(base_repo)
        .args([
            "worktree",
            "lock",
            &wt_path_str,
            "--reason",
            &format!("{TUIC_LOCK_REASON_PREFIX}session {session_id}"),
        ])
        .run()
    {
        tracing::warn!(
            source = "worktree",
            path = %wt_path_str,
            session_id = %session_id,
            "lock_worktree_for_session: git worktree lock failed (non-fatal): {e}"
        );
    }
}

/// Best-effort `git worktree unlock`, run when the last session attached to a
/// worktree detaches. Never fatal — an unlock failure just means the worktree
/// stays locked until the next successful removal attempt or startup sweep.
pub(crate) fn unlock_worktree(base_repo: &Path, worktree_path: &Path) {
    let wt_path_str = worktree_path.to_string_lossy().to_string();
    if let Err(e) = git_cmd(base_repo)
        .args(["worktree", "unlock", &wt_path_str])
        .run()
    {
        tracing::warn!(
            source = "worktree",
            path = %wt_path_str,
            "unlock_worktree: git worktree unlock failed (non-fatal): {e}"
        );
    }
}

/// Adjective + sci-fi character worktree name generator
pub(crate) fn generate_worktree_name(existing: &[String]) -> String {
    let adjectives = [
        "brave", "calm", "dark", "eager", "fair", "glad", "happy", "keen", "lush", "mild", "neat",
        "proud", "quick", "rare", "safe", "tall", "vast", "warm", "wise", "bold", "cool", "deep",
        "fast", "gold", "huge", "iron", "jade", "kind", "lean", "mint", "nova", "open", "pale",
        "red", "slim", "tidy", "ultra", "vivid", "wild", "zen",
    ];

    let names = [
        "neo",
        "ripley",
        "deckard",
        "morpheus",
        "trinity",
        "cypher",
        "nexus",
        "cortex",
        "tron",
        "hal",
        "skynet",
        "muad",
        "atreides",
        "harkonnen",
        "seldon",
        "daneel",
        "solaris",
        "neuro",
        "winter",
        "armitage",
        "molly",
        "case",
        "hiro",
        "kovacs",
        "takeshi",
        "quell",
        "pris",
        "batty",
        "zhora",
        "gaff",
        "tyrell",
        "gibson",
        "asimov",
        "vance",
        "rama",
        "ender",
        "bean",
        "valentine",
        "petra",
        "revan",
    ];

    // Simple PRNG using current time
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for attempt in 0..100u128 {
        let adj_idx =
            ((seed.wrapping_add(attempt.wrapping_mul(7))) % adjectives.len() as u128) as usize;
        let name_idx = ((seed.wrapping_add(attempt.wrapping_mul(13)).wrapping_add(3))
            % names.len() as u128) as usize;
        let num = ((seed.wrapping_add(attempt.wrapping_mul(31))) % 1000) as u16;
        let name = format!("{}-{}-{:03}", adjectives[adj_idx], names[name_idx], num);
        if !existing.contains(&name) {
            return name;
        }
    }

    // Fallback
    format!("worktree-{}", seed % 10000)
}

/// Generate a hybrid branch name for the quick-clone flow.
///
/// Format: `{source_branch}--{random_name}` (e.g., `feat-auth--brave-neo-042`).
/// The double-dash separator makes it easy to parse the source branch later.
/// Checks collision against `existing` list and regenerates random part if needed.
pub(crate) fn generate_clone_branch_name(source_branch: &str, existing: &[String]) -> String {
    let sanitized = sanitize_name(source_branch);
    for _ in 0..100 {
        let random_part = generate_worktree_name(existing);
        let name = format!("{sanitized}--{random_part}");
        if !existing.contains(&name) {
            return name;
        }
    }
    // Fallback with timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{sanitized}--wt-{}", ts % 100000)
}

/// Create a linked worktree without a PTY session.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_worktree(
    state: State<'_, Arc<AppState>>,
    base_repo: String,
    branch_name: String,
    create_branch: Option<bool>,
    base_ref: Option<String>,
) -> Result<serde_json::Value, String> {
    let config = WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo: base_repo.clone(),
        branch: Some(branch_name),
        create_branch: create_branch.unwrap_or(true),
    };
    let worktrees_dir =
        resolve_worktree_dir_for_repo(Path::new(&config.base_repo), &state.worktrees_dir);
    let workspace = tokio::task::spawn_blocking(move || {
        create_workspace(&worktrees_dir, &config, base_ref.as_deref())
    })
    .await
    .map_err(|error| format!("Task panic: {error}"))??;

    state.invalidate_repo_caches(&base_repo);
    spawn_worktree_file_sync(&state, &base_repo, &workspace.branch, &workspace.path);
    Ok(serde_json::json!({
        "status": "ok",
        "name": workspace.path.file_name().map(|name| name.to_string_lossy().to_string()),
        "path": workspace.path.to_string_lossy(),
        "workspace_id": workspace.workspace_id,
        "branch": workspace.branch,
        "base_repo": base_repo,
        "kind": workspace.kind,
        "instructions": workspace.instruction_payload(),
    }))
}
/// Get worktrees directory path.
/// When `repo_path` is provided, resolves the effective storage strategy for the repo.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_worktrees_dir(
    state: State<'_, Arc<AppState>>,
    repo_path: Option<String>,
) -> String {
    match repo_path {
        Some(rp) => resolve_worktree_dir_for_repo(Path::new(&rp), &state.worktrees_dir)
            .to_string_lossy()
            .to_string(),
        None => state.worktrees_dir.to_string_lossy().to_string(),
    }
}

/// Core logic for removing one workspace's checkout, addressed by workspace id.
///
/// When `delete_branch` is true, also deletes the local branch after removing
/// the worktree directory. When false, the branch is preserved.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct RemoveWorktreeOutcome {
    pub(crate) branch_delete_warning: Option<String>,
    /// Branch the removed workspace was on, read off the record before removal.
    /// Callers need it for branch-keyed follow-up work (config labels, logs) and
    /// cannot re-resolve it: the id stops resolving the moment the worktree is
    /// gone, and it is not the branch to begin with once ids are minted.
    pub(crate) branch: String,
}

pub(crate) fn remove_worktree_by_workspace_id(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    mode: RemovalMode,
    state: Option<&Arc<AppState>>,
    override_busy: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    let base_repo = PathBuf::from(repo_path);
    let mut branch_delete_warning = None;

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        delete_branch = %delete_branch,
        mode = ?mode,
        override_busy = %override_busy,
        "remove_worktree_by_workspace_id: start"
    );

    let workspace = resolve_any_workspace(&base_repo, workspace_id).inspect_err(|_| {
        tracing::error!(
            source = "worktree",
            workspace_id = %workspace_id,
            "remove_worktree_by_workspace_id: no workspace found for id"
        );
    })?;
    // The branch to delete comes off the resolved record instead of duplicating
    // the workspace-id representation at the call site.
    let branch_name = workspace.branch.as_str();
    let worktree_path = PathBuf::from(&workspace.path);

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        branch = %branch_name,
        path = %worktree_path.display(),
        "remove_worktree_by_workspace_id: worktree path resolved"
    );

    // Refuse to remove a worktree with a live PTY/agent session attached, unless
    // the caller has already confirmed the override. This is deliberately
    // independent of git's own dirty/lock checks — a clean, unlocked worktree
    // can still have a terminal open in it — and is the backstop for callers
    // (MCP, HTTP) that never go through the frontend's own terminal-aware
    // confirmation flow. `state` is `None` only for pre-attach callers (e.g.
    // cleanup after a failed PTY spawn) where no session could possibly exist.
    if !override_busy && let Some(state) = state {
        let attached = state.live_sessions_in_worktree(&worktree_path);
        if !attached.is_empty() {
            let ids = attached
                .iter()
                .map(|s| {
                    // Prefer the live cwd (what the JS layer's "in use" summary
                    // wants to show); fall back to the bare session id.
                    s.cwd.as_deref().unwrap_or(s.session_id.as_str())
                })
                .collect::<Vec<_>>()
                .join(", ");
            tracing::warn!(
                source = "worktree",
                branch = %branch_name,
                sessions = %ids,
                "remove_worktree_by_branch: refusing — live session(s) attached"
            );
            return Err(format!(
                "{BUSY_WORKTREE_PREFIX}{} session(s) attached: {ids}",
                attached.len()
            ));
        }
    }

    // Run archive/cleanup script before deletion (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, &worktree_path)
            .map_err(|e| format!("Archive script failed: {e}"))?;
    }

    // Remove the worktree
    let worktree = WorktreeInfo {
        name: workspace_id.to_string(),
        path: worktree_path,
        branch: Some(branch_name.to_string()),
        base_repo,
    };

    remove_worktree_internal(&worktree, mode)?;

    // Delete the local branch when requested. Always uses `-d` (safe delete):
    // unmerged branches are refused so unpushed/unmerged commits are never
    // silently discarded by a worktree removal, no matter how the worktree
    // itself was removed. `RemovalMode` governs the worktree directory only —
    // it never escalates branch deletion. (Previously `force` also selected
    // `-D` here, which is what force-deleted the branch behind the 2026-08-26
    // incident's orphaned commits; see the incident writeup.)
    if delete_branch {
        // `--` separates flags from positional args so a branch name beginning
        // with `-` cannot be misparsed as a git option.
        match git_cmd(&worktree.base_repo)
            .args(["branch", "-d", "--", branch_name])
            .run()
        {
            Ok(_) => tracing::info!(
                source = "worktree",
                branch = %branch_name,
                "git branch delete: OK"
            ),
            Err(e) => {
                let warning = format!("git branch -d {branch_name} failed: {e}");
                tracing::warn!(
                    source = "worktree",
                    branch = %branch_name,
                    "git branch delete failed (branch ref preserved): {e}"
                );
                branch_delete_warning = Some(warning);
            }
        }
    }

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        branch = %branch_name,
        "remove_worktree_by_workspace_id: done"
    );
    Ok(RemoveWorktreeOutcome {
        branch_delete_warning,
        branch: branch_name.to_string(),
    })
}

/// Remove one workspace's checkout by workspace id (Tauri command with cache invalidation)
///
/// `delete_branch` defaults to `true` when omitted (preserving existing behavior).
///
/// `force` (default `false`) selects [`RemovalMode::Forced`] — the double
/// `--force` that overrides both a dirty worktree and a `git worktree lock`.
/// The default `Safe` attempt is expected to fail informatively
/// (`worktree_dirty:` / `worktree_locked:`) so the frontend can show the right
/// confirmation before retrying with `force: true`.
///
/// `override_busy` (default `false`) is a *separate* flag: it skips the
/// live-session gate only, and never escalates dirty-file or lock handling —
/// see `remove_worktree_by_branch`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn remove_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    delete_branch: Option<bool>,
    force: Option<bool>,
    override_busy: Option<bool>,
) -> Result<RemoveWorktreeOutcome, String> {
    let delete_branch = delete_branch.unwrap_or(true);
    let force = force.unwrap_or(false);
    let override_busy = override_busy.unwrap_or(false);
    let mode = if force {
        RemovalMode::Forced
    } else {
        RemovalMode::Safe
    };
    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        repo = %repo_path,
        delete_branch = %delete_branch,
        force = %force,
        override_busy = %override_busy,
        "remove_worktree command: invoked"
    );
    let script = resolve_archive_script(&repo_path);
    let repo_path_clone = repo_path.clone();
    let workspace_id_clone = workspace_id.clone();
    let state_arc = state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        remove_worktree_by_workspace_id(
            &repo_path_clone,
            &workspace_id_clone,
            delete_branch,
            script.as_deref(),
            mode,
            Some(&state_arc),
            override_busy,
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?;

    match result {
        Ok(outcome) => {
            tracing::info!(source = "worktree", workspace_id = %workspace_id, "remove_worktree command: SUCCESS — invalidating caches");
            if outcome.branch_delete_warning.is_none() {
                // Branch labels are branch-keyed, so a removed worktree drops the
                // label only after the branch itself was deleted successfully.
                crate::config::remove_branch_label(&repo_path, &outcome.branch);
            }
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch.clone(),
            });
            Ok(outcome)
        }
        Err(e) => {
            tracing::error!(source = "worktree", workspace_id = %workspace_id, "remove_worktree command: FAILED — {e}");
            Err(e)
        }
    }
}

/// Check whether a workspace's working directory has uncommitted changes.
///
/// Resolves the workspace by id and runs `git status --porcelain` in its
/// directory. If the id resolves to no checkout (bare local ref), returns
/// `false` — there's nothing to be dirty.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn check_worktree_dirty(
    repo_path: String,
    workspace_id: String,
) -> Result<bool, String> {
    match worktree_dirtiness(Path::new(&repo_path), &workspace_id) {
        WorktreeDirtiness::Clean => Ok(false),
        WorktreeDirtiness::Dirty => Ok(true),
        // An unanswered question is an error here, never a "no". Callers that
        // gate a destructive action on this must see the failure.
        WorktreeDirtiness::Unknown(reason) => Err(reason),
    }
}

/// Return the same workspace lifecycle verdict used by repository refresh and
/// removal confirmation. This is intentionally a fresh read: a cached sidebar
/// badge is explanation, not authorization for a destructive action.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_workspace_lifecycle(
    repo_path: String,
    workspace_id: String,
) -> Result<WorkspaceLifecycleStatus, String> {
    tokio::task::spawn_blocking(move || {
        Ok(inspect_workspace_lifecycle(
            Path::new(&repo_path),
            &workspace_id,
        ))
    })
    .await
    .map_err(|e| format!("workspace lifecycle task failed: {e}"))?
}

/// Delete a local branch, disposing of the workspace `workspace_id` names.
///
/// Two different objects, two parameters: `branch_name` is the ref to delete,
/// `workspace_id` is the checkout holding it. They are the same string only
/// under the identity migration, and the id must never be parsed back into a
/// branch — so the branch always comes from the caller or the resolved record,
/// never from the id.
///
/// When the id resolves to a checkout, behaviour depends on `keep_worktree`:
/// - `false` (default): remove the worktree directory together with the branch
///   ref via `remove_worktree_by_workspace_id`.
/// - `true`: detach the worktree HEAD (so the branch ref is no longer checked
///   out anywhere), then delete the branch ref with `git branch -d`. The
///   worktree directory and its files are preserved.
///
/// When it resolves to nothing the branch is a bare ref, and only the ref goes.
///
/// Safety: refuses to delete the repository's default branch, and refuses when
/// the resolved workspace is on a different branch than the one asked for —
/// that mismatch means the caller's id and branch disagree, and guessing which
/// one it meant is how the wrong ref gets deleted.
/// Uses `git branch -d` (safe delete) which fails if the branch has unmerged commits.
pub(crate) fn delete_local_branch_impl(
    repo_path: &str,
    branch_name: &str,
    workspace_id: &str,
    keep_worktree: bool,
    state: Option<&Arc<AppState>>,
) -> Result<(), String> {
    // Refuse to delete the default branch
    let default_branch =
        get_remote_default_branch(repo_path).unwrap_or_else(|_| "main".to_string());
    if branch_name == default_branch {
        return Err(format!("Refusing to delete default branch '{branch_name}'"));
    }

    let base_repo = PathBuf::from(repo_path);

    // Resolve the checkout by id. `None` is a bare branch, not an error.
    let workspace = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run_silent()
        .and_then(|o| map_worktree_workspace_paths(&o.stdout).remove(workspace_id));

    if let Some(ref ws) = workspace
        && ws.branch != branch_name
    {
        return Err(format!(
            "Workspace '{workspace_id}' is on branch '{}', not '{branch_name}' — refusing to delete",
            ws.branch
        ));
    }
    let worktree_path = workspace.map(|ws| PathBuf::from(ws.path));

    match (worktree_path, keep_worktree) {
        (Some(wt_path), true) => {
            // Detach the worktree HEAD so `git branch -d` will accept the
            // branch as deletable while leaving the worktree files on disk.
            git_cmd(&wt_path)
                .args(["checkout", "--detach"])
                .run()
                .map_err(|e| {
                    format!(
                        "git checkout --detach in worktree {} failed: {e}",
                        wt_path.display()
                    )
                })?;
            git_cmd(&base_repo)
                .args(["branch", "-d", "--", branch_name])
                .run()
                .map_err(|e| format!("git branch -d {branch_name} failed: {e}"))?;
        }
        (Some(_), false) => {
            // Remove worktree + branch in one go. Safe mode: this is documented
            // as a safe deletion, so it must never silently discard uncommitted
            // work in the worktree either.
            remove_worktree_by_workspace_id(
                repo_path,
                workspace_id,
                true,
                None,
                RemovalMode::Safe,
                state,
                false,
            )?;
        }
        (None, _) => {
            // Bare branch — no worktree to consider
            git_cmd(&base_repo)
                .args(["branch", "-d", "--", branch_name])
                .run()
                .map_err(|e| format!("git branch -d {branch_name} failed: {e}"))?;
        }
    }

    Ok(())
}

/// Tauri command: delete a local branch.
///
/// `keep_worktree` (optional, default `false`): when `true`, preserves the
/// linked worktree directory by detaching its HEAD before removing the branch
/// ref. Used by the post-merge cleanup dialog when the user unchecks the
/// "Archive/Delete worktree" step.
///
/// Async + `spawn_blocking` because the body runs `git branch -d` and may
/// remove a whole worktree directory. A plain `fn` command runs inline on the
/// IPC thread — the macOS main thread — so it froze the WebView for the length
/// of the delete. Its HTTP twin already offloaded; see
/// `docs/backend/command-threading.md`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn delete_local_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    keep_worktree: Option<bool>,
) -> Result<(), String> {
    let keep_worktree = keep_worktree.unwrap_or(false);
    {
        let repo_path = repo_path.clone();
        let branch_name = branch_name.clone();
        let workspace_id = workspace_id.clone();
        let state_arc = state.inner().clone();
        tokio::task::spawn_blocking(move || {
            delete_local_branch_impl(
                &repo_path,
                &branch_name,
                &workspace_id,
                keep_worktree,
                Some(&state_arc),
            )
        })
        .await
        .map_err(|e| format!("Task panic: {e}"))??;
    }
    if keep_worktree {
        state.invalidate_repo_caches(&repo_path);
    } else {
        // The workspace's checkout went with the branch — the sidebar row must go too.
        // `delete_local_branch_impl` refuses when the branch and the id disagree,
        // so by here `branch_name` is this workspace's own branch.
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: repo_path.clone(),
            workspace_id: workspace_id.clone(),
            branch: branch_name.clone(),
        });
    }
    Ok(())
}

/// Cached workspaces (id -> checkout) for synchronous callers (MCP handlers, etc.).
pub(crate) fn get_worktree_paths_cached(
    state: &crate::state::AppState,
    repo_path: &str,
) -> HashMap<String, WorkspaceWorktree> {
    let p = repo_path.to_string();
    (*state
        .git_cache
        .worktree_paths
        .get_with(repo_path.to_string(), || {
            std::sync::Arc::new(
                crate::git_reads::git_reads()
                    .worktree_paths(std::path::Path::new(&p))
                    .unwrap_or_default(),
            )
        }))
    .clone()
}

/// One block of `git worktree list --porcelain` output.
struct WorktreeEntry {
    path: String,
    /// Branch from the `branch refs/heads/...` line — absent while HEAD is detached.
    branch: Option<String>,
    detached: bool,
    /// Reason from a `locked <reason>` line, if the worktree is git-locked.
    /// `Some("")` means locked with no reason recorded. `None` means unlocked.
    locked_reason: Option<String>,
}

/// Prefix `lock_worktree_for_session` writes into `git worktree lock --reason`.
/// A lock whose reason carries this prefix is safe for the startup sweep to
/// clear — it's TUIC's own bookkeeping, never a user's or another tool's
/// (e.g. Claude Code's own agent locking) deliberate lock.
const TUIC_LOCK_REASON_PREFIX: &str = "tuic: ";

fn parse_worktree_entries(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();

    for block in porcelain.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let mut path: Option<String> = None;
        let mut branch: Option<String> = None;
        let mut detached = false;
        let mut locked_reason: Option<String> = None;

        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(p.to_string());
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            } else if line == "detached" {
                detached = true;
            } else if let Some(reason) = line.strip_prefix("locked ") {
                locked_reason = Some(reason.to_string());
            } else if line == "locked" {
                locked_reason = Some(String::new());
            }
        }

        if let Some(path) = path {
            entries.push(WorktreeEntry {
                path,
                branch,
                detached,
                locked_reason,
            });
        }
    }

    entries
}

/// Unlock every TUIC-owned lock (reason prefixed [`TUIC_LOCK_REASON_PREFIX`])
/// left behind by an unclean shutdown — a lock never gets a matching
/// `unlock_worktree` call if TUIC exits (crash, force-quit) while a session is
/// still attached. Run once at startup, per repo. Never touches a lock without
/// that prefix: a user's manual lock or another tool's (e.g. Claude Code's own
/// agent locking) is left exactly as-is.
///
/// Returns the number of worktrees unlocked. Best-effort throughout — a single
/// failed unlock is warned and does not stop the sweep.
pub(crate) fn sweep_stale_tuic_locks(repo_path: &str) -> usize {
    let base_repo = Path::new(repo_path);
    let out = match git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
    {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!(
                source = "worktree",
                repo = %repo_path,
                "sweep_stale_tuic_locks: git worktree list failed: {e}"
            );
            return 0;
        }
    };

    let mut swept = 0;
    for entry in parse_worktree_entries(&out.stdout) {
        let Some(reason) = entry.locked_reason else {
            continue;
        };
        if !reason.starts_with(TUIC_LOCK_REASON_PREFIX) {
            continue;
        }
        tracing::info!(
            source = "worktree",
            repo = %repo_path,
            path = %entry.path,
            reason = %reason,
            "sweep_stale_tuic_locks: clearing stale TUIC-owned lock"
        );
        unlock_worktree(base_repo, Path::new(&entry.path));
        swept += 1;
    }
    swept
}

/// Admin dir of a linked worktree: its `.git` is a *file* holding
/// `gitdir: <repo>/.git/worktrees/<name>`. Returns `None` for the main worktree (where `.git`
/// is a directory) and for paths that no longer exist.
fn worktree_admin_dir(worktree_path: &str) -> Option<PathBuf> {
    let content = std::fs::read_to_string(Path::new(worktree_path).join(".git")).ok()?;
    let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
    Some(PathBuf::from(gitdir))
}

/// Admin dir of *any* worktree, main or linked. The main worktree's `.git` is itself the
/// admin dir (a directory); a linked worktree's is resolved via `worktree_admin_dir`. Without
/// this, in-progress-operation detection only ever worked for linked worktrees — a rebase
/// started in the main checkout was invisible (GH #112's other half).
fn resolve_admin_dir(worktree_path: &str) -> Option<PathBuf> {
    if let Some(admin) = worktree_admin_dir(worktree_path) {
        return Some(admin);
    }
    let dot_git = Path::new(worktree_path).join(".git");
    dot_git.is_dir().then_some(dot_git)
}

/// Which multi-step git operation a worktree is in the middle of. Rebase and bisect detach
/// HEAD; merge/cherry-pick/revert don't, but all five leave the working tree mid-operation in
/// a way that blocks a clean removal or an automatic archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum GitOpKind {
    Rebase,
    Merge,
    CherryPick,
    Revert,
    Bisect,
}

/// Which operation (if any) the worktree is in the middle of, detected from the marker files
/// git writes into its admin dir while the operation is in flight. Rebase and bisect detach
/// HEAD, so `git worktree list --porcelain` emits no branch line and the worktree reads as
/// dead to anything keyed on that line (GH #112). Git only ever leaves one marker at a time,
/// so the check order below only matters in the (shouldn't-happen) case of leftover markers
/// from an aborted operation.
fn operation_in_progress(worktree_path: &str) -> Option<GitOpKind> {
    let admin = resolve_admin_dir(worktree_path)?;
    if admin.join("rebase-merge").exists() || admin.join("rebase-apply").exists() {
        return Some(GitOpKind::Rebase);
    }
    if admin.join("MERGE_HEAD").exists() {
        return Some(GitOpKind::Merge);
    }
    if admin.join("CHERRY_PICK_HEAD").exists() {
        return Some(GitOpKind::CherryPick);
    }
    if admin.join("REVERT_HEAD").exists() {
        return Some(GitOpKind::Revert);
    }
    if admin.join("BISECT_LOG").exists() {
        return Some(GitOpKind::Bisect);
    }
    None
}

/// True when the worktree is in the middle of a rebase / merge / cherry-pick / revert / bisect.
fn has_operation_in_progress(worktree_path: &str) -> bool {
    operation_in_progress(worktree_path).is_some()
}

/// Branch a detached worktree was on before the in-flight operation started. Git records it in
/// `head-name` for both rebase backends; merge/cherry-pick/revert never detach, so they have no
/// equivalent (and need none). Bisect records only a raw name in `BISECT_START`, which we do not
/// trust as a branch — such a worktree stays alive as an in-progress op, just without a row.
///
/// **Stays path-addressed on purpose (#726-5ac7).** Story 726 asked for this to
/// take a `workspace_id` alongside `worktree_dirtiness` and `check_worktree_dirty`,
/// and that is not implementable: this function is an *input* to id resolution,
/// not a consumer of it. `map_worktree_workspace_paths` calls it to recover the
/// branch of a detached worktree while it is building the id-keyed map, so
/// resolving an id here would need the map that this call is helping construct.
/// It reads git's state files at a directory, which is what it is addressed by.
/// The gix backend's other call site passes a path for the same reason.
pub(crate) fn operation_head_branch(worktree_path: &str) -> Option<String> {
    let admin = worktree_admin_dir(worktree_path)?;
    for backend in ["rebase-merge", "rebase-apply"] {
        let head_name = std::fs::read_to_string(admin.join(backend).join("head-name")).ok();
        if let Some(branch) = head_name
            .as_deref()
            .and_then(|s| s.trim().strip_prefix("refs/heads/"))
        {
            return Some(branch.to_string());
        }
    }
    None
}

/// One workspace's checkout, resolved by opaque workspace id.
///
/// `branch` remains ordinary data even though linked worktree ids currently
/// use the branch name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceWorktree {
    /// What is checked out here. Ordinary data: never a key, never parsed out of
    /// the id.
    pub(crate) branch: String,
    pub(crate) path: String,
    pub(crate) kind: WorkspaceKind,
}

/// The workspace id a freshly created **git worktree** gets.
///
/// The one place allowed to produce an id from a branch, and only because the
/// identity migration defines it that way: a linked worktree keeps
/// `workspace_id == branch` so nothing persisted moves. Reading it in the other
/// direction is the forbidden move — `resolve_workspace` looks an id up, it
/// never parses one.
///
pub(crate) fn workspace_id_of_worktree(branch: &str) -> String {
    branch.to_string()
}

/// Map workspace id -> its checkout. A worktree detached by an in-progress rebase keeps its
/// row: its pre-rebase branch is recovered from git's own state files, so the sidebar entry
/// survives and its terminals are not closed mid-conflict-resolution.
///
/// For a **git worktree** the id is the branch name, because the plan's identity
/// migration is exactly that: existing entries keep `workspace_id = branch`, so
/// no persisted key moves and no id is invented for data that already works.
/// This is not a placeholder — it is the migration.
fn map_worktree_workspace_paths(porcelain: &str) -> HashMap<String, WorkspaceWorktree> {
    let mut result = HashMap::new();

    for entry in parse_worktree_entries(porcelain) {
        let branch = match entry.branch {
            Some(branch) => Some(branch),
            None => operation_head_branch(&entry.path),
        };
        // Skip entries whose directory no longer exists (double safety after prune)
        if let Some(branch) = branch
            && Path::new(&entry.path).exists()
        {
            result.insert(
                workspace_id_of_worktree(&branch),
                WorkspaceWorktree {
                    branch,
                    path: entry.path,
                    kind: WorkspaceKind::Worktree,
                },
            );
        }
    }

    result
}

/// Get every linked workspace of a repository.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_worktree_paths(
    repo_path: String,
) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    let base_repo = PathBuf::from(&repo_path);
    let output = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|error| format!("git worktree list failed: {error}"))?;
    Ok(map_worktree_workspace_paths(&output.stdout))
}
/// Resolve one workspace by its opaque id.
///
/// This is the single lookup every id-taking operation goes through — removal,
/// dirtiness, branch deletion. Resolving by *branch* instead is the #726-5ac7
/// bug: a branch label and a workspace id answer different questions, and an
/// id-taking operation must not silently select a checkout by its branch.
///
/// Returns the record, so callers that need the branch (deleting the ref,
/// logging) read it off the value rather than assuming it equals the id.
fn resolve_workspace(base_repo: &Path, workspace_id: &str) -> Result<WorkspaceWorktree, String> {
    let out = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    map_worktree_workspace_paths(&out.stdout)
        .remove(workspace_id)
        .ok_or_else(|| format!("No workspace found for id '{workspace_id}'"))
}

/// Parse `git worktree list --porcelain` output and return paths of linked worktrees that are in
/// detached HEAD state (i.e. their branch has been deleted). The main worktree (first entry) is
/// always skipped — it can't be removed without removing the repo itself. A worktree detached by
/// an in-progress operation is not an orphan: its branch is coming back when the rebase ends.
fn parse_orphan_worktrees(porcelain: &str) -> Vec<String> {
    parse_worktree_entries(porcelain)
        .into_iter()
        .skip(1)
        .filter(|e| e.detached && e.branch.is_none() && !has_operation_in_progress(&e.path))
        .map(|e| e.path)
        .collect()
}

/// A worktree directory with a git operation currently in progress, and which one.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InProgressOp {
    pub path: String,
    pub kind: GitOpKind,
}

/// List worktrees that currently have a git operation in progress
/// (rebase/merge/cherry-pick/revert/bisect), and which one each is in. `map_worktree_branch_paths`
/// already recovers a mid-rebase worktree's branch, so its sidebar row survives on its own —
/// this is purely a signal for the frontend to explain *why* the row looks the way it does (e.g.
/// a "Rebasing" badge), not something the removal logic needs to consult. Includes the main
/// worktree — `operation_in_progress` resolves its admin dir just like a linked one's.
pub(crate) fn list_in_progress_worktrees(repo_path: &str) -> Result<Vec<InProgressOp>, String> {
    let out = git_cmd(Path::new(repo_path))
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    Ok(parse_worktree_entries(&out.stdout)
        .into_iter()
        .filter_map(|e| {
            let kind = operation_in_progress(&e.path)?;
            Some(InProgressOp { path: e.path, kind })
        })
        .collect())
}

/// Detect orphan worktrees: linked worktrees present on the filesystem but in detached HEAD
/// state (i.e. their branch has been deleted). Returns a list of worktree directory paths.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_orphan_worktrees(repo_path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let base_repo = PathBuf::from(&repo_path);
        let out = git_cmd(&base_repo)
            .args(["worktree", "list", "--porcelain"])
            .run()
            .map_err(|e| format!("git worktree list failed: {e}"))?;

        Ok(parse_orphan_worktrees(&out.stdout))
    })
    .await
    .map_err(|e| format!("orphan worktree detection task failed: {e}"))?
}

/// Archive an orphan worktree by its filesystem path (detached HEAD — no branch to look up).
///
/// "Orphan" detection (detached HEAD + no branch) is a heuristic — it can't distinguish a
/// branch genuinely deleted out from under the worktree from a worktree deliberately left on
/// a detached commit for some other reason. Because a false positive here is plausible and the
/// consequence of misclassifying is otherwise unrecoverable, this archives (moves aside, same
/// as the merged-branch cleanup path) rather than deleting outright. Returns the archive
/// destination path.
///
/// Safety: `worktree_path` is validated against the repo's actual worktree list to prevent
/// arbitrary directory deletion via a crafted path.
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime. Shared by the Tauri
/// command and the MCP HTTP route so the two transports can't drift apart.
pub(crate) fn remove_orphan_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    worktree_path: String,
) -> Result<String, String> {
    validate_worktree_path(&repo_path, &worktree_path)?;

    let base_repo = PathBuf::from(&repo_path);
    let path = PathBuf::from(&worktree_path);
    let archive_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| worktree_path.clone());
    let script = resolve_archive_script(&repo_path);

    let archive_path = archive_worktree_dir(&base_repo, &path, &archive_name, script.as_deref())?;
    state.invalidate_repo_caches(&repo_path);
    Ok(archive_path)
}

/// Archive an orphan worktree (Tauri command).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn remove_orphan_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    worktree_path: String,
) -> Result<String, String> {
    remove_orphan_worktree_impl(state.inner(), repo_path, worktree_path)
}

/// Hard-delete an orphan worktree by its filesystem path — no archive step, unrecoverable.
///
/// Unlike `remove_orphan_worktree`, this is only ever reached via the `OrphanCleanup::Delete`
/// setting — a deliberate, explicit opt-in to skip archiving. `On` (the default auto-cleanup
/// mode) and `Ask` both archive instead, because orphan detection is a heuristic that can
/// misclassify a worktree that was never actually abandoned.
///
/// Safety: `worktree_path` is validated against the repo's actual worktree list to prevent
/// arbitrary directory deletion via a crafted path.
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime. Shared by the Tauri
/// command and the MCP HTTP route so the two transports can't drift apart.
pub(crate) fn delete_orphan_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    worktree_path: String,
) -> Result<(), String> {
    validate_worktree_path(&repo_path, &worktree_path)?;

    let base_repo = PathBuf::from(&repo_path);
    let path = PathBuf::from(&worktree_path);
    let worktree = WorktreeInfo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| worktree_path.clone()),
        path,
        branch: None,
        base_repo,
    };
    // This path is only reached via the explicit `OrphanCleanup::Delete`
    // opt-in — the user already chose "hard-delete, no archive, unrecoverable"
    // as a standing setting. Still refuse a live session outright: an orphan
    // (detached HEAD, no branch) is a heuristic classification, and a session
    // attached to it is strong evidence it's not actually abandoned.
    let attached = state.live_sessions_in_worktree(&worktree.path);
    if !attached.is_empty() {
        return Err(format!(
            "{BUSY_WORKTREE_PREFIX}{} session(s) attached",
            attached.len()
        ));
    }
    remove_worktree_internal(&worktree, RemovalMode::Dirty)?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Hard-delete an orphan worktree (Tauri command).
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn delete_orphan_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    worktree_path: String,
) -> Result<(), String> {
    delete_orphan_worktree_impl(state.inner(), repo_path, worktree_path)
}

/// Validate that `worktree_path` is a known worktree of the given repo by checking it against
/// `git worktree list --porcelain` output. Prevents arbitrary directory deletion.
pub(crate) fn validate_worktree_path(repo_path: &str, worktree_path: &str) -> Result<(), String> {
    let path = PathBuf::from(worktree_path);
    if !path.is_absolute() {
        return Err("worktree_path must be an absolute path".to_string());
    }

    let out = git_cmd(Path::new(repo_path))
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    let known_paths: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .collect();

    if !known_paths.contains(&worktree_path) {
        return Err(format!(
            "Refused: '{}' is not a known worktree of '{}'",
            worktree_path, repo_path
        ));
    }

    Ok(())
}

/// Generate a worktree name (Story 063)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn generate_worktree_name_cmd(existing_names: Vec<String>) -> String {
    generate_worktree_name(&existing_names)
}

/// Generate a hybrid clone branch name: `{sanitized_source}--{random_name}`
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn generate_clone_branch_name_cmd(
    source_branch: String,
    existing_names: Vec<String>,
) -> String {
    generate_clone_branch_name(&source_branch, &existing_names)
}

/// List local branch names for a repository (excludes HEAD and remote-only refs)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn list_local_branches(repo_path: String) -> Result<Vec<String>, String> {
    let out = git_cmd(Path::new(&repo_path))
        .args(["branch", "--format=%(refname:short)"])
        .run()
        .map_err(|e| format!("git branch failed: {e}"))?;

    let branches: Vec<String> = out
        .stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(branches)
}

/// Get the remote default branch for a repo.
///
/// Tries `git symbolic-ref refs/remotes/origin/HEAD` first, then falls back
/// to checking if `main` or `master` exist as local branches.
pub(crate) fn get_remote_default_branch(repo_path: &str) -> Result<String, String> {
    // Try symbolic-ref first (cheapest, no network)
    if let Some(out) = git_cmd(Path::new(repo_path))
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .run_silent()
    {
        let trimmed = out.stdout.trim().to_string();
        // Output is like "refs/remotes/origin/main"
        if let Some(branch) = trimmed.strip_prefix("refs/remotes/origin/")
            && !branch.is_empty()
        {
            return Ok(branch.to_string());
        }
    }

    // Fallback: check if main or master branches exist locally
    let branches = list_local_branches(repo_path.to_string()).unwrap_or_default();
    if branches.iter().any(|b| b == "main") {
        return Ok("main".to_string());
    }
    if branches.iter().any(|b| b == "master") {
        return Ok("master".to_string());
    }

    // Last resort: return "main"
    Ok("main".to_string())
}

/// Fetch a remote ref if the ref name is a remote tracking branch (e.g. "origin/main").
/// Local refs are a no-op. Returns Ok(()) on success or if the ref is local.
pub(crate) fn fetch_if_remote(repo_path: &str, ref_name: &str) -> Result<(), String> {
    // A "/" alone does NOT mean remote — local branches routinely contain slashes
    // (e.g. "POC-0001/merge-radar", "feature/foo"). Only fetch when the ref actually
    // resolves as a remote-tracking ref under refs/remotes/.
    let is_remote_ref = git_cmd(Path::new(repo_path))
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{ref_name}"),
        ])
        .run_silent()
        .is_some();
    if !is_remote_ref {
        return Ok(());
    }
    if let Some(slash_pos) = ref_name.find('/') {
        let remote = &ref_name[..slash_pos];
        let branch = &ref_name[slash_pos + 1..];
        if !remote.is_empty() && !branch.is_empty() {
            git_cmd(Path::new(repo_path))
                .timeout(FETCH_TIMEOUT)
                .args(["fetch", remote, branch])
                .run()
                .map_err(|e| format!("Failed to fetch {ref_name}: {e}"))?;
        }
    }
    Ok(())
}

/// Persist the base ref for a branch in git config.
/// Stored as `branch.<name>.tuicommander-base` in `.git/config`.
pub(crate) fn set_branch_base(
    repo_path: &str,
    branch_name: &str,
    base_ref: &str,
) -> Result<(), String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key, base_ref])
        .run()
        .map_err(|e| format!("Failed to set branch base: {e}"))?;
    Ok(())
}

/// Read the stored base ref for a branch from git config.
/// Returns None if not set.
pub(crate) fn get_branch_base(repo_path: &str, branch_name: &str) -> Option<String> {
    let key = format!("branch.{branch_name}.tuicommander-base");
    git_cmd(Path::new(repo_path))
        .args(["config", &key])
        .run_silent()
        .map(|out| out.stdout.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read every branch's stored base ref in a single git subprocess.
///
/// Returns a map of branch name -> base ref, parsed from the
/// `branch.<name>.tuicommander-base` config entries. Empty when none are set
/// or the lookup fails. Replaces N sequential per-branch `git config` calls in
/// `apply_base_ahead_behind_and_sort`.
pub(crate) fn get_branch_bases(repo_path: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(out) = git_cmd(Path::new(repo_path))
        .args(["config", "--get-regexp", r"^branch\..*\.tuicommander-base$"])
        .run_silent()
    else {
        return map;
    };
    for line in out.stdout.lines() {
        // Each line: `branch.<name>.tuicommander-base <base-ref>`. The key and
        // value are whitespace-separated; the branch name is the middle of the
        // key (may contain dots, so anchor on both prefix and suffix).
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(name) = key
            .strip_prefix("branch.")
            .and_then(|k| k.strip_suffix(".tuicommander-base"))
        {
            map.insert(name.to_string(), value.to_string());
        }
    }
    map
}

/// A base ref option with metadata for grouped dropdown display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BaseRefOption {
    pub name: String,
    /// "local" or "remote"
    pub kind: String,
    /// Whether this is the default branch (e.g. main/master)
    pub is_default: bool,
}

/// List available base ref options for branch/worktree creation.
///
/// Returns structured refs: default branch first (flagged), then local branches,
/// then remote tracking branches. Filters out origin/HEAD and deduplicates
/// where a local branch has the same name as its remote tracking branch.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn list_base_ref_options(repo_path: String) -> Result<Vec<BaseRefOption>, String> {
    let default_branch = get_remote_default_branch(&repo_path)?;
    let repo = Path::new(&repo_path);

    // Get all refs (local + remote) in one git call
    let out = git_cmd(repo)
        .args([
            "for-each-ref",
            "--format=%(refname:short)\t%(refname)",
            "refs/heads/",
            "refs/remotes/",
        ])
        .run()
        .map_err(|e| format!("git for-each-ref failed: {e}"))?;

    let mut local_refs: Vec<BaseRefOption> = Vec::new();
    let mut remote_refs: Vec<BaseRefOption> = Vec::new();
    let mut local_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in out.stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 {
            continue;
        }

        let short_name = parts[0].to_string();
        let full_ref = parts[1];

        if full_ref.starts_with("refs/heads/") {
            local_names.insert(short_name.clone());
            if short_name != default_branch {
                local_refs.push(BaseRefOption {
                    name: short_name,
                    kind: "local".to_string(),
                    is_default: false,
                });
            }
        } else if full_ref.starts_with("refs/remotes/") {
            // Skip origin/HEAD (synthetic ref)
            if short_name.ends_with("/HEAD") {
                continue;
            }
            remote_refs.push(BaseRefOption {
                name: short_name,
                kind: "remote".to_string(),
                is_default: false,
            });
        }
    }

    // Sort alphabetically within each group
    local_refs.sort_by(|a, b| a.name.cmp(&b.name));
    remote_refs.sort_by(|a, b| a.name.cmp(&b.name));

    // Build result: default first, then local, then remote
    let mut result = Vec::with_capacity(1 + local_refs.len() + remote_refs.len());
    result.push(BaseRefOption {
        name: default_branch,
        kind: "local".to_string(),
        is_default: true,
    });
    result.extend(local_refs);
    result.extend(remote_refs);

    Ok(result)
}

/// Result of switching the main worktree to a different branch.
#[derive(Clone, Serialize)]
pub(crate) struct SwitchBranchResult {
    pub(crate) success: bool,
    /// True if changes were auto-stashed before checkout
    pub(crate) stashed: bool,
    pub(crate) previous_branch: String,
    pub(crate) new_branch: String,
}

/// Switch the main worktree to a different branch.
///
/// Runs `git checkout` directly (no PTY involvement) so it's safe even when
/// terminals have editors or processes running — the caller is responsible
/// for checking terminal busy-state before invoking this.
///
/// When `stash` is true, performs `git stash push` before checkout and
/// does NOT auto-pop (the user can pop manually).
/// When `force` is true, passes `--force` to discard uncommitted changes.
/// Core logic for switching the checked-out branch. Blocking — callers wrap in
/// `spawn_blocking` when on an async runtime.
pub(crate) fn switch_branch_impl(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    let base_repo = PathBuf::from(&repo_path);

    // `git checkout <ref>` has no end-of-options `--` form that keeps <ref> a
    // branch (post-`--` args become pathspecs), so validate instead — a name
    // beginning with `-` would otherwise be parsed as an option.
    crate::git::validate_branch_name(&branch_name)?;

    // Read current branch before switching
    let previous_branch = crate::git::read_branch_from_head(&base_repo).unwrap_or_default();

    if previous_branch == branch_name {
        return Ok(SwitchBranchResult {
            success: true,
            stashed: false,
            previous_branch: previous_branch.clone(),
            new_branch: previous_branch,
        });
    }

    // Check for uncommitted changes (unless force or stash)
    if !force && !stash {
        let status_out = git_cmd(&base_repo)
            .args(["status", "--porcelain"])
            .run()
            .map_err(|e| format!("Failed to check working tree status: {e}"))?;
        if !status_out.stdout.trim().is_empty() {
            return Err("dirty".to_string());
        }
    }

    // Stash if requested
    let did_stash = if stash {
        let stash_msg = format!("auto-stash before switching to {branch_name}");
        let stash_out = git_cmd(&base_repo)
            .args(["stash", "push", "-m", &stash_msg])
            .run()
            .map_err(|e| {
                crate::git_locks::describe_stale_lock(&base_repo)
                    .unwrap_or_else(|| format!("Stash failed: {e}"))
            })?;

        // "No local changes to save" means nothing was stashed
        !stash_out.stdout.contains("No local changes to save")
    } else {
        false
    };

    // Checkout
    let mut args = vec!["checkout"];
    if force {
        args.push("--force");
    }
    args.push(&branch_name);

    git_cmd(&base_repo).args(&args).run().map_err(|e| {
        crate::git_locks::describe_stale_lock(&base_repo)
            .unwrap_or_else(|| format!("Checkout failed: {e}"))
    })?;

    state.invalidate_repo_caches(&repo_path);

    Ok(SwitchBranchResult {
        success: true,
        stashed: did_stash,
        previous_branch,
        new_branch: branch_name,
    })
}

/// Switch the checked-out branch (Tauri command).
///
/// Async + `spawn_blocking`: a checkout stashes, rewrites the working tree and
/// can wait on an index lock. Inline on the IPC thread that is a frozen WebView
/// for the whole operation — see `docs/backend/command-threading.md`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn switch_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        switch_branch_impl(&state, repo_path, branch_name, force, stash)
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

/// Create a local branch tracking a remote branch and switch to it.
/// Equivalent to `git checkout -b <branch> origin/<branch>`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn checkout_remote_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
) -> Result<(), String> {
    let base_repo = PathBuf::from(&repo_path);
    let remote_ref = format!("origin/{branch_name}");

    git_cmd(&base_repo)
        .args(["checkout", "-b", &branch_name, &remote_ref])
        .run()
        .map_err(|e| format!("Checkout failed: {e}"))?;

    state.invalidate_repo_caches(&repo_path);
    Ok(())
}

/// Result of a merge-and-archive operation
#[derive(Clone, Serialize)]
pub(crate) struct MergeArchiveResult {
    /// Whether the merge succeeded
    pub(crate) merged: bool,
    /// What happened to the worktree (archived / deleted / pending user choice /
    /// needs_confirmation — nothing was touched, the caller must confirm)
    pub(crate) action: String,
    /// Path to archived directory (if archived)
    pub(crate) archive_path: Option<String>,
    /// Commits the branch had that the target did not, measured BEFORE the merge.
    /// 0 means the merge was a no-op ("Already up to date") — the worktree is about
    /// to disappear from the sidebar without contributing anything.
    pub(crate) commits_ahead: usize,
    /// Whether the worktree had uncommitted changes at pre-flight time.
    pub(crate) worktree_dirty: bool,
}

/// Whether a branch's worktree holds uncommitted work.
///
/// Tri-state on purpose. "We could not tell" is not the same answer as "clean",
/// and when the answer gates an irreversible delete it must not collapse into it.
pub(crate) enum WorktreeDirtiness {
    /// The worktree exists and `git status` reported nothing, or the branch has
    /// no worktree at all — either way there is no uncommitted work to lose.
    Clean,
    /// `git status` reported uncommitted work.
    Dirty,
    /// A git command failed, so the question is unanswered. Carries the reason.
    Unknown(String),
}

impl WorktreeDirtiness {
    /// True only when git actually reported uncommitted work.
    pub(crate) fn is_dirty(&self) -> bool {
        matches!(self, WorktreeDirtiness::Dirty)
    }

    /// True unless the worktree is known to be clean. An irreversible cleanup
    /// needs a positive answer, and silence is not one.
    fn blocks_cleanup(&self) -> bool {
        !matches!(self, WorktreeDirtiness::Clean)
    }
}

/// Ask git whether the workspace `workspace_id` names has uncommitted work.
///
/// Addressed by id, not branch: this answer gates an irreversible cleanup, so
/// it must inspect the exact checkout the caller intends to remove (#726-5ac7).
///
/// The three outcomes are kept apart deliberately: an id with no checkout has
/// nothing to lose (Clean), while a git command that failed tells us nothing
/// (Unknown). Folding the second into the first is what let a dirty worktree be
/// force-removed on a transient git error.
pub(crate) fn worktree_dirtiness(base_repo: &Path, workspace_id: &str) -> WorktreeDirtiness {
    let path = match resolve_any_workspace(base_repo, workspace_id) {
        Ok(workspace) => PathBuf::from(workspace.path),
        Err(error) if error.starts_with("No workspace found") => return WorktreeDirtiness::Clean,
        Err(error) => return WorktreeDirtiness::Unknown(error),
    };

    match dirty_at(&path) {
        Ok(false) => WorktreeDirtiness::Clean,
        Ok(true) => WorktreeDirtiness::Dirty,
        Err(e) => WorktreeDirtiness::Unknown(e),
    }
}

/// The single gate every destructive worktree cleanup passes through.
///
/// Both entry points — `merge_and_archive_worktree_impl` and
/// `finalize_merged_worktree_impl` — call this, so the two cleanup paths cannot
/// drift apart. `force` is the user's confirmation, arriving from the frontend
/// after the dialog explained what is about to be destroyed.
fn cleanup_needs_confirmation(action: &str, force: bool, dirt: &WorktreeDirtiness) -> bool {
    let cleans_up = action == "archive" || action == "delete";
    if !cleans_up || force {
        return false;
    }
    if let WorktreeDirtiness::Unknown(reason) = dirt {
        tracing::warn!(
            source = "worktree",
            "Cleanup blocked: could not confirm the worktree is clean ({reason})"
        );
    }
    dirt.blocks_cleanup()
}

/// Refuse an automatic archive/delete when the branch's worktree has a git operation in
/// progress (rebase/merge/cherry-pick/revert/bisect). Checked with an attached HEAD in mind —
/// a merge or cherry-pick conflict never detaches, so it isn't already caught by
/// `worktree_dirtiness` unless the conflict also left the index dirty, and a *clean* mid-bisect
/// or mid-cherry-pick worktree would otherwise sail through `cleanup_needs_confirmation`.
///
/// This is a hard error, not a `needs_confirmation` — unlike plain dirtiness there is no sensible
/// "yes, destroy my in-flight rebase" answer to offer unattended, so `force` does not bypass it.
///
/// Used only by the *automatic* consequences of a merge (auto-archive-merged,
/// finalize-after-merge) — not the plain manual "remove this worktree" command, which stays a
/// deliberate user override.
fn err_if_workspace_worktree_busy(base_repo: &Path, workspace_id: &str) -> Result<(), String> {
    // Keyed by workspace id, not branch: `find_worktree_path_for_branch` was
    // deleted with #726-5ac7 (two workspaces on one branch made every
    // branch-keyed path lookup ambiguous) — resolve the record and use its path.
    let workspace = resolve_workspace(base_repo, workspace_id)?;
    if has_operation_in_progress(&workspace.path) {
        return Err(format!(
            "Cannot finalize worktree for branch '{}': a git operation \
             (rebase/merge/cherry-pick) is in progress",
            workspace.branch
        ));
    }
    Ok(())
}

/// What the pre-flight learned about a worktree branch before we merge it.
pub(crate) struct MergePreflight {
    pub(crate) commits_ahead: usize,
    pub(crate) worktree_dirty: WorktreeDirtiness,
}

/// Count commits on `branch` that `target` does not have, and check whether the
/// workspace `workspace_id` names has uncommitted changes.
///
/// Two keys because there are two questions: the commit count is about a *branch*
/// and the dirty check is about one exact *checkout*. Conflating them can inspect
/// a different path than the caller intends to clean up.
///
/// This is what tells a real merge apart from an "Already up to date" no-op. Both
/// succeed as far as `git merge` is concerned, but only one of them justifies
/// making the worktree row disappear.
///
/// A failing rev-list is not fatal — the merge was asked for and deserves to run,
/// so the count falls back to 0. The dirty check is different: it gates a delete,
/// so its failure is reported as Unknown rather than swallowed. See
/// `cleanup_needs_confirmation`.
pub(crate) fn merge_preflight(
    repo_path: &str,
    branch_name: &str,
    workspace_id: &str,
    target_branch: &str,
) -> MergePreflight {
    let base_repo = Path::new(repo_path);
    let commits_ahead = git_cmd(base_repo)
        .args([
            "rev-list",
            "--count",
            &format!("{target_branch}..{branch_name}"),
        ])
        .run()
        .ok()
        .and_then(|out| out.stdout.trim().parse::<usize>().ok())
        .unwrap_or(0);

    MergePreflight {
        commits_ahead,
        worktree_dirty: worktree_dirtiness(base_repo, workspace_id),
    }
}

/// Complete a pending merge by archiving or deleting the worktree.
///
/// Called after `merge_and_archive_worktree` returns `action: "pending"` (ask mode),
/// and by the auto-archive-merged sweep, which has no user in the loop at all. The
/// merge has already succeeded; this only handles the worktree cleanup.
///
/// `force` is the user's confirmation that a dirty worktree may be destroyed.
/// Without it, a worktree that is not known to be clean is left untouched and the
/// caller gets `needs_confirmation` — the same contract
/// `merge_and_archive_worktree_impl` uses, through the same gate.
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime.
pub(crate) fn finalize_merged_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = std::path::PathBuf::from(&repo_path);

    err_if_workspace_worktree_busy(&base_repo, &workspace_id)?;

    let dirt = worktree_dirtiness(&base_repo, &workspace_id);
    if cleanup_needs_confirmation(&action, force, &dirt) {
        return Ok(MergeArchiveResult {
            merged: true, // The merge itself already happened; only cleanup stopped.
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: 0,
            worktree_dirty: dirt.is_dirty(),
        });
    }

    match action.as_str() {
        "archive" => {
            // Read the branch off the record BEFORE the directory moves: archiving
            // takes the worktree out of `git worktree list`, after which the id
            // resolves to nothing and the removal event would have no branch to
            // show. Resolution here is also still safe to fail — nothing has been
            // mutated yet.
            let branch = resolve_workspace(&base_repo, &workspace_id)?.branch;
            let archive_path = archive_worktree(&base_repo, &workspace_id, script.as_deref())?;
            // Archiving moves the worktree out of the repo — as far as the sidebar
            // is concerned the row is gone, same as a delete.
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
                // The merge already happened in the "pending" call that preceded
                // this one; its pre-flight numbers were reported there.
                commits_ahead: 0,
                worktree_dirty: dirt.is_dirty(),
            })
        }
        "delete" => {
            // `cleanup_needs_confirmation` above already refused unless the
            // worktree is known clean or `force` explicitly confirmed destroying
            // it — so Dirty (not Safe) is correct here: it does the confirmed
            // override without also reaching for the lock-breaking Forced mode,
            // which this path has no business touching.
            let mode = if force {
                RemovalMode::Dirty
            } else {
                RemovalMode::Safe
            };
            let outcome = remove_worktree_by_workspace_id(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                mode,
                Some(state),
                false,
            )?;
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
                commits_ahead: 0,
                worktree_dirty: dirt.is_dirty(),
            })
        }
        _ => Err(format!(
            "Unknown action '{action}': expected 'archive' or 'delete'"
        )),
    }
}

/// Finalize a pending merge by archiving/deleting the worktree (Tauri command).
///
/// Async + `spawn_blocking`: archiving moves a directory and deleting removes
/// one, both unbounded. `finalize_merged_worktree_http` already offloaded, so
/// the same work froze the WebView over IPC and not over HTTP — the drift
/// `docs/backend/command-threading.md` exists to prevent.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn finalize_merged_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: Option<bool>,
) -> Result<MergeArchiveResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        finalize_merged_worktree_impl(
            &state,
            repo_path,
            workspace_id,
            action,
            force.unwrap_or(false),
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?
}

/// Merge a worktree branch into a target branch, then archive or delete the worktree.
///
/// Steps:
/// 1. `git checkout <target_branch>` (in the base repo)
/// 2. `git merge <source_branch>` (in the base repo)
/// 3. Based on `after_merge`: archive (move dir) or delete (remove worktree + branch)
///
/// Blocking — callers wrap in `spawn_blocking` when on an async runtime.
pub(crate) fn merge_and_archive_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = PathBuf::from(&repo_path);

    // 0. Pre-flight: would the cleanup take uncommitted work with it? Archive moves
    //    the directory aside and delete removes it outright, but either way any
    //    worktree not known to be clean must be confirmed first — whether or not
    //    the branch carries commits. `commits_ahead` is reported alongside so the
    //    dialog can also say that an empty branch's merge would be a no-op.
    let preflight = merge_preflight(&repo_path, &branch_name, &workspace_id, &target_branch);
    if cleanup_needs_confirmation(&after_merge, force, &preflight.worktree_dirty) {
        return Ok(MergeArchiveResult {
            merged: false,
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: preflight.commits_ahead,
            worktree_dirty: preflight.worktree_dirty.is_dirty(),
        });
    }

    // 1. Ensure we're on the target branch in the base repo
    git_cmd(&base_repo)
        .args(["checkout", &target_branch])
        .run()
        .map_err(|e| {
            crate::git_locks::describe_stale_lock(&base_repo)
                .unwrap_or_else(|| format!("Failed to checkout {target_branch}: {e}"))
        })?;

    // 2. Merge the source branch
    if let Err(e) = git_cmd(&base_repo)
        .args(["merge", &branch_name, "--no-edit"])
        .run()
    {
        return Err(finish_failed_git_operation_after_abort(
            &base_repo,
            "merge",
            "Merge failed",
            e,
        ));
    }

    // 3. Handle the worktree based on after_merge setting
    let MergePreflight {
        commits_ahead,
        worktree_dirty,
    } = preflight;
    let worktree_dirty = worktree_dirty.is_dirty();
    match after_merge.as_str() {
        "archive" => {
            // Before the move, for the same reason as in `finalize_merged_worktree_impl`:
            // an archived worktree no longer resolves by id. `branch_name` is the
            // merge subject the caller named, which is not necessarily what this
            // workspace has checked out — the record is.
            let branch = resolve_workspace(&base_repo, &workspace_id)?.branch;
            err_if_workspace_worktree_busy(&base_repo, &workspace_id)?;
            let archive_path = archive_worktree(&base_repo, &workspace_id, script.as_deref())?;
            // Archiving moves the worktree out of the repo — as far as the sidebar
            // is concerned the row is gone, same as a delete.
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "archived".to_string(),
                archive_path: Some(archive_path),
                commits_ahead,
                worktree_dirty,
            })
        }
        "delete" => {
            err_if_workspace_worktree_busy(&base_repo, &workspace_id)?;
            // See the comment on the equivalent branch in
            // `finalize_merged_worktree_impl`: `cleanup_needs_confirmation` above
            // already gated this on `force`, so Dirty (never Forced) is correct.
            let mode = if force {
                RemovalMode::Dirty
            } else {
                RemovalMode::Safe
            };
            let outcome = remove_worktree_by_workspace_id(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                mode,
                Some(state),
                false,
            )?;
            state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                repo_path: repo_path.clone(),
                workspace_id: workspace_id.clone(),
                branch: outcome.branch,
            });
            Ok(MergeArchiveResult {
                merged: true,
                action: "deleted".to_string(),
                archive_path: None,
                commits_ahead,
                worktree_dirty,
            })
        }
        _ => {
            // "ask" — merge succeeded, let frontend decide what to do next
            state.invalidate_repo_caches(&repo_path);
            Ok(MergeArchiveResult {
                merged: true,
                action: "pending".to_string(),
                archive_path: None,
                commits_ahead,
                worktree_dirty,
            })
        }
    }
}

/// Merge a worktree branch into a target branch, then archive/delete (Tauri command).
///
/// `force` skips the pre-flight guard that refuses to clean up a branch carrying no
/// commits while its worktree is dirty. The frontend sets it after the user confirms.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn merge_and_archive_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: Option<bool>,
) -> Result<MergeArchiveResult, String> {
    merge_and_archive_worktree_impl(
        state.inner(),
        repo_path,
        branch_name,
        workspace_id,
        target_branch,
        after_merge,
        force.unwrap_or(false),
    )
}

/// Archive a worktree: move its directory to `{worktrees_dir}/__archived/{branch_name}/`
/// and run `git worktree remove`.
///
/// If `archive_script` is provided (non-empty), it runs in the worktree directory
/// before archiving. A non-zero exit code aborts the operation.
/// Pick a non-colliding archive destination under `archive_dir` for `sanitized`.
///
/// Returns `archive_dir/sanitized` when free, otherwise the first free
/// `sanitized-2`, `sanitized-3`, … so archiving the same branch twice never
/// clobbers a prior archive. Single-user local tool: a find-first-free-suffix
/// loop is fine, no TOCTOU hardening needed.
fn free_archive_dest(archive_dir: &Path, sanitized: &str) -> PathBuf {
    let base = archive_dir.join(sanitized);
    if !base.exists() {
        return base;
    }
    let mut counter = 2;
    loop {
        let candidate = archive_dir.join(format!("{sanitized}-{counter}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

pub(crate) fn archive_worktree(
    base_repo: &Path,
    workspace_id: &str,
    archive_script: Option<&str>,
) -> Result<String, String> {
    // Resolve the checkout by id — the archive directory name is derived from the
    // record's branch, so a same-branch sibling can never be the one moved away.
    let workspace = resolve_workspace(base_repo, workspace_id)?;
    let wt_path = PathBuf::from(&workspace.path);

    archive_worktree_dir(base_repo, &wt_path, &workspace.branch, archive_script)
}

/// Archive a worktree directory by path rather than by branch lookup. Used for orphan
/// (detached-HEAD) worktrees, which by definition have no branch to look up via
/// `find_worktree_path_for_branch`.
///
/// `archive_name` seeds the destination directory name under `__archived/` (run through
/// `sanitize_name`); callers typically pass the branch name or, for orphans, the worktree
/// directory's own basename.
pub(crate) fn archive_worktree_dir(
    base_repo: &Path,
    wt_path: &Path,
    archive_name: &str,
    archive_script: Option<&str>,
) -> Result<String, String> {
    // Run archive script before archiving (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, wt_path).map_err(|e| format!("Archive script failed: {e}"))?;
    }
    let parent_dir = wt_path.parent().ok_or("Worktree has no parent directory")?;
    let archive_dir = parent_dir.join("__archived");
    let sanitized = sanitize_name(archive_name);
    let mut archive_dest = archive_dir.join(&sanitized);

    // Create archive directory
    std::fs::create_dir_all(&archive_dir)
        .map_err(|e| format!("Failed to create archive directory: {e}"))?;

    // Move the directory out FIRST. `git worktree remove --force` DELETES
    // uncommitted work, so removing before the rename made "archive" exactly as
    // destructive as "delete" for a dirty worktree — the rename then found
    // nothing left to move and silently did nothing.
    if wt_path.exists() {
        // Archive is the non-destructive alternative to delete — never clobber a
        // prior archive for the same branch/archive name; land on the next free suffix.
        archive_dest = free_archive_dest(&archive_dir, &sanitized);
        std::fs::rename(wt_path, &archive_dest)
            .map_err(|e| format!("Failed to move worktree to archive: {e}"))?;
    }

    // The directory is out of the repo now; drop git's administrative entry for
    // it. Unlock first — `prune` skips locked worktrees and would leave a ghost
    // row in the sidebar.
    let wt_path_str = wt_path.to_string_lossy().to_string();
    let _ = git_cmd(base_repo)
        .args(["worktree", "unlock", &wt_path_str])
        .run();
    if let Err(e) = git_cmd(base_repo).args(["worktree", "prune"]).run() {
        tracing::warn!(
            source = "worktree",
            "Archive: failed to prune the worktree entry: {e}"
        );
    }

    Ok(archive_dest.to_string_lossy().to_string())
}

/// Deadline for a user-supplied worktree script.
///
/// These are the user's own scripts — `pnpm install`, `cargo build`, a cleanup
/// hook — so the deadline is not there to bound how long a build may take. It is
/// there for the script that will never finish: one reading a stdin it can never
/// be given (`apply_no_window` leaves no window to type into, which is what
/// issue #7 reported), or waiting on a lock nobody will release. Fifteen minutes
/// sits above any plausible cold-cache install-and-build, so no real setup dies
/// on it.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(900);

/// Run `script` through the platform shell in `cwd`, killing it at `timeout`.
///
/// Both callers pass [`SCRIPT_TIMEOUT`]; the parameter is what lets a test drive
/// the kill path without waiting a quarter of an hour for it.
fn run_shell_script(
    script: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut cmd = std::process::Command::new(shell);
    cmd.arg(flag).arg(script).current_dir(cwd);
    crate::cli::apply_no_window(&mut cmd);
    crate::git_cli::output_with_deadline(&mut cmd, timeout).map_err(|e| match e {
        crate::git_cli::GitError::TimedOut { after } => format!(
            "Script timed out after {:.0}s and was killed",
            after.as_secs_f64()
        ),
        e => format!("Failed to execute script: {e}"),
    })
}

/// Run a shell script in a directory and return an error if it exits non-zero.
///
/// Used by archive/delete operations to run cleanup scripts before the operation.
fn run_script_in_dir(script: &str, cwd: &Path) -> Result<(), String> {
    let output = run_shell_script(script, cwd, SCRIPT_TIMEOUT)?;

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code != 0 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Script failed with exit code {exit_code}: {stderr}"
        ));
    }
    Ok(())
}

/// Run a shell script in a given directory and return exit code + output.
///
/// Used to execute setup/run scripts after worktree creation.
/// The script is passed to `sh -c` (Unix) or `cmd /C` (Windows).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn run_setup_script(script: String, cwd: String) -> Result<serde_json::Value, String> {
    let cwd = crate::cli::expand_tilde(&cwd);
    let cwd_path = Path::new(&cwd);
    if !cwd_path.exists() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }

    let output = run_shell_script(&script, cwd_path, SCRIPT_TIMEOUT)?;

    Ok(serde_json::json!({
        "exit_code": output.status.code().unwrap_or(-1),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreeStorage;
    use crate::test_support::{fail_with_stderr_script, print_file_script, touch_script};
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// The post-merge cleanup dialog runs these one after another, and each was
    /// a plain `fn` command: a checkout, a branch delete that can take a whole
    /// worktree with it, and an archive that moves a directory. A plain `fn`
    /// runs inline on the IPC thread — the macOS main thread — so pressing
    /// Execute froze the WebView for the length of every step. Their HTTP twins
    /// in `mcp_http/worktree_routes.rs` were already on the blocking pool, which
    /// is exactly the transport drift `docs/backend/command-threading.md` names.
    #[test]
    fn post_merge_cleanup_commands_never_run_on_the_ipc_thread() {
        let source = include_str!("worktree.rs");
        for command in [
            "switch_branch",
            "delete_local_branch",
            "finalize_merged_worktree",
        ] {
            let signature = format!("pub(crate) async fn {command}(");
            let at = source.find(&signature).unwrap_or_else(|| {
                panic!("{command} must be async: a plain fn command runs on the macOS main thread")
            });
            // Async alone only moves the work to a Tokio worker; the body still
            // runs git subprocesses and recursive deletes, so it needs the
            // blocking pool as well.
            // By chars, not bytes: this file has em dashes in its comments, and
            // a byte window can end inside one. Where it lands depends on the
            // line endings, so on Windows the same slice panicked.
            let body: String = source[at..].chars().take(800).collect();
            assert!(
                body.contains("spawn_blocking"),
                "{command} awaits blocking git work and must hand it to spawn_blocking"
            );
        }
    }

    fn setup_test_repo() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_path = temp_dir.path();

        git_cmd(repo_path)
            .args(["init"])
            .run()
            .expect("Failed to init git repo");
        git_cmd(repo_path)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .expect("Failed to config git");
        git_cmd(repo_path)
            .args(["config", "user.name", "Test"])
            .run()
            .expect("Failed to config git");

        fs::write(repo_path.join("README.md"), "# Test").expect("Failed to write file");
        git_cmd(repo_path)
            .args(["add", "."])
            .run()
            .expect("Failed to git add");
        git_cmd(repo_path)
            .args(["commit", "-m", "Initial commit"])
            .run()
            .expect("Failed to git commit");

        temp_dir
    }

    /// Isolate tests that exercise repository configuration. The override guard
    /// is deliberately returned with the directory so it stays live for the
    /// entire test and no write can reach the user's real config.
    fn with_temp_config_dir() -> (impl Drop, TempDir) {
        let config = TempDir::new().expect("config dir");
        let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        (guard, config)
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my-task"), "my-task");
        assert_eq!(sanitize_name("My Task Name"), "my-task-name");
        assert_eq!(sanitize_name("task/with/slashes"), "task-with-slashes");
        assert_eq!(
            sanitize_name("task_with_underscores"),
            "task_with_underscores"
        );
        assert_eq!(sanitize_name("UPPERCASE"), "uppercase");
        assert_eq!(sanitize_name("special!@#chars"), "special---chars");
    }

    #[test]
    fn test_create_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "test-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result.is_ok(), "Failed to create worktree: {:?}", result);

        let worktree = result.unwrap();
        assert_eq!(worktree.name, "test-task");
        assert!(worktree.path.exists(), "Worktree path should exist");
    }

    #[test]
    fn test_create_worktree_with_new_branch() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feature-branch-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/new-feature".to_string()),
            create_branch: true,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(
            result.is_ok(),
            "Failed to create worktree with branch: {:?}",
            result
        );

        let worktree = result.unwrap();
        assert_eq!(worktree.branch, Some("feature/new-feature".to_string()));
    }

    /// Regression (story 120-797d): a branch/ref beginning with `-` — e.g. an
    /// attacker-chosen PR head_ref like `--upload-pack=...` — must reach
    /// `git worktree add` as DATA, never be parsed as a git OPTION. The `--`
    /// end-of-options guard turns an injection attempt into a plain
    /// "invalid reference" failure instead of "unknown option".
    #[test]
    fn test_create_worktree_dash_ref_treated_as_data() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "dash-ref-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            // Checkout of an existing ref (create_branch = false) whose name looks
            // like a git option — the classic argument-injection payload.
            branch: Some("--upload-pack=touch /tmp/pwned".to_string()),
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        let err = result.expect_err("worktree add on a nonexistent dash-ref must fail");

        // Without `--`, git parses it as an option ("unknown option"/"unknown switch").
        // With the guard, git resolves it as a ref and fails "invalid reference".
        assert!(
            !err.contains("unknown option") && !err.contains("unknown switch"),
            "dash-prefixed ref was parsed as a git OPTION, not data: {err}"
        );
        assert!(
            err.contains("invalid reference"),
            "expected git to reject the ref as data (invalid reference), got: {err}"
        );
    }

    #[test]
    fn test_create_worktree_idempotent() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "idempotent-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        // Create twice - should not fail
        let result1 = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result1.is_ok());

        let result2 = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result2.is_ok(), "Second create should succeed (idempotent)");

        // Both should return same path
        assert_eq!(result1.unwrap().path, result2.unwrap().path);
    }

    #[test]
    fn test_create_worktree_stale_dir_returns_stale_error() {
        // Scenario: directory exists but is checked out on a DIFFERENT branch
        // than the one requested → create_worktree_internal must return STALE_DIR error.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // Create branch-a worktree first
        let config_a = WorktreeConfig {
            task_name: "shared-name".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("branch-a".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config_a, None)
            .expect("Failed to create branch-a worktree");

        // Now attempt to create at same path but with branch-b → should be STALE_DIR
        let config_b = WorktreeConfig {
            task_name: "shared-name".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("branch-b".to_string()),
            create_branch: true,
        };
        let result = create_worktree_internal(&worktrees_dir, &config_b, None);

        assert!(result.is_err(), "expected STALE_DIR error, got Ok");
        let err = result.unwrap_err();
        assert!(
            err.starts_with("STALE_DIR:"),
            "expected STALE_DIR prefix, got: {err}"
        );
        assert!(
            err.contains("branch-a"),
            "expected actual branch 'branch-a' in error: {err}"
        );
        assert!(
            err.contains("branch-b"),
            "expected expected branch 'branch-b' in error: {err}"
        );
    }

    #[test]
    fn test_create_worktree_same_branch_is_idempotent() {
        // Scenario: directory exists with the SAME branch → should succeed (idempotent)
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "same-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/x".to_string()),
            create_branch: true,
        };
        let first = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("First create should succeed");

        // Second call with same branch should succeed and return same path with actual branch
        let second = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Second create should succeed (idempotent same-branch)");

        assert_eq!(first.path, second.path);
        assert_eq!(second.branch, Some("feature/x".to_string()));
    }

    #[test]
    fn test_classify_worktree_add_failure() {
        // Branch collision must win over the broad "already exists" substring.
        assert_eq!(
            classify_worktree_add_failure("fatal: a branch named 'feature/x' already exists"),
            WorktreeAddFailure::BranchExists
        );
        // Path already exists.
        assert_eq!(
            classify_worktree_add_failure("fatal: '/tmp/wt/foo' already exists"),
            WorktreeAddFailure::PathExists
        );
        // Already checked out by another worktree.
        assert_eq!(
            classify_worktree_add_failure(
                "fatal: 'feature/x' is already checked out at '/tmp/wt/foo'"
            ),
            WorktreeAddFailure::PathExists
        );
        // Already used by worktree.
        assert_eq!(
            classify_worktree_add_failure(
                "fatal: '/tmp/wt/foo' is already used by worktree at '/tmp/wt/bar'"
            ),
            WorktreeAddFailure::PathExists
        );
        // Unrelated failure.
        assert_eq!(
            classify_worktree_add_failure("fatal: invalid reference: nope"),
            WorktreeAddFailure::Other
        );
    }

    #[test]
    fn test_create_worktree_orphan_branch_no_worktree_recovers() {
        // The confirmed bug: a branch exists but has NO linked worktree (left over
        // after worktree_remove with delete_branch=false). A fresh create for that
        // branch hits "a branch named 'X' already exists". The fix must NOT swallow
        // this as a phantom Ok — it must either produce a REAL worktree or Err,
        // but never return Ok with a non-existent path.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // 1. Create branch B with a worktree.
        let config = WorktreeConfig {
            task_name: "orphan-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("orphan-branch".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("First create should succeed");

        // 2. Remove the worktree but PRESERVE the branch (delete_branch=false path).
        remove_worktree_internal(&wt, RemovalMode::Safe).expect("remove should succeed");
        assert!(
            !wt.path.exists(),
            "worktree dir should be gone after removal"
        );
        // Branch still exists (we never deleted it).

        // 3. Create again for the same branch → `-b` fails "branch already exists".
        let result = create_worktree_internal(&worktrees_dir, &config, None);

        // Invariant: never Ok with a missing path.
        match result {
            Ok(info) => {
                assert!(
                    info.path.exists(),
                    "create returned Ok but worktree path does not exist: {}",
                    info.path.display()
                );
                // It must be a REAL linked worktree checked out on the branch.
                assert_eq!(
                    crate::git::read_branch_from_head(&info.path).as_deref(),
                    Some("orphan-branch"),
                    "recovered worktree should be on the existing branch"
                );
            }
            Err(e) => {
                // Failing loud is acceptable; silently-Ok-with-no-dir is not.
                assert!(!e.is_empty(), "error must carry git stderr context");
            }
        }
    }

    #[test]
    fn test_create_worktree_detached_head_is_not_stale() {
        // Scenario: worktree exists for `feature/x` but its HEAD is detached
        // (mid-rebase, bisect, or `git checkout <sha>`). A subsequent
        // create_worktree_internal call with the same branch must NOT return
        // STALE_DIR and destroy the in-progress work.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "agent-task".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature/x".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Detach HEAD inside the worktree (simulates rebase/bisect)
        git_cmd(&wt.path)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach failed");

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(
            result.is_ok(),
            "Detached HEAD must not trigger STALE_DIR: {result:?}"
        );
        let returned = result.unwrap();
        assert_eq!(returned.path, wt.path);
        // Detached HEAD is NOT stale; branch field falls back to the logical
        // owner (config.branch) so the JS layer's `string`-typed contract holds
        // even when the worktree's HEAD is transiently detached.
        assert_eq!(
            returned.branch,
            Some("feature/x".to_string()),
            "branch should fall back to config.branch on detached HEAD, got {:?}",
            returned.branch
        );
    }

    #[test]
    fn test_remove_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "to-be-removed".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        assert!(
            worktree.path.exists(),
            "Worktree should exist before removal"
        );

        let result = remove_worktree_internal(&worktree, RemovalMode::Safe);
        assert!(result.is_ok(), "Failed to remove worktree: {:?}", result);

        assert!(
            !worktree.path.exists(),
            "Worktree path should not exist after removal"
        );
    }

    /// Creates a worktree, off a repo with a submodule, that has the submodule
    /// initialized inside it — the exact state that triggers git's unconditional
    /// "working trees containing submodules cannot be moved or removed" refusal.
    fn setup_worktree_with_initialized_submodule(task_name: &str) -> (TempDir, WorktreeInfo) {
        let submodule_src = setup_test_repo();

        let repo = setup_test_repo();
        let repo_path = repo.path();

        let submodule_url = submodule_src.path().to_string_lossy().to_string();
        git_cmd(repo_path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &submodule_url,
                "sub",
            ])
            .run()
            .expect("Failed to add submodule");
        git_cmd(repo_path)
            .args(["commit", "-m", "Add submodule"])
            .run()
            .expect("Failed to commit submodule addition");

        let worktrees_dir = repo_path.join("worktrees");
        let config = WorktreeConfig {
            task_name: task_name.to_string(),
            base_repo: repo_path.to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Initialize the submodule inside the new worktree — this is what
        // actually triggers git's "cannot be moved or removed" refusal.
        git_cmd(&worktree.path)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
            ])
            .run()
            .expect("Failed to init submodule in worktree");

        // `submodule_src` and `repo` (the temp dirs) must outlive `worktree`
        // for the base repo / submodule origin to remain resolvable.
        (repo, worktree)
    }

    #[test]
    fn remove_worktree_internal_retries_with_force_when_worktree_has_submodule() {
        // Regression test for the reported failure: deleting a clean worktree
        // whose checkout has an initialized submodule fails with git's
        // "working trees containing submodules cannot be moved or removed" —
        // a refusal that `RemovalMode::Safe` (no `--force`) hits but that a
        // plain `--force` retry lifts. `remove_worktree_internal` must detect
        // this and retry with `--force` rather than surfacing the error.
        let (_repo, worktree) = setup_worktree_with_initialized_submodule("with-submodule-clean");

        let result = remove_worktree_internal(&worktree, RemovalMode::Safe);
        assert!(
            result.is_ok(),
            "remove_worktree_internal should retry with --force and succeed: {result:?}"
        );
        assert!(
            !worktree.path.exists(),
            "Worktree path should not exist after removal"
        );
    }

    #[test]
    fn remove_worktree_internal_still_refuses_dirty_worktree_with_submodule_in_safe_mode() {
        // Git's submodule-refusal check runs BEFORE its own dirty-worktree
        // check, so once we retry past the former with `--force` we could
        // otherwise silently discard uncommitted work the latter would have
        // protected (a plain `--force` retry lifts BOTH refusals at once —
        // confirmed empirically). Safe mode must still refuse and return the
        // same DIRTY_WORKTREE_PREFIX the caller already knows how to turn
        // into a confirmation prompt.
        let (_repo, worktree) = setup_worktree_with_initialized_submodule("with-submodule-dirty");

        fs::write(worktree.path.join("uncommitted.txt"), "not yet committed")
            .expect("Failed to write uncommitted file");

        let result = remove_worktree_internal(&worktree, RemovalMode::Safe);
        let err = result.expect_err("Safe removal of a dirty worktree should fail");
        assert!(
            err.starts_with(DIRTY_WORKTREE_PREFIX),
            "Error should start with DIRTY_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            worktree.path.exists(),
            "Worktree should be left in place after a refused Safe removal"
        );

        // Dirty mode already passes `--force` on its first attempt, which
        // lifts the submodule refusal directly — it never even reaches the
        // retry arm under test above, but this confirms the end-to-end
        // behavior an actual "confirm discard" caller relies on.
        let result = remove_worktree_internal(&worktree, RemovalMode::Dirty);
        assert!(
            result.is_ok(),
            "Dirty-mode removal should succeed despite uncommitted changes: {result:?}"
        );
        assert!(
            !worktree.path.exists(),
            "Worktree path should not exist after Dirty-mode removal"
        );
    }

    #[test]
    fn test_remove_nonexistent_worktree() {
        let repo = setup_test_repo();

        let worktree = WorktreeInfo {
            name: "nonexistent".to_string(),
            path: repo.path().join("worktrees").join("nonexistent"),
            branch: None,
            base_repo: repo.path().to_path_buf(),
        };

        // Should not error when removing non-existent worktree
        let result = remove_worktree_internal(&worktree, RemovalMode::Safe);
        assert!(
            result.is_ok(),
            "Removing nonexistent worktree should succeed"
        );
    }

    #[test]
    fn test_remove_locked_worktree_without_force_returns_locked_error() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "locked-branch".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Lock the worktree simulating an active agent
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "claude agent test-lock",
                worktree.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let result = remove_worktree_internal(&worktree, RemovalMode::Safe);
        assert!(
            result.is_err(),
            "Should fail on locked worktree without force"
        );
        let err = result.unwrap_err();
        assert!(
            err.starts_with(LOCKED_WORKTREE_PREFIX),
            "Error should start with LOCKED_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            worktree.path.exists(),
            "Worktree directory should still exist after failed removal"
        );
    }

    #[test]
    fn test_remove_locked_worktree_with_force_succeeds() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "locked-branch-force".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let worktree = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Lock the worktree
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "claude agent force-test",
                worktree.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let result = remove_worktree_internal(&worktree, RemovalMode::Forced);
        assert!(
            result.is_ok(),
            "Force removal of locked worktree should succeed: {:?}",
            result
        );
        assert!(
            !worktree.path.exists(),
            "Worktree directory should be gone after force removal"
        );
    }

    /// Create a worktree with a modified-and-added (tracked) uncommitted file.
    fn worktree_with_modified_file(
        repo: &TempDir,
        worktrees_dir: &Path,
        task: &str,
    ) -> WorktreeInfo {
        let config = WorktreeConfig {
            task_name: task.to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        fs::write(wt.path.join("dirty.txt"), "uncommitted work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        wt
    }

    /// Create a worktree with an untracked-only (never `git add`ed) file.
    fn worktree_with_untracked_file(
        repo: &TempDir,
        worktrees_dir: &Path,
        task: &str,
    ) -> WorktreeInfo {
        let config = WorktreeConfig {
            task_name: task.to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        fs::write(wt.path.join("untracked.txt"), "uncommitted work").unwrap();
        wt
    }

    #[test]
    fn safe_remove_refuses_a_dirty_worktree() {
        // Root cause of the 2026-08-26 incident: `remove_worktree_internal`
        // passed a single `--force` even on the "non-force" path, which
        // silently overrides git's own dirty-worktree refusal. Nothing
        // previously asserted the non-force path actually protects
        // uncommitted work — every prior non-force test removed a clean
        // worktree. `RemovalMode::Safe` must behave like plain
        // `git worktree remove`: refuse outright.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let wt = worktree_with_modified_file(&repo, &worktrees_dir, "dirty-safe");

        let result = remove_worktree_internal(&wt, RemovalMode::Safe);
        let err = result.expect_err("Safe removal of a dirty worktree must fail");
        assert!(
            err.starts_with(DIRTY_WORKTREE_PREFIX),
            "Error should start with DIRTY_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            wt.path.join("dirty.txt").exists(),
            "uncommitted work must survive a refused Safe removal"
        );
    }

    #[test]
    fn safe_remove_refuses_an_untracked_only_worktree() {
        // Same protection, for a worktree whose only uncommitted content is
        // untracked (never `git add`ed) — a distinct git refusal path from a
        // modified tracked file, but Safe mode must refuse both.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let wt = worktree_with_untracked_file(&repo, &worktrees_dir, "dirty-untracked");

        let result = remove_worktree_internal(&wt, RemovalMode::Safe);
        let err = result.expect_err("Safe removal of an untracked-only worktree must fail");
        assert!(
            err.starts_with(DIRTY_WORKTREE_PREFIX),
            "Error should start with DIRTY_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            wt.path.join("untracked.txt").exists(),
            "untracked work must survive a refused Safe removal"
        );
    }

    #[test]
    fn dirty_mode_removes_a_dirty_worktree_but_not_a_locked_one() {
        // The middle mode: overrides the dirty check (single --force) but not
        // a lock (still needs the second --force). Neither half of this was
        // reachable before `RemovalMode` existed — `force: bool` could only
        // express "override neither" or "override both".
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let dirty_wt = worktree_with_modified_file(&repo, &worktrees_dir, "dirty-mode-dirty");
        assert!(
            remove_worktree_internal(&dirty_wt, RemovalMode::Dirty).is_ok(),
            "Dirty mode should remove a merely-dirty worktree"
        );
        assert!(!dirty_wt.path.exists());

        let locked_config = WorktreeConfig {
            task_name: "dirty-mode-locked".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let locked_wt = create_worktree_internal(&worktrees_dir, &locked_config, None)
            .expect("Failed to create worktree");
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "test lock",
                locked_wt.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let result = remove_worktree_internal(&locked_wt, RemovalMode::Dirty);
        let err = result.expect_err("Dirty mode must not override a lock");
        assert!(
            err.starts_with(LOCKED_WORKTREE_PREFIX),
            "Error should start with LOCKED_WORKTREE_PREFIX, got: {err}"
        );
        assert!(locked_wt.path.exists());
    }

    #[test]
    fn remove_worktree_internal_unlocks_before_pruning() {
        // A successful removal must drop git's administrative lock entry, not
        // just delete the directory — otherwise `git worktree list` keeps
        // reporting a ghost `locked` entry for a path that no longer exists
        // (mirrors the reasoning already applied to `archive_worktree_dir`).
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "unlock-before-prune".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "tuic: session test",
                wt.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        remove_worktree_internal(&wt, RemovalMode::Forced).expect("forced removal should succeed");

        let out = git_cmd(repo.path())
            .args(["worktree", "list", "--porcelain"])
            .run()
            .expect("list worktrees");
        assert!(
            !out.stdout.contains(wt.path.to_str().unwrap()),
            "removed worktree must not linger as a ghost entry: {}",
            out.stdout
        );
    }

    #[test]
    fn parse_worktree_entries_reads_the_locked_reason() {
        let porcelain = "worktree /repo\nHEAD abc123\nbranch refs/heads/main\n\n\
                          worktree /repo__wt/feature\nHEAD def456\nbranch refs/heads/feature\n\
                          locked tuic: session xyz\n\n\
                          worktree /repo__wt/unlocked\nHEAD 789abc\nbranch refs/heads/other\n";
        let entries = parse_worktree_entries(porcelain);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].locked_reason, None);
        assert_eq!(
            entries[1].locked_reason.as_deref(),
            Some("tuic: session xyz")
        );
        assert_eq!(entries[2].locked_reason, None);
    }

    #[test]
    fn sweep_stale_tuic_locks_clears_only_the_tuic_owned_lock() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let tuic_config = WorktreeConfig {
            task_name: "sweep-tuic-owned".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let tuic_wt = create_worktree_internal(&worktrees_dir, &tuic_config, None)
            .expect("Failed to create worktree");
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "tuic: session dead-session",
                tuic_wt.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let foreign_config = WorktreeConfig {
            task_name: "sweep-foreign-owned".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let foreign_wt = create_worktree_internal(&worktrees_dir, &foreign_config, None)
            .expect("Failed to create worktree");
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "worktree",
                "lock",
                "--reason",
                "claude agent session",
                foreign_wt.path.to_str().unwrap(),
            ])
            .output()
            .expect("git worktree lock failed");

        let swept = sweep_stale_tuic_locks(repo.path().to_str().unwrap());
        assert_eq!(swept, 1, "only the tuic-owned lock should be cleared");

        // The tuic-owned lock is gone: a Safe removal (no --force at all) now succeeds.
        assert!(remove_worktree_internal(&tuic_wt, RemovalMode::Safe).is_ok());

        // The foreign lock is untouched: even Dirty (single --force) is still refused.
        let result = remove_worktree_internal(&foreign_wt, RemovalMode::Dirty);
        assert!(
            result.is_err_and(|e| e.starts_with(LOCKED_WORKTREE_PREFIX)),
            "a non-TUIC lock must survive the sweep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn remove_worktree_by_branch_refuses_a_worktree_with_a_live_session() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "busy-branch".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("busy-branch".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session_attached_to(&state, "s1", wt.clone());

        let result = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "busy-branch",
            true,
            None,
            RemovalMode::Safe,
            Some(&state),
            false,
        );
        let err = result.expect_err("a live session must refuse removal outright");
        assert!(
            err.starts_with(BUSY_WORKTREE_PREFIX),
            "Error should start with BUSY_WORKTREE_PREFIX, got: {err}"
        );
        assert!(
            wt.path.exists(),
            "worktree must survive a refused busy removal"
        );
    }

    #[cfg(unix)]
    #[test]
    fn override_busy_skips_the_liveness_gate_without_escalating_branch_delete() {
        // `override_busy` is a distinct flag from `RemovalMode` on purpose:
        // overriding a live session must never also force-delete the branch —
        // exactly the coupling that caused the 2026-08-26 incident's data loss.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "override-busy".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("override-busy".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        // Unmerged commit so a safe `git branch -d` would refuse.
        fs::write(wt.path.join("new.txt"), "unmerged work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        git_cmd(&wt.path)
            .args(["commit", "-m", "unmerged change"])
            .run()
            .unwrap();

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session_attached_to(&state, "s1", wt.clone());

        let outcome = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "override-busy",
            true,
            None,
            RemovalMode::Safe,
            Some(&state),
            true, // override_busy
        )
        .expect("override_busy should skip the liveness gate");
        assert!(!wt.path.exists(), "worktree should be removed");
        assert!(
            outcome.branch_delete_warning.is_some(),
            "override_busy must not also escalate branch deletion — the unmerged \
             branch should survive exactly as it would without an override"
        );
    }

    #[test]
    fn test_remove_main_worktree_returns_main_prefix_error() {
        let repo = setup_test_repo();

        // The main worktree IS the repo path itself — git refuses to remove it
        let main_worktree = WorktreeInfo {
            name: "main".to_string(),
            path: repo.path().to_path_buf(),
            branch: Some("main".to_string()),
            base_repo: repo.path().to_path_buf(),
        };

        let result = remove_worktree_internal(&main_worktree, RemovalMode::Safe);
        assert!(result.is_err(), "Removing main worktree should fail");
        let err = result.unwrap_err();
        assert!(
            err.starts_with(MAIN_WORKTREE_PREFIX),
            "Error should start with MAIN_WORKTREE_PREFIX, got: {err}"
        );
    }

    #[test]
    fn test_remove_worktree_by_workspace_id_safe_delete_preserves_unmerged_branch() {
        // Scenario: branch has unmerged commits, user removes worktree WITHOUT force.
        // Expected: worktree directory removed, but `git branch -d` refuses, so the
        // branch ref survives as a safety net for unpushed commits.
        let (_config_guard, _config_dir) = with_temp_config_dir();
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feat-unmerged".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feat-unmerged".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Add an unmerged commit on the branch
        std::fs::write(wt.path.join("new.txt"), "unmerged work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        git_cmd(&wt.path)
            .args(["commit", "-m", "unmerged change"])
            .run()
            .unwrap();

        // Safe remove: worktree gone, branch survives
        let outcome = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "feat-unmerged",
            true,
            None,
            RemovalMode::Safe,
            None,
            false,
        )
        .expect("remove should succeed even if -d refuses");
        assert!(
            outcome.branch_delete_warning.is_some(),
            "safe branch delete refusal must be surfaced to the caller"
        );
        assert!(!wt.path.exists(), "worktree dir should be removed");

        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-unmerged"])
            .run()
            .unwrap();
        assert!(
            branches.stdout.contains("feat-unmerged"),
            "branch ref should survive safe delete on unmerged branch (got: {})",
            branches.stdout
        );
    }

    #[test]
    fn forced_remove_never_force_deletes_the_branch() {
        // Root cause of the 2026-08-26 incident's orphaned commits:
        // `remove_worktree_by_branch` used to key `git branch -d` vs `-D` off the
        // same `force` flag that overrides git's worktree-remove safety checks,
        // so a forced worktree removal silently force-deleted an unmerged branch
        // too (`git branch -D`), orphaning its commits. Branch deletion must
        // always use the safe `-d` — worktree removal mode governs the worktree
        // directory only and never escalates branch deletion, no matter how
        // aggressively the worktree itself was removed.
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "feat-force".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feat-force".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        std::fs::write(wt.path.join("new.txt"), "unmerged work").unwrap();
        git_cmd(&wt.path).args(["add", "."]).run().unwrap();
        git_cmd(&wt.path)
            .args(["commit", "-m", "unmerged change"])
            .run()
            .unwrap();

        let res = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "feat-force",
            true,
            None,
            RemovalMode::Forced,
            None,
            false,
        );
        let outcome = res.expect("forced remove should succeed");
        assert!(
            outcome.branch_delete_warning.is_some(),
            "safe branch delete refusal must still be surfaced even under Forced worktree removal"
        );

        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-force"])
            .run()
            .unwrap();
        assert!(
            branches.stdout.contains("feat-force"),
            "branch ref with unmerged commits must survive even a Forced worktree removal (got: {})",
            branches.stdout
        );
    }

    #[test]
    fn test_worktree_name_with_special_characters() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "Fix bug #123: Add feature!".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };

        let result = create_worktree_internal(&worktrees_dir, &config, None);
        assert!(result.is_ok());

        let worktree = result.unwrap();
        assert_eq!(worktree.name, "fix-bug--123--add-feature-");
        assert!(worktree.path.exists());
    }

    #[test]
    fn resolve_worktree_dir_sibling() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::Sibling, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/myrepo__wt"));
    }

    #[test]
    fn resolve_worktree_dir_app_dir() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::AppDir, app_dir);
        assert_eq!(result, PathBuf::from("/app/worktrees/myrepo"));
    }

    #[test]
    fn resolve_worktree_dir_inside_repo() {
        let repo = Path::new("/home/user/dev/myrepo");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::InsideRepo, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/myrepo/.worktrees"));
    }

    #[test]
    fn resolve_worktree_dir_sibling_with_dots_in_name() {
        let repo = Path::new("/home/user/dev/my.project.name");
        let app_dir = Path::new("/app/worktrees");
        let result = resolve_worktree_dir(repo, &WorktreeStorage::Sibling, app_dir);
        assert_eq!(result, PathBuf::from("/home/user/dev/my.project.name__wt"));
    }

    #[test]
    fn generate_worktree_name_matches_adjective_name_number_format() {
        let name = generate_worktree_name(&[]);
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(
            parts.len(),
            3,
            "Expected adjective-name-number format, got: {name}"
        );
        assert!(
            parts[0].chars().all(|c| c.is_ascii_lowercase()),
            "Adjective part should be lowercase ascii: {name}"
        );
        assert!(
            parts[1].chars().all(|c| c.is_ascii_lowercase()),
            "Name part should be lowercase ascii: {name}"
        );
        assert_eq!(
            parts[2].len(),
            3,
            "Number part should be zero-padded to 3 digits: {name}"
        );
        assert!(
            parts[2].chars().all(|c| c.is_ascii_digit()),
            "Number part should be digits: {name}"
        );
    }

    #[test]
    fn generate_worktree_name_avoids_collision_with_existing() {
        // Pre-populate with one name and verify a different one is generated,
        // mirroring generate_clone_branch_name_avoids_collisions below — this is the
        // collision-avoidance path the dialog's "suggested name" actually relies on
        // (existing worktree branches are passed in via `existing`).
        let first = generate_worktree_name(&[]);
        let second = generate_worktree_name(std::slice::from_ref(&first));
        assert_ne!(
            first, second,
            "Should generate a name distinct from `existing`"
        );
    }

    #[test]
    fn generate_clone_branch_name_includes_source() {
        let existing: Vec<String> = vec![];
        let name = generate_clone_branch_name("feat/auth-flow", &existing);
        assert!(
            name.starts_with("feat-auth-flow--"),
            "Name should start with sanitized source branch: {name}"
        );
        // Should contain a random part after the double-dash
        let parts: Vec<&str> = name.splitn(2, "--").collect();
        assert_eq!(parts.len(), 2, "Should have source--random format: {name}");
        assert!(!parts[1].is_empty(), "Random part should not be empty");
    }

    #[test]
    fn generate_clone_branch_name_avoids_collisions() {
        // Pre-populate with one name and verify a different one is generated
        let first = generate_clone_branch_name("main", &[]);
        let second = generate_clone_branch_name("main", std::slice::from_ref(&first));
        assert_ne!(first, second, "Should generate unique names");
    }

    #[test]
    fn get_remote_default_branch_from_test_repo() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        // Test repo has no remote, so should fall back to checking local branches.
        // git init creates "master" or "main" depending on config.
        let result = get_remote_default_branch(&repo_path);
        assert!(result.is_ok());
        let branch = result.unwrap();
        // Should be "main" or "master" (depends on git version default)
        assert!(
            branch == "main" || branch == "master",
            "Expected main or master, got: {branch}"
        );
    }

    #[test]
    fn list_base_ref_options_returns_default_first() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a second branch
        git_cmd(repo.path())
            .args(["branch", "feature-x"])
            .run()
            .expect("Failed to create branch");

        let refs = list_base_ref_options(repo_path).unwrap();
        assert!(refs.len() >= 2, "Expected at least 2 refs, got: {refs:?}");
        // First entry should be the default branch (main or master), flagged is_default
        assert!(
            refs[0].name == "main" || refs[0].name == "master",
            "First ref should be default branch, got: {}",
            refs[0].name
        );
        assert!(refs[0].is_default, "First ref should have is_default=true");
        assert_eq!(refs[0].kind, "local");
        // feature-x should be in the list
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"feature-x"),
            "feature-x not found in {names:?}"
        );
        // No duplicate names
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), refs.len(), "Duplicate refs found: {names:?}");
    }

    // --- merge pre-flight ---
    //
    // The whole point: `git merge` succeeds identically for a branch carrying 7
    // commits and for one carrying none ("Already up to date"), and in both cases
    // the worktree row then disappears from the sidebar. Only the pre-flight can
    // tell the user which of the two just happened.

    /// Build a worktree on `branch` and return its path. Optionally commit a file
    /// so the branch is genuinely ahead of the base branch.
    fn worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
        let repo_path = repo.to_string_lossy().to_string();
        let config = WorktreeConfig {
            task_name: branch.to_string(),
            base_repo: repo_path,
            branch: Some(branch.to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&repo.join("worktrees"), &config, None)
            .expect("create worktree");
        if commit {
            fs::write(wt.path.join("work.txt"), "real work").expect("write");
            git_cmd(&wt.path).args(["add", "."]).run().expect("add");
            git_cmd(&wt.path)
                .args(["commit", "-m", "feat: real work"])
                .run()
                .expect("commit");
        }
        wt.path
    }

    /// The base branch of `setup_test_repo` — git's default name varies by version.
    fn base_branch_of(repo: &Path) -> String {
        git_cmd(repo)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .run()
            .expect("rev-parse HEAD")
            .stdout
            .trim()
            .to_string()
    }

    #[test]
    fn merge_preflight_counts_commits_the_target_is_missing() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-ahead", true);

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-ahead",
            "feat-ahead",
            &base,
        );
        assert_eq!(pf.commits_ahead, 1, "one commit the base branch lacks");
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Clean),
            "everything was committed"
        );
    }

    #[test]
    fn merge_preflight_reports_zero_for_a_branch_with_nothing_to_merge() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-empty", false);

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-empty",
            "feat-empty",
            &base,
        );
        assert_eq!(
            pf.commits_ahead, 0,
            "branch was cut from base and never committed — merging it is a no-op"
        );
    }

    #[test]
    fn merge_preflight_sees_uncommitted_work_in_the_worktree() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = worktree_with(repo.path(), "feat-dirty", false);
        fs::write(wt.join("scratch.txt"), "not committed yet").expect("write");

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-dirty",
            "feat-dirty",
            &base,
        );
        assert_eq!(pf.commits_ahead, 0);
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Dirty),
            "an untracked file still counts as work that archiving would sweep away"
        );
    }

    #[test]
    fn merge_preflight_is_clean_for_an_already_merged_branch() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-merged", true);
        git_cmd(repo.path())
            .args(["merge", "feat-merged", "--no-edit"])
            .run()
            .expect("merge");

        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "feat-merged",
            "feat-merged",
            &base,
        );
        assert_eq!(
            pf.commits_ahead, 0,
            "already merged — the target has everything"
        );
    }

    #[test]
    fn merge_preflight_falls_back_to_unknown_for_a_branch_that_does_not_exist() {
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        // rev-list fails on an unknown ref; the pre-flight must not panic or block
        // the merge — it is a guard rail, not a gate.
        let pf = merge_preflight(
            &repo.path().to_string_lossy(),
            "no-such-branch",
            "no-such-branch",
            &base,
        );
        assert_eq!(pf.commits_ahead, 0);
        assert!(
            matches!(pf.worktree_dirty, WorktreeDirtiness::Clean),
            "no worktree at all means there is no uncommitted work to lose"
        );
    }

    // --- the destructive-cleanup gate ---
    //
    // Both "archive" and "delete" end in `git worktree remove --force`, which
    // deletes uncommitted work without a word. Every path to that call must pass
    // `cleanup_needs_confirmation` first. These tests exercise the two entry
    // points end to end on a real repo, because the bug they cover was not in
    // the gate — it was in a caller that never reached it.

    /// An isolated config dir, so `resolve_archive_script` reads an empty config
    /// instead of the developer's real one and cannot run a live archive script.
    fn isolated_config() -> (TempDir, impl Drop) {
        let dir = TempDir::new().expect("config dir");
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        (dir, guard)
    }

    /// A worktree on `branch` with an uncommitted file in it, plus the commit
    /// count the branch is ahead of base. Returns the worktree path.
    fn dirty_worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
        let wt = worktree_with(repo, branch, commit);
        fs::write(wt.join("scratch.txt"), "hours of uncommitted work").expect("write scratch");
        wt
    }

    #[test]
    fn merge_and_archive_asks_before_destroying_a_dirty_worktree_with_commits() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-dirty-ahead", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-dirty-ahead".to_string(),
            "feat-dirty-ahead".to_string(),
            base,
            "archive".to_string(),
            false,
        )
        .expect("pre-flight returns a result, not an error");

        assert_eq!(res.action, "needs_confirmation");
        assert!(!res.merged, "nothing ran — not even the merge");
        assert_eq!(res.commits_ahead, 1, "the dialog says what would be merged");
        assert!(res.worktree_dirty);
        assert!(
            wt.join("scratch.txt").exists(),
            "the uncommitted file is still there"
        );
    }

    #[test]
    fn merge_and_archive_proceeds_once_the_user_confirms() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-confirmed", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-confirmed".to_string(),
            "feat-confirmed".to_string(),
            base,
            "archive".to_string(),
            true,
        )
        .expect("archive");

        assert_eq!(res.action, "archived");
        assert!(res.merged);
        assert!(!wt.exists(), "the worktree left its old place");
        // Archiving is the non-destructive choice, so the work must survive the
        // move. Removing the worktree before the rename used to delete it.
        let archived = PathBuf::from(res.archive_path.expect("archive path"));
        assert!(
            archived.join("scratch.txt").exists(),
            "uncommitted work moved to the archive instead of being deleted"
        );
    }

    #[test]
    fn merge_and_archive_deletes_a_dirty_worktree_once_confirmed() {
        // Gap the plan called out explicitly: `merge_and_archive_worktree_impl`'s
        // "delete" branch already confirmed dirtiness via `cleanup_needs_confirmation`
        // before ever reaching `remove_worktree_by_branch` — so once `RemovalMode::Safe`
        // became the real default (refusing a dirty worktree outright), this path had
        // to switch to `RemovalMode::Dirty` on `force`, or a confirmed cleanup would
        // start failing. `merge_and_archive_proceeds_once_the_user_confirms` covers the
        // "archive" branch (a different code path, no `remove_worktree_by_branch` call
        // at all); this is the "delete" equivalent.
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-delete-confirmed", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-delete-confirmed".to_string(),
            "feat-delete-confirmed".to_string(),
            base,
            "delete".to_string(),
            true,
        )
        .expect("delete");

        assert_eq!(res.action, "deleted");
        assert!(res.merged);
        assert!(!wt.exists(), "the user asked for it");
    }

    #[test]
    fn merge_and_archive_does_not_ask_about_a_clean_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        worktree_with(repo.path(), "feat-clean", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-clean".to_string(),
            "feat-clean".to_string(),
            base,
            "archive".to_string(),
            false,
        )
        .expect("archive");

        assert_eq!(res.action, "archived", "nothing to lose, nothing to ask");
        assert!(!res.worktree_dirty);
    }

    #[test]
    fn merge_and_archive_in_ask_mode_never_blocks() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-ask", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-ask".to_string(),
            "feat-ask".to_string(),
            base,
            "ask".to_string(),
            false,
        )
        .expect("merge");

        // "ask" destroys nothing — it merges and hands the cleanup decision to the
        // dialog, which asks for itself. Blocking here would deadlock the flow.
        assert_eq!(res.action, "pending");
        assert!(res.merged);
        assert!(res.worktree_dirty, "the dialog needs to know");
        assert!(wt.join("scratch.txt").exists());
    }

    #[test]
    fn finalize_refuses_to_destroy_a_dirty_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let wt = dirty_worktree_with(repo.path(), "feat-finalize-dirty", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        // This is the path the auto-archive sweep takes, with nobody watching.
        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-dirty".to_string(),
            "delete".to_string(),
            false,
        )
        .expect("guard returns a result");

        assert_eq!(res.action, "needs_confirmation");
        assert!(res.worktree_dirty);
        assert!(wt.join("scratch.txt").exists(), "still on disk, untouched");
    }

    #[test]
    fn finalize_deletes_once_the_user_confirms() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let wt = dirty_worktree_with(repo.path(), "feat-finalize-forced", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-forced".to_string(),
            "delete".to_string(),
            true,
        )
        .expect("delete");

        assert_eq!(res.action, "deleted");
        assert!(!wt.exists(), "the user asked for it");
    }

    #[test]
    fn finalize_leaves_a_clean_worktree_to_the_sweep() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        worktree_with(repo.path(), "feat-finalize-clean", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-clean".to_string(),
            "archive".to_string(),
            false,
        )
        .expect("archive");

        assert_eq!(res.action, "archived", "no confirmation needed");
    }

    // --- the busy-worktree guard ---
    //
    // A worktree mid-rebase/merge/cherry-pick/revert/bisect must not be swept up by the
    // *automatic* consequences of a merge (auto-archive-merged, finalize-after-merge), even
    // when it is otherwise clean (so `cleanup_needs_confirmation` alone would let it through)
    // and even with `force: true` (there is no sensible unattended answer to "destroy my
    // in-flight rebase").

    #[test]
    fn merge_and_archive_refuses_a_worktree_with_operation_in_progress() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = worktree_with(repo.path(), "feat-busy-merge", true);
        let admin = worktree_admin_dir(&wt.to_string_lossy()).expect("resolve admin dir");
        fs::create_dir_all(admin.join("rebase-merge")).expect("create rebase-merge marker");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-busy-merge".to_string(),
            "feat-busy-merge".to_string(),
            base,
            "archive".to_string(),
            true, // force: even the user's confirmation must not bypass this guard
        );

        assert!(res.is_err(), "a busy worktree must refuse the cleanup");
        assert!(wt.exists(), "the worktree survives untouched");
    }

    #[test]
    fn finalize_refuses_a_worktree_with_operation_in_progress() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let wt = worktree_with(repo.path(), "feat-busy-finalize", true);
        let admin = worktree_admin_dir(&wt.to_string_lossy()).expect("resolve admin dir");
        fs::create_dir_all(admin.join("rebase-merge")).expect("create rebase-merge marker");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-busy-finalize".to_string(),
            "delete".to_string(),
            true,
        );

        assert!(res.is_err(), "a busy worktree must refuse the cleanup");
        assert!(wt.exists(), "the worktree survives untouched");
    }

    #[test]
    fn list_in_progress_worktrees_finds_only_the_busy_one() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let idle = worktree_with(repo.path(), "idle-branch", true);
        let busy = worktree_with(repo.path(), "busy-branch", true);
        let admin = worktree_admin_dir(&busy.to_string_lossy()).expect("resolve admin dir");
        fs::create_dir_all(admin.join("rebase-merge")).expect("create rebase-merge marker");

        let in_progress =
            list_in_progress_worktrees(&repo_path).expect("list_in_progress_worktrees");

        // `git worktree list --porcelain` reports canonicalized (symlink-resolved) paths —
        // e.g. macOS /var -> /private/var — so compare against the same form.
        let busy_real = busy
            .canonicalize()
            .unwrap_or(busy)
            .to_string_lossy()
            .to_string();
        let idle_real = idle
            .canonicalize()
            .unwrap_or(idle)
            .to_string_lossy()
            .to_string();

        let busy_op = in_progress
            .iter()
            .find(|op| op.path == busy_real)
            .unwrap_or_else(|| {
                panic!("busy worktree should be reported in progress: {in_progress:?}")
            });
        assert_eq!(busy_op.kind, GitOpKind::Rebase);
        assert!(
            !in_progress.iter().any(|op| op.path == idle_real),
            "idle worktree must not be reported in progress: {in_progress:?}"
        );
    }

    #[test]
    fn list_in_progress_worktrees_reports_the_correct_kind_per_marker() {
        // Each marker file maps to a distinct GitOpKind — a regression guard against the
        // priority-ordered `if` chain in `operation_in_progress` silently misclassifying one
        // (e.g. a stray leftover MERGE_HEAD shadowing an actual cherry-pick).
        let cases: &[(&str, GitOpKind)] = &[
            ("rebase-merge", GitOpKind::Rebase),
            ("rebase-apply", GitOpKind::Rebase),
            ("MERGE_HEAD", GitOpKind::Merge),
            ("CHERRY_PICK_HEAD", GitOpKind::CherryPick),
            ("REVERT_HEAD", GitOpKind::Revert),
            ("BISECT_LOG", GitOpKind::Bisect),
        ];

        for (marker, expected_kind) in cases {
            let repo = setup_test_repo();
            let repo_path = repo.path().to_string_lossy().to_string();
            let wt = worktree_with(repo.path(), "busy-branch", true);
            let admin = worktree_admin_dir(&wt.to_string_lossy()).expect("resolve admin dir");
            if *marker == "rebase-merge" || *marker == "rebase-apply" {
                fs::create_dir_all(admin.join(marker)).expect("create marker dir");
            } else {
                fs::write(admin.join(marker), "").expect("create marker file");
            }

            let in_progress =
                list_in_progress_worktrees(&repo_path).expect("list_in_progress_worktrees");
            let wt_real = wt
                .canonicalize()
                .unwrap_or(wt)
                .to_string_lossy()
                .to_string();
            let op = in_progress
                .iter()
                .find(|op| op.path == wt_real)
                .unwrap_or_else(|| {
                    panic!("marker {marker} should be reported in progress: {in_progress:?}")
                });
            assert_eq!(
                op.kind, *expected_kind,
                "marker {marker} mapped to the wrong GitOpKind"
            );
        }
    }

    #[test]
    fn list_in_progress_worktrees_detects_the_main_worktree_too() {
        // Regression guard: `worktree_admin_dir` alone returns None for the main worktree
        // (its `.git` is a directory, not a file), which meant the main checkout could never
        // report an in-progress operation. `resolve_admin_dir` fixes this by also handling the
        // `.git`-as-directory case.
        let repo = setup_test_repo();
        let repo_path_str = repo.path().to_string_lossy().to_string();
        let main_admin = repo.path().join(".git");
        assert!(
            main_admin.is_dir(),
            "main worktree's .git must be a directory"
        );
        fs::create_dir_all(main_admin.join("rebase-merge")).expect("create rebase-merge marker");

        let in_progress =
            list_in_progress_worktrees(&repo_path_str).expect("list_in_progress_worktrees");

        let main_real = repo
            .path()
            .canonicalize()
            .unwrap_or_else(|_| repo.path().to_path_buf())
            .to_string_lossy()
            .to_string();
        let op = in_progress
            .iter()
            .find(|op| op.path == main_real)
            .unwrap_or_else(|| {
                panic!("main worktree should be reported in progress: {in_progress:?}")
            });
        assert_eq!(op.kind, GitOpKind::Rebase);
    }

    #[test]
    fn operation_in_progress_prefers_rebase_when_multiple_markers_coexist() {
        // Git only ever leaves one marker in normal operation, but a botched abort (e.g. a
        // crash mid `git rebase --abort`) can leave a stale marker behind alongside a fresh
        // one. `operation_in_progress`'s check order must be deterministic rather than
        // accidentally depending on filesystem iteration order — rebase wins because rebase
        // and bisect are the two operations that detach HEAD, which is the higher-severity
        // case for the worktree-removal-safety consumers of this function.
        let repo = setup_test_repo();
        let wt = worktree_with(repo.path(), "busy-branch", true);
        let admin = worktree_admin_dir(&wt.to_string_lossy()).expect("resolve admin dir");
        fs::create_dir_all(admin.join("rebase-merge")).expect("create rebase-merge marker");
        fs::write(admin.join("MERGE_HEAD"), "").expect("create stale MERGE_HEAD marker");
        fs::write(admin.join("CHERRY_PICK_HEAD"), "")
            .expect("create stale CHERRY_PICK_HEAD marker");

        assert_eq!(
            operation_in_progress(&wt.to_string_lossy()),
            Some(GitOpKind::Rebase),
            "rebase must win when multiple markers coexist"
        );
    }

    #[test]
    fn operation_in_progress_is_none_for_a_clean_worktree() {
        let repo = setup_test_repo();
        let wt = worktree_with(repo.path(), "clean-branch", true);
        assert_eq!(operation_in_progress(&wt.to_string_lossy()), None);
    }

    #[test]
    fn operation_in_progress_is_none_for_a_nonexistent_path() {
        assert_eq!(
            operation_in_progress("/no/such/path/anywhere"),
            None,
            "a path that doesn't exist must not be misreported as in-progress"
        );
    }

    #[test]
    fn an_unanswered_dirty_check_blocks_the_cleanup() {
        // Failing open here is what let a transient git error wipe a worktree.
        let unknown = WorktreeDirtiness::Unknown("git exploded".to_string());
        assert!(cleanup_needs_confirmation("archive", false, &unknown));
        assert!(cleanup_needs_confirmation("delete", false, &unknown));
        assert!(
            !unknown.is_dirty(),
            "reported as not-known-dirty: the field must not claim more than git said"
        );
    }

    #[test]
    fn the_gate_only_guards_the_destructive_actions() {
        let dirty = WorktreeDirtiness::Dirty;
        assert!(cleanup_needs_confirmation("archive", false, &dirty));
        assert!(cleanup_needs_confirmation("delete", false, &dirty));
        // "ask" and anything else remove nothing, so there is nothing to confirm.
        assert!(!cleanup_needs_confirmation("ask", false, &dirty));
        assert!(!cleanup_needs_confirmation("keep", false, &dirty));
        // force is the confirmation itself.
        assert!(!cleanup_needs_confirmation("delete", true, &dirty));
        assert!(!cleanup_needs_confirmation(
            "archive",
            false,
            &WorktreeDirtiness::Clean
        ));
    }

    #[test]
    fn archive_worktree_moves_directory() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a branch
        let config = WorktreeConfig {
            task_name: "feat-archive-test".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-archive-test".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        assert!(wt.path.exists(), "Worktree should exist");

        // Make a commit on the feature branch so merge has something to do
        fs::write(wt.path.join("feature.txt"), "feature work").expect("write feature");
        git_cmd(&wt.path).args(["add", "."]).run().expect("git add");
        git_cmd(&wt.path)
            .args(["commit", "-m", "feat: add feature"])
            .run()
            .expect("git commit");

        // Archive the worktree
        let result = archive_worktree(repo.path(), "feat-archive-test", None);
        assert!(result.is_ok(), "Archive should succeed: {:?}", result);

        let _archive_path = PathBuf::from(result.unwrap());
        // The worktree should no longer exist at original location
        assert!(!wt.path.exists(), "Original worktree path should be gone");
        // Archive destination should exist (only if worktree dir wasn't deleted by git)
        // Note: git worktree remove --force may delete the dir, in which case archive_dest won't exist
        // but the operation should still succeed
    }

    // --- orphan worktree cleanup: archive by default, delete only as an explicit opt-in ---
    //
    // Orphan detection (detached HEAD + no branch) is a heuristic — it can misclassify a
    // worktree that was never actually abandoned. A false positive must stay recoverable,
    // which is why `remove_orphan_worktree_impl` archives instead of hard-deleting.

    #[test]
    fn orphan_cleanup_archives_instead_of_deleting() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let wt = worktree_with(repo.path(), "feat-orphan-archive", true);
        git_cmd(&wt)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach HEAD");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        // `validate_worktree_path` compares against `git worktree list --porcelain`, which
        // reports canonicalized (symlink-resolved) paths — e.g. macOS /var -> /private/var.
        let wt_real = wt.canonicalize().unwrap_or_else(|_| wt.clone());
        let archive_path =
            remove_orphan_worktree_impl(&state, repo_path, wt_real.to_string_lossy().to_string())
                .expect("archive should succeed");

        assert!(!wt.exists(), "the original worktree path should be gone");
        let archived = PathBuf::from(archive_path);
        assert!(
            archived.exists(),
            "the worktree's contents should have moved to the archive, not been deleted"
        );
        let archive_root = repo
            .path()
            .canonicalize()
            .unwrap_or_else(|_| repo.path().to_path_buf())
            .join("worktrees")
            .join("__archived");
        assert!(
            archived.starts_with(&archive_root),
            "archive destination should live under __archived/: {archived:?}"
        );
    }

    #[test]
    fn orphan_delete_mode_is_a_true_hard_delete() {
        // `OrphanCleanup::Delete` is a deliberate opt-in to skip archiving — unlike `On`/`Ask`,
        // it must not leave a recoverable copy anywhere.
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let wt = worktree_with(repo.path(), "feat-orphan-delete", true);
        git_cmd(&wt)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach HEAD");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let wt_real = wt.canonicalize().unwrap_or_else(|_| wt.clone());
        delete_orphan_worktree_impl(&state, repo_path, wt_real.to_string_lossy().to_string())
            .expect("delete should succeed");

        assert!(!wt.exists(), "the worktree path should be gone");
        let archive_dir = repo.path().join("worktrees").join("__archived");
        assert!(
            !archive_dir.exists() || fs::read_dir(&archive_dir).unwrap().next().is_none(),
            "delete mode must not leave anything behind in __archived/"
        );
    }

    #[test]
    fn remove_worktree_by_workspace_id_deletes_branch_when_true() {
        let (_config_guard, _config_dir) = with_temp_config_dir();
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-delete-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-delete-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("Failed to create worktree");

        // Remove with delete_branch=true
        remove_worktree_by_workspace_id(
            &repo_path,
            "feat-delete-branch",
            true,
            None,
            RemovalMode::Safe,
            None,
            false,
        )
        .expect("Failed to remove worktree");

        // Branch should be gone
        let out = git_cmd(repo.path())
            .args(["branch", "--list", "feat-delete-branch"])
            .run()
            .expect("Failed to list branches");
        assert!(
            out.stdout.trim().is_empty(),
            "Branch should be deleted when delete_branch=true, but found: {}",
            out.stdout
        );
    }

    #[test]
    fn remove_worktree_by_workspace_id_keeps_branch_when_false() {
        let (_config_guard, _config_dir) = with_temp_config_dir();
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "feat-keep-branch".to_string(),
            base_repo: repo_path.clone(),
            branch: Some("feat-keep-branch".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("Failed to create worktree");

        // Remove with delete_branch=false
        remove_worktree_by_workspace_id(
            &repo_path,
            "feat-keep-branch",
            false,
            None,
            RemovalMode::Safe,
            None,
            false,
        )
        .expect("Failed to remove worktree");

        // Branch should still exist
        let out = git_cmd(repo.path())
            .args(["branch", "--list", "feat-keep-branch"])
            .run()
            .expect("Failed to list branches");
        assert!(
            !out.stdout.trim().is_empty(),
            "Branch should be preserved when delete_branch=false"
        );
    }

    #[test]
    fn resolve_worktree_dir_sibling_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::Sibling, &app_dir),
            PathBuf::from("/home/user/dev/myrepo__wt")
        );
    }

    #[test]
    fn resolve_worktree_dir_appdir_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::AppDir, &app_dir),
            PathBuf::from("/home/user/.config/tuic/worktrees/myrepo")
        );
    }

    #[test]
    fn resolve_worktree_dir_inside_repo_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::InsideRepo, &app_dir),
            PathBuf::from("/home/user/dev/myrepo/.worktrees")
        );
    }

    #[test]
    fn resolve_worktree_dir_claude_code_default_strategy() {
        use crate::config::WorktreeStorage;
        let repo = PathBuf::from("/home/user/dev/myrepo");
        let app_dir = PathBuf::from("/home/user/.config/tuic/worktrees");
        assert_eq!(
            resolve_worktree_dir(&repo, &WorktreeStorage::ClaudeCodeDefault, &app_dir),
            PathBuf::from("/home/user/dev/myrepo/.claude/worktrees")
        );
    }

    #[test]
    fn parse_orphan_worktrees_detects_detached_linked_worktrees() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
branch refs/heads/main

worktree /wt/feat-auth
HEAD def456
branch refs/heads/feat-auth

worktree /wt/orphan
HEAD deadbeef
detached

";
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert_eq!(orphans, vec!["/wt/orphan"]);
    }

    #[test]
    fn parse_orphan_worktrees_ignores_main_worktree_even_if_detached() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
detached

worktree /wt/also-detached
HEAD deadbeef
detached

";
        // Main worktree (first) is always skipped; only the second shows up
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert_eq!(orphans, vec!["/wt/also-detached"]);
    }

    #[test]
    fn parse_orphan_worktrees_returns_empty_when_all_have_branches() {
        let porcelain = "\
worktree /repo/main
HEAD abc123
branch refs/heads/main

worktree /wt/feat
HEAD def456
branch refs/heads/feat

";
        let orphans = super::parse_orphan_worktrees(porcelain);
        assert!(orphans.is_empty());
    }

    #[tokio::test]
    async fn detect_orphan_worktrees_runs_as_async_command() {
        let repo = TempDir::new().expect("temp repo");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .arg(repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());

        let orphans = super::detect_orphan_worktrees(repo.path().display().to_string())
            .await
            .expect("detect orphan worktrees");

        assert!(orphans.is_empty());
    }

    /// Build a linked-worktree fixture: `<root>/wt` with a `.git` file pointing at
    /// `<root>/admin`, plus whichever in-progress marker files the test needs.
    fn linked_worktree_fixture(root: &Path, markers: &[(&str, &str)]) -> String {
        let wt = root.join("wt");
        let admin = root.join("admin");
        std::fs::create_dir_all(&wt).expect("worktree dir");
        std::fs::create_dir_all(&admin).expect("admin dir");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display()))
            .expect("gitdir file");
        for (rel, contents) in markers {
            let target = admin.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("marker parent");
            }
            std::fs::write(target, contents).expect("marker file");
        }
        wt.to_string_lossy().into_owned()
    }

    fn detached_porcelain(wt_path: &str) -> String {
        format!(
            "worktree /repo/main\nHEAD abc123\nbranch refs/heads/main\n\nworktree {wt_path}\nHEAD deadbeef\ndetached\n\n"
        )
    }

    /// Two workspaces may share a branch (#726-5ac7), so a lookup keyed on the
    /// branch cannot say WHICH one you asked for. Resolution is by opaque
    /// workspace id, and the branch travels as a field on the value.
    ///
    /// For a git worktree the id is the branch. Keeping the id explicit avoids
    /// coupling callers to that representation.
    #[test]
    fn workspace_paths_are_keyed_by_id_and_carry_the_branch() {
        // A real directory: the mapper drops entries whose path no longer exists,
        // which is the post-prune safety guard, not something to work around.
        let dir = TempDir::new().expect("temp dir");
        let wt = dir.path().to_string_lossy().into_owned();
        let porcelain = format!("worktree {wt}\nHEAD abc123\nbranch refs/heads/main\n\n");

        let map = super::map_worktree_workspace_paths(&porcelain);

        let entry = map.get("main").expect("resolvable by workspace id");
        assert_eq!(entry.path, wt);
        assert_eq!(
            entry.branch, "main",
            "branch survives as data, not as the key"
        );
        assert_eq!(entry.kind, crate::worktree::WorkspaceKind::Worktree);
    }

    #[test]
    fn branch_already_owned_by_a_linked_checkout_is_refused() {
        let (_config_guard, _config_dir) = with_temp_config_dir();
        let repo = setup_test_repo();
        let branch = git_cmd(repo.path())
            .args(["branch", "--show-current"])
            .run()
            .expect("current branch")
            .stdout;

        let error = super::ensure_branch_has_no_workspace(repo.path(), branch.trim())
            .expect_err("main checkout already owns its branch");
        assert!(
            error.contains("already belongs to linked worktree"),
            "{error}"
        );
        assert!(error.contains("reuse that worktree"), "{error}");
    }

    /// Resolution remains keyed by workspace id while the branch travels as
    /// data on the record. The load-bearing case is the detached worktree:
    /// mid-rebase git emits no `branch refs/heads/…` line at
    /// all, so a scan for that line could not find it by branch under any
    /// argument. The id-keyed mapper recovers the branch from git's own
    /// `head-name` and keys on it, so the workspace stays resolvable — which is
    /// what stops the sidebar row vanishing and its terminals being closed
    /// mid-conflict-resolution.
    #[test]
    fn resolution_by_workspace_id_returns_that_workspace_not_a_branch_match() {
        let repo = setup_test_repo();
        let alpha = worktree_with(repo.path(), "feat-alpha", false);
        let beta = worktree_with(repo.path(), "feat-beta", false);

        let resolved_alpha =
            super::resolve_workspace(repo.path(), "feat-alpha").expect("alpha resolves");
        let resolved_beta =
            super::resolve_workspace(repo.path(), "feat-beta").expect("beta resolves");
        assert_eq!(
            std::fs::canonicalize(&resolved_alpha.path).expect("canonical alpha"),
            std::fs::canonicalize(&alpha).expect("canonical alpha dir"),
        );
        assert_eq!(
            std::fs::canonicalize(&resolved_beta.path).expect("canonical beta"),
            std::fs::canonicalize(&beta).expect("canonical beta dir"),
        );
        assert_ne!(
            resolved_alpha.path, resolved_beta.path,
            "each id must land on its own directory"
        );

        // An id is opaque: a directory path is not one, and must not resolve.
        assert!(
            super::resolve_workspace(repo.path(), &alpha.to_string_lossy()).is_err(),
            "a path is not a workspace id"
        );
        assert!(
            super::resolve_workspace(repo.path(), "no-such-workspace").is_err(),
            "an unknown id is an error, not a silent first-match"
        );
    }

    /// Criterion: removing one workspace leaves the other resolvable and on disk.
    #[test]
    fn removing_one_workspace_leaves_its_sibling_resolvable_and_on_disk() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let doomed = worktree_with(repo.path(), "feat-doomed", false);
        let survivor = worktree_with(repo.path(), "feat-survivor", false);

        let outcome = super::remove_worktree_by_workspace_id(
            &repo.path().to_string_lossy(),
            "feat-doomed",
            true,
            None,
            RemovalMode::Safe,
            None,
            false,
        )
        .expect("removal by id");
        assert_eq!(
            outcome.branch, "feat-doomed",
            "the outcome reports the branch it read off the record"
        );

        assert!(!doomed.exists(), "the targeted worktree is gone");
        assert!(survivor.exists(), "the sibling is untouched on disk");
        let still_there =
            super::resolve_workspace(repo.path(), "feat-survivor").expect("sibling resolves");
        assert_eq!(
            std::fs::canonicalize(&still_there.path).expect("canonical"),
            std::fs::canonicalize(&survivor).expect("canonical"),
        );
        assert!(
            super::resolve_workspace(repo.path(), "feat-doomed").is_err(),
            "the removed id stops resolving"
        );
    }

    /// Criterion: `worktree_dirtiness` and `check_worktree_dirty` answer about
    /// the workspace they were asked about.
    ///
    /// This is the one that gates an irreversible cleanup, so it must never
    /// report one workspace clean because another workspace is clean.
    #[test]
    fn dirtiness_answers_about_the_workspace_it_was_asked_about() {
        let repo = setup_test_repo();
        let dirty = dirty_worktree_with(repo.path(), "feat-dirty-one", false);
        worktree_with(repo.path(), "feat-clean-one", false);
        assert!(dirty.join("scratch.txt").exists(), "fixture is dirty");

        assert!(
            super::worktree_dirtiness(repo.path(), "feat-dirty-one").is_dirty(),
            "the dirty workspace reports dirty"
        );
        assert!(
            !super::worktree_dirtiness(repo.path(), "feat-clean-one").is_dirty(),
            "the clean sibling is not tainted by it"
        );

        let repo_str = repo.path().to_string_lossy().into_owned();
        assert_eq!(
            check_worktree_dirty(repo_str.clone(), "feat-dirty-one".to_string()),
            Ok(true)
        );
        assert_eq!(
            check_worktree_dirty(repo_str.clone(), "feat-clean-one".to_string()),
            Ok(false)
        );
        // An id with no checkout has nothing to lose — Clean, not an error.
        assert_eq!(
            check_worktree_dirty(repo_str, "no-such-workspace".to_string()),
            Ok(false)
        );
    }

    /// Criterion: `delete_local_branch_impl` refuses when its id and branch name
    /// disagree, instead of guessing which one the caller meant.
    #[test]
    fn delete_local_branch_refuses_an_id_branch_mismatch() {
        let repo = setup_test_repo();
        let keeper = worktree_with(repo.path(), "feat-keeper", false);
        worktree_with(repo.path(), "feat-other", false);
        let repo_str = repo.path().to_string_lossy().into_owned();

        let err = delete_local_branch_impl(&repo_str, "feat-other", "feat-keeper", false, None)
            .expect_err("mismatched id and branch must be refused");
        assert!(
            err.contains("feat-keeper") && err.contains("feat-other"),
            "the refusal must name both, got: {err}"
        );
        assert!(
            keeper.exists(),
            "nothing was destroyed while the request was ambiguous"
        );
        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-other"])
            .run()
            .expect("branch list");
        assert!(
            branches.stdout.contains("feat-other"),
            "the branch the caller named still exists"
        );
    }

    #[test]
    fn worktree_mid_rebase_is_not_orphan_and_keeps_its_branch() {
        let dir = TempDir::new().expect("temp dir");
        let wt = linked_worktree_fixture(
            dir.path(),
            &[("rebase-merge/head-name", "refs/heads/feat-auth\n")],
        );
        let porcelain = detached_porcelain(&wt);

        assert!(super::parse_orphan_worktrees(&porcelain).is_empty());
        assert_eq!(
            super::map_worktree_workspace_paths(&porcelain)
                .get("feat-auth")
                .map(|w| &w.path),
            Some(&wt)
        );
    }

    #[test]
    fn worktree_mid_rebase_apply_is_not_orphan_and_keeps_its_branch() {
        let dir = TempDir::new().expect("temp dir");
        let wt = linked_worktree_fixture(
            dir.path(),
            &[("rebase-apply/head-name", "refs/heads/feat-am\n")],
        );
        let porcelain = detached_porcelain(&wt);

        assert!(super::parse_orphan_worktrees(&porcelain).is_empty());
        assert_eq!(
            super::map_worktree_workspace_paths(&porcelain)
                .get("feat-am")
                .map(|w| &w.path),
            Some(&wt)
        );
    }

    #[test]
    fn interrupted_merge_and_cherry_pick_are_not_orphans() {
        // Merge and cherry-pick never detach HEAD, but a worktree that is BOTH detached and
        // mid-operation (e.g. a cherry-pick started from a detached HEAD) must not be archived.
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "BISECT_LOG",
        ] {
            let dir = TempDir::new().expect("temp dir");
            let wt = linked_worktree_fixture(dir.path(), &[(marker, "deadbeef\n")]);
            assert!(
                super::parse_orphan_worktrees(&detached_porcelain(&wt)).is_empty(),
                "{marker} should suppress the orphan verdict"
            );
        }
    }

    #[test]
    fn genuinely_orphaned_worktree_is_still_reported() {
        let dir = TempDir::new().expect("temp dir");
        // Same fixture, no in-progress marker: the branch really is gone.
        let wt = linked_worktree_fixture(dir.path(), &[]);
        let porcelain = detached_porcelain(&wt);

        assert_eq!(super::parse_orphan_worktrees(&porcelain), vec![wt.clone()]);
        assert!(
            !super::map_worktree_workspace_paths(&porcelain)
                .values()
                .any(|w| w.path == wt)
        );
    }

    #[test]
    fn delete_local_branch_removes_bare_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch from current HEAD
        git_cmd(repo.path())
            .args(["branch", "feat-to-delete"])
            .run()
            .expect("Failed to create branch");

        // Verify it exists
        let branches = list_local_branches(repo_path.clone()).unwrap();
        assert!(branches.contains(&"feat-to-delete".to_string()));

        // Delete it
        let result =
            delete_local_branch_impl(&repo_path, "feat-to-delete", "feat-to-delete", false, None);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl failed: {:?}",
            result
        );

        // Verify it's gone
        let branches = list_local_branches(repo_path).unwrap();
        assert!(!branches.contains(&"feat-to-delete".to_string()));
    }

    #[test]
    fn delete_local_branch_refuses_default_branch() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        let default_branch = get_remote_default_branch(&repo_path).unwrap();
        let result = delete_local_branch_impl(&repo_path, &default_branch, &default_branch, false, None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Refusing to delete default branch")
        );
    }

    #[test]
    fn delete_local_branch_with_worktree() {
        let (_config_guard, _config_dir) = with_temp_config_dir();
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        // Create a worktree with a new branch
        let config = WorktreeConfig {
            task_name: "wt-to-delete".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Verify worktree exists
        assert!(wt.path.exists());

        // Delete via delete_local_branch_impl (default cascade: keep_worktree = false)
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, false, None);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl failed: {:?}",
            result
        );

        // Worktree directory should be removed
        assert!(!wt.path.exists(), "Worktree directory should be gone");
    }

    /// Regression test for the "Keep worktree" bug.
    ///
    /// PostMergeCleanupDialog lets the user uncheck the "Archive/Delete worktree"
    /// step (intent: keep the worktree on disk) while leaving the "Delete local
    /// branch" step checked. With `keep_worktree = true`, `delete_local_branch_impl`
    /// must detach the worktree HEAD and remove only the branch ref, leaving the
    /// worktree directory and its files intact.
    #[test]
    fn delete_local_branch_should_preserve_worktree_when_user_keeps_it() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "wt-keep".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");
        assert!(wt.path.exists(), "precondition: worktree should exist");

        // Simulates PostMergeCleanupDialog flow with the worktree step
        // unchecked but delete-local checked. `keep_worktree = true` must
        // detach the worktree HEAD and remove only the branch ref.
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, true, None);
        assert!(
            result.is_ok(),
            "delete_local_branch_impl with keep_worktree=true failed: {:?}",
            result
        );

        assert!(
            wt.path.exists(),
            "Worktree directory was deleted despite keep_worktree=true"
        );

        // Branch ref must be gone
        let branches = list_local_branches(repo_path).unwrap();
        assert!(
            !branches.contains(&wt.name),
            "Branch ref should have been deleted"
        );
    }

    // --- list_local_branches tests ---
    // Previously only exercised incidentally as a setup/assertion helper inside the
    // delete_local_branch_* tests above; these cover it directly.

    #[test]
    fn list_local_branches_returns_current_branch_only_in_fresh_repo() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        let branches = list_local_branches(repo_path).expect("list_local_branches should succeed");

        assert_eq!(
            branches.len(),
            1,
            "a fresh repo with one commit should have exactly one branch, got: {branches:?}"
        );
    }

    #[test]
    fn list_local_branches_returns_all_branches() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        git_cmd(repo.path())
            .args(["branch", "feature-a"])
            .run()
            .expect("create feature-a");
        git_cmd(repo.path())
            .args(["branch", "feature-b"])
            .run()
            .expect("create feature-b");

        let branches = list_local_branches(repo_path).expect("list_local_branches should succeed");

        assert_eq!(
            branches.len(),
            3,
            "expected 3 local branches, got: {branches:?}"
        );
        assert!(branches.contains(&"feature-a".to_string()));
        assert!(branches.contains(&"feature-b".to_string()));
    }

    #[test]
    fn list_local_branches_handles_branch_names_with_slashes() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        git_cmd(repo.path())
            .args(["branch", "feature/nested-name"])
            .run()
            .expect("create feature/nested-name");

        let branches = list_local_branches(repo_path).expect("list_local_branches should succeed");

        assert!(
            branches.contains(&"feature/nested-name".to_string()),
            "branch name containing a slash should round-trip correctly, got: {branches:?}"
        );
    }

    #[test]
    fn check_worktree_dirty_clean_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "clean-wt".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        let dirty = check_worktree_dirty(repo_path, wt.name);
        assert!(dirty.is_ok());
        assert!(!dirty.unwrap(), "Clean worktree should not be dirty");
    }

    #[test]
    fn check_worktree_dirty_with_uncommitted_changes() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let worktrees_dir = repo.path().join("worktrees");

        let config = WorktreeConfig {
            task_name: "dirty-wt".to_string(),
            base_repo: repo_path.clone(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None)
            .expect("Failed to create worktree");

        // Add an uncommitted file in the worktree
        fs::write(wt.path.join("dirty.txt"), "uncommitted").expect("Failed to write dirty file");

        let dirty = check_worktree_dirty(repo_path, wt.name);
        assert!(dirty.is_ok());
        assert!(
            dirty.unwrap(),
            "Worktree with uncommitted changes should be dirty"
        );
    }

    #[test]
    fn check_worktree_dirty_no_worktree() {
        let repo = setup_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        // Create a branch without a worktree
        git_cmd(repo.path())
            .args(["branch", "bare-branch"])
            .run()
            .expect("Failed to create branch");

        let dirty = check_worktree_dirty(repo_path, "bare-branch".to_string());
        assert!(dirty.is_ok());
        assert!(
            !dirty.unwrap(),
            "Branch without worktree should not be dirty"
        );
    }

    #[test]
    fn run_setup_script_success() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("echo hello".to_string(), cwd).expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "hello");
        assert_eq!(result["stderr"].as_str().unwrap(), "");
    }

    /// A setup script that never finishes must be killed at its deadline, and
    /// the caller must be told so rather than getting a plausible-looking
    /// exit code. `sleep` stands in for the real cases: a script blocked on a
    /// stdin it can never be given, or on a lock nobody releases.
    #[cfg(unix)]
    #[test]
    fn run_shell_script_gives_up_at_the_deadline() {
        let dir = TempDir::new().expect("temp dir");

        let started = std::time::Instant::now();
        let err = run_shell_script("sleep 30", dir.path(), Duration::from_millis(300))
            .expect_err("a script that never finishes must fail");
        let waited = started.elapsed();

        assert!(
            err.contains("timed out"),
            "expected a timeout error, got: {err}"
        );
        assert!(
            waited < Duration::from_secs(5),
            "must not wait for the script; waited {waited:?}"
        );
    }

    /// The control: a script that finishes inside its deadline is not truncated.
    #[test]
    fn run_shell_script_keeps_output_of_a_script_that_finishes_in_time() {
        let dir = TempDir::new().expect("temp dir");

        let out = run_shell_script("echo alive", dir.path(), Duration::from_secs(30))
            .expect("a fast script must succeed");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "alive");
    }

    #[test]
    fn run_setup_script_failure() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("exit 42".to_string(), cwd)
            .expect("should return result even on non-zero exit");
        assert_eq!(result["exit_code"], 42);
    }

    #[test]
    fn run_setup_script_captures_stderr() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script(fail_with_stderr_script("oops", 1), cwd)
            .expect("should return result");
        assert_eq!(result["exit_code"], 1);
        assert_eq!(result["stderr"].as_str().unwrap().trim(), "oops");
    }

    #[test]
    fn run_setup_script_invalid_cwd() {
        let result = run_setup_script("echo hi".to_string(), "/nonexistent/path/xyz".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn run_setup_script_runs_in_cwd() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("marker.txt"), "found").expect("write marker");
        let cwd = dir.path().to_string_lossy().to_string();

        let result =
            run_setup_script(print_file_script("marker.txt"), cwd).expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "found");
    }

    #[test]
    fn run_script_in_dir_succeeds_with_zero_exit() {
        let dir = TempDir::new().expect("temp dir");
        let result = run_script_in_dir("echo hello", dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn run_script_in_dir_fails_with_nonzero_exit() {
        let dir = TempDir::new().expect("temp dir");
        let result = run_script_in_dir("exit 1", dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exit code 1"));
    }

    #[test]
    fn run_script_in_dir_runs_in_correct_directory() {
        let dir = TempDir::new().expect("temp dir");
        fs::write(dir.path().join("test-file.txt"), "content").expect("write");
        let result = run_script_in_dir(&print_file_script("test-file.txt"), dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn archive_worktree_runs_archive_script_before_archiving() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-script-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let _wt = create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // Script creates a marker file inside the worktree dir; archive should still succeed
        let marker = worktrees_dir.join("archive-marker.txt");
        let script = touch_script(&marker.display().to_string());
        let result = archive_worktree(repo.path(), "archive-script-test", Some(&script));
        assert!(
            result.is_ok(),
            "archive with script should succeed: {:?}",
            result
        );
    }

    #[test]
    fn archive_worktree_blocks_on_failed_script() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-block-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // Script exits non-zero — archive should be blocked
        let result = archive_worktree(repo.path(), "archive-block-test", Some("exit 1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Archive script failed"));
        // Worktree should still exist (not archived)
        assert!(
            wt.path.exists(),
            "worktree should still exist after failed script"
        );
    }

    #[test]
    fn archive_worktree_skips_empty_script() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-noscript-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // None script — should proceed normally
        let result = archive_worktree(repo.path(), "archive-noscript-test", None);
        assert!(
            result.is_ok(),
            "archive without script should succeed: {:?}",
            result
        );
    }

    #[test]
    fn free_archive_dest_avoids_clobbering_prior_archives() {
        // The anti-clobber guarantee for archive_worktree: archiving the same
        // branch name again must land on a fresh suffix, never overwrite. Tested
        // directly on the pure destination-picker because whether the worktree
        // dir survives `git worktree remove --force` (and thus reaches the rename)
        // is git-version/OS dependent — see archive_worktree_moves_directory.
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive_dir = tmp.path().join("__archived");
        fs::create_dir_all(&archive_dir).expect("mkdir archive");

        // No prior archive → base name.
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch")
        );

        // Prior archive at base name → first free suffix, base left untouched.
        let base = archive_dir.join("dup-branch");
        fs::create_dir_all(&base).expect("mkdir base");
        fs::write(base.join("marker.txt"), "first").expect("write marker");
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch-2")
        );

        // Base and -2 taken → skips to -3.
        fs::create_dir_all(archive_dir.join("dup-branch-2")).expect("mkdir -2");
        assert_eq!(
            free_archive_dest(&archive_dir, "dup-branch"),
            archive_dir.join("dup-branch-3")
        );

        // Original archive contents are never removed by the picker.
        assert_eq!(
            fs::read_to_string(base.join("marker.txt")).expect("read marker"),
            "first"
        );
    }

    #[test]
    fn test_set_and_get_branch_base() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Initially no base set
        let base = get_branch_base(&path, "main");
        assert!(
            base.is_none(),
            "expected no base initially, got: {:?}",
            base
        );

        // Set a base
        set_branch_base(&path, "main", "develop").unwrap();
        let base = get_branch_base(&path, "main");
        assert_eq!(base, Some("develop".to_string()));

        // Overwrite
        set_branch_base(&path, "main", "origin/main").unwrap();
        let base = get_branch_base(&path, "main");
        assert_eq!(base, Some("origin/main".to_string()));
    }

    #[test]
    fn test_get_branch_bases_batches_all_entries() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // No bases set yet -> empty map.
        assert!(get_branch_bases(&path).is_empty());

        // Set bases for several branches (names include a dot and a slash to
        // exercise the prefix/suffix anchoring of the key parser).
        set_branch_base(&path, "main", "develop").unwrap();
        set_branch_base(&path, "feature.x", "main").unwrap();
        set_branch_base(&path, "team/feat", "origin/main").unwrap();

        let bases = get_branch_bases(&path);
        assert_eq!(bases.len(), 3, "expected all 3 bases, got: {bases:?}");
        assert_eq!(bases.get("main").map(String::as_str), Some("develop"));
        assert_eq!(bases.get("feature.x").map(String::as_str), Some("main"));
        assert_eq!(
            bases.get("team/feat").map(String::as_str),
            Some("origin/main")
        );

        // Batch result matches the per-branch reader byte-for-byte.
        for (name, base) in &bases {
            assert_eq!(get_branch_base(&path, name).as_ref(), Some(base));
        }
    }

    #[test]
    fn test_fetch_remote_ref_for_local_is_noop() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // A local ref like "main" should not trigger a fetch (no-op)
        let result = fetch_if_remote(&path, "main");
        assert!(result.is_ok(), "local ref should not fail: {:?}", result);
    }

    #[test]
    fn test_fetch_local_branch_with_slash_is_noop() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // A local branch name can legitimately contain a slash (e.g. Jira-style
        // "POC-0001/merge-radar"). It must NOT be mistaken for a remote ref and
        // fetched — otherwise git treats "POC-0001" as a remote and fails.
        git_cmd(repo.path())
            .args(["branch", "POC-0001/merge-radar"])
            .run()
            .expect("Failed to create slashed local branch");

        let result = fetch_if_remote(&path, "POC-0001/merge-radar");
        assert!(
            result.is_ok(),
            "slashed local branch should be a no-op, not a failed fetch: {:?}",
            result
        );
    }

    /// A fetch that never answers must be killed at its deadline, not waited on.
    ///
    /// The remote is an `ext::` transport helper that only sleeps, so the hang
    /// is deterministic and needs no network — and, like a real credential
    /// helper, the sleeper is a grandchild holding git's pipes open, which is
    /// the case `output_with_deadline` refuses to join on.
    ///
    /// Unix only: the helper is `sleep`.
    #[cfg(unix)]
    #[test]
    fn fetch_if_remote_gives_up_at_the_deadline() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        git_cmd(repo.path())
            .args(["remote", "add", "origin", "ext::sleep 45"])
            .run()
            .expect("add the hanging remote");
        // git refuses the ext transport unless the repo opts in.
        git_cmd(repo.path())
            .args(["config", "protocol.ext.allow", "always"])
            .run()
            .expect("allow the ext transport");
        // fetch_if_remote only fetches a ref that resolves under refs/remotes/.
        git_cmd(repo.path())
            .args(["update-ref", "refs/remotes/origin/main", "HEAD"])
            .run()
            .expect("create the remote-tracking ref");

        // Zombies owned by this process BEFORE the fetch. Under `cargo nextest`
        // this is always empty — one process per test — but under `cargo test`
        // every test in the binary is a thread of the SAME process, so other
        // tests' unreaped children are counted too. Comparing against a
        // baseline instead of against zero is what makes the assertion below
        // mean "this fetch leaked a zombie" under either runner.
        let zombies_before = own_zombie_pids();

        // Off-thread behind a hard receive deadline: an unwired timeout means
        // the call never returns, and this must report that rather than hang
        // the suite on it.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_if_remote(&path, "origin/main"));
        });
        let result = rx
            .recv_timeout(FETCH_TIMEOUT + std::time::Duration::from_secs(30))
            .expect("fetch_if_remote must return — its deadline is not wired");

        let err = result.expect_err("a fetch that never answers must fail");
        assert!(
            err.contains("timed out"),
            "expected a timeout error, got: {err}"
        );

        // The killed git must be reaped, not left as a zombie of this process.
        let leaked: Vec<u32> = own_zombie_pids()
            .into_iter()
            .filter(|pid| !zombies_before.contains(pid))
            .collect();
        assert!(
            leaked.is_empty(),
            "the timed-out git was left as a zombie child: {leaked:?}"
        );
    }

    /// PIDs of this process's children currently in the zombie state.
    ///
    /// Returns pids rather than a count so a caller can diff two samples: a
    /// count would report "2 before, 2 after" as unchanged even if one child
    /// had been reaped and a different one leaked in the same window.
    ///
    /// Unix-only, like its caller: `ps` and the zombie state are both POSIX,
    /// and without the gate this is dead code the Windows job warns about.
    #[cfg(unix)]
    fn own_zombie_pids() -> Vec<u32> {
        let ps = Command::new("ps")
            .args(["-o", "pid=,ppid=,stat=", "-ax"])
            .output()
            .expect("ps");
        let table = String::from_utf8_lossy(&ps.stdout);
        let mine = std::process::id().to_string();
        table
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let pid = fields.next()?;
                if fields.next()? != mine {
                    return None;
                }
                fields.next()?.starts_with('Z').then(|| pid.parse().ok())?
            })
            .collect()
    }

    /// End-to-end companion to `test_fetch_local_branch_with_slash_is_noop`:
    /// the user-facing "Create Branch from <slashed local branch>" flow reaches
    /// `fetch_if_remote` through `create_worktree_internal`'s start-point, so the
    /// no-op must hold at the caller too — not just in the helper.
    #[test]
    fn test_create_worktree_from_slashed_local_start_point_succeeds() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        git_cmd(repo.path())
            .args(["branch", "POC-0001/merge-radar"])
            .run()
            .expect("Failed to create slashed local branch");

        let config = WorktreeConfig {
            task_name: "from-slashed-base".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("POC-0002/derived".to_string()),
            create_branch: true,
        };

        let result =
            create_worktree_internal(&worktrees_dir, &config, Some("POC-0001/merge-radar"));
        assert!(
            result.is_ok(),
            "slashed local start-point must not be fetched as a remote: {:?}",
            result
        );
        assert_eq!(
            result.unwrap().branch,
            Some("POC-0002/derived".to_string()),
            "new branch should be created from the slashed local base"
        );
    }

    /// Get the current branch name in a test repo (could be main or master)
    fn current_branch(repo: &TempDir) -> String {
        let out = git_cmd(repo.path())
            .args(["branch", "--show-current"])
            .run()
            .expect("Failed to get current branch");
        out.stdout.trim().to_string()
    }

    #[test]
    fn test_create_branch_persists_base_ref() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();
        let default_branch = current_branch(&repo);

        // Create branch with a start_point
        crate::git::create_branch_impl(&path, "feature-x", Some(&default_branch), false).unwrap();

        // Base ref should be persisted
        let base = get_branch_base(&path, "feature-x");
        assert_eq!(base, Some(default_branch));
    }

    #[test]
    fn test_create_worktree_persists_base_ref() {
        let repo = setup_test_repo();
        let default_branch = current_branch(&repo);
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "persist-base-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("persist-base-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, Some(&default_branch)).unwrap();

        let base = get_branch_base(&repo.path().to_string_lossy(), "persist-base-test");
        assert_eq!(base, Some(default_branch));
    }

    /// A `post-checkout` hook script kept in the build directory and rewritten
    /// only when its content is wrong, so every run execs the *same* inode.
    ///
    /// macOS scans each never-before-seen executable inode on its first exec.
    /// Two independent effects stack, and measuring only one of them misleads
    /// (both of us did, from opposite directions, before pairing the samples).
    ///
    /// 1. A **persistent** penalty on `/var/folders/…/T` — `$TMPDIR`, which is
    ///    exactly where `TempDir` lands. Paired alternating samples, fresh inode
    ///    each time, 2026-09-06: `$TMPDIR` 0.685s / 0.359s against `/tmp` 0.250s
    ///    / 0.233s and `~/Gits/.tmp` 0.263s / 0.238s in the same seconds.
    /// 2. An **episodic** scanner backlog that lifts the floor everywhere for
    ///    minutes at a time, and amplifies (1) enormously while it lasts: the
    ///    same pairing during a backlog gave `$TMPDIR` 191-393s against `/tmp`
    ///    1.9-4.5s. A 120s+ outlier is what first surfaced this test.
    ///
    /// How the two combine is NOT settled: a plain multiplicative model predicts
    /// ~9s for `$TMPDIR` under the backlog above and 393s was measured, so the
    /// interaction looks superlinear — which would mean the penalty is worst
    /// exactly when the suite is busiest. Pinning that down costs 400-second
    /// samples and changes no remedy, so it is left open on purpose.
    ///
    /// So a single timing sample proves nothing, and neither variable is worth
    /// chasing. What is stable is the caching: a re-exec of an already-scanned
    /// inode is ~0.01s, and the cache is keyed on the *inode*, so a symlink to a
    /// warm script is free while a byte-identical copy is not. Minting a hook
    /// into a fresh `TempDir` per run re-rolls both dice every run; pointing at
    /// one stable file removes the exec from the lottery under either model.
    #[cfg(unix)]
    fn shared_post_checkout_hook() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        const BODY: &[u8] = b"#!/bin/sh\ntouch .hook-ran\n";

        let dir = Path::new(env!("OUT_DIR")).join("test-hooks");
        fs::create_dir_all(&dir).expect("create shared hook dir");
        let hook = dir.join("post-checkout");

        if fs::read(&hook).ok().as_deref() != Some(BODY) {
            // Stage and rename so a concurrent run can never exec a partial file.
            let staged = dir.join(format!("post-checkout.{}.tmp", std::process::id()));
            fs::write(&staged, BODY).expect("write shared hook");
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
                .expect("chmod shared hook");
            fs::rename(&staged, &hook).expect("install shared hook");
        }
        hook
    }

    // Verify that post-checkout hooks still run after `git worktree add --quiet`.
    // --quiet only suppresses git's own checkout progress lines, not hooks.
    #[test]
    #[cfg(unix)]
    fn test_create_worktree_runs_post_checkout_hook() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");

        // Symlink rather than copy: git resolves it and execs the shared inode,
        // so the hook still installs at the canonical path real users use, with
        // no per-run exec scan. See `shared_post_checkout_hook`.
        let hooks_dir = repo.path().join(".git/hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        std::os::unix::fs::symlink(shared_post_checkout_hook(), hooks_dir.join("post-checkout"))
            .unwrap();

        let config = WorktreeConfig {
            task_name: "hook-run-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("hook-run-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).unwrap();

        let worktree_path = worktrees_dir.join("hook-run-test");
        assert!(
            worktree_path.join(".hook-ran").exists(),
            "post-checkout hook did not run — --quiet must not suppress hooks"
        );
    }

    #[test]
    fn test_list_base_ref_options_returns_structured_refs() {
        let repo = setup_test_repo();
        let path = repo.path().to_string_lossy().to_string();

        // Create extra local branches
        git_cmd(repo.path())
            .args(["branch", "feature-a"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["branch", "feature-b"])
            .run()
            .unwrap();

        let refs = list_base_ref_options(path).unwrap();

        // Should have at least the default + 2 feature branches
        assert!(
            refs.len() >= 3,
            "expected at least 3 refs, got {}",
            refs.len()
        );

        // First ref should be the default branch, flagged is_default
        let default_ref = &refs[0];
        assert!(
            default_ref.is_default,
            "first ref should be the default branch"
        );
        assert_eq!(default_ref.kind, "local");

        // All refs should have non-empty names
        for r in &refs {
            assert!(!r.name.is_empty(), "ref name should not be empty");
            assert!(
                r.kind == "local" || r.kind == "remote",
                "kind should be local or remote"
            );
        }

        // feature-a and feature-b should be present as local
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"feature-a"), "feature-a should be in refs");
        assert!(names.contains(&"feature-b"), "feature-b should be in refs");

        // No origin/HEAD should appear
        assert!(
            !names.contains(&"origin/HEAD"),
            "origin/HEAD should be filtered out"
        );
    }

    #[test]
    fn test_list_base_ref_options_includes_remote_refs() {
        let repo = setup_test_repo();
        let path_str = repo.path().to_string_lossy().to_string();

        // Create a bare remote and push to it to get remote tracking refs
        let remote_dir = TempDir::new().unwrap();
        git_cmd(remote_dir.path())
            .args(["init", "--bare"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args([
                "remote",
                "add",
                "origin",
                &remote_dir.path().to_string_lossy(),
            ])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["push", "-u", "origin", "main"])
            .run()
            .or_else(|_| {
                git_cmd(repo.path())
                    .args(["push", "-u", "origin", "master"])
                    .run()
            })
            .unwrap();

        // Create a remote-only branch
        git_cmd(repo.path())
            .args(["branch", "remote-only"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["push", "origin", "remote-only"])
            .run()
            .unwrap();
        git_cmd(repo.path())
            .args(["branch", "-D", "remote-only"])
            .run()
            .unwrap();

        // Fetch so we have remote tracking refs
        git_cmd(repo.path())
            .args(["fetch", "origin"])
            .run()
            .unwrap();

        let refs = list_base_ref_options(path_str).unwrap();

        // Should include remote refs
        let remote_refs: Vec<&BaseRefOption> = refs.iter().filter(|r| r.kind == "remote").collect();
        assert!(
            !remote_refs.is_empty(),
            "should include remote refs, got: {:?}",
            refs
        );

        // origin/remote-only should appear as remote
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"origin/remote-only"),
            "origin/remote-only should be in refs, got: {:?}",
            names
        );
    }

    // ── linked workspace creation and warming ─────────────────────────────

    fn workspace_fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().expect("temp dir");
        let repo = temp.path().join("repo");
        let workspaces = temp.path().join("repo__wt");
        fs::create_dir_all(&repo).expect("repo dir");
        git_cmd(&repo).args(["init"]).run().expect("git init");
        git_cmd(&repo)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .expect("git email");
        git_cmd(&repo)
            .args(["config", "user.name", "Test"])
            .run()
            .expect("git name");
        fs::write(repo.join("README.md"), "base\n").expect("tracked file");
        git_cmd(&repo).args(["add", "."]).run().expect("git add");
        git_cmd(&repo)
            .args(["commit", "-m", "initial"])
            .run()
            .expect("git commit");
        (temp, repo, workspaces)
    }

    #[test]
    fn create_workspace_always_returns_a_linked_worktree() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "feature".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("feature".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert_eq!(created.workspace_id, "feature");
        assert!(created.path.join(".git").is_file());
    }

    #[test]
    fn create_workspace_reports_best_effort_warming() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "warm".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("warm".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport {
                warmed: 2,
                warnings: vec!["one cache stayed cold".into()],
            }
        })
        .expect("linked workspace");

        assert_eq!(created.warmed_directories, 2);
        assert_eq!(created.warnings, vec!["one cache stayed cold"]);
    }

    #[test]
    fn workspace_payload_states_linked_isolation_and_clean_tracked_state() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let config = WorktreeConfig {
            task_name: "payload".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("payload".into()),
            create_branch: true,
        };
        let created = create_workspace_with(&workspaces, &config, None, |_, _| {
            crate::cow::WarmingReport::default()
        })
        .expect("linked workspace");
        let payload = created.instruction_payload();

        assert_eq!(payload["kind"], "worktree");
        assert_eq!(payload["state"]["carried_over"], 0);
        assert!(
            payload["isolation"]
                .as_str()
                .unwrap()
                .contains("linked worktree")
        );
        assert_eq!(payload["warm_artifacts"]["warmed_directories"], 0);
    }
}
