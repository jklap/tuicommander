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

/// One workspace, resolved to the mechanism that built it.
///
/// Typed rather than a bare path because the two mechanisms need completely
/// different lifecycle handling and the difference is invisible from the path
/// alone. `git worktree remove` on a COW clone fails with "not a working tree",
/// which `remove_worktree_internal` treats as "already gone" and follows with
/// an unconditional `remove_dir_all` — so an untyped resolver would delete an
/// independent repository, and its unpublished commits, without ever reaching
/// the guard that exists to stop that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedWorkspace {
    /// The main checkout or a linked worktree: `git worktree list` knows it,
    /// and its refs live in the parent.
    Worktree(WorkspaceWorktree),
    /// An independent clone. Only `repositories.json` knows it exists.
    Cow(crate::cow::CowRecord),
}

/// Resolve `workspace_id` against BOTH sources: git's own worktree list and the
/// COW records in the persisted document.
///
/// An id present in both is an error rather than a winner. The two id spaces
/// are disjoint by construction — a linked worktree's id is its branch, a COW
/// clone's is minted with a `~` suffix — so an overlap means a hand-edited or
/// corrupt record, and picking one silently is how the wrong directory gets
/// deleted.
pub(crate) fn resolve_any_workspace(
    base_repo: &Path,
    workspace_id: &str,
) -> Result<ResolvedWorkspace, String> {
    let from_git = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .ok()
        .and_then(|out| map_worktree_workspace_paths(&out.stdout).remove(workspace_id));

    let from_records = crate::cow::cow_workspaces_for(base_repo)
        .into_iter()
        .find(|record| record.workspace_id == workspace_id);

    match (from_git, from_records) {
        (Some(_), Some(_)) => Err(format!(
            "workspace id '{workspace_id}' names both a git worktree and a COW workspace in '{}' — \
             refusing to guess which one you meant",
            base_repo.display()
        )),
        (Some(worktree), None) => Ok(ResolvedWorkspace::Worktree(worktree)),
        (None, Some(record)) => Ok(ResolvedWorkspace::Cow(record)),
        (None, None) => Err(format!(
            "No workspace found for id '{workspace_id}' in '{}'",
            base_repo.display()
        )),
    }
}

/// Get a workspace's commits into the parent repo and out to origin.
///
/// A no-op for a linked worktree, which shares its refs with the parent
/// already — and says so, rather than reporting a success that did nothing.
pub(crate) fn publish_workspace_impl(
    repo_path: &str,
    workspace_id: &str,
) -> Result<crate::cow::PublishOutcome, String> {
    match resolve_any_workspace(Path::new(repo_path), workspace_id)? {
        ResolvedWorkspace::Cow(record) => crate::cow::publish_cow_workspace(&record),
        ResolvedWorkspace::Worktree(_) => Ok(crate::cow::PublishOutcome {
            no_op_reason: Some(
                "this is a linked worktree: its refs and objects are shared with the parent \
                 repository, so its commits are already visible there. There is nothing to publish."
                    .to_string(),
            ),
            ..Default::default()
        }),
    }
}

/// How many commits exist only in this workspace.
///
/// Zero for a linked worktree, always: its objects live in the parent and
/// survive the directory, so there is nothing a removal could destroy. That is
/// the same asymmetry the removal guard is built on, and reporting it as a
/// number lets the UI show the count on the rows where it means something.
pub(crate) fn unpublished_commits_impl(
    repo_path: &str,
    workspace_id: &str,
) -> Result<usize, String> {
    match resolve_any_workspace(Path::new(repo_path), workspace_id)? {
        ResolvedWorkspace::Cow(record) => crate::cow::unpublished_commit_count(&record),
        ResolvedWorkspace::Worktree(_) => Ok(0),
    }
}

/// Tauri command: how many commits exist only in this workspace.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn count_unpublished_commits(
    repo_path: String,
    workspace_id: String,
) -> Result<usize, String> {
    unpublished_commits_impl(&repo_path, &workspace_id)
}

/// Tauri command: publish a workspace.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn publish_workspace(
    repo_path: String,
    workspace_id: String,
) -> Result<crate::cow::PublishOutcome, String> {
    publish_workspace_impl(&repo_path, &workspace_id)
}

/// A workspace, however it was built.
///
/// One type for both mechanisms on purpose: the caller asked for a workspace,
/// and everything downstream — the sidebar row, publish, remove — needs to know
/// which one it got rather than infer it from the directory's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedWorkspace {
    /// How the caller addresses this workspace from here on. Minted HERE, in
    /// the backend, because creation happens here: an id invented by whichever
    /// client happened to ask would not exist for the other transports.
    pub(crate) workspace_id: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    pub(crate) kind: crate::cow::WorkspaceKind,
    /// Set only when `mode=auto` asked for COW and could not have it. Carries
    /// the check that said no, because "you got a linked worktree" without a
    /// reason is indistinguishable from "you asked for one".
    pub(crate) degraded_reason: Option<String>,
    /// Repo shapes the guards noticed. Empty for a linked worktree: the guards
    /// are about cloning, and a worktree is not a clone.
    pub(crate) warnings: Vec<String>,
    /// Paths the parent's working tree carried over. Always 0 for a linked
    /// worktree, which starts from a clean checkout of the branch.
    pub(crate) carried_over: usize,
    /// What was asked of the parent's uncommitted work. Reported because the
    /// caller needs to know whether an empty tree means "clean policy" or
    /// "nothing was dirty".
    pub(crate) dirty_policy: crate::cow::DirtyPolicy,
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
        let is_cow = self.kind == crate::cow::WorkspaceKind::Cow;

        let isolation = if is_cow {
            format!(
                "This is an independent repository, not a linked worktree. Commits you make exist ONLY \
                 here until they are published: the parent repo cannot see this branch, and `git merge \
                 {}` run in the parent will NOT find your work — worse, it silently merges a same-named \
                 branch there if one exists. Publish with the `publish_workspace` command (it fetches \
                 into the parent and pushes to origin). Removal refuses while unpublished commits \
                 exist; publish rather than working around it.",
                self.branch
            )
        } else {
            "This is a linked worktree: refs and objects are shared with the parent repository, so \
             your commits are visible there immediately. There is nothing to publish."
                .to_string()
        };

        let setup = if warm.is_empty() {
            "No build output came with this workspace.".to_string()
        } else {
            "These came with the workspace at near-zero cost. Do NOT run an install or a full build \
             to \"set up\" — they are already warm. Run one only if a lockfile or a dependency \
             actually changed."
                .to_string()
        };

        let carried = match self.carried_over {
            0 => {
                "Nothing was carried over: this workspace starts from a clean checkout.".to_string()
            }
            n => format!(
                "{n} modified path(s) carried over from the parent, so this workspace starts from the \
                 parent's work in progress rather than a clean HEAD. That is deliberate and free — it \
                 is not damage, and it is not yours to fix unless the task says so."
            ),
        };

        serde_json::json!({
            "workspace_id": self.workspace_id,
            "path": self.path.to_string_lossy(),
            "branch": self.branch,
            "kind": self.kind,
            "degraded_reason": self.degraded_reason,
            "warnings": self.warnings,
            "state": {
                "dirty_policy": self.dirty_policy,
                "carried_over": self.carried_over,
                "note": carried,
            },
            "warm_artifacts": {
                "present": warm,
                "note": setup,
            },
            "isolation": isolation,
        })
    }
}

