//! Copy-on-write workspace creation: the capability probe, the guards that run
//! before anything is copied, and the clone itself.
//!
//! A COW workspace is a `clonefile` copy of a whole repository directory, which
//! makes it an *independent repository* — two of them can sit on the same branch,
//! and `node_modules`/`target` arrive warm. The pre-flight half answers two
//! questions: can this pair of paths do COW at all, and is this repo in a shape
//! that can be cloned safely. The creation half copies, then applies the fixups
//! without which the copy is not a usable repository.
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

// `worktree::create_workspace` composes everything below; its own caller is a
// transport in #734-ca73 and the UI in #735-55d7. Until one of them lands the
// whole chain is unreachable from `main`. Remove this attribute with them.
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

/// A COW workspace as the persisted document describes it.
///
/// `git worktree list` in the parent has never heard of a COW clone, so this is
/// the only record that it exists. It comes from `repositories.json`, which the
/// backend already owns (`config.rs`, behind the cross-process file lock) and
/// every client already syncs — rather than a second registry that could
/// disagree with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CowRecord {
    pub(crate) workspace_id: String,
    pub(crate) branch: String,
    pub(crate) path: PathBuf,
    pub(crate) parent_repo: PathBuf,
}

/// Mint an id for a new COW workspace.
///
/// Mirrors the frontend's `generateWorkspaceId`: the branch is kept in the id
/// for legibility in logs and paths, but it is a LABEL — the suffix is what
/// makes it unique, and a reader that wants the branch reads `branch`. Minted
/// in Rust because creation happens here: an id invented by whichever client
/// happened to ask would not exist for the other transports.
pub(crate) fn mint_workspace_id(branch: &str, taken: &[String]) -> String {
    let stem = sanitize_for_id(branch);
    let taken: std::collections::HashSet<&str> = taken.iter().map(String::as_str).collect();
    // 32 bits collides at about one in four billion, but uniqueness here is a
    // correctness property — two workspaces sharing an id lose each other's
    // terminals — so it is checked rather than assumed.
    for _ in 0..100 {
        let candidate = format!("{stem}~{:08x}", rand::random::<u32>());
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    // 100 collisions against the same set is not a thing that happens; if it
    // did, a nanosecond-suffixed id is still unique and still legible.
    format!(
        "{stem}~{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    )
}

fn sanitize_for_id(branch: &str) -> String {
    let mut out = String::with_capacity(branch.len());
    for ch in branch.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    let clipped: String = trimmed.chars().take(60).collect();
    if clipped.is_empty() {
        "workspace".to_string()
    } else {
        clipped
    }
}

/// Every COW workspace `base_repo` owns, according to the persisted document.
pub(crate) fn cow_workspaces_for(base_repo: &Path) -> Vec<CowRecord> {
    cow_workspaces_in(&crate::config::load_repositories(), base_repo)
}

/// [`cow_workspaces_for`] over an explicit document, so the parsing is testable
/// without touching the user's real config.
///
/// Reads the `workspaces` map only. A document still in the pre-migration
/// `branches` shape contributes nothing, and that is correct rather than a gap:
/// no COW workspace can exist in a document written before COW workspaces did.
pub(crate) fn cow_workspaces_in(doc: &serde_json::Value, base_repo: &Path) -> Vec<CowRecord> {
    let Some(repos) = doc.get("repos").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };

    let mut records = Vec::new();
    for (repo_path, repo) in repos {
        if !same_path(Path::new(repo_path), base_repo) {
            continue;
        }
        let Some(workspaces) = repo
            .get("workspaces")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (workspace_id, workspace) in workspaces {
            if workspace.get("kind").and_then(serde_json::Value::as_str) != Some("cow") {
                continue;
            }
            let Some(path) = workspace
                .get("worktreePath")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            records.push(CowRecord {
                workspace_id: workspace_id.clone(),
                branch: workspace
                    .get("branchName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(workspace_id)
                    .to_string(),
                path: PathBuf::from(path),
                parent_repo: workspace
                    .get("parentRepoPath")
                    .and_then(serde_json::Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(repo_path)),
            });
        }
    }
    records
}

/// Two paths naming the same directory, symlinks and trailing slashes aside.
/// A repo reached as `/Users/x/repo` and as `/Users/x/repo/` is one repo, and a
/// record filed under either spelling has to be found by the other.
fn same_path(left: &Path, right: &Path) -> bool {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(left) == canonical(right)
}

/// Is this record still describing something that exists?
///
/// The realistic failure is not a hostile document — the user IS the trust
/// boundary here — it is a stale one: a workspace the user deleted in Finder,
/// or a directory that moved. Answering "gone" is what lets a caller say so
/// instead of failing inside git with something unrelated.
pub(crate) fn cow_record_is_live(record: &CowRecord) -> bool {
    record.path.join(".git").is_dir()
}

/// Which mechanism the caller is asking for.
///
/// The caller asks for a WORKSPACE, not a mechanism — so `Auto` is the default
/// and the other two exist for callers who have a reason to care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceMode {
    /// COW when the filesystem and the repo shape allow it, a linked worktree
    /// otherwise — reporting which path it took and why.
    #[default]
    Auto,
    /// COW or nothing. Fails naming the check that said no, because a caller
    /// asking for `cow` specifically wants the isolation, and a silent
    /// worktree would give it completely different semantics.
    Cow,
    /// The old behaviour, even where COW is available.
    Worktree,
}

/// Which mechanism a workspace actually got. Persisted on the workspace record
/// so downstream lifecycle code — publish, remove — never has to guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceKind {
    Cow,
    Worktree,
}

/// The decision, with everything the chosen path needs to proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mechanism {
    /// Guards passed and the probe said yes; the report travels because the
    /// clone needs its stale-lock entry and the caller needs its warnings.
    Cow(GuardReport),
    /// A linked worktree. `degraded_reason` is `Some` only when the caller
    /// asked for `Auto` and COW was unavailable — `mode=worktree` is a choice,
    /// not a degradation, and reporting it as one would be a lie.
    Worktree { degraded_reason: Option<String> },
}

