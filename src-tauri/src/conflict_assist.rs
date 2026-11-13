//! Conflict-resolution assist for pull requests.
//!
//! Creates a worktree on a PR's head branch, rebases it onto the PR base, and
//! reports whether the rebase was clean or produced conflicts. On conflicts it
//! returns the conflicted-file list plus a ready-to-inject agent prompt; the
//! frontend spawns an agent PTY in the worktree and seeds it via `sendCommand`.
//! Nothing is ever pushed or merged automatically — the push is a separate,
//! human-gated action in the UI.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

/// Outcome of a conflict-assist run.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConflictAssistResult {
    /// `"clean"`, `"clean_unverified"`, or `"conflicts"`.
    pub status: String,
    pub worktree_path: String,
    /// The PR head branch that was rebased.
    pub branch: String,
    /// The base branch it was rebased onto.
    pub base: String,
    /// Remote the head branch was fetched from (the fork's, for a PR opened against an upstream).
    pub head_remote: String,
    /// Remote the base branch was fetched from and rebased onto.
    pub base_remote: String,
    /// `"fetched_remote"`, `"existing_tracking"`, or `"local_fallback"`.
    pub base_source: String,
    /// Present when the selected base could not be refreshed from its remote.
    pub base_warning: Option<String>,
    pub conflicted_files: Vec<String>,
    /// Agent prompt to resolve the conflicts — empty unless `status == "conflicts"`.
    pub prompt: String,
}

/// Dual-emit the conflict-assist lifecycle event (desktop window + event bus).
///
/// Only the window emit is gated on the desktop feature — the same shape as
/// `remote_runtime::publish`. `POST /repo/conflict-assist` is mounted
/// unconditionally, so the headless build runs this lifecycle for a browser
/// client that has nothing but `/events` to follow it on.
fn emit_conflict_assist_status(
    state: &crate::AppState,
    repo_path: &str,
    pr_number: i64,
    status: &str,
    conflicted_files: &[String],
) {
    let payload = serde_json::json!({
        "pr_number": pr_number,
        "status": status,
        "conflicted_files": conflicted_files,
    });
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().clone() {
        use tauri::Emitter;
        let _ = app.emit(
            "conflict-assist-status",
            serde_json::json!({ "repo_path": repo_path, "payload": payload }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::ConflictAssistStatus {
            repo_path: repo_path.to_string(),
            payload,
        });
}

/// Fetch the PR base branch and decide what `git rebase` should target.
///
/// Rebasing onto the bare branch name resolves against the local `refs/heads/<base>`,
/// which may be days stale — the rebase then comes back `clean` against an old base and
/// the user pushes a PR that actually conflicts. Mirrors `local_pr_diff` in `github.rs`:
/// fetch `+refs/heads/<base>:refs/remotes/origin/<base>` and rebase the remote-tracking
/// ref, so "clean" means clean against what origin has right now.
///
/// Falls back to the bare name when no remote-tracking ref exists afterwards — a
/// local-only base branch or an unreachable origin should still get a (clearly
/// local) answer rather than a hard failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaseSource {
    FetchedRemote,
    ExistingTracking,
    LocalFallback,
}

impl BaseSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::FetchedRemote => "fetched_remote",
            Self::ExistingTracking => "existing_tracking",
            Self::LocalFallback => "local_fallback",
        }
    }

    fn warning(self, remote: &str, base_ref: &str) -> Option<String> {
        match self {
            Self::FetchedRemote => None,
            Self::ExistingTracking => Some(format!(
                "Could not refresh {remote}/{base_ref}; conflict result uses the existing remote-tracking ref."
            )),
            Self::LocalFallback => Some(format!(
                "Could not refresh {remote}/{base_ref} and no remote-tracking ref exists; conflict result uses the local {base_ref} branch."
            )),
        }
    }
}

#[derive(Debug)]
struct ResolvedBase {
    target: String,
    source: BaseSource,
}

fn ref_exists(repo: &Path, reference: &str) -> bool {
    crate::git_cli::git_cmd(repo)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ])
        .run()
        .is_ok()
}

