//! Copy-on-write helpers used to warm linked worktrees.
//!
//! TUICommander no longer creates independent copy-on-write workspace clones.
//! It still clonefiles git-ignored build directories into a freshly created
//! linked worktree so dependency and compiler caches arrive warm.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git_cli::git_cmd;

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
pub(crate) fn warm_worktree(src: &Path, dest: &Path) -> WarmingReport {
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    if candidates.is_empty() {
        // Nothing to warm: skip the probe so a non-clonefile filesystem does
        // not warn about a copy that was never going to happen.
        return WarmingReport::default();
    }
    match probe_cow_support(src, dest.parent().unwrap_or(dest)) {
        CowSupport::Supported => warm_candidates(src, dest, candidates, clone_tree),
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
/// `warm_candidates` with a copy that does not depend on the filesystem.
#[cfg(test)]
pub(crate) fn warm_worktree_with(
    src: &Path,
    dest: &Path,
    copy: impl Fn(&Path, &Path) -> Result<(), String>,
) -> WarmingReport {
    let candidates = match warming_candidates(src, dest) {
        Ok(candidates) => candidates,
        Err(report) => return report,
    };
    warm_candidates(src, dest, candidates, copy)
}

fn warm_candidates(
    src: &Path,
    dest: &Path,
    candidates: Vec<PathBuf>,
    copy: impl Fn(&Path, &Path) -> Result<(), String>,
) -> WarmingReport {
    let mut report = WarmingReport::default();
    let src_root = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let dest_root = std::fs::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf());

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
        if let Some(parent) = to.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            report.warnings.push(format!(
                "could not prepare '{}' in the new worktree, which starts cold there: {error}",
                relative.display()
            ));
            continue;
        }
        match copy(&from, &to) {
            Ok(()) => report.warmed += 1,
            Err(reason) => report.warnings.push(format!(
                "could not warm '{}' in the new worktree, which starts cold there: {reason}",
                relative.display()
            )),
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

    #[test]
    fn warming_brings_an_ignored_directory_into_the_worktree() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, plain_copy);
        assert_eq!(report.warmed, 1);
        assert!(report.warnings.is_empty());
        assert_eq!(
            std::fs::read_to_string(worktree.join("build/cache/data")).unwrap(),
            "warm"
        );
    }

    #[test]
    fn warming_leaves_a_tracked_directory_to_git() {
        let (_temp, repo, worktree) = warming_fixture();
        std::fs::write(repo.join("tracked.txt"), "parent edit\n").unwrap();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert_eq!(
            std::fs::read_to_string(worktree.join("tracked.txt")).unwrap(),
            "tracked\n"
        );
    }

    #[test]
    fn warming_never_copies_an_ignored_file() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(!worktree.join(".env").exists());
    }

    #[test]
    fn warming_leaves_the_worktree_git_link_intact() {
        let (_temp, repo, worktree) = warming_fixture();

        warm_worktree_with(&repo, &worktree, plain_copy);

        assert!(worktree.join(".git").is_file());
        git_cmd(&worktree)
            .args(["status", "--porcelain"])
            .run()
            .unwrap();
    }

    #[test]
    fn a_failed_copy_reports_a_cold_directory_instead_of_failing() {
        let (_temp, repo, worktree) = warming_fixture();
        let report = warm_worktree_with(&repo, &worktree, |_, _| Err("refused".into()));
        assert_eq!(report.warmed, 0);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("build"));
        assert!(report.warnings[0].contains("cold"));
        assert!(worktree.join(".git").is_file());
    }

    #[test]
    fn a_parent_with_no_ignored_directories_warms_nothing_and_says_nothing() {
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
        });

        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn warming_skips_the_ignored_directory_that_holds_the_destination() {
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
        });

        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn warming_skips_an_ignored_directory_that_is_its_own_repository() {
        let (_temp, repo, worktree) = warming_fixture();
        git_cmd(&repo.join("build")).args(["init"]).run().unwrap();
        let report = warm_worktree_with(&repo, &worktree, |_, _| panic!("must not copy"));
        assert_eq!(report.warmed, 0);
        assert!(report.warnings.is_empty());
    }
}
