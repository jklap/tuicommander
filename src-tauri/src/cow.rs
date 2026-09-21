//! Copy-on-write helpers used to warm linked worktrees.
//!
//! TUICommander no longer creates independent copy-on-write workspace clones.
//! It still clonefiles git-ignored build directories into a freshly created
//! linked worktree so dependency and compiler caches arrive warm.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::git_cli::git_cmd;

/// Max concurrent directory copies during warming. `cp -c`/`--reflink=always`
/// is mostly syscall/metadata-bound, not CPU-bound, and going wider risks I/O
/// contention against the same source/dest volume with little payoff.
pub(crate) const WARM_COPY_CONCURRENCY: usize = 4;

/// The copy-on-write flags supported by macOS `cp` and GNU coreutils `cp`.
/// Every flag must fail instead of degrading to a byte copy.
const COW_COPY_FLAGS: [&str; 2] = ["-c", "--reflink=always"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CowSupport {
    Supported,
    Unsupported(String),
}

/// Probe the actual source/destination pair instead of inferring reflink
/// support from a filesystem name.
pub(crate) fn probe_cow_support(src: &Path, dest_parent: &Path) -> CowSupport {
    let probe_source = src.join(".git").join("HEAD");
    if !probe_source.is_file() {
        return CowSupport::Unsupported(format!(
            "no HEAD to probe with at '{}'",
            probe_source.display()
        ));
    }
    let Some(anchor) = existing_ancestor(dest_parent) else {
        return CowSupport::Unsupported(format!(
            "no existing directory above '{}' to probe",
            dest_parent.display()
        ));
    };
    if same_volume(src, &anchor) == Some(false) {
        return CowSupport::Unsupported(format!(
            "'{}' and '{}' are on different volumes",
            src.display(),
            anchor.display()
        ));
    }

    let probe = anchor.join(format!(
        ".tuic-cow-probe.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_file(&probe);
    let copied = COW_COPY_FLAGS
        .iter()
        .any(|flag| clone_file_with(&[flag], &probe_source, &probe));
    let _ = std::fs::remove_file(&probe);

    if copied {
        CowSupport::Supported
    } else {
        CowSupport::Unsupported(format!(
            "copy-on-write is unavailable between '{}' and '{}'",
            probe_source.display(),
            anchor.display()
        ))
    }
}

fn clone_file_with(flags: &[&str], from: &Path, to: &Path) -> bool {
    Command::new("cp")
        .args(flags)
        .arg(from)
        .arg(to)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
        && to.exists()
}

fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(directory) = current {
        if directory.is_dir() {
            return Some(directory.to_path_buf());
        }
        current = directory.parent();
    }
    None
}

#[cfg(unix)]
fn same_volume(left: &Path, right: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    Some(std::fs::metadata(left).ok()?.dev() == std::fs::metadata(right).ok()?.dev())
}

#[cfg(not(unix))]
fn same_volume(_left: &Path, _right: &Path) -> Option<bool> {
    None
}

/// Copy a directory tree copy-on-write, trying the platform-specific flags in
/// order and never falling back to a byte copy.
pub(crate) fn clone_tree(src: &Path, dest: &Path) -> Result<(), String> {
    clone_tree_with(src, dest, |flag| {
        let mut command = Command::new("cp");
        command.arg(flag).arg("-R").arg(src).arg(dest);
        let (status, stderr) = run_copy_command(&mut command, COW_COPY_TIMEOUT)?;
        if status.success() {
            Ok(())
        } else {
            Err(stderr.trim().to_string())
        }
    })
}

const COW_COPY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(280);

/// Run a copy command under `timeout`, returning its status and stderr.
///
/// Delegates to [`crate::git_cli::output_with_deadline`] rather than polling
/// `try_wait` and reading stderr afterwards. That ordering deadlocks: `cp`
/// prints one line per unreadable file, and once it has written a pipe buffer's
/// worth (64 KiB) with nobody draining the other end it blocks in `write` and
/// can never exit, so the child sat there until the deadline killed it — a
/// wedge, reported as a timeout. `output_with_deadline` drains both pipes on
/// reader threads, so a chatty copy finishes and reports its real error.
fn run_copy_command(
    command: &mut Command,
    timeout: std::time::Duration,
) -> Result<(std::process::ExitStatus, String), String> {
    let output = crate::git_cli::output_with_deadline(command, timeout).map_err(|error| {
        match error {
            // Keep the copy-specific wording: GitError's own Display says "git".
            crate::git_cli::GitError::TimedOut { after } => format!(
                "copy-on-write copy exceeded its {} second operation deadline",
                after.as_secs()
            ),
            other => format!("could not run copy command: {other}"),
        }
    })?;
    Ok((
        output.status,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn clone_tree_with(
    src: &Path,
    dest: &Path,
    mut attempt: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for flag in COW_COPY_FLAGS {
        match attempt(flag) {
            Ok(()) => return Ok(()),
            Err(reason) => failures.push(format!("`cp {flag} -R`: {reason}")),
        }
        let _ = std::fs::remove_dir_all(dest);
    }
    Err(format!(
        "copy-on-write copy of '{}' into '{}' failed: {}",
        src.display(),
        dest.display(),
        failures.join("; ")
    ))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WarmingReport {
    pub(crate) warmed: usize,
    pub(crate) warnings: Vec<String>,
}

/// Ask git which directories are ignored. A git failure is a warning rather
/// than an error: the worktree is complete and valid, it just starts cold.
fn warming_candidates(src: &Path, dest: &Path) -> Result<Vec<PathBuf>, WarmingReport> {
    ignored_directories(src).map_err(|reason| WarmingReport {
        warmed: 0,
        warnings: vec![format!(
            "could not ask git which directories are ignored, so '{}' starts cold: {reason}",
            dest.display()
        )],
    })
}

/// Production wrapper. Probe once so a filesystem without clonefile support
/// produces one useful warning instead of one failure for every ignored tree.
///
/// `on_started` is called once — with the number of directories that will
/// actually be copied, after skip-rules are applied, which can be lower than
/// the raw candidate count — before any copy begins. `on_progress` is called
/// after each directory finishes, in COMPLETION order: copies now run
/// concurrently (up to [`WARM_COPY_CONCURRENCY`] at a time), so this is no
/// longer candidate order the way it was when warming ran sequentially.
pub(crate) async fn warm_worktree(
    src: &Path,
    dest: &Path,
    on_started: impl FnOnce(usize),
    on_progress: impl FnMut(usize, usize, Option<&str>),
) -> WarmingReport {
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    if candidates.is_empty() {
        // Nothing to warm: skip the probe so a non-clonefile filesystem does
        // not warn about a copy that was never going to happen.
        return WarmingReport::default();
    }
    let probe_src = src.to_path_buf();
    let probe_anchor = dest.parent().unwrap_or(dest).to_path_buf();
    let support = tokio::task::spawn_blocking(move || probe_cow_support(&probe_src, &probe_anchor))
        .await
        .unwrap_or_else(|error| CowSupport::Unsupported(format!("probe task panicked: {error}")));

    match support {
        CowSupport::Supported => {
            let sem = Arc::new(tokio::sync::Semaphore::new(WARM_COPY_CONCURRENCY));
            warm_candidates_concurrent(
                src,
                dest,
                candidates,
                sem,
                clone_tree,
                on_started,
                on_progress,
            )
            .await
        }
        CowSupport::Unsupported(reason) => WarmingReport {
            warmed: 0,
            warnings: vec![format!(
                "could not warm '{}' with copy-on-write data: {reason}",
                dest.display()
            )],
        },
    }
}

/// Copy the parent's git-ignored directories into a linked worktree with an
/// injected copy primitive. A failed copy leaves a complete, valid, but cold
/// worktree and is therefore a warning.
///
/// Test-only. Production always clonefiles, and the probe that decides whether
/// it can belongs to `warm_worktree`; this seam exists so the tests can drive
/// the same concurrent fan-out with a copy that does not depend on the
/// filesystem.
#[cfg(test)]
pub(crate) async fn warm_worktree_with<F>(src: &Path, dest: &Path, copy: F) -> WarmingReport
where
    F: Fn(&Path, &Path) -> Result<(), String> + Send + Sync + 'static,
{
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    let sem = Arc::new(tokio::sync::Semaphore::new(WARM_COPY_CONCURRENCY));
    warm_candidates_concurrent(src, dest, candidates, sem, copy, |_| {}, |_, _, _| {}).await
}

/// Fan out each candidate directory's copy across up to `sem`'s permit count
/// at a time (`tokio::task::spawn_blocking` per directory, gated by an
/// `acquire_owned` permit — the same "cap concurrent blocking work" pattern
/// `content_index.rs::spawn_build` uses for index builds). Skip-rule
/// evaluation (symlinks, self-repos, existing destinations, dest creation)
/// stays a single-threaded pre-pass — it's cheap, filesystem-metadata-only
/// work, and doing it before dispatch means `on_started`'s count reflects
/// what will actually be copied, not the raw candidate count.
async fn warm_candidates_concurrent<F>(
    src: &Path,
    dest: &Path,
    candidates: Vec<PathBuf>,
    sem: Arc<tokio::sync::Semaphore>,
    copy: F,
    on_started: impl FnOnce(usize),
    mut on_progress: impl FnMut(usize, usize, Option<&str>),
) -> WarmingReport
where
    F: Fn(&Path, &Path) -> Result<(), String> + Send + Sync + 'static,
{
    let mut report = WarmingReport::default();
    let src_root = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let dest_root = std::fs::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf());

    let mut to_dispatch: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
    for relative in candidates {
        let from = src_root.join(&relative);
        let to = dest_root.join(&relative);
        match std::fs::symlink_metadata(&from) {
            Ok(metadata) if metadata.is_dir() => {}
            _ => continue,
        }
        if from.join(".git").exists()
            || is_inside(&dest_root, &from)
            || is_inside(&from, &dest_root)
            || to.exists()
        {
            continue;
        }
        // `dest_root` is `git worktree add`'s checkout of whatever branch was
        // requested — the same untrusted-content threat model
        // `worktree_sync.rs::sync_one` already guards against for its own
        // writes into `dest`. A malicious branch can commit a directory
        // symlink at any intermediate component of a path matching one of
        // the parent's own ignored-directory names; without this check,
        // `create_dir_all`/the copy below would follow it and write this
        // (trusted) repo's real ignored content through to wherever the
        // branch pointed, using this repo's own content. Reject rather than
        // follow — same as `sync_one`, but as a warning (not a hard error):
        // warming is best-effort, so one skipped candidate must not abort
        // the rest.
        if let Some(bad) = crate::worktree_sync::first_symlinked_ancestor(&dest_root, &relative) {
            report.warnings.push(format!(
                "could not warm '{}' in the new worktree, which starts cold there: path traverses \
                 a symlink at an intermediate component ({})",
                relative.display(),
                bad.display()
            ));
            continue;
        }
        if let Some(parent) = to.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            report.warnings.push(format!(
                "could not prepare '{}' in the new worktree, which starts cold there: {error}",
                relative.display()
            ));
            continue;
        }
        to_dispatch.push((relative, from, to));
    }

    let total = to_dispatch.len();
    if total == 0 {
        return report;
    }
    on_started(total);

    let copy = Arc::new(copy);
    let mut set = tokio::task::JoinSet::new();
    for (relative, from, to) in to_dispatch {
        let permit_source = Arc::clone(&sem);
        let copy = Arc::clone(&copy);
        set.spawn(async move {
            let _permit = permit_source.acquire_owned().await;
            let result = tokio::task::spawn_blocking(move || copy(&from, &to)).await;
            (relative, result)
        });
    }

    let mut done = 0usize;
    while let Some(joined) = set.join_next().await {
        done += 1;
        let (relative, outcome) = match joined {
            Ok(pair) => pair,
            Err(join_error) => {
                report
                    .warnings
                    .push(format!("warm task panicked: {join_error}"));
                on_progress(done, total, None);
                continue;
            }
        };
        let label = relative.display().to_string();
        match outcome {
            Ok(Ok(())) => {
                report.warmed += 1;
                on_progress(done, total, Some(&label));
            }
            Ok(Err(reason)) => {
                report.warnings.push(format!(
                    "could not warm '{}' in the new worktree, which starts cold there: {reason}",
                    relative.display()
                ));
                on_progress(done, total, Some(&label));
            }
            Err(join_error) => {
                report.warnings.push(format!(
                    "could not warm '{}' in the new worktree, which starts cold there: copy task panicked: {join_error}",
                    relative.display()
                ));
                on_progress(done, total, Some(&label));
            }
        }
    }
    report
}

fn ignored_directories(src: &Path) -> Result<Vec<PathBuf>, String> {
    let listed = git_cmd(src)
        .args([
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ])
        .run()
        .map_err(|error| format!("git ls-files failed: {error}"))?;
    let mut directories: Vec<&str> = listed
        .stdout
        .split('\0')
        .filter_map(|entry| entry.strip_suffix('/'))
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            !Path::new(entry)
                .components()
                .any(|component| component.as_os_str() == ".git")
        })
        .collect();
    directories.sort_unstable();
    let mut kept: Vec<PathBuf> = Vec::new();
    for directory in directories {
        if kept
            .iter()
            .any(|ancestor| Path::new(directory).starts_with(ancestor))
        {
            continue;
        }
        kept.push(PathBuf::from(directory));
    }
    Ok(kept)
}

const WARM_ARTIFACT_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "src-tauri/target",
    ".venv",
    "venv",
    "dist",
    ".next",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct WarmArtifact {
    pub(crate) path: String,
    pub(crate) size: String,
}

pub(crate) fn warm_artifacts(workspace: &Path) -> Vec<WarmArtifact> {
    let present: Vec<&str> = WARM_ARTIFACT_DIRS
        .iter()
        .copied()
        .filter(|directory| workspace.join(directory).is_dir())
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = present
            .iter()
            .map(|directory| {
                let full = workspace.join(directory);
                scope.spawn(move || directory_size(&full))
            })
            .collect();
        present
            .iter()
            .zip(handles)
            .filter_map(|(directory, handle)| {
                handle.join().ok().flatten().map(|size| WarmArtifact {
                    path: (*directory).to_string(),
                    size,
                })
            })
            .collect()
    })
}

