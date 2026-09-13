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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git_cli::git_cmd;
use crate::git_locks::{self, LockFileInfo};
use serde::Serialize;

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

/// The `cp` flags that take a copy-on-write copy, in the order to try them:
/// macOS `clonefile`, then GNU coreutils reflink.
///
/// **Every flag here must FAIL rather than degrade to a byte copy.** That is
/// what lets the same list serve as the probe and as the clone — `--reflink=auto`
/// would report support everywhere and silently turn a 19 MB clone into a 12 GB
/// copy. One list on purpose: the probe deciding a mechanism is available while
/// the clone cannot issue it is exactly the bug this replaced.
const COW_COPY_FLAGS: [&str; 2] = ["-c", "--reflink=always"];

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

    // The same mechanisms the clone itself issues, in the same order: a probe
    // that answers Supported for a flag `create_cow_workspace` cannot run is
    // worse than no probe at all.
    let cloned = COW_COPY_FLAGS
        .iter()
        .any(|flag| clone_file_with(&[flag], &head, &probe));
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
    /// The dirty paths this workspace carried over at creation, recorded ONLY
    /// for a clone this backend created and knows the history of. `None` means
    /// no baseline was ever recorded — a clone from before this field existed,
    /// or one registered through recovery/adoption, where "what was dirty at
    /// creation" cannot be reconstructed and must not be guessed at.
    pub(crate) dirty_baseline: Option<Vec<String>>,
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
                // Absent or `null` both mean "no baseline recorded" — the key
                // is written to the JSON only for a clone this backend
                // actually created, so distinguishing "missing" from "null"
                // would draw a line the writer never draws.
                dirty_baseline: workspace.get("dirtyBaseline").and_then(|v| {
                    v.as_array().map(|arr| {
                        arr.iter()
                            .filter_map(|p| p.as_str().map(str::to_string))
                            .collect()
                    })
                }),
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
    /// The dirty paths present right after creation, i.e. `carried_over`'s own
    /// identities rather than just its count. `Some` only when this call is the
    /// one actually creating the clone ([`create_cow_workspace`]); recovery and
    /// adoption register a workspace whose creation they did not witness, so
    /// they pass `None` rather than fabricate a baseline of zero.
    pub(crate) dirty_baseline: Option<Vec<String>>,
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
///
/// `workspace_id` is written into the clone's own git config as provenance
/// (`recover_cow_workspaces`), not used by the copy itself — the caller mints it
/// before calling here so a failure claims no id, and passes the same value on
/// to `register_cow_workspace` afterwards. The two must agree, which is why
/// there is only one place either of them is supplied.
pub(crate) fn create_cow_workspace(
    src: &Path,
    dest: &Path,
    branch: &str,
    dirty: DirtyPolicy,
    guards: &GuardReport,
    workspace_id: &str,
) -> Result<CowWorkspace, String> {
    if dest.exists() {
        return Err(format!("destination '{}' already exists", dest.display()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create '{}': {e}", parent.display()))?;
    }

    clone_tree(src, dest)?;

    match fixup_clone(src, dest, branch, dirty, guards, workspace_id) {
        Ok(dirty_baseline) => Ok(CowWorkspace {
            path: dest.to_path_buf(),
            branch: branch.to_string(),
            warnings: guards.warnings.clone(),
            carried_over: dirty_baseline.len(),
            dirty_policy: dirty,
            dirty_baseline: Some(dirty_baseline),
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

/// Copy `src` to `dest` copy-on-write, trying each mechanism in
/// [`COW_COPY_FLAGS`] until one works.
///
/// The fallback is not cosmetic: `-c` is an invalid option to GNU `cp` and
/// `--reflink=always` is an invalid option to macOS `cp`, so a single hardcoded
/// flag means the probe can answer Supported on a platform where the clone
/// cannot run at all. That is what happened on Linux — `mode=auto` reported an
/// error instead of degrading, because the failure arrived after the decision.
fn clone_tree(src: &Path, dest: &Path) -> Result<(), String> {
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

/// Shorter than the HTTP server deadline: a timed-out copy is killed and its
/// partial destination is removed by `clone_tree_with` before either transport
/// can tell the caller the request timed out while `cp` keeps running.
const COW_COPY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(280);

fn run_copy_command(
    command: &mut Command,
    timeout: std::time::Duration,
) -> Result<(std::process::ExitStatus, String), String> {
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not run copy command: {e}"))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("could not wait for copy command: {e}"))?
        {
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            return Ok((status, stderr));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "copy-on-write clone exceeded its {} second operation deadline",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// [`clone_tree`] with the copy injected.
///
/// The machine running the tests can only ever exercise ONE of the two
/// mechanisms — whichever its `cp` implements — so the ordering, the fallback
/// and the cleanup between attempts are only testable through this seam.
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
        // Leave nothing half-copied behind for the next attempt to trip over —
        // the next `cp` would refuse a destination that already exists, and a
        // torn copy must never survive as something a caller could use.
        let _ = std::fs::remove_dir_all(dest);
    }

    Err(format!(
        "copy-on-write clone of '{}' into '{}' failed: {}",
        src.display(),
        dest.display(),
        failures.join("; ")
    ))
}

/// Everything a raw copy-on-write copy of a repository still needs. None of
/// this is precautionary — each step fixes something the PoC observed break.
fn fixup_clone(
    src: &Path,
    dest: &Path,
    branch: &str,
    dirty: DirtyPolicy,
    guards: &GuardReport,
    workspace_id: &str,
) -> Result<Vec<String>, String> {
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
    // The inherited `origin` gets the same treatment, and for a stronger
    // reason: publish drives origin FROM THE PARENT, after the parent has
    // accepted the work. A clone that can push `origin` itself can put a
    // history on the shared remote that the parent refused — so "the clone
    // must not reach origin" is made an invariant here rather than left as an
    // instruction. Only when an `origin` actually came across: writing the key
    // otherwise would invent a half-configured remote out of nothing.
    if git_cmd(dest)
        .args(["remote", "get-url", "origin"])
        .run()
        .is_ok()
    {
        config_or_fail(dest, "remote.origin.pushurl", NO_PUSH_URL)?;
    }
    // Inherited fsmonitor state describes the parent's path, not this one.
    config_or_fail(dest, "core.fsmonitor", "false")?;

    // Provenance a restart can trust. `repositories.json` is the only registry
    // this workspace is known to, and a crash between the copy landing and that
    // registration being written must still be recoverable — `recover_cow_workspaces`
    // is what reads these back. Written into the clone's OWN config, never the
    // parent's, and canonicalized so recovery's comparison is not fooled by a
    // symlink or a trailing slash the caller happened to pass.
    let canonical_parent = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    config_or_fail(dest, COW_MARKER_WORKSPACE_ID_KEY, workspace_id)?;
    config_or_fail(
        dest,
        COW_MARKER_PARENT_KEY,
        &canonical_parent.to_string_lossy(),
    )?;

    apply_dirty_policy(dest, dirty)?;

    // `clonefile` preserves mtime but changes ino and ctime, so every index
    // entry reads stat-dirty and the first `git status` re-hashes the whole
    // tree. Pay it here, inside creation, instead of in whatever command the
    // agent happens to run first.
    let _ = git_cmd(dest).args(["update-index", "--refresh"]).run();

    checkout_branch(dest, branch)?;

    Ok(dirty_paths(dest))
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

/// Provenance markers written into every COW clone's own git config at creation
/// (`fixup_clone`) and the only thing [`recover_cow_workspaces`] will trust after
/// a restart. Both must be present, non-empty, and — for the parent key — name
/// the repo being recovered for; anything less is a directory of unknown
/// provenance, not a COW workspace, however much it looks like one.
const COW_MARKER_WORKSPACE_ID_KEY: &str = "tuicommander.cow.workspace-id";
const COW_MARKER_PARENT_KEY: &str = "tuicommander.cow.parent";

fn config_or_fail(repo: &Path, key: &str, value: &str) -> Result<(), String> {
    git_cmd(repo)
        .args(["config", key, value])
        .run()
        .map(|_| ())
        .map_err(|e| format!("could not set {key} in the workspace: {e}"))
}

/// A marker value from a candidate's own config, trimmed and empty-checked —
/// `git config --get` on an unset key is a normal, silent failure, not
/// something to report as an error; the caller reads `None` as "not marked".
fn read_marker(repo: &Path, key: &str) -> Option<String> {
    git_cmd(repo)
        .args(["config", "--get", key])
        .run()
        .ok()
        .map(|out| out.stdout.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Durably record a freshly created COW clone before its caller can report
/// success. [`create_cow_workspace`] only produces a directory on disk — until
/// this returns, a crash between the two leaves an orphaned clone that
/// `repositories.json` (and therefore listing, refresh and removal) has never
/// heard of. Uses the same locked, read-modify-write primitive
/// `save_repositories_request` uses, entered directly instead of through its
/// two-step compare-and-swap delta: this caller just created the directory and
/// holds no client-side copy of the surrounding repository record to diff
/// against, so the only correct baseline is "whatever is on disk right now",
/// read inside the same file lock as the write.
///
/// On failure the caller must keep the clone rather than delete it: the clone
/// is the one thing [`recover_cow_workspaces`] can heal this failure from at
/// the next restart, and deleting it here would throw that away.
pub(crate) fn register_cow_workspace(
    parent_repo: &Path,
    workspace_id: &str,
    workspace: &CowWorkspace,
) -> Result<(), String> {
    crate::config::upsert_workspace_record(
        &parent_repo.to_string_lossy(),
        workspace_id,
        serde_json::json!({
            "branchName": workspace.branch,
            "worktreePath": workspace.path.to_string_lossy(),
            "kind": "cow",
            "parentRepoPath": parent_repo.to_string_lossy(),
            // `null` and "key absent" are read back identically by
            // `cow_workspaces_in` — both mean no baseline was ever recorded.
            "dirtyBaseline": workspace.dirty_baseline,
        }),
    )
}

/// Drop a COW workspace's row once its directory is already gone — the
/// removal-side twin of [`register_cow_workspace`]. Idempotent: a row already
/// absent is not an error, because the caller's next step after a crash mid
/// cleanup is "make sure the row is gone", not "prove it was still there".
///
/// A failure here must reach the caller rather than be swallowed: silently
/// leaving the row behind would claim a workspace still exists at a path that
/// is gone, and unlike a failed *registration* there is no clone left on disk
/// for a later recovery pass to heal it from.
pub(crate) fn unregister_cow_workspace(
    parent_repo: &Path,
    workspace_id: &str,
) -> Result<(), String> {
    crate::config::remove_workspace_record(&parent_repo.to_string_lossy(), workspace_id)
}

/// Why a directory under the worktree base was not trusted as a COW workspace.
/// Carried as a `String` rather than logged inline so a test can assert on
/// which check fired, not just that discovery skipped something.
type CandidateRejection = String;

/// Check one immediate child of `worktrees_dir` against every mark a COW clone
/// this backend created carries, refusing anything short of all of them. The
/// single validator both [`recover_cow_workspaces`] (read-only discovery) and
/// [`remove_cow_workspace`] (before any deletion) run a candidate through —
/// one definition of "this is a COW clone this backend made", not two that
/// could drift apart.
///
/// Deliberately narrow: a markerless directory — including a real COW clone
/// made before this recovery existed — is rejected exactly the same as an
/// unrelated repository someone dropped next to one. Silently trusting a
/// markerless directory because it looks plausible is the failure #756-cf6d
/// named; there is no safe heuristic short of the marker this backend itself
/// writes on creation. A markerless clone must be adopted explicitly
/// (`adopt_cow_workspace`) before either recovery or removal will act on it.
///
/// **Trust boundary.** Every check here is a provenance check, not a security
/// boundary against a hostile actor who already has write access to this
/// machine: the markers, the remote, and `repositories.json` are all local git
/// config and local files this same OS user can edit. What this protects
/// against is a directory being treated as a COW clone by ACCIDENT or by
/// STALE DATA — a leftover, a hand-restored backup, a row a bug left behind, a
/// repository someone dropped next to a real clone — not a local user or
/// process that deliberately reconstructs every marker and the git remote to
/// match. That threat model does not exist for a single-user desktop app
/// backend running as the same user as the repositories it manages; if it
/// ever does, the fix is process isolation, not another config key.
fn validate_cow_candidate(
    candidate: &Path,
    worktrees_dir: &Path,
    parent_repo: &Path,
) -> Result<CowRecord, CandidateRejection> {
    let canonical_candidate = canonicalize_cow_candidate(candidate, worktrees_dir)?;

    let workspace_id =
        read_marker(&canonical_candidate, COW_MARKER_WORKSPACE_ID_KEY).ok_or_else(|| {
            format!(
                "'{}' has no '{COW_MARKER_WORKSPACE_ID_KEY}' marker",
                canonical_candidate.display()
            )
        })?;
    let marked_parent =
        read_marker(&canonical_candidate, COW_MARKER_PARENT_KEY).ok_or_else(|| {
            format!(
                "'{}' has no '{COW_MARKER_PARENT_KEY}' marker",
                canonical_candidate.display()
            )
        })?;

    // `tuicommander.cow.parent` is this backend's own marker, but it is still
    // just git config — indistinguishable, to anything reading it back, from a
    // value a human or another tool wrote by hand. Git's OWN idea of where this
    // clone's parent is — the `remote.parent.url` `fixup_clone` set with
    // `git remote add` — is a second, independent witness that must agree.
    // Requiring both closes the gap a single mutable key always has: whoever
    // could forge one marker could just as easily forge the other, but forging
    // both consistently is no longer "edit one config value", it is
    // reconstructing exactly what a real clone looks like — at which point the
    // local-user trust boundary this backend runs inside of is the only thing
    // left standing, same as it always was (see this function's doc comment).
    let canonical_parent_repo = validate_parent_remote(&canonical_candidate, parent_repo)?;

    let canonical_marked_parent =
        std::fs::canonicalize(&marked_parent).unwrap_or_else(|_| PathBuf::from(&marked_parent));
    if canonical_marked_parent != canonical_parent_repo {
        return Err(format!(
            "'{}' names parent '{marked_parent}', not '{}' — not a clone of this repository",
            canonical_candidate.display(),
            parent_repo.display()
        ));
    }

    let branch = read_current_branch(&canonical_candidate)?;

    Ok(CowRecord {
        workspace_id,
        branch,
        path: canonical_candidate,
        parent_repo: canonical_parent_repo,
        // This record is re-derived from the candidate's own git config for
        // validation, not read from `repositories.json` — it never carries a
        // baseline regardless of what the persisted document says.
        dirty_baseline: None,
    })
}

/// The containment and shape checks every COW-candidate validator needs
/// first, regardless of which markers it goes on to require: the candidate is
/// not itself a symlink, canonicalizes to an immediate child of
/// `worktrees_dir`, and has a real (non-symlink) `.git` directory. Shared by
/// [`validate_cow_candidate`] (recovery and removal) and
/// [`validate_adoption_candidate`] (explicit adoption) so containment is
/// checked exactly one way, not two that could quietly diverge.
fn canonicalize_cow_candidate(
    candidate: &Path,
    worktrees_dir: &Path,
) -> Result<PathBuf, CandidateRejection> {
    let meta = std::fs::symlink_metadata(candidate)
        .map_err(|e| format!("could not stat '{}': {e}", candidate.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "'{}' is a symlink; this does not follow one out of the worktree base",
            candidate.display()
        ));
    }
    if !meta.is_dir() {
        return Err(format!("'{}' is not a directory", candidate.display()));
    }

    // Canonicalize both sides and re-check containment: the entry itself is not
    // a symlink (checked above), but a component further up `worktrees_dir` could
    // still be one, and the comparison below must be apples to apples either way.
    let canonical_base = std::fs::canonicalize(worktrees_dir)
        .map_err(|e| format!("could not canonicalize '{}': {e}", worktrees_dir.display()))?;
    let canonical_candidate = std::fs::canonicalize(candidate)
        .map_err(|e| format!("could not canonicalize '{}': {e}", candidate.display()))?;
    if canonical_candidate.parent() != Some(canonical_base.as_path()) {
        return Err(format!(
            "'{}' escapes the worktree base '{}' once symlinks are resolved",
            candidate.display(),
            worktrees_dir.display()
        ));
    }

    // A linked worktree's `.git` is a FILE (a pointer into the parent's
    // `.git/worktrees/`), not a directory — this check is what tells the two
    // mechanisms apart when they share one directory of children, and also
    // rejects a plain non-repository directory in the same breath.
    //
    // `symlink_metadata`, not `metadata`/`is_dir()`: the latter follows a
    // symlink and would happily report "yes, a directory" for a `.git` that is
    // actually a symlink pointing anywhere else on disk — an attacker- (or
    // mistake-) controlled candidate could point `.git` at a real repository
    // elsewhere and pass every check below by proxy. A COW clone's `.git` is
    // always the directory `cp`'s copy produced; anything else is refused,
    // full stop, not resolved and re-checked.
    match std::fs::symlink_metadata(canonical_candidate.join(".git")) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "'{}' has a '.git' symlink — refusing to trust a candidate whose git directory is \
             not a real directory it owns",
            canonical_candidate.display()
        )),
        Ok(meta) if !meta.is_dir() => Err(format!(
            "'{}' has no '.git' directory — not a repository, or a linked worktree rather than a COW clone",
            canonical_candidate.display()
        )),
        Err(_) => Err(format!(
            "'{}' has no '.git' directory — not a repository, or a linked worktree rather than a COW clone",
            canonical_candidate.display()
        )),
        Ok(_) => Ok(canonical_candidate),
    }
}

/// The parent-remote evidence every COW clone this backend created carries,
/// independent of the `tuicommander.cow.*` markers: the no-push `pushurl` set
/// on the `parent` remote, and that remote's `url` resolving to `parent_repo`
/// once symlinks are canonicalized on both sides. Shared by
/// [`validate_cow_candidate`] and [`validate_adoption_candidate`] — a
/// markerless legacy clone still carries this, because `fixup_clone` has
/// written it since the mechanism's first version; only the two marker keys
/// are new. Returns the canonicalized `parent_repo` on success.
fn validate_parent_remote(
    canonical_candidate: &Path,
    parent_repo: &Path,
) -> Result<PathBuf, CandidateRejection> {
    if read_marker(canonical_candidate, "remote.parent.pushurl").as_deref() != Some(NO_PUSH_URL) {
        return Err(format!(
            "'{}' is missing the no-push parent-remote evidence every COW clone has",
            canonical_candidate.display()
        ));
    }

    let canonical_parent_repo =
        std::fs::canonicalize(parent_repo).unwrap_or_else(|_| parent_repo.to_path_buf());
    let remote_parent_url =
        read_marker(canonical_candidate, "remote.parent.url").ok_or_else(|| {
            format!(
                "'{}' has no 'remote.parent.url' — missing the parent remote every COW clone has",
                canonical_candidate.display()
            )
        })?;
    let canonical_remote_parent = std::fs::canonicalize(&remote_parent_url)
        .unwrap_or_else(|_| PathBuf::from(&remote_parent_url));
    if canonical_remote_parent != canonical_parent_repo {
        return Err(format!(
            "'{}' remote 'parent' points at '{remote_parent_url}', not '{}' — not a clone of this \
             repository",
            canonical_candidate.display(),
            parent_repo.display()
        ));
    }

    Ok(canonical_parent_repo)
}

/// The branch a candidate is on, refusing a detached HEAD — a COW workspace
/// (or an adoption candidate) is only ever meaningful on a branch.
fn read_current_branch(canonical_candidate: &Path) -> Result<String, CandidateRejection> {
    let branch_out = git_cmd(canonical_candidate)
        .args(["branch", "--show-current"])
        .run()
        .map_err(|e| format!("could not read the checked-out branch: {e}"))?;
    let branch = branch_out.stdout.trim().to_string();
    if branch.is_empty() {
        return Err(format!(
            "'{}' is not on a branch (detached HEAD)",
            canonical_candidate.display()
        ));
    }
    Ok(branch)
}

/// Register every immediate child of `worktrees_dir` that is a marked COW clone
/// of `parent_repo` and is not already known, healing a `repositories.json` that
/// lost its row for one — the crash window [`register_cow_workspace`] closes
/// going forward, a hand-restored backup, or any other gap between a clone on
/// disk and the document that describes it.
///
/// Read-only over the filesystem: nothing here ever writes to, deletes, or
/// repairs a candidate directory, only the persisted document once a candidate
/// has passed every check. `taken_ids`/`taken_paths` come from the caller
/// (linked worktrees plus already-registered COW records) so a candidate that
/// would collide with either is skipped rather than silently aliased onto an
/// unrelated row. Best-effort throughout: one unreadable or rejected candidate
/// must not stop the rest of the scan, and a missing `worktrees_dir` is not an
/// error, just nothing to recover.
///
/// Returns the number of rows newly registered.
pub(crate) fn recover_cow_workspaces(
    parent_repo: &Path,
    worktrees_dir: &Path,
    taken_ids: &std::collections::HashSet<String>,
    taken_paths: &std::collections::HashSet<PathBuf>,
) -> usize {
    let Ok(entries) = std::fs::read_dir(worktrees_dir) else {
        return 0;
    };

    let mut seen_ids = taken_ids.clone();
    let mut seen_paths = taken_paths.clone();
    let mut registered = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        let candidate = match validate_cow_candidate(&path, worktrees_dir, parent_repo) {
            Ok(candidate) => candidate,
            Err(reason) => {
                tracing::debug!(
                    source = "cow",
                    path = %path.display(),
                    reason = %reason,
                    "COW recovery: candidate not adopted"
                );
                continue;
            }
        };

        if !seen_ids.insert(candidate.workspace_id.clone()) {
            tracing::warn!(
                source = "cow",
                workspace_id = %candidate.workspace_id,
                path = %candidate.path.display(),
                "COW recovery: workspace id already known, skipping duplicate"
            );
            continue;
        }
        if !seen_paths.insert(candidate.path.clone()) {
            tracing::warn!(
                source = "cow",
                path = %candidate.path.display(),
                "COW recovery: path already known under another id, skipping duplicate"
            );
            continue;
        }

        let workspace = CowWorkspace {
            path: candidate.path.clone(),
            branch: candidate.branch.clone(),
            warnings: Vec::new(),
            carried_over: 0,
            dirty_policy: DirtyPolicy::Inherit,
            // Recovery did not witness this clone's creation — it already
            // existed on disk — so there is no creation moment to have taken a
            // baseline at.
            dirty_baseline: None,
        };
        match register_cow_workspace(parent_repo, &candidate.workspace_id, &workspace) {
            Ok(()) => registered += 1,
            Err(e) => tracing::warn!(
                source = "cow",
                workspace_id = %candidate.workspace_id,
                error = %e,
                "COW recovery: found a valid clone but could not register it"
            ),
        }
    }

    registered
}

/// A markerless candidate that passed every git-native check adoption
/// requires, but does not yet carry [`validate_cow_candidate`]'s marker
/// evidence — that evidence is exactly what [`adopt_cow_workspace`] is about
/// to write.
struct AdoptionCandidate {
    path: PathBuf,
    parent_repo: PathBuf,
    branch: String,
}

/// Everything [`adopt_cow_workspace`] can check about a candidate WITHOUT the
/// two `tuicommander.cow.*` markers [`validate_cow_candidate`] requires —
/// those markers are exactly what adoption is about to write, so requiring
/// them first would make adoption impossible for the one case it exists for.
/// What IS required is every check that predates this backend's marker keys:
/// containment in the configured worktree base, a real (non-symlink) `.git`
/// directory, the `parent` remote's no-push `pushurl` and its `url` resolving
/// to `parent_repo`, and a checked-out branch.
fn validate_adoption_candidate(
    candidate: &Path,
    worktrees_dir: &Path,
    parent_repo: &Path,
) -> Result<AdoptionCandidate, String> {
    let canonical_candidate = canonicalize_cow_candidate(candidate, worktrees_dir)?;
    let canonical_parent_repo = validate_parent_remote(&canonical_candidate, parent_repo)?;
    let branch = read_current_branch(&canonical_candidate)?;

    Ok(AdoptionCandidate {
        path: canonical_candidate,
        parent_repo: canonical_parent_repo,
        branch,
    })
}

/// Explicitly adopt a markerless COW clone this backend lost track of — the
/// smallest surface that turns a directory failing [`validate_cow_candidate`]'s
/// marker checks into a registered workspace, for the real clones #756-cf6d
/// left stranded once markerless directories stopped being trusted on sight.
///
/// The call itself is the confirmation: adoption never deletes anything and
/// the only change it makes to the clone is writing the two
/// `tuicommander.cow.*` markers into its local git config — no dirty check,
/// no unpublished-commit prompt, because there is nothing else here for
/// either to protect against — it only starts trusting a directory that
/// already exists exactly as it is otherwise.
///
/// `workspace_id` lets a caller that already has a stable identifier for this
/// directory (most commonly: retrying an adoption whose registry write
/// failed) keep using it; `None` mints a fresh one the same way creation
/// does. `taken_ids`/`taken_paths` are the caller's existing id/path universe
/// — linked worktrees plus already-registered COW records, the same contract
/// [`recover_cow_workspaces`] uses — so an adoption cannot collide with
/// either.
///
/// Never mutates the index, the working tree, refs, HEAD, untracked files, or
/// any existing remote: everything this writes is exactly what `fixup_clone`
/// would have written at creation — the two `tuicommander.cow.*` markers —
/// and nothing else. The `parent` remote and its no-push `pushurl` are read
/// as evidence, never written; a directory that predates them is not a COW
/// clone this backend can adopt (see [`validate_adoption_candidate`]).
///
/// A failure before validation passes writes nothing — validation runs first,
/// full stop. A failure of the registry write AFTER the markers are written
/// is explicit (the error says so) and recoverable: retrying — with the same
/// `workspace_id` this call returned, or with `None` if the caller did not
/// keep it — finds the markers already in place and proceeds straight to
/// registration, the same idempotence [`register_cow_workspace`] already
/// gives creation.
pub(crate) fn adopt_cow_workspace(
    parent_repo: &Path,
    candidate: &Path,
    worktrees_dir: &Path,
    workspace_id: Option<&str>,
    taken_ids: &std::collections::HashSet<String>,
    taken_paths: &std::collections::HashSet<PathBuf>,
) -> Result<CowRecord, String> {
    let validated = validate_adoption_candidate(candidate, worktrees_dir, parent_repo)?;

    if taken_paths.contains(&validated.path) {
        return Err(format!(
            "'{}' is already registered under another workspace id — refusing to adopt it a second \
             time",
            validated.path.display()
        ));
    }

    // A retry of an adoption whose registry write failed lands here with both
    // markers already on disk (this call's own prior attempt wrote them). That
    // must be detected before minting or re-writing anything — otherwise a
    // retry with `workspace_id: None` (the caller didn't keep the id the first
    // call returned) mints a second id and overwrites the markers that already
    // correctly describe this candidate, contradicting the retry contract
    // documented above. A candidate carrying only one of the two markers is a
    // partial/conflicting state this backend will not guess its way out of.
    let existing_id_marker = read_marker(&validated.path, COW_MARKER_WORKSPACE_ID_KEY);
    let existing_parent_marker = read_marker(&validated.path, COW_MARKER_PARENT_KEY);

    let workspace_id = match (existing_id_marker, existing_parent_marker) {
        (Some(existing_id), Some(existing_parent)) => {
            let parent_repo_str = validated.parent_repo.to_string_lossy().to_string();
            if existing_parent != parent_repo_str {
                return Err(format!(
                    "'{}' already carries provenance markers naming a different parent repo \
                     ('{existing_parent}') than the one being adopted against ('{parent_repo_str}') \
                     — refusing to overwrite them",
                    validated.path.display()
                ));
            }
            if let Some(id) = workspace_id {
                if id.trim().is_empty() {
                    return Err("a supplied workspace id must not be blank".to_string());
                }
                if id != existing_id {
                    return Err(format!(
                        "'{}' already carries a provenance marker for workspace id '{existing_id}', \
                         which does not match the supplied id '{id}' — retry with '{existing_id}' \
                         (or omit the id) instead",
                        validated.path.display()
                    ));
                }
            }
            existing_id
        }
        (None, None) => match workspace_id {
            Some(id) => {
                if id.trim().is_empty() {
                    return Err("a supplied workspace id must not be blank".to_string());
                }
                if taken_ids.contains(id) {
                    return Err(format!(
                        "workspace id '{id}' is already in use — supply a different one, or omit it to \
                         mint a fresh one"
                    ));
                }
                id.to_string()
            }
            None => {
                let taken: Vec<String> = taken_ids.iter().cloned().collect();
                mint_workspace_id(&validated.branch, &taken)
            }
        },
        _ => {
            return Err(format!(
                "'{}' carries only one of the two adoption provenance markers — a partial or \
                 conflicting state this backend will not repair automatically; remove the stray \
                 marker (or the directory) and retry",
                validated.path.display()
            ));
        }
    };

    // Everything above is read-only. From here the candidate's OWN config is
    // written (never the parent's) — the point of no return for "a failure
    // before this writes nothing".
    config_or_fail(&validated.path, COW_MARKER_WORKSPACE_ID_KEY, &workspace_id)?;
    config_or_fail(
        &validated.path,
        COW_MARKER_PARENT_KEY,
        &validated.parent_repo.to_string_lossy(),
    )?;

    let workspace = CowWorkspace {
        path: validated.path.clone(),
        branch: validated.branch.clone(),
        warnings: Vec::new(),
        carried_over: 0,
        dirty_policy: DirtyPolicy::Inherit,
        // Adoption takes a directory that already exists exactly as it is —
        // its creation moment, if any, predates this backend knowing about it.
        dirty_baseline: None,
    };
    register_cow_workspace(parent_repo, &workspace_id, &workspace).map_err(|e| {
        format!(
            "adopted '{}' as workspace '{workspace_id}' and wrote its provenance markers, but could \
             not register it in repositories.json: {e}. The markers are already in place — retry \
             adoption (passing '{workspace_id}' as the workspace id) rather than starting over.",
            validated.path.display()
        )
    })?;

    Ok(CowRecord {
        workspace_id,
        branch: validated.branch,
        path: validated.path,
        parent_repo: validated.parent_repo,
        dirty_baseline: None,
    })
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

/// Paths git currently considers dirty — modified, staged, or untracked —
/// identified by path alone so a file staged now and unstaged later still
/// compares equal to itself. `--untracked-files=all` matches worktree.rs's
/// `dirty_at`: the two must agree on what "dirty" means, or a path this
/// records as part of the creation baseline could read as "changed since
/// creation" purely because the flags disagree, not because anything changed.
fn dirty_paths(repo: &Path) -> Vec<String> {
    git_cmd(repo)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .run()
        .map(|out| {
            out.stdout
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.get(3..).unwrap_or(l).trim().to_string())
                .collect()
        })
        .unwrap_or_default()
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

    // Step 2: origin. It reports independently of step 1 — the commits reaching
    // the parent is worth reporting even when the network is down, and a
    // failure here must not roll back what already landed — but it is not
    // independent of step 1's VERDICT. The parent's fast-forward check is the
    // only place divergence is adjudicated; pushing after it refused would put
    // on the shared remote exactly the history the parent rejected, which is
    // strictly worse than the local refusal it just produced.
    if !outcome.parent_updated {
        outcome.origin_error = Some(format!(
            "not pushed: the parent did not accept this publish ({}). Origin must never move \
             ahead of the parent that refused it — resolve the parent side and publish again.",
            outcome
                .parent_error
                .as_deref()
                .unwrap_or("no parent update was made")
        ));
        return Ok(outcome);
    }

    // Pushed FROM THE PARENT, and the accepted parent ref rather than the
    // clone's live branch: the clone may have moved on since the ancestry check
    // (or, under a projected publish, never held the history that was
    // accepted), so pushing its branch would send something the parent never
    // agreed to. The clone's own `origin` is unpushable by construction.
    if git_cmd(&record.parent_repo)
        .args(["remote", "get-url", "origin"])
        .run()
        .is_err()
    {
        outcome.origin_error = Some("the parent repository has no 'origin' remote".to_string());
        return Ok(outcome);
    }
    match git_cmd(&record.parent_repo)
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

/// Directories worth telling a model it already has. Build output, in the
/// order a reader scans them.
const WARM_ARTIFACT_DIRS: [&str; 7] = [
    "node_modules",
    "target",
    "src-tauri/target",
    ".venv",
    "build",
    "dist",
    ".next",
];

/// One warm artifact directory: what it is and how much of it there is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct WarmArtifact {
    pub(crate) path: String,
    /// Human-readable, as `du -sh` prints it — the number is for a reader to
    /// weigh "already have it" against "rebuild it", not for arithmetic.
    pub(crate) size: String,
}

/// The warm artifact directories present in `workspace`, with their sizes.
///
/// Measured with `du`, in PARALLEL. Sequentially this is the slowest part of
/// reporting a creation: measured on this repo, `node_modules` takes ~790 ms
/// and a 9.6 GB `src-tauri/target` ~1060 ms warm, so seven directories would
/// add seconds to an operation whose whole selling point is that it took 26.
/// Run together, the cost is the slowest one instead of their sum.
pub(crate) fn warm_artifacts(workspace: &Path) -> Vec<WarmArtifact> {
    let present: Vec<&str> = WARM_ARTIFACT_DIRS
        .iter()
        .copied()
        .filter(|dir| workspace.join(dir).is_dir())
        .collect();

    std::thread::scope(|scope| {
        let handles: Vec<_> = present
            .iter()
            .map(|dir| {
                let full = workspace.join(dir);
                scope.spawn(move || directory_size(&full))
            })
            .collect();

        present
            .iter()
            .zip(handles)
            .filter_map(|(dir, handle)| {
                handle.join().ok().flatten().map(|size| WarmArtifact {
                    path: (*dir).to_string(),
                    size,
                })
            })
            .collect()
    })
}

fn directory_size(path: &Path) -> Option<String> {
    let out = Command::new("du").arg("-sh").arg(path).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
}

/// The ref namespace a COW workspace mirrors its parent's branches into, so
/// "reachable from the parent" is a local question.
const PARENT_MIRROR_GLOB: &str = "refs/parent";

/// Commit reachability of one independent clone relative to the repositories
/// that can preserve its work. `Published` deliberately differs from `Merged`:
/// a remote ref may preserve the tip while the parent's default branch does not
/// contain it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CowCommitStatus {
    Unpublished,
    Published,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CowLifecycle {
    pub(crate) commit_status: CowCommitStatus,
    pub(crate) unpublished_commits: usize,
    /// Where the workspace's current dirtiness comes from, relative to what it
    /// was created with. `None` when no creation-time baseline was ever
    /// recorded — never guessed at from current state alone, because current
    /// state cannot tell inherited dirt from a later edit that happens to
    /// touch the same paths.
    pub(crate) dirty_provenance: Option<DirtyProvenance>,
}

/// Where a COW workspace's current dirtiness comes from, relative to the
/// baseline recorded at creation. Context for a human deciding what a dirty
/// badge means — NEVER evidence for removal safety, which stays keyed on
/// whether the workspace is dirty at all, inherited or not: an inherited file
/// is still a file removal would destroy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirtyProvenance {
    /// Nothing is dirty right now, regardless of what was inherited.
    Clean,
    /// Every currently dirty path was already dirty at creation — nothing
    /// visible here was written by later work in this workspace.
    InheritedOnly,
    /// At least one currently dirty path was NOT part of the baseline: this
    /// workspace has been edited since it was created, whether or not any
    /// inherited dirt also remains.
    ChangedSinceCreation,
}

/// Compare `record`'s current dirty paths against its creation-time baseline.
///
/// `None` when `record.dirty_baseline` is `None` — a clone recovered,
/// adopted, or created before this baseline existed has no history to compare
/// against, and reporting one anyway would be a guess dressed up as a fact.
fn dirty_provenance(record: &CowRecord) -> Option<DirtyProvenance> {
    let baseline = record.dirty_baseline.as_ref()?;
    let current = dirty_paths(&record.path);
    if current.is_empty() {
        return Some(DirtyProvenance::Clean);
    }
    let baseline: std::collections::HashSet<&str> = baseline.iter().map(String::as_str).collect();
    if current.iter().all(|path| baseline.contains(path.as_str())) {
        Some(DirtyProvenance::InheritedOnly)
    } else {
        Some(DirtyProvenance::ChangedSinceCreation)
    }
}

fn refresh_parent_mirror(record: &CowRecord) -> Result<(), String> {
    // Prune matters to safety: a deleted parent branch must stop proving that
    // a clone tip is preserved. A failed refresh cannot reuse an old mirror —
    // stale refs can under-count as well as over-count unpublished commits.
    git_cmd(&record.path)
        .args([
            "fetch",
            "-q",
            "--prune",
            "parent",
            "+refs/heads/*:refs/parent/*",
        ])
        .run()
        .map(|_| ())
        .map_err(|e| format!("could not refresh the parent mirror: {e}"))
}

fn unpublished_commit_count_from_mirrors(record: &CowRecord) -> Result<usize, String> {
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

/// Inspect one COW clone after refreshing its local view of the parent once.
pub(crate) fn inspect_cow_lifecycle(
    record: &CowRecord,
    default_branch: &str,
) -> Result<CowLifecycle, String> {
    refresh_parent_mirror(record)?;
    let unpublished_commits = unpublished_commit_count_from_mirrors(record)?;
    let parent_default = format!("refs/parent/{default_branch}");
    let ancestry = git_cmd(&record.path)
        .args(["merge-base", "--is-ancestor", "HEAD", &parent_default])
        .run_raw()
        .map_err(|e| {
            format!("could not compare the workspace with the parent default branch: {e}")
        })?;
    let merged = match ancestry.status.code() {
        Some(0) => true,
        // `merge-base --is-ancestor` reserves 1 for the ordinary "no" answer.
        Some(1) => false,
        code => {
            let stderr = String::from_utf8_lossy(&ancestry.stderr).trim().to_string();
            return Err(format!(
                "could not compare the workspace with the parent default branch (exit {code:?}): {stderr}"
            ));
        }
    };
    let commit_status = if merged {
        CowCommitStatus::Merged
    } else if unpublished_commits == 0 {
        CowCommitStatus::Published
    } else {
        CowCommitStatus::Unpublished
    };
    Ok(CowLifecycle {
        commit_status,
        unpublished_commits,
        dirty_provenance: dirty_provenance(record),
    })
}

/// How many commits exist ONLY in this workspace.
///
/// Counts what is reachable from HEAD and from no remote and no mirrored parent
/// ref. That is the number a removal would destroy — a linked worktree has no
/// equivalent, because its objects live in the parent and survive the
/// directory.
///
/// The parent mirror is refreshed and pruned first, so a commit published a
/// moment ago does not still read as unpublished and a deleted branch cannot
/// remain false evidence that a commit is preserved. Refresh failure is an
/// error: stale refs can under-count as well as over-count.
pub(crate) fn unpublished_commit_count(record: &CowRecord) -> Result<usize, String> {
    refresh_parent_mirror(record)?;
    unpublished_commit_count_from_mirrors(record)
}

/// Re-validate a persisted [`CowRecord`] against the directory it names,
/// immediately before [`remove_cow_workspace`] is allowed to delete anything.
///
/// `repositories.json` is trusted for *listing* — that is its job — but never
/// for *deletion*. A row is just JSON: a hand-edit, a restored backup, a bug
/// that wrote `kind: "cow"` onto the wrong entry, or a future migration bug
/// could all make an arbitrary directory — someone's other real repository —
/// look, on paper, like a COW clone this backend owns. `force` exists to waive
/// the dirty-workspace and unpublished-commit *prompts* further down; it has
/// never meant "skip provenance", and this check runs unconditionally, before
/// `force` is even consulted.
///
/// Reuses [`validate_cow_candidate`] — the exact check [`recover_cow_workspaces`]
/// runs before adopting a directory — so removal trusts nothing recovery
/// itself would not have trusted. A legacy COW clone made before markers
/// existed fails this the same way a forged row does: "no marker". That is
/// deliberate. Such a clone is real, and destroying it would be exactly the
/// failure this function exists to prevent — it must be adopted explicitly
/// (`adopt_cow_workspace`) first, which runs this same validation, before
/// removal (or recovery) will touch it.
fn validate_cow_removal_candidate(record: &CowRecord, worktrees_dir: &Path) -> Result<(), String> {
    let candidate = validate_cow_candidate(&record.path, worktrees_dir, &record.parent_repo)
        .map_err(|reason| {
            format!(
                "refusing to remove '{}': it does not validate as a copy-on-write workspace this \
                 backend created ({reason}). If this is a real clone this backend lost track of, \
                 adopt it explicitly first rather than forcing removal.",
                record.path.display()
            )
        })?;
    if candidate.workspace_id != record.workspace_id {
        return Err(format!(
            "refusing to remove '{}': its on-disk workspace id '{}' does not match the registered \
             id '{}' — this row does not describe this directory",
            record.path.display(),
            candidate.workspace_id,
            record.workspace_id
        ));
    }
    Ok(())
}

/// Delete a COW workspace, refusing while it holds commits that exist nowhere
/// else.
///
/// Removal here is an `rm -rf` of an independent repository. Every unpublished
/// commit lives ONLY in it — a failure mode a linked worktree does not have,
/// because its objects are in the parent and outlive the directory. So the
/// count is a gate, not a warning.
///
/// `worktrees_dir` is the repo's actual configured worktree base
/// (`resolve_worktree_dir_for_repo`) — the same directory creation placed this
/// clone directly under — and is what [`validate_cow_removal_candidate`]
/// checks containment against before anything is deleted.
pub(crate) fn remove_cow_workspace(
    record: &CowRecord,
    force: bool,
    worktrees_dir: &Path,
) -> Result<usize, String> {
    if !record.path.exists() {
        // Already gone. Idempotent on purpose: the caller's next step is to drop
        // the row, and refusing here would strand it forever.
        return Ok(0);
    }

    // Unconditional — not behind `if !force`. Provenance is not a prompt.
    validate_cow_removal_candidate(record, worktrees_dir)?;

    if !force {
        let status = git_cmd(&record.path)
            .args(["status", "--porcelain", "--untracked-files=all"])
            .run()
            .map_err(|e| format!("could not check the workspace for uncommitted changes: {e}"))?;
        if !status.stdout.is_empty() {
            return Err(format!(
                "'{}' has uncommitted changes: refusing to remove an independent clone and lose them. Commit or discard them first, or remove with force to lose them.",
                record.workspace_id
            ));
        }
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

    /// Point the config dir at an empty temp dir, so registration/recovery
    /// tests read and write a `repositories.json` we control instead of the
    /// user's real one. Mirrors `worktree::tests::with_repositories_document`,
    /// but with no seed document — these tests build the document themselves,
    /// through the functions under test, rather than asserting against one
    /// hand-written up front.
    fn with_temp_config() -> (impl Drop, TempDir) {
        let config = TempDir::new().expect("config dir");
        let guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        (guard, config)
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
        create_cow_workspace(
            repo,
            dest,
            branch,
            dirty,
            &guards,
            &format!("{branch}~test0000"),
        )
        .expect("clone succeeds")
    }

    fn git_status(repo: &Path) -> String {
        git_cmd(repo)
            .args(["status", "--porcelain"])
            .run()
            .expect("status")
            .stdout
    }

    /// The clone must try every mechanism the probe tries. A single hardcoded
    /// `-c` made the probe answer Supported on Linux — where `-c` is an invalid
    /// option to GNU `cp` — and the clone then failed AFTER `mode=auto` had
    /// already committed to it, so the caller got an error instead of a
    /// worktree.
    #[test]
    fn the_clone_falls_back_to_the_next_mechanism_when_the_first_is_rejected() {
        let temp = TempDir::new().expect("temp");
        let dest = temp.path().join("dest");
        let mut tried = Vec::new();

        let result = clone_tree_with(Path::new("/src"), &dest, |flag| {
            tried.push(flag.to_string());
            if flag == COW_COPY_FLAGS[0] {
                Err("cp: illegal option -- c".to_string())
            } else {
                Ok(())
            }
        });

        assert!(result.is_ok(), "the second mechanism worked: {result:?}");
        assert_eq!(
            tried,
            COW_COPY_FLAGS.to_vec(),
            "both mechanisms tried, in the order the probe uses"
        );
    }

    #[test]
    fn the_clone_stops_at_the_first_mechanism_that_works() {
        let temp = TempDir::new().expect("temp");
        let mut tried = Vec::new();

        clone_tree_with(Path::new("/src"), &temp.path().join("dest"), |flag| {
            tried.push(flag.to_string());
            Ok(())
        })
        .expect("the first mechanism worked");

        assert_eq!(
            tried,
            vec![COW_COPY_FLAGS[0].to_string()],
            "a working mechanism must not be followed by a second copy"
        );
    }

    /// A caller that cannot clone has to know what was attempted: "clone
    /// failed" with one flag's error hides the fact that the other mechanism
    /// was tried too, which is the first thing to check on an unfamiliar
    /// filesystem.
    #[test]
    fn a_clone_that_exhausts_every_mechanism_names_each_one_and_leaves_nothing() {
        let temp = TempDir::new().expect("temp");
        let dest = temp.path().join("dest");

        let err = clone_tree_with(Path::new("/src"), &dest, |flag| {
            std::fs::create_dir_all(dest.join("half")).expect("partial copy");
            Err(format!("no {flag} here"))
        })
        .expect_err("every mechanism refused");

        for flag in COW_COPY_FLAGS {
            assert!(err.contains(flag), "'{flag}' missing from: {err}");
        }
        assert!(
            !dest.exists(),
            "a clone that failed must not leave a torn copy a caller could use"
        );
    }

    /// The next `cp` refuses a destination that already exists, so a torn copy
    /// left by a rejected attempt would turn a recoverable fallback into a hard
    /// failure — and, worse, could survive as something a caller might use.
    #[test]
    fn a_partial_copy_is_removed_before_the_next_mechanism_runs() {
        let temp = TempDir::new().expect("temp");
        let dest = temp.path().join("dest");
        let mut existed_on_entry = Vec::new();

        let result = clone_tree_with(Path::new("/src"), &dest, |flag| {
            existed_on_entry.push(dest.exists());
            // A copy that died halfway: the directory is there, the content is not.
            std::fs::create_dir_all(dest.join("half")).expect("partial copy");
            if flag == COW_COPY_FLAGS[0] {
                Err("interrupted".to_string())
            } else {
                Ok(())
            }
        });

        assert!(result.is_ok());
        assert_eq!(
            existed_on_entry,
            vec![false, false],
            "each attempt starts from a destination that does not exist"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_command_timeout_kills_the_process_before_returning() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 10"]);
        let started = std::time::Instant::now();
        let error = run_copy_command(&mut command, std::time::Duration::from_millis(20))
            .expect_err("slow copy must time out");
        assert!(error.contains("operation deadline"), "{error}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "killing the child must not wait for its original duration"
        );
    }

    #[test]
    fn every_copy_mechanism_refuses_rather_than_falling_back_to_a_byte_copy() {
        // `--reflink=auto` is the trap: it degrades silently, so the probe would
        // report support everywhere and a 19 MB clone would become a 12 GB copy.
        for flag in COW_COPY_FLAGS {
            assert!(
                !flag.contains("auto"),
                "'{flag}' may fall back to a full copy"
            );
        }
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
        let workspace_id = format!("{branch}~aaaa1111");
        let workspace = create_cow_workspace(
            repo,
            &dest,
            branch,
            DirtyPolicy::Inherit,
            &guards,
            &workspace_id,
        )
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
            workspace_id,
            branch: branch.to_string(),
            path: workspace.path,
            parent_repo: repo.to_path_buf(),
            dirty_baseline: workspace.dirty_baseline,
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

    /// A bare repo the parent knows as `origin`, created BEFORE the clone so
    /// the copy inherits the remote exactly as a real workspace does.
    fn with_origin(temp: &TempDir, repo: &Path) -> PathBuf {
        let origin = temp.path().join("origin.git");
        git_cmd(temp.path())
            .args(["init", "--bare", &origin.to_string_lossy()])
            .run()
            .expect("bare origin");
        git_cmd(repo)
            .args(["remote", "add", "origin", &origin.to_string_lossy()])
            .run()
            .expect("remote add origin");
        origin
    }

    /// The P1 failure: the parent REFUSED the fast-forward, and origin advanced
    /// anyway to a history the parent rejected. Origin is the shared truth —
    /// it must never move ahead of the parent that vetoed the move.
    #[test]
    fn publish_does_not_reach_origin_when_the_parent_refuses() {
        let (temp, repo, _dest_parent) = setup();
        let origin = with_origin(&temp, &repo);
        let record = published_fixture(&temp, &repo, "feature", 1);

        // The parent gains its own divergent `feature`, then moves off it so
        // this is about divergence rather than the checked-out guard.
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
        git_cmd(&repo)
            .args(["checkout", "--detach"])
            .run()
            .expect("detach");

        let outcome = publish_cow_workspace(&record).expect("publish reports rather than throwing");

        assert!(!outcome.parent_updated, "the fixture must make it refuse");
        assert!(
            !outcome.origin_pushed,
            "origin was pushed after the parent refused: {outcome:?}"
        );
        assert_eq!(
            rev(&origin, "refs/heads/feature"),
            None,
            "origin advanced to a history the parent rejected"
        );
        assert!(
            outcome
                .origin_error
                .as_deref()
                .unwrap_or_default()
                .contains("parent"),
            "the origin error must name the parent as the reason: {:?}",
            outcome.origin_error
        );
    }

    /// Once the parent accepts, origin gets the commit the PARENT holds — the
    /// push runs from the parent, so the two can never disagree.
    #[test]
    fn publish_pushes_the_accepted_parent_ref_to_origin() {
        let (temp, repo, _dest_parent) = setup();
        let origin = with_origin(&temp, &repo);
        let record = published_fixture(&temp, &repo, "feature", 1);

        let outcome = publish_cow_workspace(&record).expect("publish runs");

        assert!(outcome.parent_updated, "{:?}", outcome.parent_error);
        assert!(outcome.origin_pushed, "{:?}", outcome.origin_error);
        assert_eq!(
            rev(&origin, "refs/heads/feature"),
            rev(&repo, "refs/heads/feature"),
            "origin and the parent disagree about what was published"
        );
    }

    /// "The clone must not reach origin" is an invariant, not an instruction:
    /// the clone's own `origin` is given the same unpushable URL `parent` gets.
    #[test]
    fn a_clone_cannot_push_to_origin_by_itself() {
        let (temp, repo, _dest_parent) = setup();
        with_origin(&temp, &repo);
        let record = published_fixture(&temp, &repo, "feature", 1);

        assert_eq!(
            read_marker(&record.path, "remote.origin.pushurl").as_deref(),
            Some(NO_PUSH_URL),
            "the clone can push straight to origin, bypassing the parent"
        );
        git_cmd(&record.path)
            .args(["push", "origin", "refs/heads/feature:refs/heads/feature"])
            .run()
            .expect_err("the clone pushed to origin");
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

        let err = remove_cow_workspace(&record, false, temp.path()).expect_err("must refuse");

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

        remove_cow_workspace(&record, false, temp.path()).expect("removes without a prompt");
        assert!(!record.path.exists());
    }

    #[test]
    fn a_workspace_with_no_commits_of_its_own_removes_without_a_prompt() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 0);

        assert_eq!(unpublished_commit_count(&record).expect("count"), 0);
        remove_cow_workspace(&record, false, temp.path()).expect("removes");
        assert!(!record.path.exists());
    }

    #[test]
    fn force_removes_a_workspace_that_would_otherwise_be_refused() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 1);
        remove_cow_workspace(&record, false, temp.path()).expect_err("refuses without force");

        let lost = remove_cow_workspace(&record, true, temp.path()).expect("force removes");

        assert_eq!(lost, 1, "force must report what it destroyed");
        assert!(!record.path.exists());
    }

    #[test]
    fn removal_refuses_each_kind_of_uncommitted_change_unless_forced() {
        for kind in ["unstaged", "staged", "untracked"] {
            let (temp, repo, _dest_parent) = setup();
            let record = published_fixture(&temp, &repo, kind, 0);
            let evidence = match kind {
                "unstaged" => {
                    let path = record.path.join("README.md");
                    fs::write(&path, "changed only in this clone\n").expect("modify tracked file");
                    path
                }
                "staged" => {
                    let path = record.path.join("staged.txt");
                    fs::write(&path, "staged only in this clone\n").expect("write staged file");
                    git_cmd(&record.path)
                        .args(["add", "staged.txt"])
                        .run()
                        .expect("stage file");
                    path
                }
                "untracked" => {
                    let path = record.path.join("scratch.txt");
                    fs::write(&path, "untracked only in this clone\n")
                        .expect("write untracked file");
                    path
                }
                _ => unreachable!(),
            };

            let err = remove_cow_workspace(&record, false, temp.path())
                .expect_err("dirty clone must be kept");

            assert!(err.contains("uncommitted changes"), "{kind}: {err}");
            assert!(evidence.exists(), "the {kind} evidence was deleted anyway");

            remove_cow_workspace(&record, true, temp.path())
                .expect("explicit force removes dirty clone");
            assert!(!record.path.exists());
        }
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

        remove_cow_workspace(&first, false, temp.path()).expect("removes");

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

        let err = remove_cow_workspace(&record, false, temp.path()).expect_err("must refuse");

        assert!(err.contains("no '.git' directory"), "{err}");
        assert!(record.path.join("something-else.txt").exists());
    }

    #[test]
    fn removing_an_already_gone_workspace_is_not_an_error() {
        let (temp, repo, _dest_parent) = setup();
        let record = published_fixture(&temp, &repo, "feature", 0);
        fs::remove_dir_all(&record.path).expect("remove");

        // Idempotent: the caller's next step is to drop the row, and refusing
        // here would strand it forever.
        assert_eq!(
            remove_cow_workspace(&record, false, temp.path()).expect("no error"),
            0
        );
    }

    // ── removal trust boundary: a persisted row is never enough on its own ──

    fn init_real_repo(dir: &Path) {
        fs::create_dir_all(dir).expect("dir");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
        ] {
            git_cmd(dir).args(args).run().expect("git setup");
        }
        fs::write(dir.join("precious.txt"), "someone else's work\n").expect("write");
        git_cmd(dir).args(["add", "."]).run().expect("add");
        git_cmd(dir)
            .args(["commit", "-m", "unrelated history"])
            .run()
            .expect("commit");
    }

    /// The P0 case: `repositories.json` says `kind: "cow"` for a directory that
    /// is a real, unrelated repository this backend never cloned — the shape a
    /// hand-edit, a restored backup, or a future bug could produce. `force`
    /// exists to waive the dirty/unpublished-commit prompts, not provenance:
    /// even forced, the row must be refused and the victim must survive,
    /// whether it sits outside the configured worktree base entirely or right
    /// alongside real COW clones inside it.
    #[test]
    fn removal_refuses_a_forged_row_naming_an_unrelated_repository_outside_the_worktree_base() {
        let (temp, repo, dest_parent) = setup();
        let victim = temp.path().join("victim-outside-worktree-base");
        init_real_repo(&victim);

        let forged = CowRecord {
            workspace_id: "feature~aaaa1111".to_string(),
            branch: "main".to_string(),
            path: victim.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: None,
        };

        let err = remove_cow_workspace(&forged, true, &dest_parent)
            .expect_err("a forged row must be refused even with force");

        assert!(
            err.contains("does not validate as a copy-on-write workspace"),
            "{err}"
        );
        assert!(victim.join(".git").is_dir(), "the victim repo was deleted");
        assert!(
            victim.join("precious.txt").exists(),
            "the victim's history was deleted"
        );
    }

    /// Same failure, but the victim sits INSIDE the configured worktree base —
    /// exactly where a real COW clone would — so containment alone cannot be
    /// what saves it. Only the missing provenance markers do.
    #[test]
    fn removal_refuses_a_forged_row_naming_an_unrelated_repository_inside_the_worktree_base() {
        let (_temp, repo, dest_parent) = setup();
        let victim = dest_parent.join("victim-inside-worktree-base");
        init_real_repo(&victim);

        let forged = CowRecord {
            workspace_id: "feature~aaaa1111".to_string(),
            branch: "main".to_string(),
            path: victim.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: None,
        };

        let err = remove_cow_workspace(&forged, true, &dest_parent)
            .expect_err("a markerless directory must be refused even with force");

        assert!(
            err.contains("does not validate as a copy-on-write workspace"),
            "{err}"
        );
        assert!(err.contains("adopt it explicitly"), "{err}");
        assert!(victim.join(".git").is_dir(), "the victim repo was deleted");
        assert!(
            victim.join("precious.txt").exists(),
            "the victim's history was deleted"
        );
    }

    /// A row whose on-disk workspace id disagrees with the registered one:
    /// same directory, but the marker was written for (or corrupted into) a
    /// different id than the row claims. Refused rather than trusted because
    /// the row does not describe this directory, whatever else lines up.
    #[test]
    fn removal_refuses_when_the_registered_id_does_not_match_the_on_disk_marker() {
        let (_temp, repo, dest_parent) = setup();
        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        let mismatched = CowRecord {
            workspace_id: "not-the-marked-id~ffffffff".to_string(),
            branch: "feature".to_string(),
            path: workspace.path.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: None,
        };

        let err = remove_cow_workspace(&mismatched, true, &dest_parent)
            .expect_err("an id mismatch must be refused even with force");

        assert!(err.contains("does not match the registered id"), "{err}");
        assert!(
            workspace.path.join(".git").is_dir(),
            "the clone was deleted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn removal_refuses_a_candidate_whose_git_directory_is_a_symlink_even_with_force() {
        let (temp, repo, dest_parent) = setup();
        // A real, validly marked clone living elsewhere...
        let elsewhere = temp.path().join("real-elsewhere");
        let workspace = clone_into(&repo, &elsewhere, "feature", DirtyPolicy::Inherit);
        let real_git = workspace.path.join(".git");
        // ...and a candidate inside the worktree base whose `.git` is a symlink
        // to it, rather than a real directory of its own. Following the symlink
        // would let this candidate pass every marker check by proxy.
        let candidate = dest_parent.join("clone");
        fs::create_dir_all(&candidate).expect("dir");
        std::os::unix::fs::symlink(&real_git, candidate.join(".git")).expect("symlink .git");

        let record = CowRecord {
            workspace_id: "feature~test0000".to_string(),
            branch: "feature".to_string(),
            path: candidate.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: None,
        };

        let err = remove_cow_workspace(&record, true, &dest_parent)
            .expect_err("a .git symlink must be refused even with force");

        assert!(err.contains("'.git' symlink"), "{err}");
        assert!(
            real_git.is_dir(),
            "the real clone's .git directory was deleted through the symlink"
        );
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
            "feature~aaaa0000",
        )
        .expect_err("must refuse");

        assert!(err.contains("already exists"), "{err}");
        assert!(
            dest.join("keep.txt").exists(),
            "the existing directory was touched"
        );
    }

    // ── registering, unregistering, and recovering workspaces ─────────────

    #[test]
    fn creation_writes_the_workspace_id_and_canonical_parent_into_the_clones_own_config() {
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);

        assert_eq!(
            read_marker(&workspace.path, COW_MARKER_WORKSPACE_ID_KEY).as_deref(),
            Some("feature~test0000")
        );
        let canonical_repo = std::fs::canonicalize(&repo).expect("canonical repo");
        assert_eq!(
            read_marker(&workspace.path, COW_MARKER_PARENT_KEY).map(PathBuf::from),
            Some(canonical_repo)
        );
    }

    #[test]
    fn register_cow_workspace_persists_id_branch_kind_path_and_parent() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);

        register_cow_workspace(&repo, "feature~test0000", &workspace).expect("register");

        let records = cow_workspaces_for(&repo);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].workspace_id, "feature~test0000");
        assert_eq!(records[0].branch, "feature");
        assert_eq!(records[0].path, workspace.path);
        assert_eq!(records[0].parent_repo, repo);
        assert_eq!(
            records[0].dirty_baseline,
            Some(Vec::new()),
            "a clean parent still records a baseline — an empty one, not a missing one"
        );
    }

    /// The baseline this backend can only ever record honestly: paths dirty at
    /// the moment of creation, written into the same persisted row `register_cow_workspace`
    /// already writes, and readable back through a brand new `cow_workspaces_for`
    /// call — the same read a restarted process would make, since nothing about
    /// this baseline lives anywhere but `repositories.json`.
    #[test]
    fn register_cow_workspace_persists_the_dirty_baseline_and_survives_a_fresh_read() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");
        fs::write(repo.join("scratch.txt"), "untracked\n").expect("untracked");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        register_cow_workspace(&repo, "feature~test0000", &workspace).expect("register");

        // A fresh read from disk — `cow_workspaces_for` opens and parses
        // `repositories.json` on every call, so this is indistinguishable from
        // what a restarted process would see.
        let records = cow_workspaces_for(&repo);
        let baseline = records[0]
            .dirty_baseline
            .as_ref()
            .expect("baseline recorded");
        assert_eq!(baseline.len(), 2, "{baseline:?}");
        assert!(
            baseline.iter().any(|p| p.contains("README.md")),
            "{baseline:?}"
        );
        assert!(
            baseline.iter().any(|p| p.contains("scratch.txt")),
            "{baseline:?}"
        );
    }

    // ── dirty provenance: inherited vs. changed-since-creation ────────────

    #[test]
    fn dirty_provenance_reports_inherited_only_when_nothing_new_is_dirty() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        let record = CowRecord {
            workspace_id: "feature~test0000".to_string(),
            branch: "feature".to_string(),
            path: workspace.path.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: workspace.dirty_baseline.clone(),
        };

        assert_eq!(
            dirty_provenance(&record),
            Some(DirtyProvenance::InheritedOnly)
        );
    }

    #[test]
    fn dirty_provenance_reports_changed_since_creation_once_a_new_edit_lands() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        let record = CowRecord {
            workspace_id: "feature~test0000".to_string(),
            branch: "feature".to_string(),
            path: workspace.path.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: workspace.dirty_baseline.clone(),
        };

        fs::write(
            workspace.path.join("new-work.txt"),
            "added after creation\n",
        )
        .expect("write new file inside the workspace");

        assert_eq!(
            dirty_provenance(&record),
            Some(DirtyProvenance::ChangedSinceCreation)
        );
    }

    #[test]
    fn dirty_provenance_reports_clean_once_the_inherited_files_are_cleaned() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");
        fs::write(repo.join("scratch.txt"), "untracked\n").expect("untracked");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        let record = CowRecord {
            workspace_id: "feature~test0000".to_string(),
            branch: "feature".to_string(),
            path: workspace.path.clone(),
            parent_repo: repo.clone(),
            dirty_baseline: workspace.dirty_baseline.clone(),
        };

        git_cmd(&workspace.path)
            .args(["checkout", "--", "README.md"])
            .run()
            .expect("revert the inherited modification");
        fs::remove_file(workspace.path.join("scratch.txt"))
            .expect("clean the inherited untracked file");

        assert_eq!(dirty_provenance(&record), Some(DirtyProvenance::Clean));
    }

    #[test]
    fn dirty_provenance_is_none_without_a_recorded_baseline() {
        let (_temp, repo, dest_parent) = setup();
        fs::write(repo.join("README.md"), "# Test\nwork in progress\n").expect("modify");

        let workspace = clone_into(
            &repo,
            &dest_parent.join("clone"),
            "feature",
            DirtyPolicy::Inherit,
        );
        // A recovered or adopted clone: this backend never witnessed its
        // creation, so it carries no baseline — never guessed at from disk.
        let record = CowRecord {
            workspace_id: "feature~test0000".to_string(),
            branch: "feature".to_string(),
            path: workspace.path,
            parent_repo: repo,
            dirty_baseline: None,
        };

        assert_eq!(dirty_provenance(&record), None);
    }

    /// The failure this guards against: a clone lands on disk, its registration
    /// fails, and something "cleans up" by deleting the one place the work
    /// still exists. The clone must survive a failed registration untouched —
    /// `recover_cow_workspaces` is what turns this into a recoverable gap
    /// rather than a lost workspace.
    #[test]
    fn register_cow_workspace_failure_leaves_the_clone_on_disk() {
        let (_guard, config) = with_temp_config();
        fs::write(config.path().join("repositories.json"), "{ not json")
            .expect("seed corrupt file");
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);

        let err = register_cow_workspace(&repo, "feature~test0000", &workspace)
            .expect_err("a corrupt document must refuse rather than silently reset");
        assert!(!err.is_empty());
        assert!(
            workspace.path.join(".git").is_dir(),
            "the clone must survive a failed registration"
        );
    }

    #[test]
    fn unregister_cow_workspace_drops_the_row_and_is_idempotent() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);
        register_cow_workspace(&repo, "feature~test0000", &workspace).expect("register");

        unregister_cow_workspace(&repo, "feature~test0000").expect("unregister");
        assert!(cow_workspaces_for(&repo).is_empty());

        unregister_cow_workspace(&repo, "feature~test0000")
            .expect("removing an already-absent row is a no-op, not an error");
    }

    #[test]
    fn recover_cow_workspaces_heals_a_registration_that_never_happened() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);
        // Simulates the crash window `register_cow_workspace` now closes: the
        // clone exists, but nothing ever registered it.
        assert!(cow_workspaces_for(&repo).is_empty());

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 1);
        let records = cow_workspaces_for(&repo);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].workspace_id, "feature~test0000");
        assert_eq!(records[0].branch, "feature");
        assert_eq!(
            records[0].path,
            std::fs::canonicalize(&workspace.path).expect("canonical clone path")
        );
    }

    #[test]
    fn recover_cow_workspaces_ignores_a_plain_git_repo_without_markers() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        // A real, unrelated git repository living in the same directory pool —
        // never created through `create_cow_workspace`, so it carries none of
        // this backend's markers. Adopting it on a guess is exactly the
        // failure a markerless legacy clone would also trigger.
        let stray = dest_parent.join("stray");
        fs::create_dir_all(&stray).expect("stray dir");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@test.com"],
            vec!["config", "user.name", "Test"],
        ] {
            git_cmd(&stray).args(args).run().expect("git setup");
        }
        fs::write(stray.join("f.txt"), "x").expect("write");
        git_cmd(&stray).args(["add", "."]).run().expect("add");
        git_cmd(&stray)
            .args(["commit", "-m", "c"])
            .run()
            .expect("commit");

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 0);
        assert!(cow_workspaces_for(&repo).is_empty());
    }

    #[test]
    fn recover_cow_workspaces_rejects_a_clone_marked_for_a_different_parent() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let (_temp2, other_repo, _other_dest) = setup();
        let dest = dest_parent.join("clone");
        // Marked parent is `other_repo`, not `repo` — recovering `repo` must
        // not adopt it.
        clone_into(&other_repo, &dest, "feature", DirtyPolicy::Inherit);

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 0, "the clone belongs to a different parent");
        assert!(cow_workspaces_for(&repo).is_empty());
    }

    #[test]
    fn recover_cow_workspaces_ignores_a_directory_with_no_git_repo() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        fs::create_dir_all(dest_parent.join("empty")).expect("empty dir");

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 0);
    }

    /// A linked worktree can share the same directory pool as a COW clone —
    /// both are created under the same `worktrees_dir` — and its `.git` is a
    /// FILE, not a directory. That is what tells the two mechanisms apart
    /// during a scan, and it must never be adopted as a COW row.
    #[test]
    fn recover_cow_workspaces_ignores_a_linked_worktree_sharing_the_directory_pool() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let linked = dest_parent.join("linked");
        git_cmd(&repo)
            .args([
                "worktree",
                "add",
                "-b",
                "linked-branch",
                &linked.to_string_lossy(),
            ])
            .run()
            .expect("git worktree add");
        assert!(
            linked.join(".git").is_file(),
            "sanity: a linked worktree's .git is a file"
        );

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 0);
        assert!(cow_workspaces_for(&repo).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn recover_cow_workspaces_refuses_to_follow_a_symlink_into_the_worktree_base() {
        let (_guard, _config) = with_temp_config();
        let (temp, repo, dest_parent) = setup();
        // A real, validly marked clone — but the scan only ever reaches it
        // through a symlink sitting inside `dest_parent`, which must not be
        // followed: a symlink is exactly how a directory of unknown
        // provenance could be made to look like an immediate child.
        let real = temp.path().join("elsewhere-clone");
        let workspace = clone_into(&repo, &real, "feature", DirtyPolicy::Inherit);
        std::os::unix::fs::symlink(&real, dest_parent.join("linked-in")).expect("symlink");

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 0, "a symlinked-in clone must not be adopted");
        assert!(cow_workspaces_for(&repo).is_empty());
        assert!(
            workspace.path.join(".git").is_dir(),
            "the real clone is untouched"
        );
    }

    #[test]
    fn recover_cow_workspaces_rejects_a_blank_marker_value() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        let workspace = clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);
        config_or_fail(&workspace.path, COW_MARKER_WORKSPACE_ID_KEY, "").expect("blank the marker");

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(
            registered, 0,
            "a blank marker must not be treated as present"
        );
    }

    /// Two clones cannot legitimately share an id — this simulates the marker
    /// being corrupted into agreement rather than trying to reproduce the
    /// underlying cause. Aliasing the second onto the first's row would point
    /// one workspace id at two different directories depending on scan order.
    #[test]
    fn recover_cow_workspaces_skips_a_duplicate_workspace_id_rather_than_aliasing_it() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let first = clone_into(
            &repo,
            &dest_parent.join("first"),
            "feature",
            DirtyPolicy::Inherit,
        );
        let second = clone_into(
            &repo,
            &dest_parent.join("second"),
            "other",
            DirtyPolicy::Inherit,
        );
        config_or_fail(
            &second.path,
            COW_MARKER_WORKSPACE_ID_KEY,
            "feature~test0000",
        )
        .expect("force a duplicate id");
        let _ = first;

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        assert_eq!(registered, 1, "only the first one seen registers");
        let records = cow_workspaces_for(&repo);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].workspace_id, "feature~test0000");
    }

    #[test]
    fn recover_cow_workspaces_does_not_re_register_an_id_the_caller_already_has() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);
        let mut taken_ids = std::collections::HashSet::new();
        taken_ids.insert("feature~test0000".to_string());

        let registered = recover_cow_workspaces(
            &repo,
            &dest_parent,
            &taken_ids,
            &std::collections::HashSet::new(),
        );

        assert_eq!(
            registered, 0,
            "an id the caller already considers taken must not be adopted from disk"
        );
        assert!(
            cow_workspaces_for(&repo).is_empty(),
            "recovery must not create a row behind an id it was told is already spoken for"
        );
    }

    #[test]
    fn recover_cow_workspaces_never_mutates_the_scanned_directories() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let dest = dest_parent.join("clone");
        clone_into(&repo, &dest, "feature", DirtyPolicy::Inherit);
        let before = tree_fingerprint(&dest_parent);

        recover_cow_workspaces(
            &repo,
            &dest_parent,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );

        let after = tree_fingerprint(&dest_parent);
        assert_eq!(
            before, after,
            "discovery must not touch the candidate directories"
        );
    }

    // ── explicit adoption of a markerless COW clone ───────────────────────

    /// A real COW clone with the two `tuicommander.cow.*` markers stripped
    /// back out — the shape a clone made before those markers existed still
    /// has: the `parent` remote and its no-push `pushurl` (present since the
    /// mechanism's first version), but neither marker.
    fn make_markerless_clone(repo: &Path, dest: &Path, branch: &str) -> PathBuf {
        let workspace = clone_into(repo, dest, branch, DirtyPolicy::Inherit);
        for key in [COW_MARKER_WORKSPACE_ID_KEY, COW_MARKER_PARENT_KEY] {
            git_cmd(&workspace.path)
                .args(["config", "--unset", key])
                .run()
                .expect("unset marker");
        }
        workspace.path
    }

    #[test]
    fn adopt_cow_workspace_registers_a_valid_markerless_clone() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");
        let canonical_candidate = std::fs::canonicalize(&candidate).expect("canonical");

        let record = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect("a valid markerless clone must be adoptable");

        assert_eq!(record.branch, "feature");
        assert_eq!(record.path, canonical_candidate);
        assert_eq!(
            read_marker(&candidate, COW_MARKER_WORKSPACE_ID_KEY).as_deref(),
            Some(record.workspace_id.as_str()),
            "adoption must write the workspace-id marker"
        );
        assert_eq!(
            read_marker(&candidate, COW_MARKER_PARENT_KEY).map(PathBuf::from),
            Some(std::fs::canonicalize(&repo).expect("canonical repo")),
            "adoption must write the parent marker"
        );

        let records = cow_workspaces_for(&repo);
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(records[0].workspace_id, record.workspace_id);
        assert_eq!(records[0].path, canonical_candidate);
    }

    #[test]
    fn adopt_cow_workspace_accepts_a_caller_supplied_workspace_id() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");

        let record = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            Some("feature~caller0"),
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect("adoption with a caller-supplied id succeeds");

        assert_eq!(record.workspace_id, "feature~caller0");
        assert_eq!(
            read_marker(&candidate, COW_MARKER_WORKSPACE_ID_KEY).as_deref(),
            Some("feature~caller0")
        );
    }

    /// Retrying an adoption whose registry write failed must be idempotent:
    /// the markers are already there from the first attempt, and re-running
    /// with the same id must not error or duplicate the row.
    #[test]
    fn adopt_cow_workspace_retried_with_the_same_id_is_idempotent() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");

        let first = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            Some("feature~retry00"),
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect("first attempt succeeds");

        // Simulate the caller retrying with the same id after, say, a transient
        // registry write failure: taken sets are recomputed from disk, so the
        // id this same candidate already carries is not "taken" by anyone else.
        let second = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            Some("feature~retry00"),
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect("retrying with the same id must be idempotent");

        assert_eq!(first, second);
        assert_eq!(
            cow_workspaces_for(&repo).len(),
            1,
            "must not duplicate the row"
        );
    }

    /// The same retry as above, but for the caller that could not keep the id
    /// the first attempt returned (the common shape of "the registry write
    /// failed" — the failure happens on the way out, after the id has already
    /// been decided but before the caller necessarily gets to record it) and
    /// so retries with `workspace_id: None`. The old code re-minted a second
    /// id in that case and overwrote the markers the first attempt had
    /// already written — contradicting the documented retry contract. The fix
    /// inspects the candidate's own markers before minting anything: both
    /// present and naming this parent means reuse, not re-mint.
    #[test]
    fn adopt_cow_workspace_retried_with_none_after_registry_failure_reuses_the_same_id() {
        let (_guard, config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");

        // Force the registry write inside `adopt_cow_workspace` to fail AFTER
        // the provenance markers are written, by making `repositories.json`
        // unparsable — the same corruption `config.rs`'s
        // `upsert_workspace_record_on_a_corrupt_file_refuses_and_backs_up_rather_than_overwrites`
        // exercises directly.
        let repo_file = config.path().join("repositories.json");
        fs::write(&repo_file, "{ not json").expect("seed corrupt registry file");

        let err = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect_err("a corrupt registry must fail registration after the markers are written");
        assert!(err.contains("could not register"), "{err}");

        let minted_id = read_marker(&candidate, COW_MARKER_WORKSPACE_ID_KEY)
            .expect("the markers must already be written despite the registry failure");
        assert!(
            !repo_file.exists(),
            "the strict loader moves a corrupt file aside rather than leaving it in place"
        );

        // Repair the config directory the way an operator/next-launch recovery
        // would: seed a valid, empty document in place of the corrupt one that
        // got backed up aside.
        fs::write(
            &repo_file,
            serde_json::json!({"repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []})
                .to_string(),
        )
        .expect("repair the registry");

        let retried = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect("retrying with None must reuse the existing markers rather than re-minting");

        assert_eq!(
            retried.workspace_id, minted_id,
            "retry must reuse the id already written to the candidate's markers, not mint a new one"
        );
        assert_eq!(
            read_marker(&candidate, COW_MARKER_WORKSPACE_ID_KEY).as_deref(),
            Some(minted_id.as_str()),
            "the marker must be unchanged (same id), not overwritten by a fresh mint"
        );

        let records = cow_workspaces_for(&repo);
        assert_eq!(records.len(), 1, "must not duplicate the row: {records:?}");
        assert_eq!(records[0].workspace_id, minted_id);
    }

    #[test]
    fn adopt_cow_workspace_refuses_a_directory_missing_the_parent_remote_evidence() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let stray = dest_parent.join("stray");
        init_real_repo(&stray);

        let err = adopt_cow_workspace(
            &repo,
            &stray,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect_err("a directory with no COW-clone evidence must be refused");

        assert!(err.contains("no-push parent-remote evidence"), "{err}");
        assert!(
            read_marker(&stray, COW_MARKER_WORKSPACE_ID_KEY).is_none(),
            "a failed adoption must not write a marker"
        );
        assert!(cow_workspaces_for(&repo).is_empty());
    }

    #[test]
    fn adopt_cow_workspace_refuses_a_clone_marked_for_a_different_parent() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let (_temp2, other_repo, _other_dest) = setup();
        let candidate = make_markerless_clone(&other_repo, &dest_parent.join("legacy"), "feature");

        let err = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect_err("a clone of a different repository must be refused");

        assert!(err.contains("not a clone of this repository"), "{err}");
        assert!(cow_workspaces_for(&repo).is_empty());
    }

    #[test]
    fn adopt_cow_workspace_refuses_a_candidate_already_registered_under_another_id() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");
        let canonical_candidate = std::fs::canonicalize(&candidate).expect("canonical");
        let mut taken_paths = std::collections::HashSet::new();
        taken_paths.insert(canonical_candidate);

        let err = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &taken_paths,
        )
        .expect_err("a path the caller already has must be refused");

        assert!(err.contains("already registered"), "{err}");
    }

    #[test]
    fn adopt_cow_workspace_refuses_a_workspace_id_already_taken() {
        let (_guard, _config) = with_temp_config();
        let (_temp, repo, dest_parent) = setup();
        let candidate = make_markerless_clone(&repo, &dest_parent.join("legacy"), "feature");
        let mut taken_ids = std::collections::HashSet::new();
        taken_ids.insert("feature~taken00".to_string());

        let err = adopt_cow_workspace(
            &repo,
            &candidate,
            &dest_parent,
            Some("feature~taken00"),
            &taken_ids,
            &std::collections::HashSet::new(),
        )
        .expect_err("a duplicate caller-supplied id must be refused");

        assert!(err.contains("already in use"), "{err}");
        assert!(
            read_marker(&candidate, COW_MARKER_WORKSPACE_ID_KEY).is_none(),
            "a refused adoption must not write a marker"
        );
    }

    #[cfg(unix)]
    #[test]
    fn adopt_cow_workspace_refuses_a_symlinked_in_candidate() {
        let (_guard, _config) = with_temp_config();
        let (temp, repo, dest_parent) = setup();
        let real = temp.path().join("elsewhere-legacy");
        let candidate = make_markerless_clone(&repo, &real, "feature");
        let linked_in = dest_parent.join("linked-in");
        std::os::unix::fs::symlink(&candidate, &linked_in).expect("symlink");

        let err = adopt_cow_workspace(
            &repo,
            &linked_in,
            &dest_parent,
            None,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        )
        .expect_err("a symlinked-in candidate must be refused");

        assert!(err.contains("symlink"), "{err}");
        assert!(cow_workspaces_for(&repo).is_empty());
    }
}