/// Decide how to build the workspace `mode` asks for.
///
/// The guards run for `Auto` and `Cow` only: they are about whether this repo
/// can be CLONED, and a linked worktree is not a clone. Refusing `mode=worktree`
/// because the source repo has an unfinished rebase would block the one
/// mechanism that never had that constraint.
pub(crate) fn choose_mechanism(
    src: &Path,
    dest: &Path,
    mode: WorkspaceMode,
) -> Result<Mechanism, String> {
    choose_mechanism_with(src, dest, mode, probe_cow_support)
}

/// [`choose_mechanism`] with the probe injected, so a test can force the
/// degrade without needing a second filesystem to fail against.
pub(crate) fn choose_mechanism_with(
    src: &Path,
    dest: &Path,
    mode: WorkspaceMode,
    probe: impl Fn(&Path, &Path) -> CowSupport,
) -> Result<Mechanism, String> {
    if mode == WorkspaceMode::Worktree {
        return Ok(Mechanism::Worktree {
            degraded_reason: None,
        });
    }

    let dest_parent = dest.parent().unwrap_or(dest);
    let refusal = match check_creation_guards(src, dest) {
        Ok(guards) => match probe(src, dest_parent) {
            CowSupport::Supported => return Ok(Mechanism::Cow(guards)),
            CowSupport::Unsupported(reason) => reason,
        },
        Err(reason) => reason,
    };

    match mode {
        // Loud, naming the check that said no.
        WorkspaceMode::Cow => Err(format!(
            "a copy-on-write workspace was requested but is not available here: {refusal}"
        )),
        // The caller asked for a workspace, and a linked worktree is one — with
        // different isolation semantics, which is why the reason travels with it.
        _ => Ok(Mechanism::Worktree {
            degraded_reason: Some(refusal),
        }),
    }
}

/// What to do with the parent's uncommitted work. Three states, three prices,
/// and only the third costs anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirtyPolicy {
    /// Carry the parent's work in progress over. Free — doing nothing writes no
    /// blocks — and the default for exactly that reason.
    #[default]
    Inherit,
    /// `clean -fdq`, deliberately WITHOUT `-x`: unlinking is metadata only, and
    /// the ignored build artifacts are the point of the clone.
    CleanUntracked,
    /// `reset --hard --recurse-submodules` then the same clean. The paid one:
    /// every rewritten block stops being shared. Measured 15 MB -> 113 MB.
    Clean,
}

/// A COW workspace that now exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CowWorkspace {
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    /// Repo shapes worth telling the caller about, from the guards.
    pub(crate) warnings: Vec<String>,
    /// How many paths the parent's working tree carried into this workspace.
    /// Reported so a model does not read inherited WIP as its own bug.
    pub(crate) carried_over: usize,
    pub(crate) dirty_policy: DirtyPolicy,
}

