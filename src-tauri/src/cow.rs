//! Copy-on-write workspace creation: the capability probe and the guards that
//! run before anything is copied.
//!
//! A COW workspace is a `clonefile` copy of a whole repository directory, which
//! makes it an *independent repository* — two of them can sit on the same branch,
//! and `node_modules`/`target` arrive warm. Everything in this module runs
//! BEFORE the copy and answers two questions: can this pair of paths do COW at
//! all, and is this repo in a shape that can be cloned safely.
//!
//! **A guard never repairs.** A repo that is not in a clonable shape is a
//! refusal, not a fixup: deleting an inherited lock or finishing someone else's
//! rebase turns a torn copy into a plausible-looking corrupt one. Every function
//! here is read-only with respect to the source; the single piece of debris the
//! caller is told about — a stale lock — is dropped in the COPY, by the caller,
//! after the copy exists.
//!
//! Measured in `scripts/cow-workspace-poc.sh` on a 12 GB repo: 19 MB of real
//! disk and 26 s for the clone. The rules below are the ones that PoC proved
//! load-bearing, not a precautionary list.

// The consumer of every item below is the creation path in #730-c047, the next
// story of this plan. Split that way on purpose: the guards are the half that
// has to be right before anything is copied, and they are testable on their
// own. Remove this attribute with the first real caller.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git_cli::git_cmd;
use crate::git_locks::{self, LockFileInfo};

/// Whether this source/destination pair can be cloned copy-on-write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CowSupport {
    Supported,
    /// Carries the reason so `mode=cow` can fail loudly naming the check that
    /// said no, and `mode=auto` can report why it degraded.
    Unsupported(String),
}

impl CowSupport {
    pub(crate) fn is_supported(&self) -> bool {
        matches!(self, CowSupport::Supported)
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        match self {
            CowSupport::Supported => None,
            CowSupport::Unsupported(reason) => Some(reason),
        }
    }
}

/// What the guards found in a repo they are willing to clone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GuardReport {
    /// Repo shapes that are unusual but not disqualifying. Reported, never acted
    /// on: refusing on them would block creation for repos the PoC cloned fine,
    /// and silently "handling" them is the repair this module does not do.
    pub(crate) warnings: Vec<String>,
    /// A stale lock inherited by the copy, RELATIVE to the source root.
    ///
    /// Relative on purpose: the caller joins it onto the destination, so the
    /// only path this value can ever name is one inside the copy. An absolute
    /// path here would be one typo away from deleting the source's lock.
    pub(crate) stale_lock: Option<PathBuf>,
}