/// Recursive size of `path`, rendered the way `du -sh` renders it.
///
/// Walks in Rust rather than shelling out to `du`. That tool does not exist on
/// Windows, where `Command::output` therefore failed and dropped every warmed
/// directory out of the report: a copy that had actually worked came back
/// saying nothing was warmed. It also spares one process per directory on the
/// platforms that do have it.
///
// DEFERRED (2026-09-14) — the other half of the portability gap. `clone_tree`
// still shells out to `cp -c` / `cp --reflink=always`, which Windows has no
// equivalent for at all. Deciding what warming even means without reflink is a
// design question, not a substitution, so it needs Boss before implementation.
fn directory_size(path: &Path) -> Option<String> {
    Some(human_size(walk_size(path)?))
}

/// Apparent bytes under `path` — file lengths, not allocated blocks, so this
/// reads slightly under a bare `du -sh` and matches `du -sh --apparent-size`.
/// The number exists to say "a warm cache this big arrived", and for that the
/// content size is the more honest of the two.
///
/// Symlinks are counted but never followed, the same choice `du` makes by
/// default: it keeps a link back into the parent repo from counting that whole
/// tree, and a cycle from never terminating.
fn walk_size(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_symlink() {
        return Some(0);
    }
    if !meta.is_dir() {
        return Some(meta.len());
    }
    let mut total = 0;
    for entry in std::fs::read_dir(path).ok()? {
        let Ok(entry) = entry else { continue };
        total += walk_size(&entry.path()).unwrap_or(0);
    }
    Some(total)
}