/// Clone `src` to `dest` copy-on-write and make the copy a usable, independent
/// repository on `branch`.
///
/// Assumes [`check_creation_guards`] passed and [`probe_cow_support`] said yes;
/// the caller (`mode=auto`) degrades to a linked worktree when either refused.
///
/// **There is no full-copy fallback, on purpose.** The PoC fell back to a plain
/// recursive copy, which on the repo it was measured against would silently
/// turn a 19 MB, 26 s clone into a 12 GB one. A `cp -c -R` that fails after the
/// probe succeeded means something changed underneath us, and the honest answer
/// is an error the caller can degrade from.
pub(crate) fn create_cow_workspace(
    src: &Path,
    dest: &Path,
    branch: &str,
    dirty: DirtyPolicy,
    guards: &GuardReport,
) -> Result<CowWorkspace, String> {
    if dest.exists() {
        return Err(format!("destination '{}' already exists", dest.display()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create '{}': {e}", parent.display()))?;
    }

    let out = Command::new("cp")
        .arg("-c")
        .arg("-R")
        .arg(src)
        .arg(dest)
        .output()
        .map_err(|e| format!("could not run cp: {e}"))?;
    if !out.status.success() {
        // Leave nothing half-copied behind for the next attempt to trip over.
        let _ = std::fs::remove_dir_all(dest);
        return Err(format!(
            "copy-on-write clone of '{}' failed: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    match fixup_clone(src, dest, branch, dirty, guards) {
        Ok(carried_over) => Ok(CowWorkspace {
            path: dest.to_path_buf(),
            branch: branch.to_string(),
            warnings: guards.warnings.clone(),
            carried_over,
            dirty_policy: dirty,
        }),
        Err(e) => {
            // A clone that failed its fixup is not a workspace: its .git/worktrees
            // still points at the parent's admin dirs and its gc is unbounded.
            // Leaving it on disk would hand the caller something that looks usable.
            let _ = std::fs::remove_dir_all(dest);
            Err(e)
        }
    }
}

/// Everything a raw `cp -c -R` of a repository still needs. None of this is
/// precautionary — each step fixes something the PoC observed break.
fn fixup_clone(
    src: &Path,
    dest: &Path,
    branch: &str,
    dirty: DirtyPolicy,
    guards: &GuardReport,
) -> Result<usize, String> {
    // Debris inherited from the source, dropped HERE, in the copy.
    drop_inherited_stale_lock(dest, guards);

    // Inherited entries point at the PARENT's worktree directories and BLOCK
    // checkout of those branches here: "fatal: 'x' is already used by worktree
    // at ...". This is the fixup without which the clone cannot do the one
    // thing it exists for.
    let inherited_worktrees = dest.join(".git").join("worktrees");
    if inherited_worktrees.exists() {
        std::fs::remove_dir_all(&inherited_worktrees)
            .map_err(|e| format!("could not drop inherited worktree admin entries: {e}"))?;
    }

    // A gc rewrites packfiles, and every rewritten block stops being shared —
    // real disk goes from ~0 back to the full size of the repo.
    config_or_fail(dest, "gc.auto", "0")?;
    // There is no fetch-only remote in git: `remote add` alone still permits a
    // push, so the push URL is set to one that cannot resolve.
    let _ = git_cmd(dest)
        .args(["remote", "add", "parent", &src.to_string_lossy()])
        .run();
    config_or_fail(dest, "remote.parent.pushurl", NO_PUSH_URL)?;
    // Inherited fsmonitor state describes the parent's path, not this one.
    config_or_fail(dest, "core.fsmonitor", "false")?;

    apply_dirty_policy(dest, dirty)?;

    // `clonefile` preserves mtime but changes ino and ctime, so every index
    // entry reads stat-dirty and the first `git status` re-hashes the whole
    // tree. Pay it here, inside creation, instead of in whatever command the
    // agent happens to run first.
    let _ = git_cmd(dest).args(["update-index", "--refresh"]).run();

    checkout_branch(dest, branch)?;

    Ok(dirty_path_count(dest))
}

/// Put the workspace on `branch`, whether or not the ref already exists.
///
/// The clone inherited every ref the parent had, so for the case this whole
/// feature exists for — a second workspace on a branch someone is already
/// working on — the branch is already there and `checkout -b` fails with
/// "a branch named 'x' already exists". A COW workspace's isolation comes from
/// being an independent repository, not from the branch being new.
fn checkout_branch(dest: &Path, branch: &str) -> Result<(), String> {
    let exists = git_cmd(dest)
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .run()
        .is_ok();

    let args: Vec<&str> = if exists {
        vec!["checkout", branch]
    } else {
        vec!["checkout", "-b", branch]
    };

    git_cmd(dest)
        .args(args)
        .run()
        .map(|_| ())
        .map_err(|e| format!("could not check out '{branch}' in the workspace: {e}"))
}

/// A URL git will accept as configuration and can never push to. Not a real
/// scheme on purpose: the failure has to be "this remote cannot be pushed to",
/// not "this host is unreachable", which a firewall could turn into a hang.
const NO_PUSH_URL: &str = "no-push://tuic-workspace-parent";

fn config_or_fail(repo: &Path, key: &str, value: &str) -> Result<(), String> {
    git_cmd(repo)
        .args(["config", key, value])
        .run()
        .map(|_| ())
        .map_err(|e| format!("could not set {key} in the workspace: {e}"))
}

fn apply_dirty_policy(dest: &Path, dirty: DirtyPolicy) -> Result<(), String> {
    match dirty {
        // Free: nothing runs, nothing is written, the parent's work in progress
        // is simply there.
        DirtyPolicy::Inherit => Ok(()),
        DirtyPolicy::CleanUntracked => clean_untracked(dest),
        DirtyPolicy::Clean => {
            // `--recurse-submodules` is required: a plain reset does not descend,
            // and leaves submodules modified (observed: ` M plugins` survived).
            git_cmd(dest)
                .args(["reset", "--hard", "--recurse-submodules", "HEAD"])
                .run()
                .map_err(|e| format!("could not reset the workspace: {e}"))?;
            clean_untracked(dest)
        }
    }
}

/// `-fd`, never `-fdx`. The ignored build artifacts (`node_modules`, `target`)
/// arrived warm at near-zero cost and are the reason to clone at all.
fn clean_untracked(dest: &Path) -> Result<(), String> {
    git_cmd(dest)
        .args(["clean", "-fdq"])
        .run()
        .map(|_| ())
        .map_err(|e| format!("could not clean the workspace: {e}"))
}

fn dirty_path_count(repo: &Path) -> usize {
    git_cmd(repo)
        .args(["status", "--porcelain"])
        .run()
        .map(|out| out.stdout.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

/// What `publish` did, step by step.
///
/// Two steps that fail independently, reported independently: getting the work
/// into the parent is what makes it reachable at all, and pushing to origin is
/// what makes it reachable by anyone else. A failure to reach origin must not
/// read as "publish failed" when the parent already has the commits.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PublishOutcome {
    /// The parent's `refs/heads/<branch>` now points at the workspace tip.
    pub(crate) parent_updated: bool,
    /// Why it does not, when it does not.
    pub(crate) parent_error: Option<String>,
    /// The commit the parent's branch was moved to, or would be.
    pub(crate) published_commit: Option<String>,
    pub(crate) origin_pushed: bool,
    pub(crate) origin_error: Option<String>,
    /// Set when there was nothing to do — a linked worktree already shares its
    /// refs with the parent, so "publish" is a question that does not apply.
    pub(crate) no_op_reason: Option<String>,
}

/// The ref a publish stages the workspace tip under, inside the parent.
///
/// Staging first separates object transfer from ref policy: after this the
/// commits are IN the parent's object store, so an ancestry check and an
/// atomic ref update are local operations that cannot half-succeed.
///
/// The id cannot go in verbatim. `~` is exactly the character `mint_workspace_id`
/// uses to mark an id as minted — because git forbids it in a branch name — and
/// git forbids it in ANY ref name, so a refspec built from a raw id is rejected
/// with "invalid refspec" before anything is transferred.
fn staged_ref(workspace_id: &str) -> String {
    let safe: String = workspace_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!("refs/tuic/published/{}", safe.trim_matches('.'))
}

/// Get a COW workspace's commits into the parent, then out to origin.
///
/// A COW clone is an independent repository: its commits exist ONLY there until
/// this runs. `git merge <branch>` in the parent does not find them — and if a
/// same-named branch exists in the parent, it silently merges that stale ref
/// instead, which is the failure this function exists to prevent.
pub(crate) fn publish_cow_workspace(record: &CowRecord) -> Result<PublishOutcome, String> {
    if !cow_record_is_live(record) {
        return Err(format!(
            "the workspace directory '{}' is gone — nothing to publish",
            record.path.display()
        ));
    }

    let branch = record.branch.clone();
    validate_branch_name(&branch)?;

    let tip = git_cmd(&record.path)
        .args(["rev-parse", "HEAD"])
        .run()
        .map_err(|e| format!("could not read the workspace's HEAD: {e}"))?
        .stdout
        .trim()
        .to_string();

    let mut outcome = PublishOutcome {
        published_commit: Some(tip.clone()),
        ..Default::default()
    };

    // Step 1: the parent. Fetch into a staging ref rather than straight into
    // refs/heads/<branch> — one git call cannot both transfer objects and let
    // us decide the ref policy, and a fetch into a checked-out branch is
    // refused with an error about the wrong thing.
    let staged = staged_ref(&record.workspace_id);
    match git_cmd(&record.parent_repo)
        .args([
            "fetch",
            &record.path.to_string_lossy(),
            &format!("+refs/heads/{branch}:{staged}"),
        ])
        .run()
    {
        Ok(_) => match update_parent_branch(&record.parent_repo, &branch, &tip) {
            Ok(()) => outcome.parent_updated = true,
            Err(e) => outcome.parent_error = Some(e),
        },
        Err(e) => {
            outcome.parent_error = Some(format!(
                "could not fetch the workspace into the parent: {e}"
            ));
        }
    }

    // Step 2: origin. Independent of step 1 on purpose — the commits reaching
    // the parent is worth reporting even when the network is down, and a
    // failure here must not roll back what already landed.
    if git_cmd(&record.path)
        .args(["remote", "get-url", "origin"])
        .run()
        .is_err()
    {
        outcome.origin_error = Some("the workspace has no 'origin' remote".to_string());
        return Ok(outcome);
    }
    match git_cmd(&record.path)
        .args([
            "push",
            "origin",
            &format!("refs/heads/{branch}:refs/heads/{branch}"),
        ])
        .timeout(crate::git_cli::FETCH_TIMEOUT)
        .run()
    {
        Ok(_) => outcome.origin_pushed = true,
        Err(e) => outcome.origin_error = Some(format!("could not push to origin: {e}")),
    }

    Ok(outcome)
}

/// Move the parent's branch to `tip`, or explain why not.
///
/// Fast-forward only. "Update the ref" does not authorise destroying commits:
/// with two workspaces on one branch a divergent parent branch is the NORMAL
/// case, and force-updating would silently orphan whichever side published
/// second. The update is a compare-and-swap against the ref we just checked,
/// so a publish racing another one fails instead of overwriting it.
fn update_parent_branch(parent: &Path, branch: &str, tip: &str) -> Result<(), String> {
    let target = format!("refs/heads/{branch}");

    // A branch checked out in the parent (or in one of its linked worktrees)
    // must not be moved behind its working tree: the ref would disagree with
    // the index and the files. Publish is not an implicit checkout.
    if let Some(where_checked_out) = branch_checkout_location(parent, branch) {
        return Err(format!(
            "'{branch}' is checked out at '{where_checked_out}' in the parent, so publishing cannot move it. \
             The commits are in the parent's object store — switch that checkout to another branch and \
             publish again, or merge them there yourself."
        ));
    }

    let current = git_cmd(parent)
        .args(["rev-parse", "--verify", "--quiet", &target])
        .run()
        .ok()
        .map(|out| out.stdout.trim().to_string())
        .filter(|s| !s.is_empty());

    match current {
        None => git_cmd(parent)
            .args(["update-ref", &target, tip])
            .run()
            .map(|_| ())
            .map_err(|e| format!("could not create '{target}' in the parent: {e}")),
        Some(old) if old == tip => Ok(()),
        Some(old) => {
            let is_ancestor = git_cmd(parent)
                .args(["merge-base", "--is-ancestor", &old, tip])
                .run()
                .is_ok();
            if !is_ancestor {
                return Err(format!(
                    "the parent's '{branch}' is at {} and carries commits this workspace does not have, so \
                     publishing would lose them. Merge or rebase first.",
                    &old[..old.len().min(8)]
                ));
            }
            git_cmd(parent)
                .args(["update-ref", &target, tip, &old])
                .run()
                .map(|_| ())
                .map_err(|e| {
                    format!("could not fast-forward '{target}' in the parent (it moved underneath us): {e}")
                })
        }
    }
}

/// Where `branch` is checked out in `parent` or any of its linked worktrees,
/// if it is. `git worktree list --porcelain` reports the main checkout too, so
/// one scan answers for both.
fn branch_checkout_location(parent: &Path, branch: &str) -> Option<String> {
    let out = git_cmd(parent)
        .args(["worktree", "list", "--porcelain"])
        .run()
        .ok()?;
    let mut current_path: Option<String> = None;
    for line in out.stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(path.to_string());
        } else if line.strip_prefix("branch ") == Some(&format!("refs/heads/{branch}")) {
            return current_path;
        }
    }
    None
}

/// Refuse anything git would not accept as a branch, before it is pasted into
/// a refspec.
fn validate_branch_name(branch: &str) -> Result<(), String> {
    if branch.is_empty() {
        return Err("the workspace has no branch to publish".to_string());
    }
    git_cmd(Path::new("."))
        .args(["check-ref-format", "--branch", branch])
        .run()
        .map(|_| ())
        .map_err(|_| format!("'{branch}' is not a valid branch name"))
}

/// The ref namespace a COW workspace mirrors its parent's branches into, so
/// "reachable from the parent" is a local question.
const PARENT_MIRROR_GLOB: &str = "refs/parent";

/// How many commits exist ONLY in this workspace.
///
/// Counts what is reachable from HEAD and from no remote and no mirrored parent
/// ref. That is the number a removal would destroy — a linked worktree has no
/// equivalent, because its objects live in the parent and survive the
/// directory.
///
/// The parent mirror is refreshed first, so a commit published a moment ago
/// does not still read as unpublished. A failure to refresh is deliberately
/// non-fatal: the count then errs high, which refuses a removal that might
/// have been safe rather than allowing one that is not.
pub(crate) fn unpublished_commit_count(record: &CowRecord) -> Result<usize, String> {
    let _ = git_cmd(&record.path)
        .args(["fetch", "-q", "parent", "+refs/heads/*:refs/parent/*"])
        .run();

    let out = git_cmd(&record.path)
        .args([
            "rev-list",
            "--count",
            "HEAD",
            "--not",
            "--glob=refs/remotes",
            &format!("--glob={PARENT_MIRROR_GLOB}"),
        ])
        .run()
        .map_err(|e| format!("could not count unpublished commits: {e}"))?;

    out.stdout
        .trim()
        .parse()
        .map_err(|e| format!("could not read the unpublished commit count: {e}"))
}

/// Delete a COW workspace, refusing while it holds commits that exist nowhere
/// else.
///
/// Removal here is an `rm -rf` of an independent repository. Every unpublished
/// commit lives ONLY in it — a failure mode a linked worktree does not have,
/// because its objects are in the parent and outlive the directory. So the
/// count is a gate, not a warning.
pub(crate) fn remove_cow_workspace(record: &CowRecord, force: bool) -> Result<usize, String> {
    if !record.path.exists() {
        // Already gone. Idempotent on purpose: the caller's next step is to drop
        // the row, and refusing here would strand it forever.
        return Ok(0);
    }
    if !cow_record_is_live(record) {
        return Err(format!(
            "'{}' does not look like a COW workspace any more (no .git directory) — \
             refusing to delete a directory this record may no longer describe",
            record.path.display()
        ));
    }

    let unpublished = unpublished_commit_count(record)?;
    if unpublished > 0 && !force {
        return Err(format!(
            "{unpublished} commit{} in '{}' exist{} only there: this is an independent clone, so \
             removing it destroys them for good. Publish first, or remove with force to lose them.",
            if unpublished == 1 { "" } else { "s" },
            record.workspace_id,
            if unpublished == 1 { "s" } else { "" },
        ));
    }

    std::fs::remove_dir_all(&record.path)
        .map_err(|e| format!("could not remove '{}': {e}", record.path.display()))?;

    Ok(unpublished)
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

    // ── the clone ────────────────────────────────────────────────────────

    /// Guard-check then clone, the pair the caller always runs together.
    fn clone_into(repo: &Path, dest: &Path, branch: &str, dirty: DirtyPolicy) -> CowWorkspace {
        let guards = check_creation_guards(repo, dest).expect("guards pass");
        create_cow_workspace(repo, dest, branch, dirty, &guards).expect("clone succeeds")
    }

    fn git_status(repo: &Path) -> String {
        git_cmd(repo)
            .args(["status", "--porcelain"])
            .run()
            .expect("status")
            .stdout
    }

    /// The fixup this whole mechanism exists for. The parent holds `taken` in a
    /// linked worktree, so its `.git/worktrees` entry travels with the copy and
    /// git in the clone refuses the checkout with "already used by worktree at"
    /// — pointing at a directory belonging to the parent.
    #[test]
    fn the_clone_can_check_out_a_branch_the_parent_holds_in_a_linked_worktree() {
        let (_temp, repo, dest_parent) = setup();
        let parent_worktree = dest_parent.join("parent-wt");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "taken",
                &parent_worktree.to_string_lossy(),
            ])
            .run()
            .expect("worktree add");
        assert!(repo.join(".git").join("worktrees").exists());

        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "taken", DirtyPolicy::Inherit);

        assert!(
            !dest.join(".git").join("worktrees").exists(),
            "the inherited worktree admin entries were not dropped"
        );
        let head = git_cmd(&workspace.path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .run()
            .expect("rev-parse")
            .stdout;
        assert_eq!(head.trim(), "taken");
    }

    #[test]
    fn the_clone_can_fetch_from_the_parent_but_not_push_to_it() {
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);

        git_cmd(&workspace.path)
            .args(["fetch", "parent"])
            .run()
            .expect("fetch from the parent must work — that is how work gets published");

        let pushed = git_cmd(&workspace.path)
            .args(["push", "parent", "HEAD:refs/heads/should-never-arrive"])
            .run();
        assert!(pushed.is_err(), "push to the parent must fail");
        assert!(
            git_cmd(&repo)
                .args(["rev-parse", "--verify", "should-never-arrive"])
                .run()
                .is_err(),
            "the push reached the parent anyway"
        );
    }

    #[test]
    fn the_fixup_pins_gc_and_turns_the_inherited_fsmonitor_off() {
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        // The parent's fsmonitor setting describes the parent's path, and travels.
        git_cmd(&repo)
            .args(["config", "core.fsmonitor", "true"])
            .run()
            .expect("set fsmonitor");

        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);

        assert_eq!(read_config(&workspace.path, "gc.auto"), "0");
        assert_eq!(read_config(&workspace.path, "core.fsmonitor"), "false");
        assert_eq!(
            read_config(&workspace.path, "remote.parent.pushurl"),
            NO_PUSH_URL
        );
    }

    fn read_config(repo: &Path, key: &str) -> String {
        git_cmd(repo)
            .args(["config", "--get", key])
            .run()
            .map(|out| out.stdout.trim().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn inherit_carries_the_parents_modified_paths_over() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");
        fs::write(repo.join("scratch.txt"), "untracked\n").expect("untracked");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );

        let status = git_status(&workspace.path);
        assert!(
            status.contains("README.md"),
            "tracked modification lost: {status:?}"
        );
        assert!(
            status.contains("scratch.txt"),
            "untracked file lost: {status:?}"
        );
        assert_eq!(
            workspace.carried_over, 2,
            "the count is what tells a model this WIP is not its own bug"
        );
        assert_eq!(
            workspace.dirty_policy,
            DirtyPolicy::default(),
            "inherit is the default"
        );
    }

    #[test]
    fn clean_untracked_removes_untracked_files_but_keeps_ignored_build_artifacts() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join(".gitignore"), "node_modules/\ntarget/\n").expect("gitignore");
        git_cmd(&repo)
            .args(["add", ".gitignore"])
            .run()
            .expect("add");
        git_cmd(&repo)
            .args(["commit", "-m", "ignore build output"])
            .run()
            .expect("commit");
        fs::create_dir_all(repo.join("node_modules").join("left-pad")).expect("node_modules");
        fs::write(
            repo.join("node_modules").join("left-pad").join("index.js"),
            "module.exports = 1;\n",
        )
        .expect("artifact");
        fs::write(repo.join("scratch.txt"), "untracked\n").expect("untracked");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::CleanUntracked,
        );

        assert!(
            !workspace.path.join("scratch.txt").exists(),
            "the untracked file survived the clean"
        );
        assert!(
            workspace
                .path
                .join("node_modules")
                .join("left-pad")
                .join("index.js")
                .exists(),
            "the warm build artifacts were deleted — they are the reason to clone at all"
        );
    }

    #[test]
    fn clean_resets_a_modified_submodule_which_a_plain_reset_leaves_alone() {
        let (temp, repo, dest_parent) = setup();

        // A local submodule, which modern git refuses over the file transport
        // unless asked explicitly.
        let sub = temp.path().join("sub-origin");
        fs::create_dir_all(&sub).expect("sub dir");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
        ] {
            git_cmd(&sub).args(args).run().expect("sub setup");
        }
        fs::write(sub.join("lib.txt"), "original\n").expect("sub file");
        git_cmd(&sub).args(["add", "."]).run().expect("sub add");
        git_cmd(&sub)
            .args(["commit", "-m", "sub initial"])
            .run()
            .expect("sub commit");

        git_cmd(&repo)
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub.to_string_lossy(),
                "vendor",
            ])
            .run()
            .expect("submodule add");
        git_cmd(&repo)
            .args(["commit", "-m", "add submodule"])
            .run()
            .expect("commit submodule");

        // Dirty the submodule's working tree, the case a plain `reset --hard`
        // does not descend into (observed as a surviving ` M plugins`).
        fs::write(repo.join("vendor").join("lib.txt"), "edited\n").expect("dirty submodule");
        assert!(
            git_status(&repo).contains("vendor"),
            "the fixture must start dirty"
        );

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Clean,
        );

        let status = git_status(&workspace.path);
        assert!(
            !status.contains("vendor"),
            "the submodule is still modified — the reset did not recurse: {status:?}"
        );
        assert_eq!(
            fs::read_to_string(workspace.path.join("vendor").join("lib.txt")).expect("read"),
            "original\n"
        );
    }

    /// `clonefile` keeps mtime but changes ino and ctime, so every index entry
    /// reads stat-dirty until something refreshes it. Without the refresh inside
    /// creation, the first `status` an agent runs re-hashes the whole tree —
    /// and on a repo with a stale index it reports modifications that are not.
    #[test]
    fn the_first_status_in_a_new_workspace_reports_nothing_spurious() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("a.txt"), "one\n").expect("a");
        fs::write(repo.join("b.txt"), "two\n").expect("b");
        git_cmd(&repo).args(["add", "."]).run().expect("add");
        git_cmd(&repo)
            .args(["commit", "-m", "two files"])
            .run()
            .expect("commit");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );

        assert_eq!(
            git_status(&workspace.path).trim(),
            "",
            "the clone reports phantom changes"
        );
        assert_eq!(workspace.carried_over, 0);
    }

    /// The thing `git worktree` cannot do, and the reason this feature exists:
    /// `git worktree add` refuses a branch that is already checked out
    /// ("fatal: 'x' is already used by worktree at ..."). Two independent
    /// clones have no such relationship, and a commit in one is invisible to
    /// the other until it is published.
    #[test]
    fn two_workspaces_can_sit_on_one_branch_and_do_not_see_each_other() {
        let (_temp, repo, dest_parent) = setup();
        git_cmd(&repo)
            .args(["branch", "shared"])
            .run()
            .expect("branch");

        let first = clone_into(
            &repo,
            &dest_parent.join("one"),
            "shared",
            DirtyPolicy::Inherit,
        );
        let second = clone_into(
            &repo,
            &dest_parent.join("two"),
            "shared",
            DirtyPolicy::Inherit,
        );

        assert_eq!(first.branch, second.branch);
        assert_ne!(first.path, second.path);

        fs::write(first.path.join("only-here.txt"), "first\n").expect("write");
        git_cmd(&first.path).args(["add", "."]).run().expect("add");
        git_cmd(&first.path)
            .args(["commit", "-m", "work in the first workspace"])
            .run()
            .expect("commit");

        let second_head = git_cmd(&second.path)
            .args(["rev-parse", "HEAD"])
            .run()
            .expect("rev-parse")
            .stdout;
        let first_head = git_cmd(&first.path)
            .args(["rev-parse", "HEAD"])
            .run()
            .expect("rev-parse")
            .stdout;
        assert_ne!(
            first_head.trim(),
            second_head.trim(),
            "the two workspaces share a HEAD — they are not independent repositories"
        );
        assert!(!second.path.join("only-here.txt").exists());
        assert!(
            !repo.join("only-here.txt").exists(),
            "the commit reached the parent's working tree"
        );
    }

    // ── publish ──────────────────────────────────────────────────────────

    /// A COW workspace with `n` commits of its own, and the record that names
    /// it. Built through the real creation path so publish is tested against a
    /// clone with the real fixups, not a hand-made directory.
    fn published_fixture(temp: &TempDir, repo: &Path, branch: &str, commits: usize) -> CowRecord {
        let dest = temp.path().join(format!("ws-{branch}"));
        let guards = check_creation_guards(repo, &dest).expect("guards");
        let workspace = create_cow_workspace(repo, &dest, branch, DirtyPolicy::Inherit, &guards)
            .expect("clone");

        for i in 0..commits {
            fs::write(
                workspace.path.join(format!("work-{i}.txt")),
                format!("{i}\n"),
            )
            .expect("write");
            git_cmd(&workspace.path)
                .args(["add", "."])
                .run()
                .expect("add");
            git_cmd(&workspace.path)
                .args(["commit", "-m", &format!("work {i}")])
                .run()
                .expect("commit");
        }

        CowRecord {
            workspace_id: format!("{branch}~aaaa1111"),
            branch: branch.to_string(),
            path: workspace.path,
            parent_repo: repo.to_path_buf(),
        }
    }

    fn rev(repo: &Path, refname: &str) -> Option<String> {
        git_cmd(repo)
            .args(["rev-parse", "--verify", "--quiet", refname])
            .run()
            .ok()
            .map(|out| out.stdout.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    #[test]
    fn the_parent_cannot_see_the_work_before_publish_and_can_merge_it_after() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);

        assert_eq!(
            rev(&repo, "refs/heads/feature"),
            None,
            "the parent must not have the branch before publish — that is what makes it unpublished"
        );

        let outcome = publish_cow_workspace(&record).expect("publish runs");

        assert!(outcome.parent_updated, "{:?}", outcome.parent_error);
        assert_eq!(
            rev(&repo, "refs/heads/feature").as_deref(),
            outcome.published_commit.as_deref()
        );
        // And the parent can actually merge it, which is the point.
        git_cmd(&repo)
            .args(["merge", "feature", "--no-edit"])
            .run()
            .expect("the parent can merge the published branch");
        assert!(repo.join("work-0.txt").exists());
    }

    /// The failure mode publish exists to prevent: a same-named branch already
    /// in the parent, which `git merge` would silently take instead.
    #[test]
    fn publish_fast_forwards_a_stale_same_named_branch_in_the_parent() {
        let (temp, repo, _dest_parent) = setup();
        // The parent already has `feature`, pointing at the commit the
        // workspace was cloned from — stale, but present.
        git_cmd(&repo)
            .args(["branch", "feature"])
            .run()
            .expect("branch");
        let stale = rev(&repo, "refs/heads/feature").expect("stale ref");
        let record = published_fixture(&temp, &repo, "feature", 1);

        let outcome = publish_cow_workspace(&record).expect("publish runs");

        assert!(outcome.parent_updated, "{:?}", outcome.parent_error);
        let updated = rev(&repo, "refs/heads/feature").expect("ref");
        assert_ne!(updated, stale, "the parent is still on the stale commit");
        assert_eq!(Some(updated), outcome.published_commit);
    }

    /// Two workspaces on one branch both committing makes a divergent parent
    /// branch normal, not exotic. Publishing must refuse rather than orphan
    /// whichever side went second.
    #[test]
    fn publish_refuses_to_move_a_parent_branch_that_has_its_own_commits() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);

        // The parent gains its own `feature` with a different commit.
        git_cmd(&repo)
            .args(["checkout", "-b", "feature"])
            .run()
            .expect("branch");
        fs::write(repo.join("parent-side.txt"), "parent\n").expect("write");
        git_cmd(&repo).args(["add", "."]).run().expect("add");
        git_cmd(&repo)
            .args(["commit", "-m", "parent-side work"])
            .run()
            .expect("commit");
        let parent_tip = rev(&repo, "refs/heads/feature").expect("ref");
        // Move off it, so this test is about divergence and not about the
        // checked-out guard below.
        git_cmd(&repo)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach");

        let outcome = publish_cow_workspace(&record).expect("publish reports rather than throwing");

        assert!(!outcome.parent_updated);
        assert!(
            outcome
                .parent_error
                .as_deref()
                .unwrap_or_default()
                .contains("would lose them"),
            "{:?}",
            outcome.parent_error
        );
        assert_eq!(
            rev(&repo, "refs/heads/feature"),
            Some(parent_tip),
            "the parent's commits were orphaned"
        );
    }

    /// Moving a ref behind its own working tree would leave the branch
    /// disagreeing with the index and the files. Publish is not an implicit
    /// checkout, so it stages the objects and says so.
    #[test]
    fn publish_refuses_to_move_a_branch_the_parent_has_checked_out() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);
        git_cmd(&repo)
            .args(["checkout", "-b", "feature"])
            .run()
            .expect("checkout");

        let outcome = publish_cow_workspace(&record).expect("publish reports rather than throwing");

        assert!(!outcome.parent_updated);
        assert!(
            outcome
                .parent_error
                .as_deref()
                .unwrap_or_default()
                .contains("checked out"),
            "{:?}",
            outcome.parent_error
        );
        // The objects DID arrive: the staged ref is what makes the failure
        // recoverable without a second transfer.
        assert!(
            rev(&repo, &staged_ref(&record.workspace_id)).is_some(),
            "the staged ref is missing, so the commits never reached the parent"
        );
    }

    /// The two steps report independently: no origin is not a failed publish.
    #[test]
    fn a_missing_origin_does_not_undo_the_parent_side_of_a_publish() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);

        let outcome = publish_cow_workspace(&record).expect("publish runs");

        assert!(outcome.parent_updated, "{:?}", outcome.parent_error);
        assert!(!outcome.origin_pushed);
        assert!(
            outcome
                .origin_error
                .as_deref()
                .unwrap_or_default()
                .contains("origin"),
            "{:?}",
            outcome.origin_error
        );
        assert!(
            rev(&repo, "refs/heads/feature").is_some(),
            "the parent update was rolled back"
        );
    }

    #[test]
    fn publishing_a_workspace_whose_directory_is_gone_says_so() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);
        fs::remove_dir_all(&record.path).expect("remove");

        let err = publish_cow_workspace(&record).expect_err("must not pretend to publish");
        assert!(err.contains("gone"), "{err}");
    }

    // ── removal ──────────────────────────────────────────────────────────

    #[test]
    fn removal_refuses_while_commits_exist_only_in_the_workspace() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 2);

        let err = remove_cow_workspace(&record, false).expect_err("must refuse");

        assert!(err.contains('2'), "the refusal must name the count: {err}");
        assert!(
            err.contains("Publish first"),
            "the refusal must name the way out: {err}"
        );
        assert!(record.path.exists(), "the workspace was deleted anyway");
    }

    #[test]
    fn removal_proceeds_once_the_commits_are_published() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);
        assert_eq!(unpublished_commit_count(&record).expect("count"), 1);

        publish_cow_workspace(&record).expect("publish");
        assert_eq!(
            unpublished_commit_count(&record).expect("count"),
            0,
            "a published commit must stop counting as unpublished"
        );

        remove_cow_workspace(&record, false).expect("removes without a prompt");
        assert!(!record.path.exists());
    }

    #[test]
    fn a_workspace_with_no_commits_of_its_own_removes_without_a_prompt() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 0);

        assert_eq!(unpublished_commit_count(&record).expect("count"), 0);
        remove_cow_workspace(&record, false).expect("removes");
        assert!(!record.path.exists());
    }

    #[test]
    fn force_removes_a_workspace_that_would_otherwise_be_refused() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);
        remove_cow_workspace(&record, false).expect_err("refuses without force");

        let lost = remove_cow_workspace(&record, true).expect("force removes");

        assert_eq!(lost, 1, "force must report what it destroyed");
        assert!(!record.path.exists());
    }

    /// Two workspaces on one branch: removing one must not touch the other's
    /// directory. They are separate repositories, and the removal is addressed
    /// by id.
    #[test]
    fn removing_one_workspace_leaves_its_same_branch_sibling_on_disk() {
        let (temp, repo, _dest_parent) = setup();
        git_cmd(&repo)
            .args(["branch", "shared"])
            .run()
            .expect("branch");
        let first = published_fixture(&temp, &repo, "shared", 0);
        let second = CowRecord {
            workspace_id: "shared~bbbb2222".to_string(),
            ..published_fixture(&temp, &repo, "shared-second", 0)
        };

        remove_cow_workspace(&first, false).expect("removes");

        assert!(!first.path.exists());
        assert!(
            second.path.exists(),
            "the sibling workspace was deleted too"
        );
        assert!(
            second.path.join(".git").is_dir(),
            "the sibling is still a repository"
        );
    }

    #[test]
    fn a_directory_that_is_no_longer_a_repository_is_not_deleted_blindly() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 0);
        // Whatever is at that path now, it is not the workspace this record
        // described — the realistic cause being a user who moved or replaced it.
        fs::remove_dir_all(record.path.join(".git")).expect("remove gitdir");
        fs::write(record.path.join("something-else.txt"), "not ours\n").expect("write");

        let err = remove_cow_workspace(&record, false).expect_err("must refuse");

        assert!(err.contains("no .git directory"), "{err}");
        assert!(record.path.join("something-else.txt").exists());
    }

    #[test]
    fn removing_an_already_gone_workspace_is_not_an_error() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 0);
        fs::remove_dir_all(&record.path).expect("remove");

        // Idempotent: the caller's next step is to drop the row, and refusing
        // here would strand it forever.
        assert_eq!(remove_cow_workspace(&record, false).expect("no error"), 0);
    }

    // ── reading COW records out of the persisted document ────────────────

    #[test]
    fn cow_records_are_read_from_the_workspaces_map_and_filtered_by_kind() {
        let doc = serde_json::json!({
            "repos": {
                "/repo": {
                    "path": "/repo",
                    "workspaces": {
                        "main": { "branchName": "main", "kind": "main", "worktreePath": "/repo" },
                        "feature": { "branchName": "feature", "kind": "worktree", "worktreePath": "/repo__wt/feature" },
                        "feature~aaaa1111": {
                            "branchName": "feature",
                            "kind": "cow",
                            "worktreePath": "/repo__cow/feature-1",
                            "parentRepoPath": "/repo"
                        }
                    }
                },
                "/other": {
                    "path": "/other",
                    "workspaces": {
                        "x~bbbb2222": { "branchName": "x", "kind": "cow", "worktreePath": "/other__cow/x" }
                    }
                }
            }
        });

        let records = cow_workspaces_in(&doc, Path::new("/repo"));

        assert_eq!(
            records.len(),
            1,
            "only the cow row of THIS repo: {records:?}"
        );
        assert_eq!(records[0].workspace_id, "feature~aaaa1111");
        assert_eq!(records[0].branch, "feature");
        assert_eq!(records[0].path, PathBuf::from("/repo__cow/feature-1"));
    }

    /// A document written before COW workspaces existed keys its rows under
    /// `branches`. Reading nothing from it is correct, not a gap: no COW
    /// workspace can be described by a document that predates them.
    #[test]
    fn a_pre_migration_document_yields_no_cow_records() {
        let doc = serde_json::json!({
            "repos": {
                "/repo": {
                    "path": "/repo",
                    "branches": {
                        "main": { "name": "main", "isMain": true, "worktreePath": null }
                    }
                }
            }
        });

        assert!(cow_workspaces_in(&doc, Path::new("/repo")).is_empty());
    }

    #[test]
    fn a_minted_id_is_legible_keeps_the_branch_as_a_label_and_avoids_collisions() {
        let id = mint_workspace_id("feature/shared identity", &[]);
        assert!(id.starts_with("feature-shared-identity~"), "{id}");
        assert!(!id.contains('/'), "an id ends up in logs and paths: {id}");

        // The stem is a label; uniqueness comes from the suffix, and it is
        // checked rather than assumed.
        let taken = vec![id.clone()];
        assert_ne!(mint_workspace_id("feature/shared identity", &taken), id);

        assert_eq!(
            mint_workspace_id("///", &[]).split('~').next(),
            Some("workspace")
        );
    }

    #[test]
    fn an_existing_destination_is_refused_before_anything_is_copied() {
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        fs::create_dir_all(&dest).expect("dest");
        fs::write(dest.join("keep.txt"), "mine\n").expect("existing content");

        let err = create_cow_workspace(
            &repo,
            &dest,
            "feature",
            DirtyPolicy::Inherit,
            &GuardReport::default(),
        )
        .expect_err("must refuse");

        assert!(err.contains("already exists"), "{err}");
        assert!(
            dest.join("keep.txt").exists(),
            "the existing directory was touched"
        );
    }
}
