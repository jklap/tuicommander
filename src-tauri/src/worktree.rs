//! App-side half of the worktree module: config-driven directory and archive
//! script resolution, the removal paths that notify `AppState`, and the Tauri
//! wrappers over [`tuic_git::worktree`]. The domain functions are re-exported
//! so `crate::worktree::*` paths are unchanged.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::State;

pub(crate) use tuic_git::worktree::*;

pub(crate) fn merged_github_pr_proves_tip(repo: &Path, branch: &str, tip: &str) -> bool {
    let Some(url) = crate::git::read_remote_url(repo) else {
        return false;
    };
    let Some((host, owner, name)) = crate::github_account::parse_remote_url(&url) else {
        return false;
    };
    let query = r#"query($owner: String!, $name: String!, $branch: String!, $endCursor: String) {
      repository(owner: $owner, name: $name) {
        pullRequests(first: 100, after: $endCursor, headRefName: $branch, states: [MERGED]) {
          nodes { number state headRefName headRefOid }
          pageInfo { hasNextPage endCursor }
        }
      }
    }"#;
    let mut command = Command::new(crate::agent::resolve_cli("gh"));
    command.current_dir(repo).args([
        "api",
        "graphql",
        "--paginate",
        "--slurp",
        "--hostname",
        host.as_str(),
        "-f",
        &format!("query={query}"),
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("name={name}"),
        "-f",
        &format!("branch={branch}"),
    ]);
    crate::cli::apply_no_window(&mut command);
    let Ok(output) = crate::git_cli::output_with_deadline(&mut command, Duration::from_secs(20))
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(pages) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return false;
    };
    tuic_git::worktree::merged_pr_proof_from_pages(repo, branch, tip, &pages)
}

pub(crate) fn inspect_workspace_lifecycle(
    base_repo: &Path,
    workspace_id: &str,
) -> WorkspaceLifecycleStatus {
    tuic_git::worktree::inspect_workspace_lifecycle_with_pr(
        base_repo,
        workspace_id,
        merged_github_pr_proves_tip,
    )
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorktreeRemovalPreview {
    #[serde(flatten)]
    pub lifecycle: WorkspaceLifecycleStatus,
    pub untracked_files: Option<usize>,
    pub live_sessions: Vec<WorktreeLiveSession>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WorktreeLiveSession {
    pub session_id: String,
    pub name: String,
}

/// Live sessions of the registry working inside `checkout`: its live cwd (OSC 7,
/// so a shell that `cd`'d in counts) OR the worktree it was spawned into (a
/// session registered to the worktree that has since `cd`'d out still has its
/// tab and process tied to it). A session whose process has already exited
/// does not count, even while its registry entry is still being torn down.
/// The one source of "who is working in this checkout" for every removal guard.
pub(crate) fn live_sessions_in(state: &AppState, checkout: &Path) -> Vec<WorktreeLiveSession> {
    let root = checkout
        .canonicalize()
        .unwrap_or_else(|_| checkout.to_path_buf());
    let inside = |path: &Path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .starts_with(&root)
    };
    let mut live_sessions = Vec::new();
    for entry in &state.session_maps.sessions {
        let mut session = entry.value().lock();
        let attached = session
            .cwd
            .as_deref()
            .is_some_and(|cwd| inside(Path::new(cwd)))
            || session
                .worktree
                .as_ref()
                .is_some_and(|worktree| inside(&worktree.path));
        // `try_wait` caches the status once reaped, so `mark_session_exited`
        // still reads the real exit code afterwards.
        let exited = matches!(session._child.try_wait(), Ok(Some(_)));
        if attached && !exited {
            live_sessions.push(WorktreeLiveSession {
                session_id: entry.key().clone(),
                name: session
                    .display_name
                    .clone()
                    .unwrap_or_else(|| entry.key().clone()),
            });
        }
    }
    live_sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    live_sessions
}

/// Stable prefix of the live-session refusal. Every transport carries the same
/// message (IPC `Err`, HTTP 409 `error`, MCP `error`), so a client matches the
/// prefix, not the wording after it.
pub(crate) const BUSY_WORKTREE_PREFIX: &str = "worktree_busy:";

/// A removal refused because live sessions still work in the checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WorktreeBusy {
    pub live_sessions: Vec<WorktreeLiveSession>,
}

impl WorktreeBusy {
    pub(crate) fn message(&self) -> String {
        let sessions: Vec<String> = self
            .live_sessions
            .iter()
            .map(|session| format!("{} [{}]", session.name, session.session_id))
            .collect();
        format!(
            "{BUSY_WORKTREE_PREFIX} {} live session(s) in this worktree: {}. \
             Close them, or retry with override_busy to remove it anyway",
            self.live_sessions.len(),
            sessions.join(", ")
        )
    }

    /// The JSON error body HTTP and MCP return: the shared message plus the
    /// sessions, so a client can show them without parsing the message.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "error": self.message(),
            "code": "worktree_busy",
            "live_sessions": self.live_sessions,
        })
    }
}

/// The live-session guard every removal of a workspace's checkout shares (IPC
/// `remove_worktree`, HTTP `DELETE /worktrees/{id}`, MCP `repo worktree_remove`).
/// git's own refusals cover dirty files and locks; a CLEAN, unlocked checkout
/// can still have a terminal or agent working in it, which is how a live
/// worktree was deleted twice on 2026-08-26. Only an explicit `override_busy`
/// (a user who saw the sessions) lifts it — never `force` or `override_lock`.
///
/// A checkout that does not exist holds no work, and the main checkout is
/// never removable here (the removal itself refuses it), so neither is
/// guarded. An id that resolves to nothing is left to the removal's own error.
pub(crate) fn workspace_removal_guard(
    state: &AppState,
    repo_path: &str,
    workspace_id: &str,
    override_busy: bool,
) -> Result<(), WorktreeBusy> {
    if override_busy {
        return Ok(());
    }
    let Ok(workspace) = resolve_any_workspace(Path::new(repo_path), workspace_id) else {
        return Ok(());
    };
    let checkout = Path::new(&workspace.path);
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !checkout.exists() || canonical(checkout) == canonical(Path::new(repo_path)) {
        return Ok(());
    }
    let live_sessions = live_sessions_in(state, checkout);
    if live_sessions.is_empty() {
        Ok(())
    } else {
        Err(WorktreeBusy { live_sessions })
    }
}

pub(crate) fn inspect_worktree_removal(
    state: &AppState,
    repo_path: &Path,
    workspace_id: &str,
) -> WorktreeRemovalPreview {
    let lifecycle = inspect_workspace_lifecycle(repo_path, workspace_id);
    let worktree_path = resolve_any_workspace(repo_path, workspace_id)
        .ok()
        .map(|workspace| PathBuf::from(workspace.path));
    let untracked_files = worktree_path.as_ref().and_then(|path| {
        git_cmd(path)
            .args([
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--ignore-submodules=none",
            ])
            .run()
            .ok()
            .map(|output| {
                output
                    .stdout
                    .lines()
                    .filter(|line| line.starts_with("??"))
                    .count()
            })
    });
    let live_sessions = worktree_path
        .as_deref()
        .map(|path| live_sessions_in(state, path))
        .unwrap_or_default();
    let mut warnings = Vec::new();
    match lifecycle.commit_status {
        WorkspaceCommitStatus::InSync => {
            warnings.push("This branch has nothing of its own, not merged work".to_string())
        }
        WorkspaceCommitStatus::Merged => {
            warnings.push("This branch's commits are in the default branch".to_string())
        }
        WorkspaceCommitStatus::Unmerged => {
            warnings.push("This branch has unmerged commits".to_string())
        }
        WorkspaceCommitStatus::PushedUnmerged => {
            warnings.push("This branch is not merged, but all its commits are pushed".to_string())
        }
        WorkspaceCommitStatus::Unknown => {
            warnings.push("Branch history could not be verified".to_string())
        }
    }
    if let Some(total) = lifecycle.dirty_files.filter(|total| *total > 0) {
        let untracked = untracked_files.unwrap_or(0);
        warnings.push(format!(
            "{total} uncommitted files, including {untracked} untracked files"
        ));
    }
    warnings.extend(
        live_sessions
            .iter()
            .map(|session| format!("Live session: {}", session.name)),
    );
    WorktreeRemovalPreview {
        lifecycle,
        untracked_files,
        live_sessions,
        warnings,
    }
}

pub(crate) fn remove_worktree_by_workspace_id_with_confirmation(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
) -> Result<RemoveWorktreeOutcome, String> {
    tuic_git::worktree::remove_worktree_by_workspace_id_with_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script.map(|script| UserScript {
            script,
            env: &archive_script_env,
        }),
        force,
        override_lock,
        expected_fingerprint,
        merged_github_pr_proves_tip,
    )
}

// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) fn remove_worktree_with_presence_confirmation(
    repo_path: &str,
    workspace_id: &str,
    delete_branch: bool,
    archive_script: Option<&str>,
    force: bool,
    override_lock: bool,
    expected_fingerprint: Option<&str>,
    confirm_missing_checkout: bool,
) -> Result<RemoveWorktreeOutcome, String> {
    tuic_git::worktree::remove_worktree_by_workspace_id_with_missing_confirmation_and_pr(
        repo_path,
        workspace_id,
        delete_branch,
        archive_script.map(|script| UserScript {
            script,
            env: &archive_script_env,
        }),
        force,
        override_lock,
        expected_fingerprint,
        Some(confirm_missing_checkout),
        merged_github_pr_proves_tip,
    )
}

pub(crate) fn branch_integration(repo: &Path, branch: &str) -> Result<BranchIntegration, String> {
    tuic_git::worktree::branch_integration_with_pr(repo, branch, merged_github_pr_proves_tip)
}

pub(crate) fn branch_integrations(repo: &Path) -> Result<Vec<BranchIntegration>, String> {
    tuic_git::worktree::branch_integrations_with_pr(repo, merged_github_pr_proves_tip)
}

pub(crate) fn delete_integrated_local_branch(
    repo_path: &str,
    branch_name: &str,
) -> Result<tuic_git::worktree::DeletedBranch, String> {
    tuic_git::worktree::delete_integrated_local_branch_with_pr(
        repo_path,
        branch_name,
        merged_github_pr_proves_tip,
    )
}

/// Test-only since the post-create chain (`spawn_worktree_setup_chain`) owns
/// the production warm.
#[cfg(test)]
pub fn spawn_background_warm(
    source: PathBuf,
    destination: PathBuf,
    token: u64,
    warm: impl FnOnce(&Path, &Path) -> tuic_git::cow::WarmingReport + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        tuic_git::worktree::finish_background_warm_blocking(source, destination, token, warm)
    })
}

use crate::git_cli::{finish_failed_git_operation_after_abort, git_cmd};
use crate::state::AppState;

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