/// Every workspace id `base_repo` already has, from both sources, so a minted
/// one cannot collide with either.
fn taken_workspace_ids(base_repo: &Path) -> Vec<String> {
    let from_git: Vec<String> = git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .ok()
        .map(|out| {
            map_worktree_workspace_paths(&out.stdout)
                .into_keys()
                .collect()
        })
        .unwrap_or_default();

    let from_records = crate::cow::cow_workspaces_for(base_repo)
        .into_iter()
        .map(|record| record.workspace_id);

    from_git.into_iter().chain(from_records).collect()
}

/// Create the workspace `mode` asks for, degrading rather than failing.
///
/// The one entry point that knows both mechanisms exist. Both derive the same
/// destination from `worktrees_dir` + the sanitized task name, so a caller
/// cannot end up with a COW clone and a worktree in different places depending
/// on which path ran.
///
/// Reached from a transport in #734-ca73 (MCP and HTTP `worktree_create` gain
/// `mode` and `dirty`) and from the UI in #735-55d7. Until then the existing
/// creation path still calls `create_worktree_with_stale_recovery` directly,
/// which is what this wraps for `mode=worktree` — there is one implementation
/// of each mechanism, not two.
#[allow(dead_code)]
pub(crate) fn create_workspace(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
    mode: crate::cow::WorkspaceMode,
    dirty: crate::cow::DirtyPolicy,
) -> Result<CreatedWorkspace, String> {
    create_workspace_with(
        worktrees_dir,
        config,
        base_ref,
        mode,
        dirty,
        crate::cow::probe_cow_support,
    )
}

/// [`create_workspace`] with the COW probe injected, so a test can force the
/// degrade without a second filesystem to fail against.
#[allow(dead_code)]
pub(crate) fn create_workspace_with(
    worktrees_dir: &Path,
    config: &WorktreeConfig,
    base_ref: Option<&str>,
    mode: crate::cow::WorkspaceMode,
    dirty: crate::cow::DirtyPolicy,
    probe: impl Fn(&Path, &Path) -> crate::cow::CowSupport,
) -> Result<CreatedWorkspace, String> {
    let src = PathBuf::from(&config.base_repo);
    let dest = worktrees_dir.join(sanitize_name(&config.task_name));
    let branch = config
        .branch
        .clone()
        .unwrap_or_else(|| sanitize_name(&config.task_name));

    match crate::cow::choose_mechanism_with(&src, &dest, mode, probe)? {
        crate::cow::Mechanism::Cow(guards) => {
            // A clone's id is MINTED: it is a second workspace on a branch that
            // may already have one, so the branch cannot name it. Minted before
            // the copy so a failure leaves no id claimed.
            let workspace_id = crate::cow::mint_workspace_id(&branch, &taken_workspace_ids(&src));
            let workspace = crate::cow::create_cow_workspace(&src, &dest, &branch, dirty, &guards)?;
            Ok(CreatedWorkspace {
                workspace_id,
                path: workspace.path,
                branch: workspace.branch,
                kind: crate::cow::WorkspaceKind::Cow,
                degraded_reason: None,
                warnings: workspace.warnings,
                carried_over: workspace.carried_over,
                dirty_policy: workspace.dirty_policy,
            })
        }
        crate::cow::Mechanism::Worktree { degraded_reason } => {
            let worktree = create_worktree_with_stale_recovery(worktrees_dir, config, base_ref)?;
            let branch = worktree.branch.unwrap_or(branch);
            Ok(CreatedWorkspace {
                // A linked worktree's id IS its branch — the identity migration,
                // the same rule `map_worktree_workspace_paths` applies when it
                // reads them back.
                workspace_id: workspace_id_of_worktree(&branch),
                path: worktree.path,
                branch,
                kind: crate::cow::WorkspaceKind::Worktree,
                degraded_reason,
                warnings: Vec::new(),
                // A linked worktree is a fresh checkout of the branch: the
                // parent's uncommitted work stays in the parent, which is the
                // isolation difference the caller has to be told about.
                carried_over: 0,
                // Recorded as asked for, not as applied: no dirty policy runs
                // on a worktree, because there is nothing carried over to clean.
                dirty_policy: dirty,
            })
        }
    }
}