/// Decide what `git rebase` should target for `base_ref`, refreshing it first.
///
/// `fetch_timeout` is a parameter rather than a constant because a fetch that runs
/// out of time is indistinguishable here from a fetch that failed: both fall through
/// to the remote-tracking ref, and the caller is told `ExistingTracking`. Production
/// passes `crate::git_cli::FETCH_TIMEOUT`; the tests pass a bound they cannot hit, so
/// a slow machine cannot quietly turn a fetch that worked into one that "failed".
fn resolve_rebase_target(
    repo: &Path,
    remote: &str,
    base_ref: &str,
    fetch_timeout: Duration,
) -> Result<ResolvedBase, String> {
    let remote_ref = format!("refs/remotes/{remote}/{base_ref}");
    let refspec = format!("+refs/heads/{base_ref}:{remote_ref}");
    let fetch = crate::git_cli::git_cmd(repo)
        .timeout(fetch_timeout)
        .args(["fetch", "--no-tags", remote, &refspec])
        .run();

    if fetch.is_ok() && ref_exists(repo, &remote_ref) {
        return Ok(ResolvedBase {
            target: remote_ref,
            source: BaseSource::FetchedRemote,
        });
    }
    if ref_exists(repo, &remote_ref) {
        return Ok(ResolvedBase {
            target: remote_ref,
            source: BaseSource::ExistingTracking,
        });
    }
    if ref_exists(repo, base_ref) {
        return Ok(ResolvedBase {
            target: base_ref.to_string(),
            source: BaseSource::LocalFallback,
        });
    }

    let fetch_error = fetch
        .err()
        .map(|error| format!(": {error}"))
        .unwrap_or_default();
    Err(format!(
        "Could not resolve PR base {base_ref} from {remote}, remote tracking, or a local branch{fetch_error}"
    ))
}

/// Rebase `worktree` onto `rebase_target` and report what happened.
///
/// Split out from the command so the three outcomes — clean, conflicted, and failed for
/// some other reason — are reachable from tests against a fixture repo without any
/// network or PR plumbing.
fn rebase_and_collect_conflicts(
    worktree: &Path,
    rebase_target: &str,
) -> (bool, Vec<String>, String) {
    // `run_raw` so a conflicting rebase (non-zero exit) is inspected rather than
    // treated as a hard error.
    let rebase = crate::git_cli::git_cmd(worktree)
        .args(["rebase", "--", rebase_target])
        .run_raw();
    let (rebase_ok, rebase_stderr) = match rebase {
        Ok(out) => (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ),
        Err(e) => (false, format!("git rebase failed to start: {e}")),
    };

    let status_out = crate::git_cli::git_cmd(worktree)
        .args(["status", "--porcelain"])
        .run()
        .map(|o| o.stdout)
        .unwrap_or_default();
    let conflicted = crate::git_cli::parse_conflicted_files_porcelain(&status_out);

    (rebase_ok, conflicted, rebase_stderr)
}

/// Which git remotes a conflict-assist run reads from.
///
/// For a PR inside one repository both are the same remote. For a fork's PR opened against
/// an upstream they differ: the head branch lives in the fork (usually `origin`) and the
/// base branch in the upstream (`upstream`), so the head is fetched from one and rebased
/// onto the other.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemotePlan {
    head_remote: String,
    base_remote: String,
}