/// Copy ignored/untracked/explicit-listed files into a freshly created
/// worktree, awaiting completion. Resolves the repo's effective copy settings
/// from disk (`config::resolve_effective_copy_settings`) itself, so a
/// worktree created from the desktop app and one created via the MCP HTTP
/// path (no frontend in the loop) sync identically.
///
/// Returns `None` — and emits nothing at all — when there is nothing
/// configured to copy (a plain worktree creation never shows a sync toast).
/// Otherwise dual-emits (event_bus + Tauri window) `worktree-sync-*` events
/// as it goes and returns the final summary.
///
/// Only called from [`spawn_worktree_setup_chain`], which awaits this before
/// resolving/running the setup script — seeing that function's doc comment
/// for why the two must be sequenced this way.
async fn run_worktree_file_sync(
    state: &Arc<AppState>,
    base_repo: &str,
    branch: &str,
    dest_path: &Path,
) -> Option<crate::worktree_sync::SyncSummary> {
    let (copy_ignored, copy_untracked, copy_paths) =
        crate::config::resolve_effective_copy_settings(base_repo);
    if !copy_ignored && !copy_untracked && copy_paths.is_empty() {
        return None;
    }

    let source = PathBuf::from(base_repo);
    let dest = dest_path.to_path_buf();
    let repo_path = base_repo.to_string();
    let branch = branch.to_string();
    let explicit = crate::worktree_sync::specs_from_copy_path_entries(&copy_paths);

    emit_worktree_sync_started(state, &repo_path, &branch);

    let repo_path_progress = repo_path.clone();
    let branch_progress = branch.clone();
    let state_progress = Arc::clone(state);

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

    emit_worktree_sync_completed(state, &repo_path, &branch, &summary);
    Some(summary)
}

/// Generation of the newest post-create chain per `(repo_path, branch)`.
///
/// The setup status cache and the `worktree-setup-script-completed` event are
/// keyed by that pair, which a removed-then-recreated workspace reuses. Every
/// status write and terminal event of a chain is gated on its generation still
/// being the newest one, so a stale chain (stopped by the removal, or simply
/// slower) can never wipe or overwrite the status of the chain that replaced
/// it, nor resolve the new creation's waiter early. Process-wide like
/// tuic-git's warm state, which it complements; entries are removed when the
/// chain ends.
static SETUP_CHAIN_GENERATIONS: std::sync::LazyLock<dashmap::DashMap<(String, String), u64>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);
static NEXT_SETUP_CHAIN_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn begin_setup_chain(key: &(String, String)) -> u64 {
    let generation = NEXT_SETUP_CHAIN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SETUP_CHAIN_GENERATIONS.insert(key.clone(), generation);
    generation
}

/// Run `write` only while `generation` is the newest chain for `key`. The map
/// entry's read guard is held across `write`, so a chain beginning for the
/// same key cannot interleave between the check and the write.
fn if_current_setup_chain(key: &(String, String), generation: u64, write: impl FnOnce()) -> bool {
    match SETUP_CHAIN_GENERATIONS.get(key) {
        Some(current) if *current == generation => {
            write();
            true
        }
        _ => false,
    }
}

fn end_setup_chain(key: &(String, String), generation: u64) {
    SETUP_CHAIN_GENERATIONS.remove_if(key, |_, current| *current == generation);
}

/// Settles a chain that is dropped (task aborted or panicked, runtime shutting
/// down) before it published its outcome: a still-`pending` warm is marked
/// `failed`, the pollable setup status reports the stop, and a terminal
/// `worktree-setup-script-completed` (`outcome: stopped`) is emitted, so
/// neither a poller nor a frontend waiter can read "in progress" forever.
/// `finish_warm` is token-checked and the status/event are generation-checked,
/// so firing after a removal or a re-creation is a harmless no-op.
struct ChainStopGuard {
    state: Arc<AppState>,
    status_key: (String, String),
    generation: u64,
    worktree_path: PathBuf,
    warm: Option<u64>,
    armed: bool,
}

impl Drop for ChainStopGuard {
    fn drop(&mut self) {
        if self.armed {
            const REASON: &str = "worktree setup chain stopped before it finished";
            if let Some(token) = self.warm {
                finish_warm(
                    &self.worktree_path,
                    token,
                    serde_json::json!({"status": "failed", "reason": REASON}),
                );
            }
            if_current_setup_chain(&self.status_key, self.generation, || {
                self.state.worktree_setup_status.insert(
                    self.status_key.clone(),
                    Arc::new(crate::state::WorktreeSetupStatus::Completed {
                        exit_code: None,
                        error: Some(REASON.to_string()),
                    }),
                );
                emit_worktree_setup_script_completed(
                    &self.state,
                    &self.status_key.0,
                    &self.status_key.1,
                    &self.worktree_path.to_string_lossy(),
                    crate::state::SetupChainOutcome::Stopped,
                    None,
                    Some(REASON.to_string()),
                );
            });
        }
        end_setup_chain(&self.status_key, self.generation);
    }
}

/// Kick off the post-create background chain for a freshly created worktree:
/// **CoW warm → file sync → Setup Script**, strictly in that order.
///
/// - The warm honours the repo's `warm_ignored_directories` opt-out
///   (`config::resolve_effective_warm_setting`, resolved inside the chain so
///   every creation path behaves the same) and copies with bounded parallelism
///   (`crate::cow::WARM_COPY_CONCURRENCY` directories at a time), reporting
///   progress as `worktree-warm-*` events and in the workspace's pending
///   `warm_artifacts` detail (see [`warm_with_events`]).
/// - The file sync AWAITS the warm: both write into the same destination, and
///   running them concurrently let either one win (CoW benefit lost, or every
///   synced path reported "destination already exists"), or an interleaved
///   `cp -c -R` nest directories.
/// - The Setup Script awaits the sync, so a script that depends on a synced
///   file (a `copy_paths` entry symlinking `node_modules`, a synced `.env`)
///   can no longer run before it exists — the old "KNOWN, ACCEPTED ORDERING
///   GAP".
/// - `warm_token` is the token from `begin_warm` (`None` only for a caller
///   that does not warm). The warm status is published only after the LAST
///   step, so the workspace reads `warm_artifacts.status == "pending"` until
///   the whole chain is done — the setup script itself still observes
///   `pending`, as it did when main ran it before the warm. A removal that
///   clears the token mid-chain stops the remaining steps (nothing more is
///   written into a removed checkout).
///
/// Fire-and-forget for the caller: worktree creation has already returned by
/// the time this runs, on every creation path (desktop `create_worktree`, HTTP
/// `create_worktree_shared` incl. MCP `repo worktree_create`, HTTP
/// `create_session_with_worktree`). That is why no creation response carries
/// `setup_script`/`setup_script_error` any more: the outcome does not exist yet
/// when the response is built. When the chain ends, it dual-emits (event_bus +
/// Tauri window) `AppEvent::WorktreeSetupScriptCompleted` exactly once, with an
/// `outcome` of `completed`, `not_configured` or `stopped` (removal/abort) —
/// unless a newer chain for the same `(repo_path, branch)` has replaced it.
///
/// The event alone left MCP clients (no SSE/event stream to listen on) with
/// no way to ever learn the outcome — the chain also writes a
/// [`crate::state::WorktreeSetupStatus`] snapshot into
/// `AppState::worktree_setup_status`, keyed by `(repo_path, branch)`:
/// `Running` synchronously before this returns, then `NotConfigured` or
/// `Completed` once the chain finishes (the entry is dropped — reads as
/// `unknown` — when a removal stopped the chain). Pollable via
/// [`get_worktree_setup_status`] / `repo action=worktree_setup_status` /
/// `GET /worktrees/setup-status`. Writes are gated by a per-chain generation
/// ([`SETUP_CHAIN_GENERATIONS`]).
pub(crate) fn spawn_worktree_setup_chain(
    state: &Arc<AppState>,
    base_repo: String,
    branch: String,
    worktree_path: PathBuf,
    warm_token: Option<u64>,
) -> tokio::task::JoinHandle<()> {
    let state = Arc::clone(state);
    let status_key = (base_repo.clone(), branch.clone());
    let generation = begin_setup_chain(&status_key);
    if_current_setup_chain(&status_key, generation, || {
        state.worktree_setup_status.insert(
            status_key.clone(),
            Arc::new(crate::state::WorktreeSetupStatus::Running),
        );
    });
    let warm = warm_token.map(|token| {
        (
            token,
            warm_with_events(
                Arc::clone(&state),
                base_repo.clone(),
                branch.clone(),
                token,
                |source, destination, on_started, on_progress| {
                    crate::cow::warm_worktree_reporting(
                        source,
                        destination,
                        crate::cow::WARM_COPY_CONCURRENCY,
                        on_started,
                        on_progress,
                    )
                },
            ),
        )
    });
    tokio::spawn(run_worktree_setup_chain_generation(
        state,
        base_repo,
        branch,
        worktree_path,
        warm,
        generation,
    ))
}

/// Wrap a progress-reporting warm so it publishes what it does: the real
/// directory count and per-directory completions go out as dual-emitted
/// `worktree-warm-started`/`-progress`/`-completed` events (progress throttled
/// to ~150 ms, like the file sync) and into the workspace's pending
/// `warm_artifacts` detail (`phase: "warming"`, `copied`, `total`), which is
/// what an MCP/HTTP poller of `worktree_list`/`GET /worktrees/paths` sees.
/// Silent — no events at all — when nothing is copied. `reporting` is
/// injectable so tests can drive the events without a real copy.
pub(crate) fn warm_with_events<R>(
    state: Arc<AppState>,
    repo_path: String,
    branch: String,
    token: u64,
    reporting: R,
) -> impl FnOnce(&Path, &Path) -> crate::cow::WarmingReport + Send + 'static
where
    R: FnOnce(
            &Path,
            &Path,
            &mut dyn FnMut(usize),
            &mut dyn FnMut(usize, usize, &Path),
        ) -> crate::cow::WarmingReport
        + Send
        + 'static,
{
    move |source: &Path, destination: &Path| {
        let worktree_path = destination.to_string_lossy().into_owned();
        let mut started = false;
        let mut last_emit = std::time::Instant::now();
        let report = {
            let mut on_started = |total: usize| {
                started = true;
                update_pending_warm(
                    destination,
                    token,
                    serde_json::json!({"phase": "warming", "copied": 0, "total": total}),
                );
                emit_worktree_warm_started(&state, &repo_path, &branch, &worktree_path, total);
            };
            let mut on_progress = |copied: usize, total: usize, current: &Path| {
                let now = std::time::Instant::now();
                if copied == total || now.duration_since(last_emit).as_millis() >= 150 {
                    last_emit = now;
                    update_pending_warm(
                        destination,
                        token,
                        serde_json::json!({"phase": "warming", "copied": copied, "total": total}),
                    );
                    emit_worktree_warm_progress(
                        &state,
                        &repo_path,
                        &branch,
                        &worktree_path,
                        copied,
                        total,
                        Some(&current.to_string_lossy()),
                    );
                }
            };
            reporting(source, destination, &mut on_started, &mut on_progress)
        };
        if started {
            emit_worktree_warm_completed(
                &state,
                &repo_path,
                &branch,
                &worktree_path,
                report.warmed,
                &report.warnings,
            );
        }
        report
    }
}

/// The body of [`spawn_worktree_setup_chain`], with the warm step injectable
/// so tests can observe the ordering deterministically. Starts its own chain
/// generation; production goes through [`spawn_worktree_setup_chain`], which
/// begins the generation synchronously before spawning.
#[cfg(test)]
pub(crate) async fn run_worktree_setup_chain<W>(
    state: Arc<AppState>,
    base_repo: String,
    branch: String,
    worktree_path: PathBuf,
    warm: Option<(u64, W)>,
) where
    W: FnOnce(&Path, &Path) -> crate::cow::WarmingReport + Send + 'static,
{
    let generation = begin_setup_chain(&(base_repo.clone(), branch.clone()));
    run_worktree_setup_chain_generation(state, base_repo, branch, worktree_path, warm, generation)
        .await;
}