pub(crate) fn remove_worktree_internal(worktree: &WorktreeInfo, force: bool) -> Result<(), String> {
    let wt_path_str = worktree.path.to_string_lossy().to_string();
    tracing::info!(
        source = "worktree",
        branch = %worktree.name,
        path = %wt_path_str,
        force = %force,
        "remove_worktree_internal: start"
    );

    let force_args: &[&str] = if force {
        &["worktree", "remove", "--force", "--force"]
    } else {
        &["worktree", "remove", "--force"]
    };

    match git_cmd(&worktree.base_repo)
        .args(
            force_args
                .iter()
                .chain(std::iter::once(&wt_path_str.as_str())),
        )
        .run()
    {
        Ok(_) => {
            tracing::info!(source = "worktree", branch = %worktree.name, force = %force, "git worktree remove: OK");
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
            if !force
                && (stderr.contains("locked working tree")
                    || stderr.contains("cannot remove a locked")) =>
        {
            // Worktree is locked and caller did not request force. Surface a
            // distinctive error so the JS layer can prompt the user to confirm
            // before retrying with force=true.
            tracing::warn!(
                source = "worktree",
                branch = %worktree.name,
                stderr = %stderr,
                "git worktree remove: locked — returning error for JS confirmation prompt"
            );
            return Err(format!("{LOCKED_WORKTREE_PREFIX}{stderr}"));
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

/// Create a worktree without a PTY session
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn create_worktree(
    state: State<'_, Arc<AppState>>,
    base_repo: String,
    branch_name: String,
    create_branch: Option<bool>,
    base_ref: Option<String>,
    mode: Option<crate::cow::WorkspaceMode>,
    dirty: Option<crate::cow::DirtyPolicy>,
) -> Result<serde_json::Value, String> {
    // The desktop path gets the same two levers as MCP and HTTP: without them
    // the UI could not create a COW workspace at all, and the transports would
    // disagree about what `create_worktree` means.
    let mode = mode.unwrap_or_default();
    let dirty = dirty.unwrap_or_default();
    if mode != crate::cow::WorkspaceMode::Worktree {
        let worktrees_dir =
            resolve_worktree_dir_for_repo(Path::new(&base_repo), &state.worktrees_dir);
        let config = WorktreeConfig {
            task_name: branch_name.clone(),
            base_repo: base_repo.clone(),
            branch: Some(branch_name.clone()),
            create_branch: create_branch.unwrap_or(true),
        };
        let base_ref_owned = base_ref.clone();
        let workspace = tokio::task::spawn_blocking(move || {
            create_workspace(
                &worktrees_dir,
                &config,
                base_ref_owned.as_deref(),
                mode,
                dirty,
            )
        })
        .await
        .map_err(|e| format!("Task panic: {e}"))??;

        state.invalidate_repo_caches(&base_repo);
        return Ok(serde_json::json!({
            "status": "ok",
            "name": workspace.path.file_name().map(|n| n.to_string_lossy().to_string()),
            "path": workspace.path.to_string_lossy(),
            "workspace_id": workspace.workspace_id,
            "branch": workspace.branch,
            "base_repo": base_repo,
            "kind": workspace.kind,
            "degraded_reason": workspace.degraded_reason,
            "instructions": workspace.instruction_payload(),
        }));
    }

    // `mode=worktree` keeps the original path, which carries the stale-directory
    // recovery the UI depends on (the `status: "pending"` placeholder and its
    // background recreate). Routing it through `create_workspace` would drop
    // that, and it is the path the "+" button has always taken.
    let config = WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo,
        branch: Some(branch_name),
        create_branch: create_branch.unwrap_or(true),
    };

    let worktrees_dir =
        resolve_worktree_dir_for_repo(Path::new(&config.base_repo), &state.worktrees_dir);

    // All git operations are blocking — run them off the async executor
    let first = {
        let d = worktrees_dir.clone();
        let c = config.clone();
        let r = base_ref.clone();
        tokio::task::spawn_blocking(move || create_worktree_internal(&d, &c, r.as_deref()))
            .await
            .map_err(|e| format!("Task panic: {e}"))?
    };

    match first {
        Ok(worktree) => {
            state.invalidate_repo_caches(&config.base_repo);
            let branch = worktree.branch.clone().unwrap_or_default();
            Ok(serde_json::json!({
                "status": "ok",
                "name": worktree.name,
                "path": worktree.path.to_string_lossy(),
                // Same field the HTTP route reports, for the same reason: this is
                // how the caller addresses the workspace afterwards, and it must
                // not be re-derived from the branch.
                "workspace_id": workspace_id_of_worktree(&branch),
                "branch": worktree.branch,
                "base_repo": worktree.base_repo.to_string_lossy(),
            }))
        }
        Err(ref e) if e.starts_with(STALE_DIR_PREFIX) => {
            // Stale directory: return immediately with pending status, clean up + recreate in background.
            let worktree_name = sanitize_name(&config.task_name);
            let stale_path = worktrees_dir.join(&worktree_name);
            let in_flight_key = format!("{}::{worktree_name}", config.base_repo);

            // Re-entrancy guard: if another background task is already recreating
            // this path, don't spawn a second one — that would race on git worktree
            // remove + recreate against the same directory.
            if !state
                .worktree_recreate_in_flight
                .insert(in_flight_key.clone())
            {
                tracing::info!(
                    source = "worktree",
                    key = %in_flight_key,
                    "create_worktree: recreate already in-flight, returning pending without re-spawning"
                );
                let branch = config
                    .branch
                    .clone()
                    .unwrap_or_else(|| worktree_name.clone());
                return Ok(serde_json::json!({
                    "status": "pending",
                    "name": worktree_name,
                    "path": stale_path.to_string_lossy(),
                    "workspace_id": workspace_id_of_worktree(&branch),
                    "branch": branch,
                    "base_repo": config.base_repo,
                }));
            }

            let state_arc = Arc::clone(&*state);
            let config_bg = config.clone();
            let worktrees_dir_bg = worktrees_dir.clone();
            let base_ref_bg = base_ref.clone();
            let stale_path_bg = stale_path.clone();
            let in_flight_key_bg = in_flight_key.clone();
            let branch_for_err = config
                .branch
                .clone()
                .unwrap_or_else(|| worktree_name.clone());

            tokio::spawn(async move {
                // Ensure the in-flight key is removed on every exit path.
                struct Guard(Arc<crate::state::AppState>, String);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.worktree_recreate_in_flight.remove(&self.1);
                    }
                }
                let _guard = Guard(Arc::clone(&state_arc), in_flight_key_bg);

                let emit_repo_changed = || {
                    state_arc.invalidate_repo_caches(&config_bg.base_repo);
                    // Dual-emit: bus (SSE/PWA/remote) + Tauri window (desktop).
                    // A new worktree is `.git/worktrees` admin plus a ref, so it is
                    // git-state: the branch list and every panel reading committed
                    // history has to re-read.
                    let _ = state_arc
                        .event_bus
                        .send(crate::state::AppEvent::RepoChanged {
                            repo_path: config_bg.base_repo.clone(),
                            kind: crate::repo_watcher::RepoChangeKind::GitState,
                        });
                    // Clone the handle out of the lock so we don't hold the read
                    // guard across the (potentially blocking) emit call.
                    let handle = state_arc.app_handle.read().clone();
                    if let Some(handle) = handle {
                        use tauri::Emitter as _;
                        let _ = handle.emit(
                            "repo-changed",
                            crate::repo_watcher::RepoChangedPayload {
                                repo_path: config_bg.base_repo.clone(),
                                kind: crate::repo_watcher::RepoChangeKind::GitState,
                            },
                        );
                    }
                };

                let emit_creation_failed = |reason: String| {
                    // Dual-emit: bus (SSE/PWA/remote) + Tauri window (desktop).
                    let _ =
                        state_arc
                            .event_bus
                            .send(crate::state::AppEvent::WorktreeCreateFailed {
                                repo_path: config_bg.base_repo.clone(),
                                branch: branch_for_err.clone(),
                                reason: reason.clone(),
                            });
                    let handle = state_arc.app_handle.read().clone();
                    if let Some(handle) = handle {
                        use tauri::Emitter as _;
                        let _ = handle.emit(
                            "worktree-create-failed",
                            serde_json::json!({
                                "repoPath": config_bg.base_repo.clone(),
                                "branch": branch_for_err.clone(),
                                "reason": reason,
                            }),
                        );
                    }
                };

                // Steps 1-2: clean up the stale directory (git worktree remove
                // --force + fs::remove_dir_all fallback). Reuses the synchronous
                // `cleanup_stale_worktree_dir` via spawn_blocking.
                let cleanup_ok = tokio::task::spawn_blocking({
                    let p = stale_path_bg.clone();
                    let r = config_bg.base_repo.clone();
                    move || cleanup_stale_worktree_dir(&r, &p)
                })
                .await
                .unwrap_or_else(|e| Err(format!("cleanup task panicked: {e}")));
                if let Err(reason) = cleanup_ok {
                    tracing::error!(source = "worktree", reason = %reason);
                    emit_creation_failed(reason);
                    emit_repo_changed();
                    return;
                }

                // Step 3: recreate the worktree.
                let result = tokio::task::spawn_blocking({
                    let d = worktrees_dir_bg.clone();
                    let c = config_bg.clone();
                    let r = base_ref_bg.clone();
                    move || create_worktree_internal(&d, &c, r.as_deref())
                })
                .await;

                match result {
                    Ok(Ok(_)) => emit_repo_changed(),
                    Ok(Err(e)) => {
                        let reason = format!("recreation failed: {e}");
                        tracing::error!(source = "worktree", reason = %reason);
                        emit_creation_failed(reason);
                        emit_repo_changed();
                    }
                    Err(e) => {
                        let reason = format!("background task panicked: {e}");
                        tracing::error!(source = "worktree", reason = %reason);
                        emit_creation_failed(reason);
                        emit_repo_changed();
                    }
                }
            });

            // Return pending immediately — JS will show placeholder until
            // either repo-changed (success/cleared) or worktree-create-failed
            // (error toast) fires.
            let branch = config
                .branch
                .clone()
                .unwrap_or_else(|| worktree_name.clone());
            Ok(serde_json::json!({
                "status": "pending",
                "name": worktree_name,
                "path": stale_path.to_string_lossy(),
                "workspace_id": workspace_id_of_worktree(&branch),
                "branch": branch,
                "base_repo": config.base_repo,
            }))
        }
        Err(e) => Err(e),
    }
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
    force: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    let base_repo = PathBuf::from(repo_path);
    let mut branch_delete_warning = None;

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        delete_branch = %delete_branch,
        "remove_worktree_by_workspace_id: start"
    );

    // Resolve by id, never by branch: two workspaces may share a branch, and
    // the branch-keyed lookup would hand us whichever git listed first. TYPED,
    // because the two mechanisms need completely different removals and the
    // difference is invisible from the path: `git worktree remove` on a COW
    // clone fails with "not a working tree", which `remove_worktree_internal`
    // treats as "already gone" and follows with an unconditional
    // `remove_dir_all` — deleting an independent repository, and every commit
    // that exists only in it, without ever reaching the guard below.
    let workspace = match resolve_any_workspace(&base_repo, workspace_id).inspect_err(|_| {
        tracing::error!(
            source = "worktree",
            workspace_id = %workspace_id,
            "remove_worktree_by_workspace_id: no workspace found for id"
        );
    })? {
        ResolvedWorkspace::Worktree(worktree) => worktree,
        ResolvedWorkspace::Cow(record) => {
            let branch = record.branch.clone();
            if let Some(script) = archive_script
                && !script.is_empty()
            {
                run_script_in_dir(script, &record.path)
                    .map_err(|e| format!("Archive script failed: {e}"))?;
            }
            crate::cow::remove_cow_workspace(&record, force)?;
            // No branch to delete in the parent: the clone's refs were its own,
            // and the parent's same-named branch (if any) belongs to the parent.
            return Ok(RemoveWorktreeOutcome {
                branch_delete_warning: None,
                branch,
            });
        }
    };
    // The branch to delete comes off the resolved record. Deriving it from the
    // id would be wrong the moment a COW workspace carries a minted id.
    let branch_name = workspace.branch.as_str();
    let worktree_path = PathBuf::from(&workspace.path);

    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        branch = %branch_name,
        path = %worktree_path.display(),
        "remove_worktree_by_workspace_id: worktree path resolved"
    );

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

    remove_worktree_internal(&worktree, force)?;

    // Delete the local branch when requested. Default uses `-d` (safe delete):
    // unmerged branches are refused so unpushed commits aren't silently lost.
    // Only when the caller passes `force=true` (e.g. the locked-worktree
    // confirmation dialog already warned the user) do we use `-D`.
    if delete_branch {
        let flag = if force { "-D" } else { "-d" };
        // `--` separates flags from positional args so a branch name beginning
        // with `-` (e.g. `-D`, `--force`) cannot be misparsed as a git option.
        match git_cmd(&worktree.base_repo)
            .args(["branch", flag, "--", branch_name])
            .run()
        {
            Ok(_) => tracing::info!(
                source = "worktree",
                branch = %branch_name,
                flag = %flag,
                "git branch delete: OK"
            ),
            Err(e) => {
                let warning = format!("git branch {flag} {branch_name} failed: {e}");
                tracing::warn!(
                    source = "worktree",
                    branch = %branch_name,
                    flag = %flag,
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn remove_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    delete_branch: Option<bool>,
    force: Option<bool>,
) -> Result<RemoveWorktreeOutcome, String> {
    let delete_branch = delete_branch.unwrap_or(true);
    let force = force.unwrap_or(false);
    tracing::info!(
        source = "worktree",
        workspace_id = %workspace_id,
        repo = %repo_path,
        delete_branch = %delete_branch,
        force = %force,
        "remove_worktree command: invoked"
    );
    let script = resolve_archive_script(&repo_path);
    let repo_path_clone = repo_path.clone();
    let workspace_id_clone = workspace_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        remove_worktree_by_workspace_id(
            &repo_path_clone,
            &workspace_id_clone,
            delete_branch,
            script.as_deref(),
            force,
        )
    })
    .await
    .map_err(|e| format!("Task panic: {e}"))?;

    match result {
        Ok(outcome) => {
            tracing::info!(source = "worktree", workspace_id = %workspace_id, "remove_worktree command: SUCCESS — invalidating caches");
            if outcome.branch_delete_warning.is_none() {
                // DEFERRED (2026-09-10) — branch labels are still a branch-keyed
                // config map, so two workspaces on one branch share one label and
                // removing either drops it. Migrating that map belongs with the
                // rest of the persisted branch keys (#728-bc76), not here.
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
            // Remove worktree + branch in one go
            remove_worktree_by_workspace_id(repo_path, workspace_id, true, None, false)?;
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn delete_local_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    keep_worktree: Option<bool>,
) -> Result<(), String> {
    let keep_worktree = keep_worktree.unwrap_or(false);
    delete_local_branch_impl(&repo_path, &branch_name, &workspace_id, keep_worktree)?;
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
}

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

        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(p.to_string());
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            } else if line == "detached" {
                detached = true;
            }
        }

        if let Some(path) = path {
            entries.push(WorktreeEntry {
                path,
                branch,
                detached,
            });
        }
    }

    entries
}