/// Decide the remotes for a PR from the checkout's `(name, url)` remotes and the PR's refs.
///
/// A PR whose head repository is not a remote of this checkout (someone else's fork) is
/// refused: there is nothing local to rebase and push back to.
fn plan_remotes(
    remotes: &[(String, String)],
    host: &crate::github_account::GitHubHost,
    base: (&str, &str),
    refs: &crate::github::PrRefs,
) -> Result<RemotePlan, String> {
    let base_remote = crate::github::remote_name_in(remotes, host, base.0, base.1);
    if !refs.head_from_fork {
        // Same repository on both sides. An unparseable origin URL (an SSH host alias)
        // still behaves as before: the repo was bound by hand, and origin is the remote.
        let remote = base_remote.unwrap_or_else(|| "origin".to_string());
        return Ok(RemotePlan {
            head_remote: remote.clone(),
            base_remote: remote,
        });
    }
    let Some(base_remote) = base_remote else {
        return Err(format!(
            "Conflict assist needs a git remote for {}/{}, where this PR is merged, to rebase onto its base branch.",
            base.0, base.1
        ));
    };
    let head_remote = refs
        .head_repo
        .as_deref()
        .and_then(|repo| repo.split_once('/'))
        .and_then(|(owner, name)| crate::github::remote_name_in(remotes, host, owner, name));
    match head_remote {
        Some(head_remote) => Ok(RemotePlan {
            head_remote,
            base_remote,
        }),
        None => Err(format!(
            "Conflict assist can't rebase this PR: its head branch lives in {}, which is not a git remote of this checkout.",
            refs.head_repo
                .as_deref()
                .unwrap_or("a repository GitHub no longer has")
        )),
    }
}

/// Everything the blocking git side of a run produces.
struct PreparedRebase {
    worktree_path: String,
    conflicted: Vec<String>,
    rebase_ok: bool,
    rebase_stderr: String,
    base_source: BaseSource,
}

/// Fetch the head, create its worktree, rebase it onto the freshest base and collect the
/// conflicts. All blocking git, kept apart from the GitHub lookups so it can be tested
/// against real repositories.
fn prepare_rebase(
    repo_path: &str,
    worktrees_dir: &Path,
    plan: &RemotePlan,
    head_ref: &str,
    base_ref: &str,
    pr_number: i64,
    fetch_timeout: Duration,
) -> Result<PreparedRebase, String> {
    // Make the head branch available locally. Best-effort: it may already be present, or
    // the remote may be unreachable for a local-only branch.
    let _ = crate::git_cli::git_cmd(Path::new(repo_path))
        .timeout(fetch_timeout)
        .args(["fetch", &plan.head_remote, "--", head_ref])
        .run_silent();

    let config = crate::worktree::WorktreeConfig {
        task_name: format!("conflict-pr-{pr_number}"),
        base_repo: repo_path.to_string(),
        branch: Some(head_ref.to_string()),
        create_branch: false,
    };
    let wt = crate::worktree::create_worktree_with_stale_recovery(worktrees_dir, &config, None)?;
    let wt_path = wt.path.clone();

    // Rebase onto what the base remote has now, not a possibly-stale local branch.
    let resolved_base = resolve_rebase_target(
        Path::new(repo_path),
        &plan.base_remote,
        base_ref,
        fetch_timeout,
    )?;
    let (rebase_ok, conflicted, rebase_stderr) =
        rebase_and_collect_conflicts(&wt_path, &resolved_base.target);

    Ok(PreparedRebase {
        worktree_path: wt_path.to_string_lossy().to_string(),
        conflicted,
        rebase_ok,
        rebase_stderr,
        base_source: resolved_base.source,
    })
}