/// The largest unit that keeps the number under 1024, with one decimal below
/// ten — `du -sh`'s rendering, because this string is read next to sizes the
/// user has seen from `du` and a second convention would only invite comparison
/// of two numbers that do not mean the same thing.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes}B"),
        _ if value < 10.0 => format!("{value:.1}{}", UNITS[unit]),
        _ => format!("{value:.0}{}", UNITS[unit]),
    }
}

fn is_inside(inner: &Path, outer: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(inner).starts_with(canonical(outer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        git_cmd(path).args(["init"]).run().unwrap();
        std::fs::write(path.join("tracked.txt"), "tracked\n").unwrap();
        git_cmd(path).args(["add", "."]).run().unwrap();
        git_cmd(path)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ])
            .run()
            .unwrap();
    }

    #[test]
    fn probe_uses_a_real_copy_and_leaves_no_debris() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let before = std::fs::read_dir(temp.path()).unwrap().count();
        let support = probe_cow_support(&repo, &temp.path().join("missing/dest"));
        assert_eq!(before, std::fs::read_dir(temp.path()).unwrap().count());
        assert!(matches!(
            support,
            CowSupport::Supported | CowSupport::Unsupported(_)
        ));
    }

    #[test]
    fn clone_tree_falls_back_and_cleans_partial_destinations() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        let mut attempts = Vec::new();
        clone_tree_with(&src, &dest, |flag| {
            attempts.push(flag.to_string());
            if attempts.len() == 1 {
                std::fs::create_dir_all(&dest).unwrap();
                Err("unsupported".into())
            } else {
                assert!(!dest.exists());
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, vec!["-c", "--reflink=always"]);
    }

    #[test]
    fn clone_tree_exhaustion_names_each_flag_and_leaves_nothing() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        let error = clone_tree_with(&src, &dest, |flag| {
            std::fs::create_dir_all(&dest).unwrap();
            Err(format!("{flag} failed"))
        })
        .unwrap_err();
        assert!(error.contains("cp -c -R"));
        assert!(error.contains("cp --reflink=always -R"));
        assert!(!dest.exists());
    }

    /// A copy that prints more than a pipe buffer's worth of warnings must still
    /// finish and report its own exit status. `cp` emits one line per unreadable
    /// file, and reading stderr only after the child has exited inverts the
    /// dependency: once the child has written 64 KiB with nobody draining the
    /// other end it blocks in `write` and can never exit, so a noisy copy wedged
    /// until the deadline killed it and reported a timeout that never happened.
    /// The deadline here is deliberately short — a regression fails in seconds
    /// instead of hanging for the real 280.
    #[test]
    fn a_copy_that_floods_stderr_still_reports_its_own_failure() {
        // The flood has to be spelled for the host shell. `sh` is not a Windows
        // program: this test spawned it anyway and only passed on GitHub's
        // runner, whose image carries `C:\Program Files\Git\usr\bin` on `PATH`.
        let (shell, flag) = crate::test_support::host_shell();
        let script = if cfg!(windows) {
            // What has to exceed the pipe buffer is a byte count, not a line
            // count, and `cmd`'s `for /L` costs about 14ms an iteration on a
            // small machine: measured 2026-09-16, 4000 short lines took 58s
            // and blew the 20s deadline below, while 200 long ones take 3s.
            // 200 lines of 342 bytes is 68,400 — past 64 KiB with room.
            let line = format!("cp: cannot read file{}", "x".repeat(320));
            format!("(for /L %i in (1,1,200) do @echo {line}) 1>&2 & exit /b 1")
        } else {
            "i=0; while [ $i -lt 4000 ]; do echo 'cp: cannot read file' >&2; i=$((i+1)); done; exit 1"
                .to_string()
        };
        let mut command = Command::new(shell);
        command.arg(flag).arg(script);

        let (status, stderr) =
            run_copy_command(&mut command, std::time::Duration::from_secs(20)).unwrap();

        assert_eq!(status.code(), Some(1));
        assert!(
            stderr.len() > 64 * 1024,
            "the flood must exceed a pipe buffer, got {} bytes",
            stderr.len()
        );
    }

    /// The report is the only thing telling the user a warm cache arrived, so a
    /// size it cannot measure removes the directory from the list entirely. On
    /// Windows `du` is absent and that happened to every directory, every time.
    #[test]
    fn a_directory_is_measured_without_leaving_the_process() {
        let temp = TempDir::new().unwrap();
        let tree = temp.path().join("cache/deep");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("blob"), vec![0u8; 3000]).unwrap();
        std::fs::write(temp.path().join("cache/small"), vec![0u8; 1096]).unwrap();

        assert_eq!(
            directory_size(&temp.path().join("cache")),
            Some("4.0K".to_string())
        );
        assert_eq!(directory_size(&temp.path().join("missing")), None);
    }

    /// Read next to sizes the user has seen from `du`, so it must round the way
    /// `du -sh` rounds rather than merely being close.
    #[test]
    fn sizes_are_rendered_the_way_du_renders_them() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(1023), "1023B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(10 * 1024), "10K");
        assert_eq!(human_size(11 * 1024 * 1024), "11M");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024 / 2), "1.5G");
    }

    fn warming_fixture() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::write(repo.join(".gitignore"), "build/\n.env\n").unwrap();
        std::fs::create_dir_all(repo.join("build/cache")).unwrap();
        std::fs::write(repo.join("build/cache/data"), "warm").unwrap();
        std::fs::write(repo.join(".env"), "secret").unwrap();
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        (temp, repo, worktree)
    }

    fn plain_copy(from: &Path, to: &Path) -> Result<(), String> {
        std::fs::create_dir_all(to).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir(from).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                plain_copy(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn warming_brings_an_ignored_directory_into_the_worktree() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, plain_copy).await;
        assert_eq!(report.warmed, 1);
        assert!(report.warnings.is_empty());
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "warm"
        );
    }

    #[tokio::test]
    async fn warming_leaves_a_tracked_directory_to_git() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join("tracked.txt"), "parent edit\n").unwrap();

        warm_worktree_with(&repo, &worktree, plain_copy).await;

        assert_eq!(
            std::fs::read_to_string(worktree.join("tracked.txt")).unwrap(),
            "tracked\n"
        );
    }

    #[tokio::test]
    async fn warming_never_copies_an_ignored_file() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy).await;

        assert!(!worktree.join(".env").exists());
    }

    #[tokio::test]
    async fn warming_leaves_the_worktree_git_link_intact() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy).await;

        assert!(worktree.join(".git").is_file());
        git_cmd(&worktree)
            .args(["status", "--porcelain"])
            .run()
            .unwrap();
    }

    /// Phase 2 update of the Phase 0 baseline (worktree-warming-async-plan.md):
    /// now that copies run concurrently (up to `WARM_COPY_CONCURRENCY` at a
    /// time), completion order is no longer candidate order, so this compares
    /// the warnings as a sorted set rather than asserting a fixed position.
    #[tokio::test]
    async fn warming_collects_a_failure_warning_for_every_candidate_order_independent() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::write(repo.join(".gitignore"), "alpha/\nbeta/\n").unwrap();
        std::fs::create_dir_all(repo.join("alpha")).unwrap();
        std::fs::write(repo.join("alpha/file"), "a").unwrap();
        std::fs::create_dir_all(repo.join("beta")).unwrap();
        std::fs::write(repo.join("beta/file"), "b").unwrap();
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |_, _| Err("refused".into())).await;

        assert_eq!(report.warnings.len(), 2, "got: {:?}", report.warnings);
        let mut warnings = report.warnings.clone();
        warnings.sort();
        assert!(warnings[0].contains("alpha"), "got: {:?}", warnings);
        assert!(warnings[1].contains("beta"), "got: {:?}", warnings);
    }

    /// Concurrency-cap test: a fake copy that blocks until released proves no
    /// more than `WARM_COPY_CONCURRENCY` copies ever run at once, even with
    /// far more candidates than that available.
    #[tokio::test]
    async fn warming_never_exceeds_the_concurrency_cap() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let candidate_count = WARM_COPY_CONCURRENCY * 3;
        let mut gitignore = String::new();
        for i in 0..candidate_count {
            let name = format!("dir{i:02}");
            gitignore.push_str(&format!("{name}/\n"));
            std::fs::create_dir_all(repo.join(&name)).unwrap();
            std::fs::write(repo.join(&name).join("f"), "x").unwrap();
        }
        std::fs::write(repo.join(".gitignore"), gitignore).unwrap();
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let in_flight_for_copy = Arc::clone(&in_flight);
        let max_for_copy = Arc::clone(&max_in_flight);
        let report = warm_worktree_with(&repo, &worktree, move |_, to| {
            let now = in_flight_for_copy.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            max_for_copy.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(30));
            let result = std::fs::create_dir_all(to).map_err(|e| e.to_string());
            in_flight_for_copy.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            result
        })
        .await;

        assert_eq!(report.warmed, candidate_count);
        assert!(
            max_in_flight.load(std::sync::atomic::Ordering::SeqCst) <= WARM_COPY_CONCURRENCY,
            "observed {} concurrent copies, cap is {}",
            max_in_flight.load(std::sync::atomic::Ordering::SeqCst),
            WARM_COPY_CONCURRENCY
        );
    }

    /// Wall-time test: with more candidates than the concurrency cap, running
    /// them concurrently must take roughly one "wave" of the artificial delay,
    /// not `sum(delays)` — proving the fan-out is actually parallel, not just
    /// concurrency-capped-but-still-serial.
    #[tokio::test]
    async fn warming_runs_concurrently_not_sequentially() {
        // Structural, not wall-clock: a wall-clock bound here is exactly the
        // load-bearing timing-assertion pitfall AGENTS.md warns about — this
        // test used to assert `elapsed < 500ms`, which is robust on an idle
        // machine but not under `cargo nextest run --workspace`'s full
        // parallel load (observed failing at 2.35s elapsed in that
        // configuration, nowhere near a scheduling-jitter margin fix could
        // cover). Tracking `max_in_flight` via the same atomic technique
        // `warming_never_exceeds_the_concurrency_cap` already uses proves
        // real parallelism happened regardless of how slowly the OS
        // schedules the threads.
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let candidate_count = WARM_COPY_CONCURRENCY * 2;
        let mut gitignore = String::new();
        for i in 0..candidate_count {
            let name = format!("dir{i:02}");
            gitignore.push_str(&format!("{name}/\n"));
            std::fs::create_dir_all(repo.join(&name)).unwrap();
            std::fs::write(repo.join(&name).join("f"), "x").unwrap();
        }
        std::fs::write(repo.join(".gitignore"), gitignore).unwrap();
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_in_flight = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let in_flight_for_copy = Arc::clone(&in_flight);
        let max_for_copy = Arc::clone(&max_in_flight);
        let report = warm_worktree_with(&repo, &worktree, move |_, to| {
            let now = in_flight_for_copy.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            max_for_copy.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            let result = std::fs::create_dir_all(to).map_err(|e| e.to_string());
            in_flight_for_copy.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            result
        })
        .await;

        assert_eq!(report.warmed, candidate_count);
        let observed = max_in_flight.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            observed > 1,
            "observed only {observed} concurrent copy at a time — dispatch is running \
             sequentially, not fanning out"
        );
    }

    #[tokio::test]
    async fn a_failed_copy_reports_a_cold_directory_instead_of_failing() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, |_, _| Err("refused".into())).await;
        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("build"));
        assert!(report.warnings[0].contains("cold"));
        assert!(worktree.join(".git").is_file());
    }

    #[tokio::test]
    async fn a_parent_with_no_ignored_directories_warms_nothing_and_says_nothing() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, _| {
            panic!(
                "nothing should have been copied, but '{}' was",
                from.display()
            )
        })
        .await;

        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }

    #[tokio::test]
    async fn warming_skips_the_ignored_directory_that_holds_the_destination() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::write(repo.join(".gitignore"), "/.worktrees\n").unwrap();
        git_cmd(&repo).args(["add", "."]).run().unwrap();
        git_cmd(&repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "ignore",
            ])
            .run()
            .unwrap();
        let worktree = repo.join(".worktrees/feature");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        let report = warm_worktree_with(&repo, &worktree, |from, _| {
            panic!(
                "'{}' contains the destination and must never be copied",
                from.display()
            )
        })
        .await;

        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }

    #[tokio::test]
    async fn warming_skips_an_ignored_directory_that_is_its_own_repository() {
        let (_temp, repo, worktree) = warming_fixture();
        git_cmd(&repo.join("build")).args(["init"]).run().unwrap();
        let report = warm_worktree_with(&repo, &worktree, |_, _| panic!("must not copy")).await;
        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }

    /// Same threat model as `worktree_sync.rs`'s `first_symlinked_ancestor`
    /// (a malicious branch commits a directory symlink at an intermediate
    /// path component) — this repro plants the symlink directly rather than
    /// via a second git branch, since `warm_candidates_concurrent` only ever
    /// sees the destination's on-disk state, not how it got there.
    ///
    /// `vendor` needs *tracked* content of its own (`README.md`) so git's
    /// `ls-files --directory` collapse reports the nested `vendor/cache/`
    /// candidate specifically, rather than collapsing all the way up to the
    /// single component `vendor` (which `first_symlinked_ancestor` would
    /// never check — it only ever guards *intermediate* components, by
    /// design, since the final one has its own "already exists" check).
    #[tokio::test]
    async fn warming_refuses_to_follow_a_symlinked_intermediate_component_in_dest() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        init_repo(&repo);
        std::fs::create_dir_all(repo.join("vendor")).unwrap();
        std::fs::write(repo.join("vendor/README.md"), "tracked\n").unwrap();
        git_cmd(&repo)
            .args(["add", "vendor/README.md"])
            .run()
            .unwrap();
        git_cmd(&repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "add vendor readme",
            ])
            .run()
            .unwrap();
        std::fs::write(repo.join(".gitignore"), "vendor/cache/\n").unwrap();
        std::fs::create_dir_all(repo.join("vendor/cache")).unwrap();
        std::fs::write(repo.join("vendor/cache/secret-build-artifact"), "warm").unwrap();

        let worktree = temp.path().join("worktree");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ])
            .run()
            .unwrap();

        // The normal checkout above created a real `vendor/README.md` in the
        // worktree from tracked content — replace it with a symlink to stand
        // in for a malicious branch that committed `vendor` as a symlink
        // instead of an ordinary tracked directory.
        std::fs::remove_dir_all(worktree.join("vendor")).unwrap();
        let escape_target = temp.path().join("escape-target");
        std::fs::create_dir_all(&escape_target).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&escape_target, worktree.join("vendor")).unwrap();

        let report = warm_worktree_with(&repo, &worktree, |_, _| {
            panic!("must not copy through the symlink")
        })
        .await;

        assert_eq!(report.warmed, 0, "the symlinked candidate must not warm");
        assert_eq!(report.warnings.len(), 1);
        assert!(
            report.warnings[0].contains("symlink at an intermediate component"),
            "unexpected warning: {}",
            report.warnings[0]
        );
        assert!(
            !escape_target.join("cache").exists(),
            "warming must never write through the planted symlink into escape_target"
        );
    }
}