async fn run_worktree_setup_chain_generation<W>(
    state: Arc<AppState>,
    base_repo: String,
    branch: String,
    worktree_path: PathBuf,
    warm: Option<(u64, W)>,
    generation: u64,
) where
    W: FnOnce(&Path, &Path) -> crate::cow::WarmingReport + Send + 'static,
{
    let status_key = (base_repo.clone(), branch.clone());
    let worktree_path_str = worktree_path.to_string_lossy().into_owned();
    let mut guard = ChainStopGuard {
        state: Arc::clone(&state),
        status_key: status_key.clone(),
        generation,
        worktree_path: worktree_path.clone(),
        warm: warm.as_ref().map(|(token, _)| *token),
        armed: true,
    };
    // A removal won: nothing more may be written into the checkout. The
    // status entry is dropped (there is no outcome for a workspace that no
    // longer exists), but the waiter still gets its terminal event.
    let stop_removed = |guard: &mut ChainStopGuard| {
        guard.armed = false;
        if_current_setup_chain(&status_key, generation, || {
            state.worktree_setup_status.invalidate(&status_key);
            emit_worktree_setup_script_completed(
                &state,
                &base_repo,
                &branch,
                &worktree_path_str,
                crate::state::SetupChainOutcome::Stopped,
                None,
                Some("the worktree was removed before its setup chain finished".to_string()),
            );
        });
    };

    // 1. CoW warm (awaited; its status is held back until the end), unless
    //    the repo opted out — the copy is then skipped but the token is still
    //    honoured, so a removal stops the chain exactly as before.
    let mut warm_result: Option<(u64, serde_json::Value)> = None;
    if let Some((token, warm_fn)) = warm {
        let repo_for_setting = base_repo.clone();
        let enabled = tokio::task::spawn_blocking(move || {
            crate::config::resolve_effective_warm_setting(&repo_for_setting)
        })
        .await
        .unwrap_or(true);
        let source = PathBuf::from(&base_repo);
        let destination = worktree_path.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            tuic_git::worktree::run_background_warm_blocking(
                &source,
                &destination,
                token,
                |source, destination| {
                    if enabled {
                        warm_fn(source, destination)
                    } else {
                        crate::cow::WarmingReport::default()
                    }
                },
            )
        })
        .await;
        match outcome {
            Ok(Some(mut status)) => {
                if !enabled {
                    status["skipped"] = serde_json::json!(
                        "warming ignored directories is disabled for this repository"
                    );
                }
                warm_result = Some((token, status));
            }
            // Removed before the copy could start: nothing may be written
            // into this checkout any more.
            Ok(None) => {
                stop_removed(&mut guard);
                return;
            }
            Err(error) => {
                warm_result = Some((
                    token,
                    serde_json::json!({"status": "failed", "reason": format!("warm task panicked: {error}")}),
                ));
            }
        }
    }
    let still_current =
        |token: Option<u64>| token.is_none_or(|token| warm_token_is_current(&worktree_path, token));
    let token = warm_result.as_ref().map(|(token, _)| *token);
    if let Some(token) = token {
        update_pending_warm(
            &worktree_path,
            token,
            serde_json::json!({"phase": "file_sync_and_setup_script"}),
        );
    }

    // 2. File sync, only after the warm has finished writing.
    if still_current(token) {
        run_worktree_file_sync(&state, &base_repo, &branch, &worktree_path).await;
    }

    // 3. Setup Script, only after the sync.
    if !still_current(token) {
        stop_removed(&mut guard);
        return;
    }
    {
        let repo_for_script = base_repo.clone();
        let script = tokio::task::spawn_blocking(move || {
            crate::config::resolve_effective_setup_script(&repo_for_script)
        })
        .await
        .ok()
        .flatten();
        if let Some(script) = script {
            let cwd_for_script = worktree_path_str.clone();
            let outcome =
                tokio::task::spawn_blocking(move || run_setup_script(script, cwd_for_script)).await;
            let (exit_code, error) = match outcome {
                Ok(Ok(result)) => (result["exit_code"].as_i64(), None),
                Ok(Err(e)) => (None, Some(e)),
                Err(e) => (None, Some(format!("task panic: {e}"))),
            };
            if_current_setup_chain(&status_key, generation, || {
                state.worktree_setup_status.insert(
                    status_key.clone(),
                    Arc::new(crate::state::WorktreeSetupStatus::Completed {
                        exit_code,
                        error: error.clone(),
                    }),
                );
                emit_worktree_setup_script_completed(
                    &state,
                    &base_repo,
                    &branch,
                    &worktree_path_str,
                    crate::state::SetupChainOutcome::Completed,
                    exit_code,
                    error,
                );
            });
        } else {
            if_current_setup_chain(&status_key, generation, || {
                state.worktree_setup_status.insert(
                    status_key.clone(),
                    Arc::new(crate::state::WorktreeSetupStatus::NotConfigured),
                );
                emit_worktree_setup_script_completed(
                    &state,
                    &base_repo,
                    &branch,
                    &worktree_path_str,
                    crate::state::SetupChainOutcome::NotConfigured,
                    None,
                    None,
                );
            });
        }
    }

    // 4. Only now does the workspace stop reading `pending`.
    if let Some((token, status)) = warm_result {
        finish_warm(&worktree_path, token, status);
    }
    guard.armed = false;
}

/// Poll the current status of a worktree's background setup chain, keyed by
/// `(repo_path, branch)` — the same pair `spawn_worktree_setup_chain` tracks
/// and the `worktree-setup-script-completed` event carries. Returns `None`
/// when nothing is tracked for this pair: no worktree creation ever started a
/// chain for it, the entry aged out (30 minute TTL — see
/// `build_worktree_setup_status_cache`), or the app restarted since. A caller
/// that gets `None` right after creating a worktree should treat it as "still
/// starting," not "definitely no script" — the entry is inserted synchronously
/// by `spawn_worktree_setup_chain` before it returns, so a `None` for a
/// worktree just created moments ago most likely means the key doesn't match
/// (wrong repo_path/branch), not a real race.
pub(crate) fn get_worktree_setup_status(
    state: &AppState,
    repo_path: &str,
    branch: &str,
) -> Option<crate::state::WorktreeSetupStatus> {
    state
        .worktree_setup_status
        .get(&(repo_path.to_string(), branch.to_string()))
        .map(|arc| (*arc).clone())
}

// The worktree post-create events below all go through `AppState::emit_dual`:
// one `AppEvent`, whose wire payload (`event_wire::event_payload`, which calls
// the shared builders in `state.rs`) feeds both the desktop window emit and the
// `/events` SSE arm, so the camelCase keys cannot drift between transports.
fn emit_worktree_sync_started(state: &Arc<AppState>, repo_path: &str, branch: &str) {
    state.emit_dual(crate::state::AppEvent::WorktreeSyncStarted {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
    });
}

fn emit_worktree_sync_progress(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    copied: usize,
    total: usize,
) {
    state.emit_dual(crate::state::AppEvent::WorktreeSyncProgress {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        copied,
        total,
    });
}

fn emit_worktree_sync_completed(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    summary: &crate::worktree_sync::SyncSummary,
) {
    state.emit_dual(crate::state::AppEvent::WorktreeSyncCompleted {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        copied: summary.copied,
        total: summary.total,
        errors: summary.errors.clone(),
    });
}

fn emit_worktree_setup_script_completed(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    worktree_path: &str,
    outcome: crate::state::SetupChainOutcome,
    exit_code: Option<i64>,
    error: Option<String>,
) {
    state.emit_dual(crate::state::AppEvent::WorktreeSetupScriptCompleted {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        worktree_path: worktree_path.to_string(),
        outcome,
        exit_code,
        error,
    });
}

fn emit_worktree_warm_started(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    worktree_path: &str,
    total: usize,
) {
    state.emit_dual(crate::state::AppEvent::WorktreeWarmStarted {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        worktree_path: worktree_path.to_string(),
        total,
    });
}

fn emit_worktree_warm_progress(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    worktree_path: &str,
    copied: usize,
    total: usize,
    current: Option<&str>,
) {
    state.emit_dual(crate::state::AppEvent::WorktreeWarmProgress {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        worktree_path: worktree_path.to_string(),
        copied,
        total,
        current: current.map(str::to_string),
    });
}

fn emit_worktree_warm_completed(
    state: &Arc<AppState>,
    repo_path: &str,
    branch: &str,
    worktree_path: &str,
    warmed: usize,
    warnings: &[String],
) {
    state.emit_dual(crate::state::AppEvent::WorktreeWarmCompleted {
        repo_path: repo_path.to_string(),
        branch: branch.to_string(),
        worktree_path: worktree_path.to_string(),
        warmed,
        warnings: warnings.to_vec(),
    });
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
        create_workspace_unwarmed(&worktrees_dir, &config, base_ref.as_deref())
    })
    .await
    .map_err(|error| format!("Task panic: {error}"))??;

    let token = begin_warm(&workspace.path);
    state.invalidate_repo_caches(&base_repo);
    // warm -> file sync -> Setup Script, in that order, in the background. The
    // chain owns the warm handle (it awaits it before the sync starts) and
    // publishes the warm status only once the last step is done, so the
    // workspace reads `pending` until then.
    spawn_worktree_setup_chain(
        &state,
        base_repo.clone(),
        workspace.branch.clone(),
        workspace.path.clone(),
        Some(token),
    );
    Ok(ipc_worktree_response(&workspace, &base_repo))
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

/// Remove one workspace's checkout by workspace id (Tauri command with cache invalidation)
///
/// `delete_branch` defaults to `false` with force and `true` otherwise.
#[cfg(feature = "desktop")]
#[tauri::command]
// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) async fn remove_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
    delete_branch: Option<bool>,
    force: Option<bool>,
    override_lock: Option<bool>,
    expected_fingerprint: Option<String>,
    confirm_missing_checkout: Option<bool>,
    override_busy: Option<bool>,
) -> Result<RemoveWorktreeOutcome, String> {
    remove_worktree_ipc_impl(
        state.inner(),
        repo_path,
        workspace_id,
        delete_branch,
        force,
        override_lock,
        expected_fingerprint,
        confirm_missing_checkout,
        override_busy,
    )
    .await
}