/// `base_repo` is the `owner/name` the PR targets (`BranchPrStatus.base_repo`); it keeps a PR
/// number shared by a fork and its upstream unambiguous.
pub(crate) async fn start_conflict_assist_impl(
    repo_path: String,
    pr_number: i64,
    base_repo: Option<&str>,
    state: &crate::AppState,
) -> Result<ConflictAssistResult, String> {
    let refs = crate::github::get_pr_refs_targeted(&repo_path, pr_number, base_repo, state).await?;
    let (account, _token, base_owner, base_name) =
        crate::github::resolve_repo_for_pr(state, &repo_path, pr_number, base_repo).await?;
    let remotes = crate::git::list_remotes(Path::new(&repo_path));
    let plan = plan_remotes(&remotes, &account.host, (&base_owner, &base_name), &refs)?;

    let worktrees_dir =
        crate::worktree::resolve_worktree_dir_for_repo(Path::new(&repo_path), &state.worktrees_dir);

    let head_ref = refs.head_ref.clone();
    let base_ref = refs.base_ref.clone();
    let repo_path_for_git = repo_path.clone();
    let plan_for_git = plan.clone();

    // All blocking git work (fetch, worktree add, rebase, status) runs off the
    // async executor so a slow checkout/rebase doesn't stall other commands.
    let base_ref_for_git = base_ref.clone();
    let PreparedRebase {
        worktree_path,
        conflicted,
        rebase_ok,
        rebase_stderr,
        base_source,
    } = tokio::task::spawn_blocking(move || {
        prepare_rebase(
            &repo_path_for_git,
            &worktrees_dir,
            &plan_for_git,
            &head_ref,
            &base_ref_for_git,
            pr_number,
            crate::git_cli::FETCH_TIMEOUT,
        )
    })
    .await
    .map_err(|e| format!("conflict-assist task panic: {e}"))??;

    // A non-zero rebase with no unmerged files isn't a conflict — it failed for
    // another reason (missing base ref, dirty tree, …). Surface it instead of
    // reporting a phantom "conflicts" state with an empty file list.
    if !rebase_ok && conflicted.is_empty() {
        // Abort before returning: git may have stopped mid-rebase (detached HEAD,
        // rebase-merge state on disk), and leaving it that way strands the worktree
        // so every later run fails with "a rebase is already in progress". Same
        // recovery the merge/rebase paths in git.rs and worktree.rs use.
        return Err(crate::git_cli::finish_failed_git_operation_after_abort(
            Path::new(&worktree_path),
            "rebase",
            &format!("Rebase onto {base_ref} failed (not a conflict)"),
            rebase_stderr.trim(),
        ));
    }

    let base_warning = base_source.warning(&plan.base_remote, &base_ref);
    let (status, prompt) = if conflicted.is_empty() {
        (
            if base_source == BaseSource::FetchedRemote {
                "clean"
            } else {
                "clean_unverified"
            }
            .to_string(),
            String::new(),
        )
    } else {
        (
            "conflicts".to_string(),
            crate::git_cli::build_conflict_assist_prompt(pr_number, &base_ref, &conflicted),
        )
    };

    emit_conflict_assist_status(state, &repo_path, pr_number, &status, &conflicted);

    Ok(ConflictAssistResult {
        status,
        worktree_path,
        branch: refs.head_ref,
        base: refs.base_ref,
        head_remote: plan.head_remote,
        base_remote: plan.base_remote,
        base_source: base_source.as_str().to_string(),
        base_warning,
        conflicted_files: conflicted,
        prompt,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn start_conflict_assist(
    repo_path: String,
    pr_number: i64,
    base_repo: Option<String>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<ConflictAssistResult, String> {
    let state = state.inner().clone();
    start_conflict_assist_impl(repo_path, pr_number, base_repo.as_deref(), &state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    /// The third of the three unconditionally-mounted repo routes, held to the
    /// same rule: the window emit belongs to the desktop, the bus send belongs
    /// to every build. A headless client watching `/events` is the only reader
    /// this event has.
    #[test]
    fn conflict_assist_status_reaches_the_bus_on_every_build() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();

        emit_conflict_assist_status(&state, "/repo", 7, "conflicts", &["src/lib.rs".to_string()]);

        match events
            .try_recv()
            .expect("the conflict-assist status reaches the event bus")
        {
            crate::state::AppEvent::ConflictAssistStatus { repo_path, payload } => {
                assert_eq!(repo_path, "/repo");
                assert_eq!(payload["pr_number"], 7);
                assert_eq!(payload["status"], "conflicts");
            }
            other => panic!("expected ConflictAssistStatus, got {other:?}"),
        }
    }

    /// Fetch bound for the tests below.
    ///
    /// Every fetch these tests trigger is local — a fixture clone reading its own
    /// origin off the filesystem, never a network. `FETCH_TIMEOUT` is 5s under
    /// `cfg(test)`, which measures nothing here and can only be lost: when a loaded
    /// machine stretched one local fetch past 5s, `resolve_rebase_target` reported
    /// `ExistingTracking`, and a test about which base ref wins failed over how fast
    /// git had been. The bound below is one no local fetch can plausibly reach, so a
    /// real hang is caught by nextest's slow-timeout instead — which is the right
    /// place for it, because a hang is not this module's subject either.
    const TEST_FETCH_TIMEOUT: Duration = Duration::from_secs(120);

    fn git(repo: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .expect("git")
    }

    fn write(repo: &Path, name: &str, body: &str) {
        std::fs::write(repo.join(name), body).expect("write");
    }

    /// A repo on `main` with one commit, plus a `feature` branch forked from it.
    /// Both branches are left ready for the caller to add divergent commits.
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        git(&path, &["init", "--initial-branch=main"]);
        git(&path, &["config", "user.email", "test@test.com"]);
        git(&path, &["config", "user.name", "Test"]);
        write(&path, "shared.txt", "base\n");
        git(&path, &["add", "."]);
        git(&path, &["commit", "-m", "base"]);
        git(&path, &["branch", "feature"]);
        (dir, path)
    }

    /// Advance `branch` with a commit touching `file`.
    fn commit_on(repo: &Path, branch: &str, file: &str, body: &str, msg: &str) {
        git(repo, &["checkout", branch]);
        write(repo, file, body);
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", msg]);
    }

    #[test]
    fn clean_rebase_reports_no_conflicts() {
        let (_d, repo) = fixture();
        // main and feature touch different files — nothing to collide.
        commit_on(&repo, "main", "on_main.txt", "main\n", "main work");
        commit_on(
            &repo,
            "feature",
            "on_feature.txt",
            "feature\n",
            "feature work",
        );

        let (ok, conflicted, stderr) = rebase_and_collect_conflicts(&repo, "main");
        assert!(ok, "clean rebase must succeed: {stderr}");
        assert!(
            conflicted.is_empty(),
            "unexpected conflicts: {conflicted:?}"
        );
    }

    #[test]
    fn conflicting_rebase_reports_the_conflicted_file() {
        let (_d, repo) = fixture();
        // Both sides edit the same line of the same file.
        commit_on(&repo, "main", "shared.txt", "from main\n", "main edit");
        commit_on(
            &repo,
            "feature",
            "shared.txt",
            "from feature\n",
            "feature edit",
        );

        let (ok, conflicted, _) = rebase_and_collect_conflicts(&repo, "main");
        assert!(!ok, "a conflicting rebase must not report success");
        assert_eq!(conflicted, vec!["shared.txt"]);
    }

    /// The case the abort exists for: the rebase fails without producing any
    /// unmerged file, so the caller cannot tell it apart from a conflict by the
    /// file list alone and must not leave the repo mid-rebase.
    #[test]
    fn rebase_failure_without_conflicts_yields_empty_file_list() {
        let (_d, repo) = fixture();
        commit_on(
            &repo,
            "feature",
            "on_feature.txt",
            "feature\n",
            "feature work",
        );

        let (ok, conflicted, stderr) =
            rebase_and_collect_conflicts(&repo, "refs/remotes/origin/nope");
        assert!(!ok, "rebase onto a missing ref must fail");
        assert!(
            conflicted.is_empty(),
            "a missing base is not a conflict, got {conflicted:?}"
        );
        assert!(!stderr.trim().is_empty(), "the failure must carry a reason");
    }

    /// After a failed rebase the abort must leave the worktree usable — otherwise
    /// every later conflict-assist run dies with "a rebase is already in progress".
    #[test]
    fn abort_after_a_failed_rebase_leaves_the_worktree_clean() {
        let (_d, repo) = fixture();
        commit_on(&repo, "main", "shared.txt", "from main\n", "main edit");
        commit_on(
            &repo,
            "feature",
            "shared.txt",
            "from feature\n",
            "feature edit",
        );

        let (ok, conflicted, _) = rebase_and_collect_conflicts(&repo, "main");
        assert!(
            !ok && !conflicted.is_empty(),
            "precondition: conflicted rebase"
        );

        let msg = crate::git_cli::finish_failed_git_operation_after_abort(
            &repo,
            "rebase",
            "Rebase failed",
            "boom",
        );
        assert!(msg.contains("aborted"), "abort must be reported: {msg}");

        let status = git(&repo, &["status", "--porcelain"]);
        assert!(
            String::from_utf8_lossy(&status.stdout).trim().is_empty(),
            "worktree must be clean after abort"
        );
        // And a fresh rebase must be startable again.
        let (_, _, stderr) = rebase_and_collect_conflicts(&repo, "main");
        assert!(
            !stderr.contains("already in progress"),
            "repo left mid-rebase: {stderr}"
        );
    }

    /// No origin configured: the fetch fails, no remote-tracking ref appears, and the
    /// bare branch name must still be usable rather than a ref that does not exist.
    #[test]
    fn rebase_target_falls_back_to_the_local_branch_without_a_remote() {
        let (_d, repo) = fixture();
        let resolved = resolve_rebase_target(&repo, "origin", "main", TEST_FETCH_TIMEOUT)
            .expect("local fallback");
        assert_eq!(resolved.target, "main");
        assert_eq!(resolved.source, BaseSource::LocalFallback);
    }

    /// With a reachable origin the remote-tracking ref wins — that is the whole point:
    /// "clean" must mean clean against what origin has now, not a stale local branch.
    #[test]
    fn rebase_target_prefers_the_remote_tracking_ref() {
        let (_origin_dir, origin) = fixture();
        let clone_dir = tempfile::tempdir().expect("tempdir");
        let clone = clone_dir.path().join("clone");
        let out = Command::new("git")
            .args(["clone", origin.to_str().unwrap(), clone.to_str().unwrap()])
            .output()
            .expect("git clone");
        assert!(out.status.success(), "clone failed");
        git(&clone, &["config", "user.email", "test@test.com"]);
        git(&clone, &["config", "user.name", "Test"]);

        // origin moves ahead after the clone — the local refs/heads/main is now stale.
        commit_on(&origin, "main", "on_main.txt", "main\n", "main work");

        let resolved = resolve_rebase_target(&clone, "origin", "main", TEST_FETCH_TIMEOUT)
            .expect("fetched remote");
        assert_eq!(resolved.target, "refs/remotes/origin/main");
        assert_eq!(resolved.source, BaseSource::FetchedRemote);
        // And the fetch actually advanced it, so a rebase would see the new commit.
        let rev = git(&clone, &["rev-parse", "refs/remotes/origin/main"]);
        let local = git(&clone, &["rev-parse", "refs/heads/main"]);
        assert_ne!(
            String::from_utf8_lossy(&rev.stdout),
            String::from_utf8_lossy(&local.stdout),
            "remote-tracking ref must be ahead of the stale local branch"
        );
    }

    #[test]
    fn rebase_target_uses_existing_tracking_ref_when_fetch_fails() {
        let (_d, repo) = fixture();
        git(
            &repo,
            &["update-ref", "refs/remotes/origin/main", "refs/heads/main"],
        );

        let resolved = resolve_rebase_target(&repo, "origin", "main", TEST_FETCH_TIMEOUT)
            .expect("existing tracking ref");
        assert_eq!(resolved.target, "refs/remotes/origin/main");
        assert_eq!(resolved.source, BaseSource::ExistingTracking);
        assert!(resolved.source.warning("origin", "main").is_some());
    }

    #[test]
    fn rebase_target_rejects_a_missing_remote_and_local_ref() {
        let (_d, repo) = fixture();
        let error = resolve_rebase_target(&repo, "origin", "missing", TEST_FETCH_TIMEOUT)
            .expect_err("missing base must fail");
        assert!(
            error.contains("Could not resolve PR base missing"),
            "{error}"
        );
    }

    // ---- remote planning --------------------------------------------------------------

    fn refs(
        head_from_fork: bool,
        head_repo: Option<&str>,
        base_repo: &str,
    ) -> crate::github::PrRefs {
        crate::github::PrRefs {
            head_ref: "feature".into(),
            base_ref: "main".into(),
            head_sha: None,
            base_sha: None,
            head_from_fork,
            head_repo: head_repo.map(str::to_string),
            base_repo: Some(base_repo.into()),
        }
    }

    fn remotes(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(n, u)| (n.to_string(), u.to_string()))
            .collect()
    }

    fn host() -> crate::github_account::GitHubHost {
        crate::github_account::parse_remote_url("git@github.com:o/r.git")
            .expect("github.com remote")
            .0
    }

    const FORK_URL: &str = "git@github.com:me/hello.git";
    const UPSTREAM_URL: &str = "https://github.com/org/hello.git";

    // Catches: a PR inside one repository changing behaviour (it must keep using one remote).
    #[test]
    fn a_pr_within_one_repository_uses_one_remote() {
        let plan = plan_remotes(
            &remotes(&[("origin", FORK_URL)]),
            &host(),
            ("me", "hello"),
            &refs(false, Some("me/hello"), "me/hello"),
        )
        .unwrap();
        assert_eq!(
            plan,
            RemotePlan {
                head_remote: "origin".into(),
                base_remote: "origin".into()
            }
        );
    }

    // Catches: a hand-bound repo whose origin URL is an SSH alias losing conflict assist.
    #[test]
    fn an_unparseable_origin_still_means_origin_for_a_same_repo_pr() {
        let plan = plan_remotes(
            &remotes(&[("origin", "git@personal:me/hello.git")]),
            &host(),
            ("me", "hello"),
            &refs(false, None, "me/hello"),
        )
        .unwrap();
        assert_eq!(
            (plan.head_remote.as_str(), plan.base_remote.as_str()),
            ("origin", "origin")
        );
    }

    // Catches: the headline case — a fork PR against its upstream — being refused again.
    #[test]
    fn a_fork_pr_fetches_its_head_from_the_fork_and_its_base_from_the_upstream() {
        let plan = plan_remotes(
            &remotes(&[("origin", FORK_URL), ("upstream", UPSTREAM_URL)]),
            &host(),
            ("org", "hello"),
            &refs(true, Some("me/hello"), "org/hello"),
        )
        .unwrap();
        assert_eq!(
            plan,
            RemotePlan {
                head_remote: "origin".into(),
                base_remote: "upstream".into()
            }
        );
        // Owner/name case is irrelevant, and the remote's name is not assumed.
        let plan = plan_remotes(
            &remotes(&[("mine", FORK_URL), ("canon", UPSTREAM_URL)]),
            &host(),
            ("ORG", "Hello"),
            &refs(true, Some("Me/Hello"), "org/hello"),
        )
        .unwrap();
        assert_eq!(
            (plan.head_remote.as_str(), plan.base_remote.as_str()),
            ("mine", "canon")
        );
    }

    // Catches: rebasing someone else's fork branch we cannot push back to.
    #[test]
    fn a_head_in_a_repository_that_is_not_a_remote_is_refused_with_its_name() {
        let err = plan_remotes(
            &remotes(&[("origin", FORK_URL), ("upstream", UPSTREAM_URL)]),
            &host(),
            ("org", "hello"),
            &refs(true, Some("stranger/hello"), "org/hello"),
        )
        .unwrap_err();
        assert!(
            err.contains("stranger/hello") && err.contains("not a git remote"),
            "{err}"
        );
        let err = plan_remotes(
            &remotes(&[("origin", FORK_URL), ("upstream", UPSTREAM_URL)]),
            &host(),
            ("org", "hello"),
            &refs(true, None, "org/hello"),
        )
        .unwrap_err();
        assert!(err.contains("no longer has"), "{err}");
    }

    // Catches: a fork PR with no remote for the repository it merges into rebasing onto nothing.
    #[test]
    fn a_fork_pr_without_a_remote_for_its_base_repository_is_refused() {
        let err = plan_remotes(
            &remotes(&[("origin", FORK_URL)]),
            &host(),
            ("org", "hello"),
            &refs(true, Some("me/hello"), "org/hello"),
        )
        .unwrap_err();
        assert!(
            err.contains("org/hello") && err.contains("needs a git remote"),
            "{err}"
        );
    }

    // ---- the git flow, against real repositories ---------------------------------------

    /// `origin` = the fork (feature edits shared.txt), `upstream` = the repo the PR targets
    /// (its main later edits the same line). Returns the work checkout cloned from the fork.
    fn fork_checkout() -> (tempfile::TempDir, PathBuf, tempfile::TempDir) {
        let (_seed_dir, seed) = fixture();
        commit_on(
            &seed,
            "feature",
            "shared.txt",
            "from feature\n",
            "feature edit",
        );
        let origin_dir = tempfile::tempdir().expect("tempdir");
        let upstream_dir = tempfile::tempdir().expect("tempdir");
        for dir in [&origin_dir, &upstream_dir] {
            git(dir.path(), &["init", "--bare", "--initial-branch=main"]);
        }
        // The fork has the base main plus the PR branch.
        git(
            &seed,
            &[
                "push",
                origin_dir.path().to_str().unwrap(),
                "main",
                "feature",
            ],
        );
        // The upstream's main moves on and touches the same line.
        commit_on(
            &seed,
            "main",
            "shared.txt",
            "from upstream main\n",
            "upstream edit",
        );
        git(
            &seed,
            &["push", upstream_dir.path().to_str().unwrap(), "main"],
        );

        let work_dir = tempfile::tempdir().expect("tempdir");
        let work = work_dir.path().join("work");
        let out = Command::new("git")
            .args([
                "clone",
                origin_dir.path().to_str().unwrap(),
                work.to_str().unwrap(),
            ])
            .output()
            .expect("git clone");
        assert!(out.status.success(), "clone failed");
        git(&work, &["config", "user.email", "test@test.com"]);
        git(&work, &["config", "user.name", "Test"]);
        git(
            &work,
            &[
                "remote",
                "add",
                "upstream",
                upstream_dir.path().to_str().unwrap(),
            ],
        );
        // Keep the bare repos alive for the test's duration.
        std::mem::forget(origin_dir);
        std::mem::forget(upstream_dir);
        (work_dir, work, _seed_dir)
    }

    // Catches: a fork PR being rebased onto the fork's stale main and reported "clean" while it
    // conflicts with the upstream it will actually merge into.
    #[test]
    fn a_fork_pr_is_rebased_onto_the_upstreams_base_not_the_forks() {
        let (_work_dir, work, _seed) = fork_checkout();
        let wt_dir = tempfile::tempdir().expect("tempdir");
        let run = |base_remote: &str| {
            prepare_rebase(
                work.to_str().unwrap(),
                wt_dir.path(),
                &RemotePlan {
                    head_remote: "origin".into(),
                    base_remote: base_remote.into(),
                },
                "feature",
                "main",
                7,
                TEST_FETCH_TIMEOUT,
            )
        };

        // Against the fork's own main (stale: it has no upstream edit) the PR looks clean.
        let wrong = run("origin").expect("rebase onto the fork's main");
        assert!(
            wrong.rebase_ok && wrong.conflicted.is_empty(),
            "{wrong:?}",
            wrong = wrong.rebase_stderr
        );
        assert_eq!(wrong.base_source, BaseSource::FetchedRemote);
    }

    #[test]
    fn a_fork_pr_conflicting_with_the_upstream_base_reports_the_conflict() {
        let (_work_dir, work, _seed) = fork_checkout();
        let wt_dir = tempfile::tempdir().expect("tempdir");
        let prepared = prepare_rebase(
            work.to_str().unwrap(),
            wt_dir.path(),
            &RemotePlan {
                head_remote: "origin".into(),
                base_remote: "upstream".into(),
            },
            "feature",
            "main",
            7,
            TEST_FETCH_TIMEOUT,
        )
        .expect("rebase onto the upstream's main");
        assert!(!prepared.rebase_ok);
        assert_eq!(prepared.conflicted, vec!["shared.txt"]);
        assert_eq!(prepared.base_source, BaseSource::FetchedRemote);
        // The base ref it rebased onto is the upstream's, fetched for this run.
        let out = git(
            &work,
            &["rev-parse", "--verify", "refs/remotes/upstream/main"],
        );
        assert!(out.status.success(), "the upstream base was fetched");
        // The worktree is left mid-rebase on the conflicted file, for the agent to resolve.
        let status = git(Path::new(&prepared.worktree_path), &["status"]);
        let status = String::from_utf8_lossy(&status.stdout);
        assert!(status.contains("rebase in progress"), "{status}");
        assert!(status.contains("shared.txt"), "{status}");
    }
}