/// Marker files git writes into a worktree's admin dir while a multi-step operation is in
/// flight. Rebase and bisect detach HEAD, so `git worktree list --porcelain` emits no branch
/// line and the worktree reads as dead to anything keyed on that line (GH #112).
const IN_PROGRESS_MARKERS: [&str; 6] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

/// Admin dir of a linked worktree: its `.git` is a *file* holding
/// `gitdir: <repo>/.git/worktrees/<name>`. Returns `None` for the main worktree (where `.git`
/// is a directory) and for paths that no longer exist.
fn worktree_admin_dir(worktree_path: &str) -> Option<PathBuf> {
    let content = std::fs::read_to_string(Path::new(worktree_path).join(".git")).ok()?;
    let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
    Some(PathBuf::from(gitdir))
}

/// True when the worktree is in the middle of a rebase / merge / cherry-pick / revert / bisect.
fn has_operation_in_progress(worktree_path: &str) -> bool {
    let Some(admin) = worktree_admin_dir(worktree_path) else {
        return false;
    };
    IN_PROGRESS_MARKERS
        .iter()
        .any(|marker| admin.join(marker).exists())
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
/// `branch` is deliberately a field and not the key. Two workspaces may sit on
/// the same branch — that is the whole point of COW workspaces, since a clone is
/// an independent repository and git will not object — so a map keyed on the
/// branch collapses them into whichever was inserted last, and a caller asking
/// for one silently gets the other's directory (#726-5ac7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceWorktree {
    /// What is checked out here. Ordinary data: never a key, never parsed out of
    /// the id.
    pub(crate) branch: String,
    pub(crate) path: String,
}

/// The workspace id a freshly created **git worktree** gets.
///
/// The one place allowed to produce an id from a branch, and only because the
/// identity migration defines it that way: a linked worktree keeps
/// `workspace_id == branch` so nothing persisted moves. Reading it in the other
/// direction is the forbidden move — `resolve_workspace` looks an id up, it
/// never parses one.
///
/// A COW clone does not come through here: it is not in `git worktree list` and
/// its id is minted independently of its branch.
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
/// This is not a placeholder — it is the migration. A COW clone, which
/// `git worktree list` never reports at all, carries a minted id through this
/// same single lookup path rather than a second parallel map, which is the shape
/// that produces races.
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
                },
            );
        }
    }

    result
}

/// Get every workspace of a repo: maps workspace id -> its checkout.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_worktree_paths(
    repo_path: String,
) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    let base_repo = PathBuf::from(&repo_path);

    let out = git_cmd(&base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .map_err(|e| format!("git worktree list failed: {e}"))?;

    Ok(map_worktree_workspace_paths(&out.stdout))
}

/// Resolve one workspace by its opaque id.
///
/// This is the single lookup every id-taking operation goes through — removal,
/// dirtiness, branch deletion. Resolving by *branch* instead is the #726-5ac7
/// bug: `find_worktree_path_for_branch` returns the first porcelain block
/// carrying that branch, so with two workspaces on one branch a caller asking
/// about the second silently operates on the first.
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