/// Files git leaves in the gitdir while a multi-step operation is unfinished.
/// Copying a repo mid-rebase gives a workspace holding half a transaction.
const OPERATION_MARKERS: [&str; 6] = [
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

/// Can `src` be cloned copy-on-write into `dest_parent`?
///
/// Two checks, and the second is the real one:
///
/// 1. Same volume, compared by device id. A cheap pre-filter — `clonefile`
///    cannot cross volumes — and the same fact `df` reports in its device
///    column, read from `stat` instead of parsed out of a subprocess.
/// 2. An actual copy-on-write copy of `.git/HEAD`. **Never infer support from a
///    filesystem name.** "apfs" or "btrfs" says nothing about the specific mount
///    or the specific pair of paths: a directory can be on a case-sensitive
///    sub-volume, a network mount, or a filesystem whose reflink support is
///    compiled out. The only trustworthy answer is a copy that succeeded.
///
/// The probe file is removed on both paths.
pub(crate) fn probe_cow_support(src: &Path, dest_parent: &Path) -> CowSupport {
    let head = src.join(".git").join("HEAD");
    if !head.is_file() {
        return CowSupport::Unsupported(format!(
            "no HEAD to probe with at '{}' — the source is not a repository with a gitdir here",
            head.display()
        ));
    }

    // `dest_parent` need not exist yet; the volume that matters is the nearest
    // ancestor that does, which is where the copy will land.
    let Some(anchor) = existing_ancestor(dest_parent) else {
        return CowSupport::Unsupported(format!(
            "no existing directory above '{}' to probe",
            dest_parent.display()
        ));
    };

    if same_volume(src, &anchor) == Some(false) {
        return CowSupport::Unsupported(format!(
            "'{}' and '{}' are on different volumes — a copy-on-write clone cannot cross one",
            src.display(),
            anchor.display()
        ));
    }

    let probe = anchor.join(format!(
        ".tuic-cow-probe.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_file(&probe);

    // macOS `cp -c` (clonefile) first, then Linux `cp --reflink=always`. Both
    // FAIL rather than falling back to a full copy, which is what makes them a
    // probe: a `cp` that silently degraded would report support everywhere.
    let cloned = clone_file_with(&["-c"], &head, &probe)
        || clone_file_with(&["--reflink=always"], &head, &probe);
    let _ = std::fs::remove_file(&probe);

    if cloned {
        CowSupport::Supported
    } else {
        CowSupport::Unsupported(format!(
            "a copy-on-write copy of '{}' into '{}' failed — this filesystem does not support it \
             for these paths",
            head.display(),
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
        .map(|out| out.status.success())
        .unwrap_or(false)
        && to.exists()
}

/// The nearest ancestor of `path` that exists, including `path` itself.
fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(dir) = current {
        if dir.is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// `Some(true)`/`Some(false)` when both paths can be stat'ed, `None` when the
/// question cannot be answered — in which case the real copy below decides,
/// which it would have anyway.
#[cfg(unix)]
fn same_volume(left: &Path, right: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let left = std::fs::metadata(left).ok()?;
    let right = std::fs::metadata(right).ok()?;
    Some(left.dev() == right.dev())
}

#[cfg(not(unix))]
fn same_volume(_left: &Path, _right: &Path) -> Option<bool> {
    // No cheap device id, and no `cp -c` either: the probe below refuses and the
    // caller degrades to a linked worktree.
    None
}

/// Everything that must hold before `src` may be cloned to `dest`.
///
/// Returns the observations the caller needs (warnings to report, a stale lock
/// to drop inside the copy) or the first refusal, phrased so the reason reaches
/// the user rather than a generic "cannot create workspace".
pub(crate) fn check_creation_guards(src: &Path, dest: &Path) -> Result<GuardReport, String> {
    check_creation_guards_inner(src, dest, |lock| {
        git_locks::is_lock_stale(&lock.path, lock.len, lock.age_secs)
    })
}

/// [`check_creation_guards`] with the staleness verdict injected.
///
/// Same seam `git_locks::is_lock_stale_inner` uses, and for the same reason: the
/// real verdict runs an `lsof` probe and a timed mtime comparison, so a test
/// driving it end to end would be asserting against the host's process table.
fn check_creation_guards_inner(
    src: &Path,
    dest: &Path,
    is_stale: impl Fn(&LockFileInfo) -> bool,
) -> Result<GuardReport, String> {
    // 1. Containment. The recursive copy walks into its own destination and
    //    never finishes. Reachable in normal use: the `InsideRepo` and
    //    `ClaudeCodeDefault` worktree storage strategies both put the new
    //    directory under the repo.
    if is_inside(dest, src) {
        return Err(format!(
            "destination '{}' is inside the source repository '{}' — the copy would walk into itself",
            dest.display(),
            src.display()
        ));
    }

    let git_path = src.join(".git");

    // 2. A linked worktree's `.git` is a FILE holding `gitdir: <parent>/.git/worktrees/<n>`.
    //    Cloning one leaves that file pointing at the PARENT's admin dir, so the
    //    clone and the original share HEAD and index — and `git status` in the
    //    clone succeeds, silently, while writes corrupt the original.
    if git_path.is_file() {
        return Err(format!(
            "'{}' is a linked worktree, not a primary one: its .git is a file pointing at the \
             parent's admin directory. A clone would share HEAD and the index with the original \
             and corrupt it on the first write. Clone the parent repository instead.",
            src.display()
        ));
    }
    // 3. Bare repos have no working tree to give a workspace. Asked BEFORE the
    //    layout check below, because a bare repo has no `.git` directory at all
    //    — its admin files are the root — so the layout check would answer
    //    "not a git repository", which is both wrong and unhelpful.
    if is_bare(src) {
        return Err(format!(
            "'{}' is a bare repository — there is no working tree to clone",
            src.display()
        ));
    }

    if !git_path.is_dir() {
        return Err(format!("'{}' is not a git repository", src.display()));
    }

    // 4. An unfinished multi-step operation. Copying now yields a workspace
    //    holding half of somebody else's transaction.
    for marker in OPERATION_MARKERS {
        if git_path.join(marker).exists() {
            return Err(format!(
                "a git operation is in progress in '{}' ({marker} is present) — finish or abort it \
                 before creating a workspace",
                src.display()
            ));
        }
    }

    // 5. Locks. A lock held by a LIVE writer means a transaction is in flight
    //    and the recursive copy would capture files at different generations.
    //    A STALE lock is not that — it is debris, and refusing on it blocks
    //    creation forever with an opaque error (measured: an 11-day-old
    //    index.lock in a submodule gitdir). Git stores no pid in a lock, so
    //    `git_locks` adjudicates by size, age, ownership and mtime stability.
    let mut stale_lock = None;
    for lock in git_locks::find_lock_files(&git_path) {
        if is_stale(&lock) {
            // First one only: dropping one inherited lock is enough to let the
            // copy work, and a repo with several is worth reporting as-is
            // rather than quietly sweeping.
            if stale_lock.is_none() {
                stale_lock = lock
                    .path
                    .strip_prefix(src)
                    .ok()
                    .map(|relative| relative.to_path_buf());
            }
            continue;
        }
        return Err(format!(
            "'{}' is locked by a live git process ({}s old) — a copy taken now would capture the \
             repository mid-write. Wait for it to finish.",
            lock.path.display(),
            lock.age_secs
        ));
    }

    Ok(GuardReport {
        warnings: collect_warnings(src, &git_path),
        stale_lock,
    })
}

/// Delete an inherited stale lock — inside the COPY, never in the source.
///
/// The invariant this makes mechanical: [`GuardReport::stale_lock`] is relative,
/// this function is the only thing that resolves it, and it resolves it against
/// the destination. There is no argument shape here that could name the source's
/// lock, which a bare `fs::remove_file` at the call site would be one typo away
/// from doing.
pub(crate) fn drop_inherited_stale_lock(dest: &Path, report: &GuardReport) -> Option<PathBuf> {
    let relative = report.stale_lock.as_ref()?;
    let inside_copy = dest.join(relative);
    std::fs::remove_file(&inside_copy).ok()?;
    Some(inside_copy)
}

/// Repo shapes that survive a clone but are worth naming.
///
/// None of these blocks: every one of them was present in a repo the PoC cloned
/// successfully, and a guard that refuses on a warning is a guard nobody can
/// get past.
fn collect_warnings(src: &Path, git_path: &Path) -> Vec<String> {
    let mut warnings = Vec::new();

    if src.join(".gitmodules").is_file() {
        warnings.push(
            "submodules present — their gitdirs are indirected through .git/modules and travel with the copy"
                .to_string(),
        );
    }
    if git_config_bool(src, "core.sparseCheckout") {
        warnings.push(
            "sparse-checkout is enabled — the workspace inherits the same partial tree".to_string(),
        );
    }
    if git_path
        .join("objects")
        .join("info")
        .join("alternates")
        .exists()
    {
        warnings.push(
            "object alternates present — the copy keeps pointing at the same borrowed object store"
                .to_string(),
        );
    }
    if has_split_index(git_path) {
        warnings
            .push("split index present — the shared index file travels with the copy".to_string());
    }
    if git_has_config_prefix(src, "^lfs\\.") {
        warnings.push(
            "git-lfs is configured — pointer files resolve against the copy's own lfs storage"
                .to_string(),
        );
    }

    warnings
}

fn is_bare(repo: &Path) -> bool {
    git_config_bool(repo, "core.bare")
}

fn git_config_bool(repo: &Path, key: &str) -> bool {
    git_cmd(repo)
        .args(["config", "--bool", key])
        .run()
        .map(|out| out.stdout.trim() == "true")
        .unwrap_or(false)
}

fn git_has_config_prefix(repo: &Path, pattern: &str) -> bool {
    git_cmd(repo)
        .args(["config", "--get-regexp", pattern])
        .run()
        .map(|out| !out.stdout.trim().is_empty())
        .unwrap_or(false)
}

fn has_split_index(git_path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(git_path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with("sharedindex.")
    })
}

/// Is `inner` at or below `outer`? Compared on the paths as given: both come
/// from the caller already canonicalised, and canonicalising here would resolve
/// a destination that does not exist yet to nothing.
fn is_inside(inner: &Path, outer: &Path) -> bool {
    inner == outer || inner.starts_with(outer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A repo with one commit, plus a second directory to clone into. Both live
    /// in one TempDir so the probe's volume check is satisfied by construction —
    /// the interesting probe cases below fail for reasons that are not the volume.
    fn setup() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().expect("temp dir");
        let repo = temp.path().join("repo");
        let dest_parent = temp.path().join("workspaces");
        fs::create_dir_all(&repo).expect("repo dir");
        fs::create_dir_all(&dest_parent).expect("dest dir");

        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
        ] {
            git_cmd(&repo).args(args).run().expect("git setup");
        }
        fs::write(repo.join("README.md"), "# Test").expect("write");
        git_cmd(&repo).args(["add", "."]).run().expect("git add");
        git_cmd(&repo)
            .args(["commit", "-m", "initial"])
            .run()
            .expect("git commit");

        (temp, repo, dest_parent)
    }

    /// Every guard call in these tests goes through this, so a guard that
    /// mutated the source would fail the test that called it rather than only
    /// the one test written to notice.
    fn tree_fingerprint(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut entries = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(bytes) = fs::read(&path) {
                    entries.push((path, bytes));
                }
            }
        }
        entries.sort();
        entries
    }

    fn never_stale(_: &LockFileInfo) -> bool {
        false
    }

    fn always_stale(_: &LockFileInfo) -> bool {
        true
    }

    // ── the probe ────────────────────────────────────────────────────────

    #[test]
    fn the_probe_answers_from_a_real_copy_not_from_a_filesystem_name() {
        let (_temp, repo, dest_parent) = setup();

        // Same filesystem, same everything — except the file the probe copies is
        // gone. A probe that concluded "APFS, therefore supported" would say yes
        // here; one that copies cannot.
        fs::remove_file(repo.join(".git").join("HEAD")).expect("remove HEAD");

        let support = probe_cow_support(&repo, &dest_parent);
        assert!(
            !support.is_supported(),
            "no HEAD to copy, so nothing was proven"
        );
        assert!(
            support.reason().unwrap_or_default().contains("HEAD"),
            "the reason must name what it could not copy: {support:?}"
        );
    }

    #[test]
    fn the_probe_leaves_nothing_behind_on_either_path() {
        let (_temp, repo, dest_parent) = setup();

        let supported = probe_cow_support(&repo, &dest_parent);
        assert!(
            probe_files_in(&dest_parent).is_empty(),
            "probe file survived a {supported:?} run"
        );

        fs::remove_file(repo.join(".git").join("HEAD")).expect("remove HEAD");
        let unsupported = probe_cow_support(&repo, &dest_parent);
        assert!(!unsupported.is_supported());
        assert!(
            probe_files_in(&dest_parent).is_empty(),
            "probe file survived the failure path"
        );
    }

    fn probe_files_in(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().starts_with(".tuic-cow-probe"))
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn the_probe_uses_the_nearest_existing_ancestor_of_a_destination_that_does_not_exist_yet() {
        let (_temp, repo, dest_parent) = setup();
        let not_created_yet = dest_parent.join("a").join("b").join("c");

        // The volume that matters is the one the copy will land on, and the only
        // directory that can be stat'ed is an ancestor.
        let support = probe_cow_support(&repo, &not_created_yet);
        assert!(
            support.is_supported(),
            "a destination that does not exist yet must not read as unsupported: {support:?}"
        );
        assert!(probe_files_in(&dest_parent).is_empty());
    }

    // ── the guards ───────────────────────────────────────────────────────

    #[test]
    fn a_destination_inside_the_source_is_refused() {
        let (_temp, repo, _dest_parent) = setup();
        let inside = repo.join("worktrees").join("feature");

        let err = check_creation_guards(&repo, &inside).expect_err("must refuse");
        assert!(err.contains("inside"), "{err}");
    }

    #[test]
    fn a_linked_worktree_source_is_refused_naming_the_shared_head() {
        let (_temp, repo, dest_parent) = setup();
        let linked = dest_parent.join("linked");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "feature",
                &linked.to_string_lossy(),
            ])
            .run()
            .expect("worktree add");
        assert!(
            linked.join(".git").is_file(),
            "a linked worktree's .git is a file"
        );

        let err =
            check_creation_guards(&linked, &dest_parent.join("clone")).expect_err("must refuse");
        assert!(
            err.contains("HEAD"),
            "the error must name the corruption: {err}"
        );
        assert!(
            err.contains("index"),
            "the error must name the corruption: {err}"
        );
    }

    #[test]
    fn every_operation_in_progress_marker_refuses() {
        for marker in OPERATION_MARKERS {
            let (_temp, repo, dest_parent) = setup();
            let path = repo.join(".git").join(marker);
            // rebase-merge and rebase-apply are directories, the rest are files.
            // Both are created here as files: the guard tests for existence, and
            // asserting on `exists()` is what keeps it honest for both shapes.
            fs::write(&path, "").expect("marker");

            let err = check_creation_guards(&repo, &dest_parent.join("clone"))
                .expect_err("an operation in progress must refuse");
            assert!(
                err.contains(marker),
                "the refusal must name {marker}: {err}"
            );
        }
    }

    #[test]
    fn a_live_lock_refuses_and_a_stale_one_does_not() {
        let (_temp, repo, dest_parent) = setup();
        let lock = repo.join(".git").join("index.lock");
        fs::write(&lock, "").expect("lock");
        let dest = dest_parent.join("clone");

        let err = check_creation_guards_inner(&repo, &dest, never_stale)
            .expect_err("live lock must refuse");
        assert!(err.contains("live git process"), "{err}");

        let report =
            check_creation_guards_inner(&repo, &dest, always_stale).expect("stale lock must pass");
        assert_eq!(
            report.stale_lock,
            Some(PathBuf::from(".git").join("index.lock")),
            "the stale lock is reported RELATIVE to the source, so the caller can only delete it inside the copy"
        );
    }

    #[test]
    fn a_stale_lock_is_not_removed_from_the_source() {
        let (_temp, repo, dest_parent) = setup();
        let lock = repo.join(".git").join("index.lock");
        fs::write(&lock, "").expect("lock");

        let before = tree_fingerprint(&repo);
        check_creation_guards_inner(&repo, &dest_parent.join("clone"), always_stale)
            .expect("passes");

        assert!(
            lock.exists(),
            "the guard dropped a lock in the SOURCE — it may only report it"
        );
        assert_eq!(
            before,
            tree_fingerprint(&repo),
            "the source repository was modified"
        );
    }

    #[test]
    fn the_inherited_stale_lock_is_dropped_in_the_copy_and_only_there() {
        let (_temp, repo, dest_parent) = setup();
        let source_lock = repo.join(".git").join("index.lock");
        fs::write(&source_lock, "").expect("lock");
        let dest = dest_parent.join("clone");

        let report = check_creation_guards_inner(&repo, &dest, always_stale).expect("passes");

        // Stand in for the clone: the same relative path, in the copy.
        let copied_lock = dest.join(".git").join("index.lock");
        fs::create_dir_all(copied_lock.parent().expect("parent")).expect("copy dirs");
        fs::write(&copied_lock, "").expect("copied lock");

        let dropped = drop_inherited_stale_lock(&dest, &report).expect("the lock was dropped");

        assert_eq!(dropped, copied_lock);
        assert!(
            !copied_lock.exists(),
            "the copy still holds the inherited lock"
        );
        assert!(
            source_lock.exists(),
            "the SOURCE lock was deleted — it must never be"
        );
    }

    #[test]
    fn a_refusal_leaves_the_source_untouched() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join(".git").join("MERGE_HEAD"), "deadbeef").expect("marker");
        fs::write(repo.join(".git").join("index.lock"), "").expect("lock");

        let before = tree_fingerprint(&repo);
        check_creation_guards_inner(&repo, &dest_parent.join("clone"), never_stale)
            .expect_err("refuses");

        assert_eq!(
            before,
            tree_fingerprint(&repo),
            "a guard repaired the source instead of refusing"
        );
    }

    /// A real bare repo, not a fabricated one: `git init --bare` puts the admin
    /// files at the root and creates no `.git` directory, so the layout check
    /// would call it "not a git repository". The refusal has to come from the
    /// config, and it has to come first.
    #[test]
    fn a_bare_repository_is_refused() {
        let temp = TempDir::new().expect("temp");
        let bare = temp.path().join("bare.git");
        fs::create_dir_all(&bare).expect("dir");
        git_cmd(&bare)
            .args(["init", "--bare"])
            .run()
            .expect("init bare");
        assert!(
            !bare.join(".git").exists(),
            "a bare repo has no .git directory"
        );

        let err =
            check_creation_guards(&bare, &temp.path().join("clone")).expect_err("must refuse");
        assert!(err.contains("bare"), "{err}");
    }

    #[test]
    fn unusual_repo_shapes_warn_without_blocking() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join(".gitmodules"), "[submodule \"x\"]\n\tpath = x\n").expect("gitmodules");
        git_cmd(&repo)
            .args(["config", "core.sparseCheckout", "true"])
            .run()
            .expect("sparse");
        let info = repo.join(".git").join("objects").join("info");
        fs::create_dir_all(&info).expect("info dir");
        fs::write(info.join("alternates"), "/some/other/objects\n").expect("alternates");
        fs::write(repo.join(".git").join("sharedindex.abc123"), "").expect("shared index");
        git_cmd(&repo)
            .args(["config", "lfs.repositoryformatversion", "0"])
            .run()
            .expect("lfs");

        let report =
            check_creation_guards(&repo, &dest_parent.join("clone")).expect("must not block");

        let joined = report.warnings.join("\n");
        for expected in [
            "submodules",
            "sparse-checkout",
            "alternates",
            "split index",
            "git-lfs",
        ] {
            assert!(
                joined.contains(expected),
                "missing a warning for {expected}: {joined}"
            );
        }
    }

    #[test]
    fn a_clean_repository_passes_with_nothing_to_report() {
        let (_temp, repo, dest_parent) = setup();

        let report = check_creation_guards(&repo, &dest_parent.join("clone")).expect("passes");

        assert_eq!(report, GuardReport::default());
    }
}