/// The body of the desktop `remove_worktree` command, outside the
/// `#[tauri::command]` so it can be tested without a Tauri runtime.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) async fn remove_worktree_ipc_impl(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    delete_branch: Option<bool>,
    force: Option<bool>,
    override_lock: Option<bool>,
    expected_fingerprint: Option<String>,
    confirm_missing_checkout: Option<bool>,
    override_busy: Option<bool>,
) -> Result<RemoveWorktreeOutcome, String> {
    let force = force.unwrap_or(false);
    let confirm_missing_checkout = confirm_missing_checkout.unwrap_or(false);
    if force && expected_fingerprint.is_none() && !confirm_missing_checkout {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    let delete_branch = delete_branch.unwrap_or(!force);
    let override_lock = override_lock.unwrap_or(false);
    let override_busy = override_busy.unwrap_or(false);
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
    let guard_state = Arc::clone(state);
    let result = tokio::task::spawn_blocking(move || {
        workspace_removal_guard(
            &guard_state,
            &repo_path_clone,
            &workspace_id_clone,
            override_busy,
        )
        .map_err(|busy| busy.message())?;
        remove_worktree_with_presence_confirmation(
            &repo_path_clone,
            &workspace_id_clone,
            delete_branch,
            script.as_deref(),
            force,
            override_lock,
            expected_fingerprint.as_deref(),
            confirm_missing_checkout,
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
        tokio::task::spawn_blocking(move || {
            delete_local_branch_impl(&repo_path, &branch_name, &workspace_id, keep_worktree)
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

/// Everything an orphan removal must pass before anything is deleted: the
/// path is one of the repo's own worktrees, and [`orphan_removal_guard`]'s
/// live-session verdict. `remove_orphan_checkout` and the HTTP route (which
/// maps a refusal to 400 and a failed removal to 500) both call this, so the
/// two can never guard differently.
pub(crate) fn check_orphan_removal(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
    safe_only: bool,
    confirmed_sessions: &[String],
) -> Result<(), String> {
    validate_worktree_path(repo_path, worktree_path)?;
    orphan_removal_guard(
        state,
        repo_path,
        worktree_path,
        safe_only,
        confirmed_sessions,
    )
}

/// Remove an orphan checkout by path after the shared guard. Both the desktop
/// command and the MCP `worktree_remove` call this.
pub(crate) fn remove_orphan_checkout(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
    safe_only: bool,
    confirmed_sessions: &[String],
) -> Result<(), String> {
    check_orphan_removal(
        state,
        repo_path,
        worktree_path,
        safe_only,
        confirmed_sessions,
    )?;

    let path = PathBuf::from(worktree_path);
    let worktree = WorktreeInfo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| worktree_path.to_string()),
        path,
        branch: None,
        base_repo: PathBuf::from(repo_path),
    };
    tuic_git::worktree::remove_orphan_worktree_internal(&worktree)?;
    state.invalidate_repo_caches(repo_path);
    Ok(())
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
    safe_only: Option<bool>,
    confirmed_sessions: Option<Vec<String>>,
) -> Result<(), String> {
    remove_orphan_checkout(
        &state,
        &repo_path,
        &worktree_path,
        safe_only.unwrap_or(false),
        &confirmed_sessions.unwrap_or_default(),
    )
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
#[cfg(test)]
pub(crate) fn finalize_merged_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    finalize_merged_worktree_impl_with_confirmation(
        state,
        repo_path,
        workspace_id,
        action,
        force,
        None,
    )
}

/// Refuse an automatic archive when the workspace's checkout has a git operation in
/// progress (rebase/merge/cherry-pick/revert/bisect). A merge or cherry-pick conflict never
/// detaches HEAD, and a *clean* mid-bisect or mid-cherry-pick worktree passes the dirtiness
/// and lifecycle gates, so neither catches this on its own.
///
/// A hard error, not `needs_confirmation`: there is no sensible "yes, destroy my in-flight
/// rebase" answer to offer unattended, so `force` does not bypass it. The delete path needs
/// no extra check — `tuic_git`'s id-based removal already refuses a busy worktree.
fn err_if_worktree_busy(base_repo: &Path, workspace_id: &str) -> Result<(), String> {
    let workspace = resolve_workspace(base_repo, workspace_id)?;
    if has_operation_in_progress(&workspace.path) {
        return Err(format!(
            "Cannot archive worktree for branch '{}': a git operation \
             (rebase/merge/cherry-pick/revert/bisect) is in progress",
            workspace.branch
        ));
    }
    Ok(())
}

/// Both one-click merge cleanup and post-merge finalization use this review
/// gate. Before the merge, an unmerged commit is expected; afterwards, only
/// merged work may be cleaned up without an explicit confirmation.
fn cleanup_needs_lifecycle_confirmation(
    state: &AppState,
    repo_path: &Path,
    workspace_id: &str,
    action: &str,
    force: bool,
    dirt: &WorktreeDirtiness,
    require_merged: bool,
) -> bool {
    if cleanup_needs_confirmation(action, force, dirt) {
        return true;
    }
    if force || (action != "archive" && action != "delete") {
        return false;
    }
    let preview = inspect_worktree_removal(state, repo_path, workspace_id);
    preview.lifecycle.removal_safety != WorkspaceRemovalSafety::Safe
        || !preview.live_sessions.is_empty()
        || preview.lifecycle.commit_status == WorkspaceCommitStatus::Unknown
        || (require_merged && preview.lifecycle.commit_status != WorkspaceCommitStatus::Merged)
}

pub(crate) fn finalize_merged_worktree_impl_with_confirmation(
    state: &Arc<AppState>,
    repo_path: String,
    workspace_id: String,
    action: String,
    force: bool,
    expected_fingerprint: Option<&str>,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = std::path::PathBuf::from(&repo_path);

    if let Some(expected) = expected_fingerprint {
        let workspace = resolve_any_workspace(&base_repo, &workspace_id)?;
        if dirty_fingerprint_at(Path::new(&workspace.path))?.0 != expected {
            return Err(
                "Worktree state changed since confirmation; review it before cleanup".into(),
            );
        }
    }

    let dirt = worktree_dirtiness(&base_repo, &workspace_id);
    if cleanup_needs_lifecycle_confirmation(
        state,
        &base_repo,
        &workspace_id,
        &action,
        force,
        &dirt,
        true,
    ) {
        return Ok(MergeArchiveResult {
            merged: true, // The merge itself already happened; only cleanup stopped.
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: 0,
            worktree_dirty: dirt.is_dirty(),
            branch_delete_warning: None,
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
            err_if_worktree_busy(&base_repo, &workspace_id)?;
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
                branch_delete_warning: None,
            })
        }
        "delete" => {
            let outcome = remove_worktree_by_workspace_id_with_confirmation(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                force,
                false,
                expected_fingerprint,
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
                branch_delete_warning: outcome.branch_delete_warning,
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
    expected_fingerprint: Option<String>,
) -> Result<MergeArchiveResult, String> {
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path,
            workspace_id,
            action,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
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
#[cfg(test)]
pub(crate) fn merge_and_archive_worktree_impl(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: bool,
) -> Result<MergeArchiveResult, String> {
    merge_and_archive_worktree_impl_with_confirmation(
        state,
        repo_path,
        branch_name,
        workspace_id,
        target_branch,
        after_merge,
        force,
        None,
    )
}

// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) fn merge_and_archive_worktree_impl_with_confirmation(
    state: &Arc<AppState>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: bool,
    expected_fingerprint: Option<&str>,
) -> Result<MergeArchiveResult, String> {
    let script = resolve_archive_script(&repo_path);
    let base_repo = PathBuf::from(&repo_path);
    if let Some(expected) = expected_fingerprint {
        let workspace = resolve_any_workspace(&base_repo, &workspace_id)?;
        if dirty_fingerprint_at(Path::new(&workspace.path))?.0 != expected {
            return Err(
                "Worktree state changed since confirmation; review it before cleanup".into(),
            );
        }
    }

    // 0. Pre-flight: would the cleanup take uncommitted work with it? Archive moves
    //    the directory aside and delete removes it outright, but either way any
    //    worktree not known to be clean must be confirmed first — whether or not
    //    the branch carries commits. `commits_ahead` is reported alongside so the
    //    dialog can also say that an empty branch's merge would be a no-op.
    let preflight = merge_preflight(&repo_path, &branch_name, &workspace_id, &target_branch);
    if cleanup_needs_lifecycle_confirmation(
        state,
        &base_repo,
        &workspace_id,
        &after_merge,
        force,
        &preflight.worktree_dirty,
        false,
    ) {
        return Ok(MergeArchiveResult {
            merged: false,
            action: "needs_confirmation".to_string(),
            archive_path: None,
            commits_ahead: preflight.commits_ahead,
            worktree_dirty: preflight.worktree_dirty.is_dirty(),
            branch_delete_warning: None,
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
            err_if_worktree_busy(&base_repo, &workspace_id)?;
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
                branch_delete_warning: None,
            })
        }
        "delete" => {
            let outcome = remove_worktree_by_workspace_id_with_confirmation(
                &repo_path,
                &workspace_id,
                true,
                script.as_deref(),
                force,
                false,
                expected_fingerprint,
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
                branch_delete_warning: outcome.branch_delete_warning,
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
                branch_delete_warning: None,
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
// Keep the independently supplied boundary fields explicit; grouping changes this contract.
#[expect(
    clippy::too_many_arguments,
    reason = "flat IPC safety-confirmation contract"
)]
pub(crate) fn merge_and_archive_worktree(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    branch_name: String,
    workspace_id: String,
    target_branch: String,
    after_merge: String,
    force: Option<bool>,
    expected_fingerprint: Option<String>,
) -> Result<MergeArchiveResult, String> {
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return Err(
            "force requires expected_fingerprint from the confirmed lifecycle status".into(),
        );
    }
    merge_and_archive_worktree_impl_with_confirmation(
        state.inner(),
        repo_path,
        branch_name,
        workspace_id,
        target_branch,
        after_merge,
        force.unwrap_or(false),
        expected_fingerprint.as_deref(),
    )
}

// --- Tauri wrappers over tuic_git::worktree ---

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn check_worktree_dirty(
    repo_path: String,
    workspace_id: String,
) -> Result<bool, String> {
    tuic_git::worktree::check_worktree_dirty(repo_path, workspace_id)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_workspace_lifecycle(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    workspace_id: String,
) -> Result<WorktreeRemovalPreview, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        Ok(inspect_worktree_removal(
            &state,
            Path::new(&repo_path),
            &workspace_id,
        ))
    })
    .await
    .map_err(|e| format!("workspace lifecycle task failed: {e}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn get_worktree_paths(
    repo_path: String,
) -> Result<HashMap<String, WorkspaceWorktree>, String> {
    tuic_git::worktree::get_worktree_paths(repo_path)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn detect_orphan_worktrees(repo_path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        tuic_git::worktree::detect_orphan_worktrees_blocking(repo_path)
    })
    .await
    .map_err(|e| format!("orphan worktree detection task failed: {e}"))?
}

/// A detached checkout's removal verdict, plus the sessions working in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OrphanCleanupReview {
    #[serde(flatten)]
    pub assessment: tuic_git::worktree::OrphanCleanupAssessment,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub live_sessions: Vec<WorktreeLiveSession>,
}

fn live_session_reason(sessions: &[WorktreeLiveSession]) -> String {
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
    format!("live session: {}", names.join(", "))
}

/// Git safety plus the session registry: an orphan checkout a session still
/// works in is never safe to remove without a human review. A checkout that
/// is already gone holds no work, so its stale sessions do not block.
pub(crate) fn orphan_cleanup_safety_with_sessions(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
) -> Result<(), String> {
    tuic_git::worktree::orphan_cleanup_safety(repo_path, worktree_path)?;
    let path = Path::new(worktree_path);
    if !path.exists() {
        return Ok(());
    }
    let live = live_sessions_in(state, path);
    if live.is_empty() {
        Ok(())
    } else {
        Err(live_session_reason(&live))
    }
}

/// The removal guard both transports share. A safe-only removal needs the full
/// verdict. A review-confirmed one (`safe_only` false) needs no sessions beyond
/// those the user saw in the dialog: one that started since was never reviewed.
pub(crate) fn orphan_removal_guard(
    state: &AppState,
    repo_path: &str,
    worktree_path: &str,
    safe_only: bool,
    confirmed_sessions: &[String],
) -> Result<(), String> {
    if safe_only {
        return orphan_cleanup_safety_with_sessions(state, repo_path, worktree_path);
    }
    let path = Path::new(worktree_path);
    if !path.exists() {
        return Ok(());
    }
    let unseen: Vec<WorktreeLiveSession> = live_sessions_in(state, path)
        .into_iter()
        .filter(|session| !confirmed_sessions.contains(&session.session_id))
        .collect();
    if unseen.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{}; not part of the confirmed removal",
            live_session_reason(&unseen)
        ))
    }
}

pub(crate) fn assess_orphan_cleanup_with_sessions(
    state: &AppState,
    repo_path: &str,
) -> Result<Vec<OrphanCleanupReview>, String> {
    Ok(tuic_git::worktree::assess_orphan_worktrees(repo_path)?
        .into_iter()
        .map(|mut assessment| {
            let live_sessions = if Path::new(&assessment.path).exists() {
                live_sessions_in(state, Path::new(&assessment.path))
            } else {
                Vec::new()
            };
            if !live_sessions.is_empty() {
                assessment.safe = false;
                let live = live_session_reason(&live_sessions);
                assessment.reason = Some(match assessment.reason.take() {
                    Some(reason) => format!("{reason}; {live}"),
                    None => live,
                });
            }
            OrphanCleanupReview {
                assessment,
                live_sessions,
            }
        })
        .collect())
}

pub(crate) async fn assess_orphan_cleanup_internal(
    state: Arc<AppState>,
    repo_path: String,
) -> Result<Vec<OrphanCleanupReview>, String> {
    tokio::task::spawn_blocking(move || assess_orphan_cleanup_with_sessions(&state, &repo_path))
        .await
        .map_err(|error| format!("orphan cleanup assessment task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn assess_orphan_cleanup(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Result<Vec<OrphanCleanupReview>, String> {
    assess_orphan_cleanup_internal(state.inner().clone(), repo_path).await
}

#[derive(Clone)]
pub(crate) struct PendingOrphanCleanup {
    pub(crate) paths: Vec<String>,
    pub(crate) answer: Option<bool>,
    /// The dialog has ended. A Keep stays readable for other clients that still
    /// show it, but nothing is left to answer.
    pub(crate) settled: bool,
}

pub(crate) fn begin_orphan_cleanup_internal(
    state: &AppState,
    repo_path: &str,
    paths: Vec<String>,
) -> Result<(), String> {
    let current = tuic_git::worktree::assess_orphan_worktrees(repo_path)?;
    if paths.is_empty()
        || paths.iter().any(|path| {
            !current.iter().any(|entry| {
                tuic_core::path_spelling::portable_spelling(&entry.path)
                    == tuic_core::path_spelling::portable_spelling(path)
            })
        })
    {
        return Err("Pending cleanup must list current orphan worktrees".into());
    }
    state.pending_orphan_cleanup.insert(
        repo_path.to_string(),
        PendingOrphanCleanup {
            paths,
            answer: None,
            settled: false,
        },
    );
    Ok(())
}

pub(crate) fn answer_orphan_cleanup_internal(
    state: &AppState,
    repo_path: &str,
    remove: bool,
) -> Result<(), String> {
    let pending = state
        .pending_orphan_cleanup
        .get(repo_path)
        .filter(|entry| !entry.settled)
        .ok_or("No pending orphan cleanup for this repository")?
        .paths
        .clone();
    if remove {
        for path in &pending {
            orphan_cleanup_safety_with_sessions(state, repo_path, path)?;
        }
    }
    let mut current = state
        .pending_orphan_cleanup
        .get_mut(repo_path)
        .ok_or("Orphan cleanup was already dismissed")?;
    if current.paths != pending || current.answer.is_some() {
        return Err("Orphan cleanup changed while it was being answered".into());
    }
    current.answer = Some(remove);
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn begin_orphan_cleanup(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || begin_orphan_cleanup_internal(&state, &repo_path, paths))
        .await
        .map_err(|error| format!("orphan cleanup registration task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn pending_orphan_cleanup_answer(
    state: State<'_, Arc<AppState>>,
    repo_path: String,
) -> Option<bool> {
    state
        .pending_orphan_cleanup
        .get(&repo_path)
        .and_then(|entry| entry.answer)
}

/// End a client's cleanup dialog. A Keep stays on the shared entry as `answer =
/// Some(false)` so other clients polling it close their own dialog instead of
/// counting down to a removal; anything else drops the entry.
pub(crate) fn clear_orphan_cleanup_internal(state: &AppState, repo_path: &str, kept: bool) {
    if kept && let Some(mut entry) = state.pending_orphan_cleanup.get_mut(repo_path) {
        entry.answer = Some(false);
        entry.settled = true;
        return;
    }
    state.pending_orphan_cleanup.remove(repo_path);
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn clear_orphan_cleanup(state: State<'_, Arc<AppState>>, repo_path: String, kept: bool) {
    clear_orphan_cleanup_internal(&state, &repo_path, kept);
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn generate_worktree_name_cmd(existing_names: Vec<String>) -> String {
    tuic_git::worktree::generate_worktree_name_cmd(existing_names)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn generate_clone_branch_name_cmd(
    source_branch: String,
    existing_names: Vec<String>,
) -> String {
    tuic_git::worktree::generate_clone_branch_name_cmd(source_branch, existing_names)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_local_branches(repo_path: String) -> Result<Vec<String>, String> {
    tuic_git::worktree::list_local_branches(repo_path)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn list_base_ref_options(repo_path: String) -> Result<Vec<BaseRefOption>, String> {
    tuic_git::worktree::list_base_ref_options(repo_path)
}

/// `TUIC_*` context (`script_env.rs`) for an Archive Script about to run in
/// `cwd` — the worktree being archived or deleted.
fn archive_script_env(cwd: &Path) -> Vec<(String, String)> {
    crate::script_env::ScriptContext::derive(crate::script_env::ScriptKind::Archive, cwd)
        .std_pairs()
}

/// tuic-git's `archive_worktree` with the Archive Script given its `TUIC_*`
/// context. Shadows the glob re-export so every app-crate caller gets it.
pub(crate) fn archive_worktree(
    base_repo: &Path,
    workspace_id: &str,
    archive_script: Option<&str>,
) -> Result<String, String> {
    tuic_git::worktree::archive_worktree(
        base_repo,
        workspace_id,
        archive_script.map(|script| UserScript {
            script,
            env: &archive_script_env,
        }),
    )
}

/// Run a Setup Script with its `TUIC_*` context. Not desktop-gated: the HTTP
/// twins (`POST /worktrees/run-script`, post-create setup in
/// worktree_routes.rs/session.rs) call this too, so the headless build injects
/// the same variables.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn run_setup_script(script: String, cwd: String) -> Result<serde_json::Value, String> {
    tuic_git::worktree::run_setup_script_with_env(script, cwd, &|cwd| {
        crate::script_env::ScriptContext::derive(crate::script_env::ScriptKind::Setup, cwd)
            .std_pairs()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreeStorage;
    use std::fs;
    use tempfile::TempDir;
    use tuic_git::test_fixtures::{
        base_branch_of, dirty_worktree_with, setup_test_repo, worktree_with,
    };

    // Catches accepting an empty or unregistered orphan list after weakening
    // the validation OR: neither request may create an actionable dialog.
    #[test]
    fn cleanup_rejects_empty_and_unknown_paths_1488() {
        let repo = setup_test_repo();
        assert!(
            repo.path()
                .canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        let state = crate::state::tests_support::make_test_app_state();
        let path = repo.path().to_string_lossy();
        for paths in [
            vec![],
            vec![repo.path().join("unknown").to_string_lossy().into_owned()],
        ] {
            assert!(begin_orphan_cleanup_internal(&state, &path, paths).is_err());
            assert!(state.pending_orphan_cleanup.get(path.as_ref()).is_none());
        }
    }

    // Catches replacing a client's answer because the paths are unchanged:
    // answer.is_some alone must forbid a second answer.
    #[test]
    fn cleanup_cannot_overwrite_an_existing_answer_1488() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        answer_orphan_cleanup_internal(&state, "/repo", false).unwrap();
        assert!(answer_orphan_cleanup_internal(&state, "/repo", false).is_err());
        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    fn pending_cleanup(state: &AppState, repo: &str) {
        state.pending_orphan_cleanup.insert(
            repo.to_string(),
            PendingOrphanCleanup {
                paths: vec!["/wt/a".into()],
                answer: None,
                settled: false,
            },
        );
    }

    fn pending_answer(state: &AppState, repo: &str) -> Option<bool> {
        state
            .pending_orphan_cleanup
            .get(repo)
            .and_then(|entry| entry.answer)
    }

    // Catches: clear deleting the entry after a Keep, so a second client that shows the
    // same dialog polls null, keeps counting down and removes the worktree (1289-27f8).
    #[test]
    fn clearing_a_kept_cleanup_leaves_the_keep_visible_to_other_clients() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Catches: a Keep left on the shared entry turning every later answer into
    // "changed while it was being answered", so an agent could never remove the
    // orphan that had since become clean (orphan-dialog-repeats).
    #[test]
    fn answering_a_dialog_that_was_already_kept_reports_nothing_pending() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        clear_orphan_cleanup_internal(&state, "/repo", true);

        let error = answer_orphan_cleanup_internal(&state, "/repo", true).unwrap_err();

        assert_eq!(error, "No pending orphan cleanup for this repository");
        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Catches: a non-Keep clear leaving a stale entry behind that answers later dialogs.
    #[test]
    fn clearing_a_finished_cleanup_removes_the_entry() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", false);

        assert!(state.pending_orphan_cleanup.get("/repo").is_none());
    }

    // Attack: a Keep for a dialog nobody registered must not invent a pending entry.
    #[test]
    fn keeping_without_a_pending_cleanup_creates_nothing() {
        let state = crate::state::tests_support::make_test_app_state();

        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert!(state.pending_orphan_cleanup.get("/repo").is_none());
    }

    // Attack: two clients both end in Keep; the second must not flip the answer back.
    #[test]
    fn keeping_twice_stays_kept() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");

        clear_orphan_cleanup_internal(&state, "/repo", true);
        clear_orphan_cleanup_internal(&state, "/repo", true);

        assert_eq!(pending_answer(&state, "/repo"), Some(false));
    }

    // Attack: a kept entry is per repo and a new dialog starts unanswered again.
    #[test]
    fn keep_does_not_leak_to_another_repo_or_to_the_next_dialog() {
        let state = crate::state::tests_support::make_test_app_state();
        pending_cleanup(&state, "/repo");
        pending_cleanup(&state, "/other");
        clear_orphan_cleanup_internal(&state, "/repo", true);
        assert_eq!(pending_answer(&state, "/other"), None);

        pending_cleanup(&state, "/repo"); // what begin_orphan_cleanup does for a new dialog

        assert_eq!(pending_answer(&state, "/repo"), None);
    }

    #[cfg(unix)]
    #[test]
    fn removal_preview_names_live_nested_session_and_counts_untracked_work() {
        let repo = setup_test_repo();
        assert!(
            repo.path()
                .canonicalize()
                .unwrap()
                .starts_with(tuic_test_support::test_temp_root().canonicalize().unwrap())
        );
        let worktree = worktree_with(repo.path(), "active-work", false);
        fs::create_dir_all(worktree.join("nested")).expect("nested cwd");
        fs::write(worktree.join("README.md"), "changed").expect("modified file");
        fs::write(worktree.join("new-a.txt"), "a").expect("first untracked file");
        fs::write(worktree.join("new-b.txt"), "b").expect("second untracked file");
        let state = crate::state::tests_support::make_test_app_state();
        crate::state::tests_support::insert_dummy_session(&state, "pty-active");
        crate::state::tests_support::set_session_cwd(
            &state,
            "pty-active",
            &worktree.join("nested").to_string_lossy(),
        );
        state
            .session_maps
            .sessions
            .get("pty-active")
            .unwrap()
            .lock()
            .display_name = Some("Codex: gate work".to_string());
        crate::state::tests_support::insert_dummy_session(&state, "pty-other");
        crate::state::tests_support::set_session_cwd(
            &state,
            "pty-other",
            &repo.path().to_string_lossy(),
        );

        let preview = inspect_worktree_removal(&state, repo.path(), "active-work");

        assert_eq!(preview.lifecycle.dirty_files, Some(3));
        assert_eq!(preview.untracked_files, Some(2));
        assert_eq!(
            preview.live_sessions,
            vec![WorktreeLiveSession {
                session_id: "pty-active".to_string(),
                name: "Codex: gate work".to_string(),
            }]
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("Codex: gate work"))
        );
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.contains("2 untracked"))
        );
    }

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
    #[cfg(unix)]
    fn one_click_cleanup_keeps_a_clean_worktree_with_a_live_agent() {
        let (_cfg, _guard) = isolated_config();
        for action in ["archive", "delete"] {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "active-feature", true);
            let original_head = git_cmd(repo.path())
                .args(["rev-parse", "HEAD"])
                .run()
                .unwrap()
                .stdout;
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            crate::state::tests_support::insert_dummy_session(&state, "active-agent");
            crate::state::tests_support::set_session_cwd(
                &state,
                "active-agent",
                &worktree.to_string_lossy(),
            );

            let result = merge_and_archive_worktree_impl(
                &state,
                repo.path().to_string_lossy().into_owned(),
                "active-feature".into(),
                "active-feature".into(),
                base_branch_of(repo.path()),
                action.into(),
                false,
            )
            .unwrap();

            assert_eq!(result.action, "needs_confirmation", "{action}");
            assert!(
                !result.merged,
                "{action} must wait for approval before merging"
            );
            assert_eq!(
                git_cmd(repo.path())
                    .args(["rev-parse", "HEAD"])
                    .run()
                    .unwrap()
                    .stdout,
                original_head
            );
            assert!(worktree.exists(), "{action} must leave the checkout intact");

            let fingerprint = inspect_worktree_removal(&state, repo.path(), "active-feature")
                .lifecycle
                .dirty_fingerprint
                .expect("confirmed checkout fingerprint");
            let confirmed = merge_and_archive_worktree_impl_with_confirmation(
                &state,
                repo.path().to_string_lossy().into_owned(),
                "active-feature".into(),
                "active-feature".into(),
                base_branch_of(repo.path()),
                action.into(),
                true,
                Some(&fingerprint),
            )
            .unwrap();
            assert_eq!(
                confirmed.action,
                if action == "archive" {
                    "archived"
                } else {
                    "deleted"
                }
            );
            assert!(!worktree.exists(), "{action} proceeds after confirmation");
        }
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
        let base = base_branch_of(repo.path());
        let wt = dirty_worktree_with(repo.path(), "feat-finalize-forced", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        // Finalize always follows the "ask" merge, as in the dialog flow. Without
        // it the branch keeps unmerged commits, and removal rightly refuses to
        // strand them before the confirmation is ever consulted.
        let pending = merge_and_archive_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-finalize-forced".to_string(),
            "feat-finalize-forced".to_string(),
            base,
            "ask".to_string(),
            false,
        )
        .expect("merge");
        assert_eq!(pending.action, "pending");

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
    fn forced_finalize_reports_that_an_unmerged_branch_was_kept() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "feat-finalize-unmerged", true);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "feat-finalize-unmerged".into(),
            "delete".into(),
            true,
        )
        .unwrap();

        assert_eq!(result.action, "deleted");
        assert!(!worktree.exists());
        let payload = serde_json::to_value(result).unwrap();
        assert!(
            payload["branch_delete_warning"]
                .as_str()
                .is_some_and(|w| w.contains("unmerged")),
            "{payload}"
        );
        assert!(
            git_cmd(repo.path())
                .args(["show-ref", "--verify", "refs/heads/feat-finalize-unmerged"])
                .run()
                .is_ok()
        );
    }

    #[test]
    fn finalize_leaves_a_clean_worktree_to_the_sweep() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        worktree_with(repo.path(), "feat-finalize-clean", true);
        git_cmd(repo.path())
            .args(["merge", "feat-finalize-clean", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
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
    // A worktree mid-rebase/merge/cherry-pick/revert/bisect must not be archived by the
    // *automatic* consequences of a merge (Merge & Archive, finalize-after-merge /
    // auto-archive-merged), even when it is otherwise clean and even with `force: true`
    // (there is no sensible unattended answer to "destroy my in-flight rebase"). The
    // delete path is already refused by `tuic_git`'s id-based removal.

    /// Plant a fake in-progress-rebase marker in a linked worktree's admin dir.
    fn mark_rebase_in_progress(wt: &Path) {
        let dot_git = fs::read_to_string(wt.join(".git")).expect("read worktree .git file");
        let admin = PathBuf::from(
            dot_git
                .trim()
                .strip_prefix("gitdir:")
                .expect("linked worktree gitdir")
                .trim(),
        );
        fs::create_dir_all(admin.join("rebase-merge")).expect("create rebase-merge marker");
    }

    #[test]
    fn merge_and_archive_refuses_a_worktree_with_operation_in_progress() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let wt = worktree_with(repo.path(), "feat-busy-merge", true);
        mark_rebase_in_progress(&wt);
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

        let Err(error) = res else {
            panic!("a busy worktree must refuse the cleanup");
        };
        assert!(error.contains("in progress"), "{error}");
        assert!(wt.exists(), "the worktree survives untouched");
    }

    #[test]
    fn finalize_archive_refuses_a_worktree_with_operation_in_progress() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let wt = worktree_with(repo.path(), "feat-busy-finalize", true);
        mark_rebase_in_progress(&wt);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let res = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().to_string(),
            "feat-busy-finalize".to_string(),
            "archive".to_string(),
            true,
        );

        let Err(error) = res else {
            panic!("a busy worktree must refuse the cleanup");
        };
        assert!(error.contains("in progress"), "{error}");
        assert!(wt.exists(), "the worktree survives untouched");
    }

    #[test]
    fn automatic_archive_keeps_a_clean_untouched_worktree() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "untouched-archive", false);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "untouched-archive".into(),
            "archive".into(),
            false,
        )
        .expect("unsafe automatic cleanup returns a review result");

        assert_eq!(result.action, "needs_confirmation");
        assert!(worktree.exists());
    }

    #[test]
    #[cfg(unix)]
    fn automatic_archive_keeps_a_merged_worktree_with_a_live_session() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "active-archive", true);
        git_cmd(repo.path())
            .args(["merge", "active-archive", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::state::tests_support::insert_dummy_session(&state, "active-agent");
        crate::state::tests_support::set_session_cwd(
            &state,
            "active-agent",
            &worktree.to_string_lossy(),
        );

        let result = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "active-archive".into(),
            "archive".into(),
            false,
        )
        .expect("unsafe automatic cleanup returns a review result");

        assert_eq!(result.action, "needs_confirmation");
        assert!(worktree.exists());
    }

    #[test]
    fn automatic_archive_leaves_a_locked_worktree_untouched() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let worktree = worktree_with(repo.path(), "feat-archive-locked", true);
        git_cmd(repo.path())
            .args(["merge", "feat-archive-locked", "--no-edit"])
            .run()
            .expect("merge before finalizing cleanup");
        git_cmd(repo.path())
            .args(["worktree", "lock", &worktree.to_string_lossy()])
            .run()
            .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());

        let error = finalize_merged_worktree_impl(
            &state,
            repo.path().to_string_lossy().into_owned(),
            "feat-archive-locked".into(),
            "archive".into(),
            false,
        )
        .err()
        .expect("locked checkout must not be archived");

        assert!(error.starts_with(tuic_git::worktree::LOCKED_WORKTREE_PREFIX));
        assert!(worktree.exists());
        let listed = git_cmd(repo.path())
            .args(["worktree", "list", "--porcelain"])
            .run()
            .unwrap()
            .stdout;
        #[cfg(unix)]
        let expected = format!(
            "{}/worktrees/feat-archive-locked",
            repo.path().canonicalize().unwrap().display()
        );
        #[cfg(windows)]
        let expected = format!(
            "{}/worktrees/feat-archive-locked",
            repo.path().display().to_string().replace('\\', "/")
        );
        assert!(listed.contains(&expected));
        assert!(
            listed
                .lines()
                .filter_map(|line| line.strip_prefix("worktree "))
                .any(|path| {
                    Path::new(path).canonicalize().unwrap() == worktree.canonicalize().unwrap()
                })
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
    fn merge_cleanup_entry_points_refuse_stale_force_confirmation() {
        let (_cfg, _guard) = isolated_config();
        let repo = setup_test_repo();
        let base = base_branch_of(repo.path());
        let worktree = dirty_worktree_with(repo.path(), "stale-merge-cleanup", false);
        let confirmed = dirty_fingerprint_at(&worktree).unwrap().0;
        fs::write(worktree.join("after-confirmation.txt"), "new work\n").unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.path().to_string_lossy().to_string();

        let finalize_error = finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path.clone(),
            "stale-merge-cleanup".into(),
            "delete".into(),
            true,
            Some(&confirmed),
        )
        .err()
        .expect("stale confirmation must be rejected");
        assert!(
            finalize_error.contains("changed since confirmation"),
            "{finalize_error}"
        );

        let merge_error = merge_and_archive_worktree_impl_with_confirmation(
            &state,
            repo_path,
            "stale-merge-cleanup".into(),
            "stale-merge-cleanup".into(),
            base,
            "archive".into(),
            true,
            Some(&confirmed),
        )
        .err()
        .expect("stale confirmation must be rejected");
        assert!(
            merge_error.contains("changed since confirmation"),
            "{merge_error}"
        );
        assert!(worktree.join("after-confirmation.txt").exists());
    }
    /// The live-session guard shared by IPC `remove_worktree`, HTTP
    /// `DELETE /worktrees/{id}` and MCP `repo worktree_remove` (wip 9586bf02c:
    /// a CLEAN worktree with a live session was deleted twice on 2026-08-26).
    #[cfg(unix)]
    mod workspace_removal_guard_tests {
        use super::*;
        use crate::state::tests_support::{
            insert_dummy_session, insert_session_running, make_test_app_state, set_session_cwd,
        };

        fn guard(state: &AppState, repo: &Path, id: &str, over: bool) -> Result<(), WorktreeBusy> {
            workspace_removal_guard(state, &repo.to_string_lossy(), id, over)
        }

        // Catches: a clean worktree whose only claim is a session's live cwd
        // (no registered terminal) being removable — the frontend's
        // `branch.terminals` misses exactly this session.
        #[test]
        fn refuses_a_clean_worktree_a_session_only_has_its_cwd_in() {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "busy", false);
            fs::create_dir_all(worktree.join("nested")).unwrap();
            let state = make_test_app_state();
            insert_dummy_session(&state, "pty-cwd");
            set_session_cwd(
                &state,
                "pty-cwd",
                &worktree.join("nested").to_string_lossy(),
            );

            let busy = guard(&state, repo.path(), "busy", false).expect_err("live cwd refuses");
            assert_eq!(busy.live_sessions.len(), 1);
            assert_eq!(busy.live_sessions[0].session_id, "pty-cwd");
            let message = busy.message();
            assert!(message.starts_with(BUSY_WORKTREE_PREFIX), "{message}");
            assert!(message.contains("1 live session(s)"), "{message}");
            assert!(message.contains("pty-cwd"), "{message}");
            assert_eq!(busy.to_json()["code"], "worktree_busy");
        }

        // Catches: matching only by cwd, so a terminal spawned into the
        // worktree that later `cd`'d out stops counting.
        #[test]
        fn refuses_a_worktree_a_session_was_spawned_into() {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "spawned", false);
            let state = make_test_app_state();
            insert_dummy_session(&state, "pty-registered");
            set_session_cwd(&state, "pty-registered", &repo.path().to_string_lossy());
            state
                .session_maps
                .sessions
                .get("pty-registered")
                .unwrap()
                .lock()
                .worktree = Some(WorktreeInfo {
                name: "spawned".into(),
                path: worktree.clone(),
                branch: Some("spawned".into()),
                base_repo: repo.path().to_path_buf(),
            });

            assert!(guard(&state, repo.path(), "spawned", false).is_err());
        }

        // Catches: the override not lifting the refusal (the user's explicit
        // "Delete anyway" must still work).
        #[test]
        fn override_busy_lifts_the_refusal() {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "override", false);
            let state = make_test_app_state();
            insert_dummy_session(&state, "pty-live");
            set_session_cwd(&state, "pty-live", &worktree.to_string_lossy());

            assert!(guard(&state, repo.path(), "override", false).is_err());
            guard(&state, repo.path(), "override", true).expect("override");
        }

        // Catches: a session whose process already exited still blocking the
        // removal while its registry entry waits to be torn down.
        #[test]
        fn an_exited_session_does_not_count() {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "exited", false);
            let state = make_test_app_state();
            insert_session_running(&state, "pty-done", "true");
            set_session_cwd(&state, "pty-done", &worktree.to_string_lossy());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !matches!(
                state
                    .session_maps
                    .sessions
                    .get("pty-done")
                    .unwrap()
                    .lock()
                    ._child
                    .try_wait(),
                Ok(Some(_))
            ) {
                assert!(std::time::Instant::now() < deadline, "child never exited");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }

            guard(&state, repo.path(), "exited", false).expect("exited session is not live");
        }

        // Catches: a session elsewhere in the repo (main checkout, a sibling)
        // or an unknown id blocking a removal it has nothing to do with.
        #[test]
        fn unrelated_sessions_and_unknown_ids_pass() {
            let repo = setup_test_repo();
            worktree_with(repo.path(), "free", false);
            let sibling = worktree_with(repo.path(), "free-2", false);
            let state = make_test_app_state();
            insert_dummy_session(&state, "pty-main");
            set_session_cwd(&state, "pty-main", &repo.path().to_string_lossy());
            insert_dummy_session(&state, "pty-sibling");
            set_session_cwd(&state, "pty-sibling", &sibling.to_string_lossy());

            guard(&state, repo.path(), "free", false).expect("nothing in this worktree");
            guard(&state, repo.path(), "no-such-workspace", false).expect("left to removal");
        }

        // Catches: the IPC command removing a live worktree (the guard must
        // run before git is asked to remove anything), and the override path
        // not reaching the removal.
        #[tokio::test]
        async fn ipc_remove_worktree_refuses_then_overrides() {
            let repo = setup_test_repo();
            let worktree = worktree_with(repo.path(), "ipc", false);
            let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
            let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
            let state = Arc::new(make_test_app_state());
            insert_dummy_session(&state, "pty-ipc");
            set_session_cwd(&state, "pty-ipc", &worktree.to_string_lossy());
            let repo_path = repo.path().to_string_lossy().into_owned();
            let remove = |over: Option<bool>| {
                remove_worktree_ipc_impl(
                    &state,
                    repo_path.clone(),
                    "ipc".into(),
                    Some(false),
                    None,
                    None,
                    None,
                    None,
                    over,
                )
            };

            let error = remove(None).await.expect_err("live session refuses");
            assert!(error.starts_with(BUSY_WORKTREE_PREFIX), "{error}");
            assert!(worktree.exists(), "nothing may be removed on refusal");

            remove(Some(true)).await.expect("override removes");
            assert!(!worktree.exists());
        }

        // Catches: the orphan path (desktop, MCP detached, HTTP
        // `/worktrees/orphan`) matching a registered session only when it
        // has no cwd at all — one spawned into the checkout that `cd`'d out
        // slipped past the shared check.
        #[test]
        fn orphan_check_counts_a_session_registered_to_the_checkout() {
            let repo = setup_test_repo();
            let linked = repo.path().join("linked");
            let out = std::process::Command::new("git")
                .current_dir(repo.path())
                .args(["worktree", "add", "--detach"])
                .arg(&linked)
                .arg("HEAD")
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
            let state = make_test_app_state();
            insert_dummy_session(&state, "pty-orphan");
            set_session_cwd(&state, "pty-orphan", &repo.path().to_string_lossy());
            state
                .session_maps
                .sessions
                .get("pty-orphan")
                .unwrap()
                .lock()
                .worktree = Some(WorktreeInfo {
                name: "linked".into(),
                path: linked.clone(),
                branch: None,
                base_repo: repo.path().to_path_buf(),
            });

            let repo_path = repo.path().to_string_lossy();
            let linked_path = linked.to_string_lossy();
            assert!(check_orphan_removal(&state, &repo_path, &linked_path, false, &[]).is_err());
            check_orphan_removal(
                &state,
                &repo_path,
                &linked_path,
                false,
                &["pty-orphan".to_string()],
            )
            .expect("reviewed session");
        }
    }

    #[cfg(unix)]
    mod orphan_removal_guard_critic {
        use super::*;
        use crate::state::tests_support::{insert_dummy_session, set_session_cwd};

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        fn guard(
            state: &AppState,
            repo: &Path,
            checkout: &Path,
            confirmed: &[&str],
        ) -> Result<(), String> {
            let confirmed: Vec<String> = confirmed.iter().map(|s| s.to_string()).collect();
            orphan_removal_guard(
                state,
                &repo.to_string_lossy(),
                &checkout.to_string_lossy(),
                false,
                &confirmed,
            )
        }

        // Catches: a session in a subdirectory of the checkout escaping the unreviewed-session
        // check on the confirmed path (exact cwd match instead of prefix).
        #[test]
        fn refuses_an_unreviewed_session_in_a_subdirectory() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::create_dir_all(linked.join("sub/deeper")).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(
                &state,
                "agent",
                &linked.join("sub/deeper").to_string_lossy(),
            );

            assert!(guard(&state, repo.path(), &linked, &[]).is_err());
            guard(&state, repo.path(), &linked, &["agent"]).expect("reviewed");
        }

        // Catches: a session in a sibling checkout whose name shares a string prefix
        // ("linked" vs "linked-2") being counted as inside this checkout.
        #[test]
        fn a_sibling_with_a_shared_name_prefix_is_not_unreviewed() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let sibling = detached(repo.path(), "linked-2");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &sibling.to_string_lossy());

            guard(&state, repo.path(), &linked, &[]).expect("sibling is outside");
        }

        // Catches: the refusal text hiding which session was not reviewed, so the caller cannot
        // act on a 400.
        #[test]
        fn the_refusal_names_the_unreviewed_session_and_not_the_reviewed_one() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            for (id, name) in [("seen", "Seen Agent"), ("late", "Late Agent")] {
                insert_dummy_session(&state, id);
                set_session_cwd(&state, id, &linked.to_string_lossy());
                state
                    .session_maps
                    .sessions
                    .get(id)
                    .unwrap()
                    .lock()
                    .display_name = Some(name.to_string());
            }

            let error = guard(&state, repo.path(), &linked, &["seen"]).unwrap_err();

            assert!(error.contains("Late Agent"), "{error}");
            assert!(!error.contains("Seen Agent"), "{error}");
        }

        // Catches: an already-removed checkout with stale sessions still refusing the
        // confirmed removal (idempotent retry must succeed).
        #[test]
        fn a_missing_checkout_never_blocks() {
            let repo = setup_test_repo();
            let gone = repo.path().join("gone");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &gone.to_string_lossy());

            guard(&state, repo.path(), &gone, &[]).expect("nothing left to protect");
        }
    }
    #[cfg(unix)]
    mod orphan_session_guard {
        use super::*;
        use crate::state::tests_support::{insert_dummy_session, set_session_cwd};

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        fn guard(state: &AppState, repo: &Path, checkout: &Path) -> Result<(), String> {
            orphan_cleanup_safety_with_sessions(
                state,
                &repo.to_string_lossy(),
                &checkout.to_string_lossy(),
            )
        }

        // Catches: an equality or exact-match cwd test, so an agent whose shell sits
        // in a subdirectory of the checkout is not seen and its checkout is removed.
        #[test]
        fn refuses_a_session_working_in_a_subdirectory_of_the_checkout() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::create_dir(linked.join("src")).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.join("src").to_string_lossy());

            let error = guard(&state, repo.path(), &linked).expect_err("live session");
            assert!(error.contains("live session"), "{error}");
        }

        // Catches: a string-prefix cwd test, so a session in `wt-2` blocks the
        // cleanup of `wt` for ever (or, inverted, `wt` hides a session of `wt-2`).
        #[test]
        fn ignores_a_sibling_directory_whose_name_extends_the_checkout_name() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "wt");
            let sibling = repo.path().join("wt-2");
            fs::create_dir(&sibling).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &sibling.to_string_lossy());

            guard(&state, repo.path(), &linked)
                .expect("sibling session is not inside the checkout");
        }

        // Catches: comparing raw cwd text, so a session that entered the checkout
        // through a symlink is invisible to the guard.
        #[test]
        fn refuses_a_session_whose_cwd_reaches_the_checkout_through_a_symlink() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let alias = repo.path().join("alias");
            std::os::unix::fs::symlink(&linked, &alias).unwrap();
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &alias.to_string_lossy());

            assert!(guard(&state, repo.path(), &linked).is_err());
        }

        // Catches: reading only `cwd`, so a session created for a worktree but with
        // no OSC 7 cwd report yet does not protect that worktree.
        #[test]
        fn falls_back_to_the_session_worktree_when_the_cwd_is_unknown() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            state
                .session_maps
                .sessions
                .get("agent")
                .unwrap()
                .lock()
                .worktree = Some(WorktreeInfo {
                name: "linked".into(),
                path: linked.clone(),
                branch: None,
                base_repo: repo.path().to_path_buf(),
            });

            assert!(guard(&state, repo.path(), &linked).is_err());
        }

        // Catches: treating a session with no location as "inside everything", which
        // would block every orphan cleanup while any such session exists.
        #[test]
        fn a_session_with_no_cwd_and_no_worktree_blocks_nothing() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");

            guard(&state, repo.path(), &linked).expect("no location, no claim on the checkout");
        }

        // Catches: a dead session left in the registry blocking the cleanup for
        // ever — the exit path must drop it from what the guard reads.
        #[test]
        fn a_session_that_exited_no_longer_blocks_the_cleanup() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.to_string_lossy());
            assert!(guard(&state, repo.path(), &linked).is_err());

            crate::pty::mark_session_exited("agent", &state);

            guard(&state, repo.path(), &linked).expect("exited session is gone");
        }

        // Catches: one live session flipping the verdict of every orphan in the
        // repo (a shared flag instead of a per-checkout check).
        #[test]
        fn assessment_marks_only_the_orphan_a_session_works_in() {
            let repo = setup_test_repo();
            let idle = detached(repo.path(), "idle");
            let busy = detached(repo.path(), "busy");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &busy.to_string_lossy());

            let rows = assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy())
                .unwrap();

            let row_of = |dir: &Path| {
                rows.iter()
                    .find(|row| Path::new(&row.assessment.path).ends_with(dir.file_name().unwrap()))
                    .expect("orphan listed")
            };
            assert!(row_of(&idle).assessment.safe);
            assert!(row_of(&idle).live_sessions.is_empty());
            assert!(!row_of(&busy).assessment.safe);
            assert_eq!(row_of(&busy).live_sessions.len(), 1);
        }

        // Catches: the live-session reason overwriting the git reason, so a dirty
        // checkout with a session in it hides the uncommitted work from the dialog.
        #[test]
        fn assessment_keeps_the_git_reason_next_to_the_live_session() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            fs::write(linked.join("scratch.txt"), "unsaved").unwrap();
            let git_reason =
                tuic_git::worktree::assess_orphan_worktrees(&repo.path().to_string_lossy())
                    .unwrap()
                    .remove(0)
                    .reason
                    .expect("untracked file is unsafe");
            let state = crate::state::tests_support::make_test_app_state();
            insert_dummy_session(&state, "agent");
            set_session_cwd(&state, "agent", &linked.to_string_lossy());

            let rows = assess_orphan_cleanup_with_sessions(&state, &repo.path().to_string_lossy())
                .unwrap();

            let reason = rows[0].assessment.reason.clone().unwrap();
            assert!(reason.contains(&git_reason), "{reason}");
            assert!(reason.contains("live session"), "{reason}");
        }
    }

    mod orphan_keep_settled_critic {
        use super::*;

        fn detached(repo: &Path, name: &str) -> PathBuf {
            let path = repo.join(name);
            let out = std::process::Command::new("git")
                .current_dir(repo)
                .args(["worktree", "add", "--detach"])
                .arg(&path)
                .arg("HEAD")
                .output()
                .expect("run git");
            assert!(out.status.success(), "{out:?}");
            path
        }

        // Catches (critic-1367): the settled filter applied only to a "remove" answer, so a
        // "keep" answer after a Keep still reports "changed while it was being answered".
        #[test]
        fn a_keep_answer_after_a_keep_reports_nothing_pending() {
            let state = crate::state::tests_support::make_test_app_state();
            pending_cleanup(&state, "/repo");
            clear_orphan_cleanup_internal(&state, "/repo", true);

            let error = answer_orphan_cleanup_internal(&state, "/repo", false).unwrap_err();

            assert_eq!(error, "No pending orphan cleanup for this repository");
        }

        // Catches (critic-1367): a new dialog inheriting the settled flag (or the old
        // Keep) from the entry it replaces, so after one Keep every later dialog is
        // unanswerable by agents and closes at once on the stale Keep.
        #[test]
        fn a_dialog_begun_after_a_keep_is_pending_and_answerable_again() {
            let repo = setup_test_repo();
            let linked = detached(repo.path(), "linked");
            let repo_path = repo.path().to_string_lossy().to_string();
            let paths = vec![linked.to_string_lossy().to_string()];
            let state = crate::state::tests_support::make_test_app_state();
            begin_orphan_cleanup_internal(&state, &repo_path, paths.clone()).unwrap();
            clear_orphan_cleanup_internal(&state, &repo_path, true);

            begin_orphan_cleanup_internal(&state, &repo_path, paths).unwrap();

            assert_eq!(pending_answer(&state, &repo_path), None);
            answer_orphan_cleanup_internal(&state, &repo_path, false)
                .expect("a fresh dialog is answerable");
            assert_eq!(pending_answer(&state, &repo_path), Some(false));
        }

        // Catches (critic-1367): a confirmed removal that settles the entry like a Keep, or
        // a Keep on a repo with no dialog creating a phantom entry that a later "remove"
        // answer would then trip over.
        #[test]
        fn clearing_without_a_dialog_leaves_nothing_to_answer() {
            let state = crate::state::tests_support::make_test_app_state();

            clear_orphan_cleanup_internal(&state, "/repo", true);
            clear_orphan_cleanup_internal(&state, "/repo", false);

            let error = answer_orphan_cleanup_internal(&state, "/repo", true).unwrap_err();
            assert_eq!(error, "No pending orphan cleanup for this repository");
            assert_eq!(pending_answer(&state, "/repo"), None);
        }
    }

    // --- TUIC_* env injection into Setup/Archive Scripts (script_env.rs,
    // passed to tuic-git's runner as data via `UserScript::env` /
    // `run_setup_script_with_env`). ---

    #[cfg(unix)]
    #[test]
    fn run_setup_script_sets_enriched_path() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();

        let result = run_setup_script("echo \"$PATH\"".to_string(), cwd).expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(
            result["stdout"].as_str().unwrap().trim(),
            crate::cli::enriched_path(),
            "run_setup_script should enrich PATH the same way git subprocesses already do"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_setup_script_injects_worktree_context() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "env-inject-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("env-inject-test".to_string()),
            create_branch: true,
        };
        let wt = create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        let cwd = wt.path.to_string_lossy().to_string();

        let result = run_setup_script(
            "echo \"$TUIC_MAIN_REPO_PATH|$TUIC_BRANCH|$TUIC_WORKTREE_NAME|$TUIC_IS_WORKTREE\""
                .to_string(),
            cwd,
        )
        .expect("should succeed");
        assert_eq!(result["exit_code"], 0);
        let stdout = result["stdout"].as_str().unwrap().trim();
        let parts: Vec<&str> = stdout.split('|').collect();
        assert_eq!(
            parts[0],
            repo.path().canonicalize().unwrap().to_string_lossy(),
            "TUIC_MAIN_REPO_PATH should be the main checkout: {stdout}"
        );
        assert_eq!(parts[1], "env-inject-test");
        assert_eq!(parts[2], "env-inject-test");
        assert_eq!(parts[3], "true");
    }

    /// RAII guard restoring an env var's prior value (or absence) on drop —
    /// runs even if the body panics.
    struct EnvVarGuard(&'static str, Option<String>);

    impl EnvVarGuard {
        fn unset(key: &'static str) -> Self {
            let guard = EnvVarGuard(key, std::env::var(key).ok());
            unsafe { std::env::remove_var(key) };
            guard
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.1.take() {
                Some(previous) => unsafe { std::env::set_var(self.0, previous) },
                None => unsafe { std::env::remove_var(self.0) },
            }
        }
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn run_setup_script_does_not_set_unknown_vars() {
        // TUIC_BRANCH is process-global env state, and this test may itself
        // be running inside a live TUIC-hosted terminal (which sets it
        // ambiently for its own child processes) — Setup/Archive Scripts get
        // full parent-env inheritance, so an ambient TUIC_BRANCH would
        // otherwise leak straight through and falsely read as "SET". Clear it
        // for the duration of this test only.
        let _env_guard = EnvVarGuard::unset("TUIC_BRANCH");

        // Detached HEAD: TUIC_BRANCH should be entirely absent, not empty.
        let repo = setup_test_repo();
        let sha = git_cmd(repo.path())
            .args(["rev-parse", "HEAD"])
            .run()
            .expect("rev-parse")
            .stdout;
        let sha = sha.trim();
        git_cmd(repo.path())
            .args(["checkout", sha])
            .run()
            .expect("checkout detached");

        let cwd = repo.path().to_string_lossy().to_string();
        let result = run_setup_script(
            "if [ -z \"${TUIC_BRANCH+x}\" ]; then echo UNSET; else echo \"SET:$TUIC_BRANCH\"; fi"
                .to_string(),
            cwd,
        )
        .expect("should succeed");
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "UNSET");
    }

    #[cfg(unix)]
    #[test]
    fn run_setup_script_reports_script_kind_setup() {
        let dir = TempDir::new().expect("temp dir");
        let cwd = dir.path().to_string_lossy().to_string();
        let result = run_setup_script("echo \"$TUIC_SCRIPT_KIND\"".to_string(), cwd)
            .expect("should succeed");
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "setup");
    }

    #[cfg(unix)]
    #[test]
    fn archive_script_receives_the_worktree_being_archived() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "archive-env-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("archive-env-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        // Marker written outside the worktree so it survives the archive move.
        let marker = repo.path().join("archive-env-marker.txt");
        let script = format!(
            "echo \"$TUIC_SCRIPT_KIND|$TUIC_WORKTREE_NAME|$TUIC_MAIN_REPO_PATH\" > {}",
            marker.display()
        );
        let result = archive_worktree(repo.path(), "archive-env-test", Some(&script));
        assert!(result.is_ok(), "archive should succeed: {:?}", result);

        let content = fs::read_to_string(&marker).expect("read marker");
        let parts: Vec<&str> = content.trim().split('|').collect();
        assert_eq!(parts[0], "archive");
        assert_eq!(parts[1], "archive-env-test");
        assert_eq!(
            parts[2],
            repo.path().canonicalize().unwrap().to_string_lossy()
        );
    }

    #[cfg(unix)]
    #[test]
    fn delete_archive_script_receives_its_worktree_context() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = WorktreeConfig {
            task_name: "delete-env-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("delete-env-test".to_string()),
            create_branch: true,
        };
        create_worktree_internal(&worktrees_dir, &config, None).expect("create worktree");
        let marker = repo.path().join("delete-env-marker.txt");
        let script = format!(
            "echo \"$TUIC_SCRIPT_KIND|$TUIC_WORKTREE_NAME\" > {}",
            marker.display()
        );
        remove_worktree_by_workspace_id_with_confirmation(
            &repo.path().to_string_lossy(),
            "delete-env-test",
            false,
            Some(&script),
            false,
            false,
            None,
        )
        .expect("remove should succeed");

        let content = fs::read_to_string(&marker).expect("read marker");
        assert_eq!(content.trim(), "archive|delete-env-test");
    }
}