/// Remove an orphan worktree by its filesystem path (detached HEAD — no branch to look up).
///
/// Safety: `worktree_path` is validated against the repo's actual worktree list to prevent
/// arbitrary directory deletion via a crafted path.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn remove_orphan_worktree(
    state: State<'_, Arc<AppState>>,
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
    remove_worktree_internal(&worktree, false)?;
    state.invalidate_repo_caches(&repo_path);
    Ok(())
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn switch_branch(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    force: bool,
    stash: bool,
) -> Result<SwitchBranchResult, String> {
    switch_branch_impl(state.inner(), repo_path, branch_name, force, stash)
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
/// reading the *other* same-branch workspace's status would authorise deleting
/// dirty work (#726-5ac7).
///
/// The three outcomes are kept apart deliberately: an id with no checkout has
/// nothing to lose (Clean), while a git command that failed tells us nothing
/// (Unknown). Folding the second into the first is what let a dirty worktree be
/// force-removed on a transient git error.
pub(crate) fn worktree_dirtiness(base_repo: &Path, workspace_id: &str) -> WorktreeDirtiness {
    let list = match git_cmd(base_repo)
        .args(["worktree", "list", "--porcelain"])
        .run()
    {
        Ok(out) => out.stdout,
        Err(e) => return WorktreeDirtiness::Unknown(format!("Failed to list worktrees: {e}")),
    };

    let Some(workspace) = map_worktree_workspace_paths(&list).remove(workspace_id) else {
        return WorktreeDirtiness::Clean; // No checkout = nothing to lose
    };
    let wt_path = PathBuf::from(&workspace.path);

    match git_cmd(&wt_path).args(["status", "--porcelain"]).run() {
        Ok(out) if out.stdout.trim().is_empty() => WorktreeDirtiness::Clean,
        Ok(_) => WorktreeDirtiness::Dirty,
        Err(e) => WorktreeDirtiness::Unknown(format!("Failed to check worktree status: {e}")),
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

/// What the pre-flight learned about a worktree branch before we merge it.
pub(crate) struct MergePreflight {
    pub(crate) commits_ahead: usize,
    pub(crate) worktree_dirty: WorktreeDirtiness,
}

/// Count commits on `branch` that `target` does not have, and check whether the
/// workspace `workspace_id` names has uncommitted changes.
///
/// Two keys because there are two questions: the commit count is about a *branch*
/// and the dirty check is about a *checkout*. Asking both by branch is what let
/// a same-branch sibling's clean status authorise destroying this one's work.
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
            let outcome = remove_worktree_by_workspace_id(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn finalize_merged_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: Option<bool>,
) -> Result<MergeArchiveResult, String> {
    finalize_merged_worktree_impl(
        state.inner(),
        repo_path,
        workspace_id,
        action,
        force.unwrap_or(false),
    )
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

    // 0. Pre-flight: would the cleanup take uncommitted work with it? Both
    //    "archive" and "delete" end in `git worktree remove --force`, so any
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
            let outcome = remove_worktree_by_workspace_id(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
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

    // Run archive script before archiving (if configured)
    if let Some(script) = archive_script
        && !script.is_empty()
    {
        run_script_in_dir(script, &wt_path).map_err(|e| format!("Archive script failed: {e}"))?;
    }
    let parent_dir = wt_path.parent().ok_or("Worktree has no parent directory")?;
    let archive_dir = parent_dir.join("__archived");
    let sanitized = sanitize_name(&workspace.branch);
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
        // prior archive for the same branch name; land on the next free suffix.
        archive_dest = free_archive_dest(&archive_dir, &sanitized);
        std::fs::rename(&wt_path, &archive_dest)
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
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

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
        remove_worktree_internal(&wt, true).expect("remove should succeed");
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

        let result = remove_worktree_internal(&worktree, false);
        assert!(result.is_ok(), "Failed to remove worktree: {:?}", result);

        assert!(
            !worktree.path.exists(),
            "Worktree path should not exist after removal"
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
        let result = remove_worktree_internal(&worktree, false);
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

        let result = remove_worktree_internal(&worktree, false);
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

        let result = remove_worktree_internal(&worktree, true);
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

        let result = remove_worktree_internal(&main_worktree, false);
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

        // Safe remove (force=false): worktree gone, branch survives
        let outcome = remove_worktree_by_workspace_id(
            repo.path().to_str().unwrap(),
            "feat-unmerged",
            true,
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
    fn test_remove_worktree_by_workspace_id_force_delete_removes_unmerged_branch() {
        // Scenario: same as above but with force=true (user confirmed via locked-worktree dialog).
        // Expected: branch ref is force-deleted via `git branch -D`.
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
            true,
        );
        let outcome = res.expect("force remove should succeed");
        assert!(
            outcome.branch_delete_warning.is_none(),
            "force branch delete should not report a partial warning"
        );

        let branches = git_cmd(repo.path())
            .args(["branch", "--list", "feat-force"])
            .run()
            .unwrap();
        assert!(
            !branches.stdout.contains("feat-force"),
            "branch ref should be force-deleted (got: {})",
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

    #[test]
    fn remove_worktree_by_workspace_id_deletes_branch_when_true() {
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
        remove_worktree_by_workspace_id(&repo_path, "feat-delete-branch", true, None, false)
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
        remove_worktree_by_workspace_id(&repo_path, "feat-keep-branch", false, None, false)
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
    /// For a git worktree the id IS the branch — the plan's migration is the
    /// identity function, so nothing persisted moves. The point of the seam is
    /// that a COW clone, which git never lists, can carry a minted id through
    /// the same one lookup path instead of a second parallel map.
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
    }

    /// The failure this replaces: a lookup that scanned porcelain for a
    /// `branch refs/heads/<name>` line returned the FIRST block carrying it, so
    /// with two workspaces on one branch every caller silently got the other
    /// one's directory. Keyed by id, each resolves to its own path.
    ///
    /// Note what git can and cannot produce here. `git worktree list` refuses to
    /// report two worktrees on one branch — it will not create the second — so
    /// this pair is written as porcelain directly. That is not a shortcut around
    /// the parser: it is the only shape a COW clone can arrive in, since git
    /// never lists a clone at all, and it is exactly the input the branch-keyed
    /// lookup got wrong. The porcelain is real (the mapper parses it), and both
    /// directories exist, because the mapper drops entries whose path is gone.
    #[test]
    fn two_workspaces_on_one_branch_resolve_to_their_own_paths() {
        let dir = TempDir::new().expect("temp dir");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir_all(&first).expect("first dir");
        std::fs::create_dir_all(&second).expect("second dir");
        let (first, second) = (
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        );

        // Both blocks name branch `feat-x`; only the ids differ.
        let porcelain = format!(
            "worktree {first}\nHEAD abc123\nbranch refs/heads/feat-x\n\n\
             worktree {second}\nHEAD abc123\nbranch refs/heads/feat-x\n\n"
        );
        let mut map = super::map_worktree_workspace_paths(&porcelain);
        // The porcelain path keys by branch (identity migration), so the pair
        // arrives collapsed — a COW clone is what carries the minted id. Re-key
        // the second under its minted id, the shape #729-983e will insert.
        let minted = map.remove("feat-x").expect("parsed entry");
        map.insert(
            "feat-x".to_string(),
            super::WorkspaceWorktree {
                branch: "feat-x".to_string(),
                path: first.clone(),
            },
        );
        map.insert(
            "feat-x~a1b2c3d4".to_string(),
            super::WorkspaceWorktree {
                branch: minted.branch,
                path: second.clone(),
            },
        );

        let one = map.get("feat-x").expect("first workspace");
        let two = map.get("feat-x~a1b2c3d4").expect("second workspace");
        assert_eq!(one.path, first);
        assert_eq!(
            two.path, second,
            "the second workspace is not shadowed by the first"
        );
        assert_eq!(
            one.branch, two.branch,
            "both are on one branch — that is the point"
        );
        assert_eq!(map.len(), 2, "same branch, two distinct entries");
    }

    /// Criterion: resolution by workspace_id returns the exact path, never the
    /// first branch match.
    ///
    /// Two real worktrees, and the assertion that fails under the old lookup is
    /// the *detached* one: mid-rebase git emits no `branch refs/heads/…` line at
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
    /// This is the one that gates an irreversible cleanup, so the direction that
    /// matters is a dirty workspace being reported clean because a sibling is:
    /// `cleanup_needs_confirmation` would then wave a `--force` removal through
    /// and delete uncommitted work.
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

        let err = delete_local_branch_impl(&repo_str, "feat-other", "feat-keeper", false)
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
            delete_local_branch_impl(&repo_path, "feat-to-delete", "feat-to-delete", false);
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
        let result = delete_local_branch_impl(&repo_path, &default_branch, &default_branch, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Refusing to delete default branch")
        );
    }

    #[test]
    fn delete_local_branch_with_worktree() {
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
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, false);
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
        let result = delete_local_branch_impl(&repo_path, &wt.name, &wt.name, true);
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

        let result = run_setup_script("echo oops >&2; exit 1".to_string(), cwd)
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

        let result = run_setup_script("cat marker.txt".to_string(), cwd).expect("should succeed");
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
        let result = run_script_in_dir("cat test-file.txt", dir.path());
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
        let script = format!("touch {}", marker.display());
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

    // ── create_workspace: one caller, two mechanisms ─────────────────────

    use crate::cow::{CowSupport, DirtyPolicy, WorkspaceKind, WorkspaceMode};

    /// A repo and a workspaces directory that is its SIBLING, which is what the
    /// default storage strategy produces (`<repo>__wt/`). `setup_test_repo`
    /// makes the temp dir itself the repo, so a workspaces dir under it is
    /// inside the source — the one shape the containment guard refuses, and the
    /// subject of its own test below rather than an accident in every other.
    fn workspace_fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().expect("temp dir");
        let repo = temp.path().join("repo");
        let workspaces = temp.path().join("repo__wt");
        fs::create_dir_all(&repo).expect("repo dir");

        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
        ] {
            git_cmd(&repo).args(args).run().expect("git setup");
        }
        fs::write(repo.join("README.md"), "# Test").expect("write");
        git_cmd(&repo).args(["add", "."]).run().expect("add");
        git_cmd(&repo)
            .args(["commit", "-m", "initial"])
            .run()
            .expect("commit");

        (temp, repo, workspaces)
    }

    fn workspace_config(repo: &Path, task: &str) -> WorktreeConfig {
        WorktreeConfig {
            task_name: task.to_string(),
            base_repo: repo.to_string_lossy().to_string(),
            branch: Some(task.to_string()),
            create_branch: true,
        }
    }

    /// A probe that always refuses, standing in for a destination on another
    /// volume, a filesystem without reflink, or Windows. Injected because the
    /// alternative is a test that needs a second filesystem to be honest.
    fn probe_unavailable(_: &Path, _: &Path) -> CowSupport {
        CowSupport::Unsupported("no reflink support on this pair of paths".to_string())
    }

    #[test]
    fn auto_produces_a_cow_workspace_where_copy_on_write_works() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "feature-cow"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("auto creates a workspace");

        assert_eq!(created.kind, WorkspaceKind::Cow);
        assert_eq!(
            created.degraded_reason, None,
            "nothing degraded, so nothing to explain"
        );
        // An independent repository: its .git is a directory, not a pointer file.
        assert!(created.path.join(".git").is_dir());
    }

    /// The degrade is the whole point of `auto`: the caller asked for a
    /// workspace, and a linked worktree is one.
    #[test]
    fn auto_degrades_to_a_working_linked_worktree_and_says_why() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let created = create_workspace_with(
            &workspaces,
            &workspace_config(&repo, "feature-degraded"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
            probe_unavailable,
        )
        .expect("auto must degrade, not fail");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert!(
            created
                .degraded_reason
                .as_deref()
                .unwrap_or_default()
                .contains("reflink"),
            "the reason must name what was unavailable: {:?}",
            created.degraded_reason
        );

        // And it is a real, usable worktree — a linked one, so .git is a file.
        assert!(created.path.join(".git").is_file());
        let head = git_cmd(&created.path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .run()
            .expect("rev-parse")
            .stdout;
        assert_eq!(head.trim(), "feature-degraded");
    }

    #[test]
    fn mode_cow_fails_loudly_naming_the_check_that_said_no() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let err = create_workspace_with(
            &workspaces,
            &workspace_config(&repo, "feature-strict"),
            None,
            WorkspaceMode::Cow,
            DirtyPolicy::Inherit,
            probe_unavailable,
        )
        .expect_err("mode=cow must not silently give a worktree");

        assert!(
            err.contains("reflink"),
            "the failure must name the check: {err}"
        );
        assert!(
            !workspaces.join("feature-strict").exists(),
            "a refused creation left a directory behind"
        );
    }

    /// A guard refusal reaches `mode=cow` as its own reason, not as a generic
    /// "unavailable" — the caller can act on "finish your rebase".
    #[test]
    fn mode_cow_reports_a_guard_refusal_rather_than_the_probe() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join(".git").join("MERGE_HEAD"), "deadbeef").expect("marker");

        let err = create_workspace(
            &workspaces,
            &workspace_config(&repo, "feature-midmerge"),
            None,
            WorkspaceMode::Cow,
            DirtyPolicy::Inherit,
        )
        .expect_err("a repo mid-merge cannot be cloned");

        assert!(err.contains("MERGE_HEAD"), "{err}");
    }

    #[test]
    fn mode_worktree_forces_a_worktree_even_where_cow_works() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "feature-forced"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree mode always works");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert_eq!(
            created.degraded_reason, None,
            "asking for a worktree is a choice, not a degradation — reporting a reason would be a lie"
        );
        assert!(created.path.join(".git").is_file());
    }

    /// `mode=worktree` must not inherit the clone guards: they are about whether
    /// a repo can be COPIED, and a linked worktree never had that constraint.
    #[test]
    fn mode_worktree_is_not_blocked_by_a_guard_that_only_applies_to_cloning() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join(".git").join("MERGE_HEAD"), "deadbeef").expect("marker");

        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "feature-anyway"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("a clone guard must not block a worktree");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
    }

    /// The real cross-volume degrade, against a real second volume.
    ///
    /// Ignored by default because it needs one, and mounting a disk image is
    /// not something a test suite should do on every run. To drive it:
    ///
    /// ```sh
    /// hdiutil create -size 64m -fs APFS -volname TuicCow -type SPARSE /tmp/tuiccow
    /// hdiutil attach /tmp/tuiccow.sparseimage
    /// TUIC_COW_CROSS_VOLUME_DEST=/Volumes/TuicCow \
    ///   cargo nextest run --lib -E 'test(cross_volume)' --run-ignored all
    /// hdiutil detach /Volumes/TuicCow
    /// ```
    ///
    /// Everything below it is the injected-probe version, which is what keeps
    /// the behaviour covered on every run.
    #[test]
    #[ignore = "needs a second volume; see the doc comment for the hdiutil recipe"]
    fn auto_degrades_across_a_real_volume_boundary() {
        let Ok(other_volume) = std::env::var("TUIC_COW_CROSS_VOLUME_DEST") else {
            panic!("set TUIC_COW_CROSS_VOLUME_DEST to a directory on another volume");
        };
        let (_temp, repo, _sibling) = workspace_fixture();
        let workspaces = PathBuf::from(other_volume).join("tuic-cow-test");
        let _ = fs::remove_dir_all(&workspaces);

        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "feature-cross-volume"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("auto must degrade across a volume boundary, not fail");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert!(
            created
                .degraded_reason
                .as_deref()
                .unwrap_or_default()
                .contains("volume"),
            "{:?}",
            created.degraded_reason
        );
        let _ = fs::remove_dir_all(&workspaces);
    }

    /// The `InsideRepo` and `ClaudeCodeDefault` storage strategies put the new
    /// directory UNDER the repo, where a recursive copy would walk into its own
    /// destination. Those users keep working — with a linked worktree, and a
    /// reason that names the containment rather than blaming the filesystem.
    #[test]
    fn a_workspaces_directory_inside_the_repo_degrades_instead_of_failing() {
        let (_temp, repo, _sibling) = workspace_fixture();
        let inside = repo.join("worktrees");

        let created = create_workspace(
            &inside,
            &workspace_config(&repo, "feature-inside"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("must degrade, not fail");

        assert_eq!(created.kind, WorkspaceKind::Worktree);
        assert!(
            created
                .degraded_reason
                .as_deref()
                .unwrap_or_default()
                .contains("inside the source repository"),
            "{:?}",
            created.degraded_reason
        );
    }

    // ── the model-facing creation payload ────────────────────────────────

    /// The payload is the ONLY instruction channel — Boss ruled out deny hooks,
    /// shell overrides and PATH shims — so a clone claiming shared refs, or a
    /// worktree telling a model to publish, is not a wording bug. It is the
    /// model acting on the wrong isolation model with no backstop.
    #[test]
    fn the_creation_payload_says_opposite_things_for_the_two_mechanisms() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let cloned = create_workspace(
            &workspaces,
            &workspace_config(&repo, "cloned"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow")
        .instruction_payload();
        let linked = create_workspace(
            &workspaces,
            &workspace_config(&repo, "linked"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree")
        .instruction_payload();

        let cow_isolation = cloned["isolation"].as_str().expect("isolation");
        assert!(cow_isolation.contains("independent repository"));
        assert!(cow_isolation.contains("ONLY here"), "{cow_isolation}");
        assert!(
            cow_isolation.contains("silently merges"),
            "the silent-wrong-merge is the failure it must name: {cow_isolation}"
        );
        assert!(cow_isolation.contains("publish_workspace"));
        assert!(
            !cow_isolation.contains("shared with the parent"),
            "a clone must never claim shared refs: {cow_isolation}"
        );

        let worktree_isolation = linked["isolation"].as_str().expect("isolation");
        assert!(worktree_isolation.contains("shared with the parent"));
        assert!(worktree_isolation.contains("nothing to publish"));
        assert!(
            !worktree_isolation
                .to_lowercase()
                .contains("publish_workspace"),
            "a worktree must not send a model looking for a publish step: {worktree_isolation}"
        );

        assert_eq!(cloned["kind"], "cow");
        assert_eq!(linked["kind"], "worktree");
    }

    #[test]
    fn the_payload_reports_the_dirty_policy_and_what_it_carried_over() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join("README.md"), "# Test\nin progress\n").expect("dirty");

        let payload = create_workspace(
            &workspaces,
            &workspace_config(&repo, "carries"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow")
        .instruction_payload();

        assert_eq!(payload["state"]["dirty_policy"], "inherit");
        assert_eq!(payload["state"]["carried_over"], 1);
        let note = payload["state"]["note"].as_str().expect("note");
        assert!(
            note.contains("not yours to fix"),
            "inherited WIP must not read as the model's own bug: {note}"
        );
    }

    #[test]
    fn the_payload_lists_warm_artifacts_and_tells_the_model_not_to_rebuild_them() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join(".gitignore"), "node_modules/\n").expect("gitignore");
        git_cmd(&repo).args(["add", "."]).run().expect("add");
        git_cmd(&repo)
            .args(["commit", "-m", "ignore node_modules"])
            .run()
            .expect("commit");
        fs::create_dir_all(repo.join("node_modules").join("left-pad")).expect("dir");
        fs::write(
            repo.join("node_modules").join("left-pad").join("index.js"),
            "module.exports = 1;\n",
        )
        .expect("artifact");

        let payload = create_workspace(
            &workspaces,
            &workspace_config(&repo, "warm"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow")
        .instruction_payload();

        let present = payload["warm_artifacts"]["present"]
            .as_array()
            .expect("present");
        assert_eq!(present.len(), 1, "{present:?}");
        assert_eq!(present[0]["path"], "node_modules");
        assert!(
            !present[0]["size"].as_str().unwrap_or_default().is_empty(),
            "a size the model can weigh against rebuilding: {present:?}"
        );
        let note = payload["warm_artifacts"]["note"].as_str().expect("note");
        assert!(
            note.contains("Do NOT run an install or a full build"),
            "{note}"
        );
    }

    #[test]
    fn a_degraded_workspace_says_so_in_the_payload() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let payload = create_workspace_with(
            &workspaces,
            &workspace_config(&repo, "degraded"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
            probe_unavailable,
        )
        .expect("degrades")
        .instruction_payload();

        assert_eq!(payload["kind"], "worktree");
        assert!(
            payload["degraded_reason"]
                .as_str()
                .unwrap_or_default()
                .contains("reflink"),
            "{payload:?}"
        );
    }

    /// A minted id must reach the caller: `worktree_remove`, `publish_workspace`
    /// and `check_worktree_dirty` all take one, and a clone's is not its branch.
    #[test]
    fn a_cow_workspace_reports_a_minted_id_and_a_worktree_reports_its_branch() {
        let (_temp, repo, workspaces) = workspace_fixture();

        let cloned = create_workspace(
            &workspaces,
            &workspace_config(&repo, "minted"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow");
        let linked = create_workspace(
            &workspaces,
            &workspace_config(&repo, "plain"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");

        assert!(
            cloned.workspace_id.starts_with("minted~"),
            "{}",
            cloned.workspace_id
        );
        assert_ne!(cloned.workspace_id, cloned.branch);
        assert_eq!(
            linked.workspace_id, "plain",
            "a linked worktree's id IS its branch"
        );
    }

    // ── resolving an id against both sources, and publish ────────────────

    /// Point the config dir at a temp dir holding `doc`, so a resolver test
    /// reads a document we control instead of the user's real one.
    fn with_repositories_document(doc: serde_json::Value) -> (impl Drop, TempDir) {
        let config = TempDir::new().expect("config dir");
        fs::write(
            config.path().join("repositories.json"),
            serde_json::to_string_pretty(&doc).expect("serialize"),
        )
        .expect("write");
        let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        (guard, config)
    }

    #[test]
    fn an_id_git_knows_resolves_to_a_worktree() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "resolvable"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");
        let (_guard, _config) = with_repositories_document(serde_json::json!({ "repos": {} }));

        match resolve_any_workspace(&repo, "resolvable").expect("resolves") {
            ResolvedWorkspace::Worktree(worktree) => {
                assert_eq!(PathBuf::from(worktree.path), created.path);
                assert_eq!(worktree.branch, "resolvable");
            }
            other => panic!("expected a linked worktree, got {other:?}"),
        }
    }

    /// A COW clone is invisible to `git worktree list`, so the persisted
    /// document is the only thing that knows it exists.
    #[test]
    fn an_id_only_the_document_knows_resolves_to_a_cow_workspace() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "cloned"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow");
        assert_eq!(created.kind, WorkspaceKind::Cow);
        assert!(
            git_cmd(&repo)
                .args(["worktree", "list", "--porcelain"])
                .run()
                .expect("list")
                .stdout
                .lines()
                .all(|line| !line.contains("cloned")),
            "git must not report the clone — that is the whole reason for the document"
        );

        let (_guard, _config) = with_repositories_document(serde_json::json!({
            "repos": {
                repo.to_string_lossy(): {
                    "path": repo.to_string_lossy(),
                    "workspaces": {
                        "cloned~aaaa1111": {
                            "branchName": "cloned",
                            "kind": "cow",
                            "worktreePath": created.path.to_string_lossy(),
                            "parentRepoPath": repo.to_string_lossy(),
                        }
                    }
                }
            }
        }));

        match resolve_any_workspace(&repo, "cloned~aaaa1111").expect("resolves") {
            ResolvedWorkspace::Cow(record) => {
                assert_eq!(record.path, created.path);
                assert_eq!(record.branch, "cloned");
            }
            other => panic!("expected a cow workspace, got {other:?}"),
        }
    }

    /// The two id spaces are disjoint by construction, so an overlap means a
    /// corrupt record — and picking one silently is how the wrong directory
    /// gets deleted.
    #[test]
    fn an_id_both_sources_claim_is_an_error_rather_than_a_winner() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "contested"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");

        let (_guard, _config) = with_repositories_document(serde_json::json!({
            "repos": {
                repo.to_string_lossy(): {
                    "path": repo.to_string_lossy(),
                    "workspaces": {
                        "contested": {
                            "branchName": "contested",
                            "kind": "cow",
                            "worktreePath": created.path.to_string_lossy(),
                        }
                    }
                }
            }
        }));

        let err = resolve_any_workspace(&repo, "contested").expect_err("must refuse");
        assert!(err.contains("refusing to guess"), "{err}");
    }

    // ── removal dispatches on the mechanism ──────────────────────────────

    /// A linked worktree is removed by git, which also cleans up the admin
    /// entry under the parent's `.git/worktrees`. An `rm -rf` of the directory
    /// would leave that entry behind, and it BLOCKS a later checkout of the
    /// same branch.
    #[test]
    fn removing_a_linked_worktree_cleans_up_the_parents_admin_entry() {
        let (_temp, repo, workspaces) = workspace_fixture();
        create_workspace(
            &workspaces,
            &workspace_config(&repo, "linked"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");
        let admin = repo.join(".git").join("worktrees").join("linked");
        assert!(admin.exists(), "the fixture must start with an admin entry");
        let (_guard, _config) = with_repositories_document(serde_json::json!({ "repos": {} }));

        remove_worktree_by_workspace_id(&repo.to_string_lossy(), "linked", true, None, false)
            .expect("removes");

        assert!(
            !admin.exists(),
            "the parent's worktree admin entry survived the removal"
        );
        assert!(!workspaces.join("linked").exists());
    }

    /// The whole reason removal resolves to a TYPE: a COW id must never reach
    /// `git worktree remove`, whose "not a working tree" failure the removal
    /// path treats as success before deleting the directory unconditionally.
    #[test]
    fn removing_a_cow_workspace_by_id_refuses_while_its_commits_are_unpublished() {
        let (_temp, repo, workspaces) = workspace_fixture();
        let created = create_workspace(
            &workspaces,
            &workspace_config(&repo, "cloned"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow");
        assert_eq!(created.kind, WorkspaceKind::Cow);
        fs::write(created.path.join("only-here.txt"), "work\n").expect("write");
        git_cmd(&created.path)
            .args(["add", "."])
            .run()
            .expect("add");
        git_cmd(&created.path)
            .args(["commit", "-m", "unpublished work"])
            .run()
            .expect("commit");

        let (_guard, _config) = with_repositories_document(serde_json::json!({
            "repos": {
                repo.to_string_lossy(): {
                    "path": repo.to_string_lossy(),
                    "workspaces": {
                        "cloned~aaaa1111": {
                            "branchName": "cloned",
                            "kind": "cow",
                            "worktreePath": created.path.to_string_lossy(),
                            "parentRepoPath": repo.to_string_lossy(),
                        }
                    }
                }
            }
        }));

        // force defaults to false on every transport, and this is the shared
        // entry point all three of them call.
        let err = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "cloned~aaaa1111",
            true,
            None,
            false,
        )
        .expect_err("must refuse");

        assert!(err.contains("only there"), "{err}");
        assert!(created.path.exists(), "the workspace was deleted anyway");

        // And with force it goes, reporting the branch it was on.
        let outcome = remove_worktree_by_workspace_id(
            &repo.to_string_lossy(),
            "cloned~aaaa1111",
            true,
            None,
            true,
        )
        .expect("force removes");
        assert_eq!(outcome.branch, "cloned");
        assert!(!created.path.exists());
    }

    /// A linked worktree shares its refs with the parent, so publishing is a
    /// question that does not apply — and saying that is not the same as
    /// reporting a success that did nothing.
    #[test]
    fn publishing_a_linked_worktree_is_a_no_op_that_says_why() {
        let (_temp, repo, workspaces) = workspace_fixture();
        create_workspace(
            &workspaces,
            &workspace_config(&repo, "shared-refs"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");
        let (_guard, _config) = with_repositories_document(serde_json::json!({ "repos": {} }));

        let outcome =
            publish_workspace_impl(&repo.to_string_lossy(), "shared-refs").expect("no-op");

        assert!(!outcome.parent_updated);
        assert!(!outcome.origin_pushed);
        assert_eq!(outcome.parent_error, None, "a no-op is not a failure");
        assert!(
            outcome
                .no_op_reason
                .as_deref()
                .unwrap_or_default()
                .contains("nothing to publish"),
            "{:?}",
            outcome.no_op_reason
        );
    }

    /// The two mechanisms differ in exactly the way the caller has to be told
    /// about: a clone carries the parent's work in progress, a worktree does not.
    #[test]
    fn only_the_cow_path_carries_the_parents_uncommitted_work() {
        let (_temp, repo, workspaces) = workspace_fixture();
        fs::write(repo.join("README.md"), "# Test\nin progress\n").expect("dirty");

        let cloned = create_workspace(
            &workspaces,
            &workspace_config(&repo, "carries"),
            None,
            WorkspaceMode::Auto,
            DirtyPolicy::Inherit,
        )
        .expect("cow");
        let linked = create_workspace(
            &workspaces,
            &workspace_config(&repo, "does-not-carry"),
            None,
            WorkspaceMode::Worktree,
            DirtyPolicy::Inherit,
        )
        .expect("worktree");

        assert_eq!(cloned.carried_over, 1);
        assert_eq!(linked.carried_over, 0);
    }
}
