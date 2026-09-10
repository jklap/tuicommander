//! Copies (or symlinks) ignored, untracked, and explicitly-listed files from a
//! repo's source worktree into a freshly created worktree.
//!
//! `git worktree add` only ever checks out tracked, committed content — this
//! module is what makes the "Copy ignored files" / "Copy untracked files"
//! settings (`config.rs`'s `copy_ignored_files`/`copy_untracked_files`) and a
//! repo's explicit `copy_paths` list actually do something. Those settings
//! were previously fully plumbed through persistence, resolution, and the
//! settings UI, but had no consumer anywhere — this is that consumer.

use crate::config::{CopyPathEntry, CopyPathMode};
use crate::git_cli::git_cmd;
use std::collections::HashSet;
use std::path::{Component, Path};

/// One path to sync from the source worktree into the new one, and how.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SyncPathSpec {
    /// Relative to the repo root, forward-slash separated.
    pub(crate) relative_path: String,
    pub(crate) mode: CopyPathMode,
}

/// Outcome of a sync pass, reported via `worktree-sync-completed`.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct SyncSummary {
    pub(crate) copied: usize,
    pub(crate) total: usize,
    /// One message per path that failed or was skipped (e.g. the source didn't
    /// exist, or the destination already exists). Not fatal — the rest of the
    /// sync still runs — but surfaced so a silently-missing `.env` doesn't look
    /// like it actually got copied.
    pub(crate) errors: Vec<String>,
}

/// Enumerate untracked (`--others`) and/or ignored (`--others --ignored`)
/// paths in `source`, respecting `.gitignore` (`--exclude-standard`). Returns
/// relative, forward-slash paths as reported by git. Empty on any git failure
/// (e.g. `source` isn't a git worktree) rather than propagating an error —
/// enumeration failure just means "nothing to copy from git status," and must
/// never block worktree creation.
///
/// Uses `-z` (NUL-terminated, unquoted): without it, git C-quotes any
/// filename with a non-ASCII or otherwise "unusual" byte (e.g. `café.txt`
/// comes back as the literal 12-character string `"caf\303\251.txt"`,
/// quote marks and octal escapes included) whenever `core.quotepath`'s
/// default (`true`) is in effect, which `source.join(...)` would then treat
/// as a literal (nonexistent) filename — silently dropping that entry as
/// "source path does not exist" instead of syncing it.
fn enumerate_git_paths(source: &Path, untracked: bool, ignored: bool) -> Vec<String> {
    let mut out = Vec::new();
    if untracked
        && let Some(output) = git_cmd(source)
            .args(["ls-files", "-z", "--others", "--exclude-standard"])
            .run_silent()
    {
        out.extend(
            output
                .stdout
                .split('\0')
                .filter(|l| !l.is_empty())
                .map(String::from),
        );
    }
    if ignored
        && let Some(output) = git_cmd(source)
            .args([
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
            ])
            .run_silent()
    {
        out.extend(
            output
                .stdout
                .split('\0')
                .filter(|l| !l.is_empty())
                .map(String::from),
        );
    }
    out
}

/// Build the full, deduplicated list of paths to sync: git-enumerated
/// ignored/untracked paths, plus the repo's explicit `copy_paths` list, which
/// applies unconditionally regardless of the two toggles above. An explicit
/// entry always wins on a duplicate path (its mode — e.g. Symlink — can
/// override the plain Copy a git-enumerated entry would otherwise use).
pub(crate) fn build_sync_specs(
    source: &Path,
    copy_ignored: bool,
    copy_untracked: bool,
    explicit: &[SyncPathSpec],
) -> Vec<SyncPathSpec> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut specs = Vec::new();

    for rel in enumerate_git_paths(source, copy_untracked, copy_ignored) {
        if seen.insert(rel.clone()) {
            specs.push(SyncPathSpec {
                relative_path: rel,
                mode: CopyPathMode::Copy,
            });
        }
    }
    for spec in explicit {
        specs.retain(|s| s.relative_path != spec.relative_path);
        specs.push(spec.clone());
    }
    specs
}

/// Convert the config-layer `CopyPathEntry` list (as persisted per-repo) into
/// the specs `build_sync_specs`/`sync_paths` operate on.
pub(crate) fn specs_from_copy_path_entries(entries: &[CopyPathEntry]) -> Vec<SyncPathSpec> {
    entries
        .iter()
        .map(|e| SyncPathSpec {
            relative_path: e.path.clone(),
            mode: e.mode,
        })
        .collect()
}

/// Copy or symlink each spec's relative path from `source` into `dest`, in
/// order. Calls `on_progress(completed, total)` after every entry (success or
/// failure) so a caller can throttle progress events. Never panics or
/// short-circuits on a single bad entry — every entry gets a chance, and
/// failures land in `SyncSummary.errors`.
pub(crate) fn sync_paths(
    source: &Path,
    dest: &Path,
    specs: &[SyncPathSpec],
    mut on_progress: impl FnMut(usize, usize),
) -> SyncSummary {
    let total = specs.len();
    let mut summary = SyncSummary {
        copied: 0,
        total,
        errors: Vec::new(),
    };

    for (i, spec) in specs.iter().enumerate() {
        match sync_one(source, dest, spec) {
            Ok(()) => summary.copied += 1,
            Err(e) => summary.errors.push(format!("{}: {e}", spec.relative_path)),
        }
        on_progress(i + 1, total);
    }
    summary
}

fn sync_one(source: &Path, dest: &Path, spec: &SyncPathSpec) -> Result<(), String> {
    let rel = Path::new(&spec.relative_path);
    if rel.as_os_str().is_empty() {
        return Err("empty path".to_string());
    }
    // An explicit `copy_paths` entry is user-authored config (no `.tuic.json`/
    // global tier to inherit from — see `config::resolve_copy_settings_from`),
    // but it must still never escape the (source, dest) pair, and a typo like
    // "." or ".git" must not turn into recursively copying (or symlinking)
    // the ENTIRE source repo — including its real `.git` — on top of the new
    // worktree's own linked-worktree `.git` *file*, which would corrupt it.
    if rel.is_absolute() || rel.components().all(|c| matches!(c, Component::CurDir)) {
        return Err("path escapes the repository (absolute or is the repository root)".to_string());
    }
    for component in rel.components() {
        match component {
            Component::ParentDir => {
                return Err("path escapes the repository (contains '..')".to_string());
            }
            Component::Normal(part) if part.to_str() == Some(".git") => {
                return Err("path touches .git".to_string());
            }
            _ => {}
        }
    }

    // `dest` is `git worktree add`'s checkout of whatever branch the caller
    // asked for — which can be an attacker-influenced PR head_ref (see the
    // `--` end-of-options guard in `create_worktree_internal` for the same
    // threat model elsewhere in this file). A branch can commit a symlink at
    // any *intermediate* path component (e.g. a directory named `config`,
    // `node_modules`, or anything matching this repo's own `copy_paths`
    // entries) — plain path joins and `fs::create_dir_all`/`fs::copy` follow
    // symlinks in every component except the final one, so without this
    // check a synced file would be written through that symlink to wherever
    // the malicious branch pointed it, using this (trusted) repo's own
    // content. Reject outright rather than following it.
    if let Some(bad) = first_symlinked_ancestor(dest, rel) {
        return Err(format!(
            "path traverses a symlink at an intermediate component ({})",
            bad.display()
        ));
    }
    // Deliberately NOT applied to `source`: `source` is the trusted main
    // checkout, not attacker-influenced content, so an intermediate symlink
    // there (e.g. `vendor -> /some/shared/cache`) is just the user's own
    // environment — the same "user is the trust boundary" precedent this
    // codebase already applies elsewhere (see AGENTS.md's Accepted Security
    // Decisions). Restricting it would also break the legitimate case of an
    // ignored/untracked entry that already lives behind such a symlink.

    let src_path = source.join(rel);
    let dest_path = dest.join(rel);

    if src_path.symlink_metadata().is_err() {
        return Err("source path does not exist".to_string());
    }

    // Never overwrite something already in the new worktree — a tracked file
    // git already checked out, or an earlier entry in this same sync pass.
    if dest_path.symlink_metadata().is_ok() {
        return Err("destination already exists".to_string());
    }

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create parent dir: {e}"))?;
    }

    match spec.mode {
        CopyPathMode::Symlink => create_symlink(&src_path, &dest_path),
        CopyPathMode::Copy => copy_recursive(&src_path, &dest_path),
    }
}

/// Walk `root.join(rel)` one path component at a time and return the first
/// *intermediate* component (i.e. never the final one — that's the caller's
/// own responsibility, e.g. the "destination already exists" check) that
/// already exists as a symlink. `None` means the whole intermediate chain is
/// real directories (or doesn't exist yet), so it's safe to `create_dir_all`
/// through it.
fn first_symlinked_ancestor(root: &Path, rel: &Path) -> Option<std::path::PathBuf> {
    let mut current = root.to_path_buf();
    let mut components = rel.components().peekable();
    while let Some(component) = components.next() {
        current.push(component);
        if components.peek().is_none() {
            break;
        }
        if current
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Some(current);
        }
    }
    None
}

/// Create a symlink at `dest` pointing at `src` (canonicalized, so the link
/// keeps working if `dest`'s worktree is relocated relative to `src`'s).
fn create_symlink(src: &Path, dest: &Path) -> Result<(), String> {
    let target = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dest)
            .map_err(|e| format!("failed to create symlink: {e}"))
    }
    #[cfg(windows)]
    {
        let meta = std::fs::metadata(&target)
            .map_err(|e| format!("failed to stat symlink target: {e}"))?;
        if meta.is_dir() {
            std::os::windows::fs::symlink_dir(&target, dest)
                .map_err(|e| format!("failed to create dir symlink: {e}"))
        } else {
            std::os::windows::fs::symlink_file(&target, dest)
                .map_err(|e| format!("failed to create file symlink: {e}"))
        }
    }
}

/// Recursively copy `src` into `dest`, recreating symlinks (not dereferencing
/// them) rather than copying their target's contents.
fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| format!("failed to stat source: {e}"))?;

    if meta.is_symlink() {
        let target = std::fs::read_link(src).map_err(|e| format!("failed to read symlink: {e}"))?;
        #[cfg(unix)]
        return std::os::unix::fs::symlink(&target, dest)
            .map_err(|e| format!("failed to recreate symlink: {e}"));
        #[cfg(windows)]
        {
            let resolved = src.parent().unwrap_or(src).join(&target);
            let is_dir = std::fs::metadata(&resolved)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            return if is_dir {
                std::os::windows::fs::symlink_dir(&target, dest)
            } else {
                std::os::windows::fs::symlink_file(&target, dest)
            }
            .map_err(|e| format!("failed to recreate symlink: {e}"));
        }
    }

    if meta.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| format!("failed to create dir: {e}"))?;
        for entry in std::fs::read_dir(src).map_err(|e| format!("failed to read dir: {e}"))? {
            let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        return Ok(());
    }

    std::fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| format!("failed to copy file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo(dir: &Path) {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-q", "-m", "init"])
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .status()
            .unwrap();
    }

    #[test]
    fn enumerate_git_paths_separates_untracked_and_ignored() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(dir.path().join("untracked.txt"), "u").unwrap();
        std::fs::write(dir.path().join("ignored.txt"), "i").unwrap();

        let untracked_only = enumerate_git_paths(dir.path(), true, false);
        assert!(untracked_only.contains(&"untracked.txt".to_string()));
        assert!(!untracked_only.contains(&"ignored.txt".to_string()));

        let ignored_only = enumerate_git_paths(dir.path(), false, true);
        assert!(!ignored_only.contains(&"untracked.txt".to_string()));
        assert!(ignored_only.contains(&"ignored.txt".to_string()));

        let both = enumerate_git_paths(dir.path(), true, true);
        assert!(both.contains(&"untracked.txt".to_string()));
        assert!(both.contains(&"ignored.txt".to_string()));

        let neither = enumerate_git_paths(dir.path(), false, false);
        assert!(neither.is_empty());
    }

    #[test]
    fn enumerate_git_paths_does_not_c_quote_non_ascii_filenames() {
        // Without `-z`, git C-quotes a filename like this under the default
        // core.quotepath=true — returning the literal 12-character string
        // `"caf\303\251.txt"` (quote marks and octal escapes included)
        // instead of the real 8-character filename. `-z` must be present.
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("café.txt"), "u").unwrap();

        let untracked = enumerate_git_paths(dir.path(), true, false);
        assert_eq!(untracked, vec!["café.txt".to_string()]);
    }

    #[test]
    fn enumerate_git_paths_returns_empty_on_non_repo() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("stray.txt"), "x").unwrap();
        assert!(enumerate_git_paths(dir.path(), true, true).is_empty());
    }

    #[test]
    fn build_sync_specs_dedupes_and_lets_explicit_override_mode() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join(".gitignore"), "shared.txt\n").unwrap();
        std::fs::write(dir.path().join("shared.txt"), "x").unwrap();

        let explicit = vec![SyncPathSpec {
            relative_path: "shared.txt".to_string(),
            mode: CopyPathMode::Symlink,
        }];
        let specs = build_sync_specs(dir.path(), true, false, &explicit);

        let matching: Vec<&SyncPathSpec> = specs
            .iter()
            .filter(|s| s.relative_path == "shared.txt")
            .collect();
        assert_eq!(matching.len(), 1, "the path must appear exactly once");
        assert_eq!(
            matching[0].mode,
            CopyPathMode::Symlink,
            "explicit entry's mode wins"
        );
    }

    #[test]
    fn build_sync_specs_includes_explicit_paths_not_seen_by_git() {
        let dir = TempDir::new().unwrap();
        init_repo(dir.path());
        let explicit = vec![SyncPathSpec {
            relative_path: "only-explicit.txt".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let specs = build_sync_specs(dir.path(), false, false, &explicit);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].relative_path, "only-explicit.txt");
    }

    #[test]
    fn sync_paths_copies_a_file() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(src.path().join(".env"), "SECRET=1").unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: ".env".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let mut ticks = Vec::new();
        let summary = sync_paths(src.path(), dest.path(), &specs, |c, t| ticks.push((c, t)));

        assert_eq!(summary.copied, 1);
        assert_eq!(summary.total, 1);
        assert!(summary.errors.is_empty());
        assert_eq!(ticks, vec![(1, 1)]);
        assert_eq!(
            std::fs::read_to_string(dest.path().join(".env")).unwrap(),
            "SECRET=1"
        );
    }

    #[test]
    fn sync_paths_copies_a_directory_recursively() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("node_modules/pkg")).unwrap();
        std::fs::write(src.path().join("node_modules/pkg/index.js"), "1").unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "node_modules".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        assert_eq!(
            std::fs::read_to_string(dest.path().join("node_modules/pkg/index.js")).unwrap(),
            "1"
        );
    }

    #[test]
    fn sync_paths_symlinks_a_directory_instead_of_copying() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("node_modules")).unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "node_modules".to_string(),
            mode: CopyPathMode::Symlink,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        let link = dest.path().join("node_modules");
        let meta = std::fs::symlink_metadata(&link).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            src.path().join("node_modules").canonicalize().unwrap()
        );
    }

    #[test]
    fn sync_paths_recreates_a_nested_symlink_when_copying() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(src.path().join("real.txt"), "hello").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("real.txt", src.path().join("link.txt")).unwrap();

        #[cfg(unix)]
        {
            let specs = vec![SyncPathSpec {
                relative_path: "link.txt".to_string(),
                mode: CopyPathMode::Copy,
            }];
            let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});
            assert_eq!(summary.copied, 1);
            let copied_link = dest.path().join("link.txt");
            assert!(
                std::fs::symlink_metadata(&copied_link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(
                std::fs::read_link(&copied_link).unwrap(),
                PathBuf::from("real.txt")
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn sync_paths_recreates_a_symlink_whose_target_is_outside_the_source_repo() {
        // Documents current, intentional behavior: `copy_recursive` recreates a
        // symlink verbatim (via read_link) rather than dereferencing it, so a
        // pre-existing symlink that already points outside the repo (which the
        // user, as the trust boundary, could already read/write directly) is
        // copied as the SAME symlink, not resolved/inlined into a real file.
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "outside").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            src.path().join("link.txt"),
        )
        .unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "link.txt".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        let copied_link = dest.path().join("link.txt");
        assert!(
            std::fs::symlink_metadata(&copied_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_link(&copied_link).unwrap(),
            outside.path().join("secret.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_paths_copies_a_directory_containing_a_cyclic_symlink_without_recursing_into_it() {
        // A directory symlink is recreated as a symlink, never traversed into
        // (`copy_recursive` checks `symlink_metadata` before deciding to
        // recurse) — so a cycle (a symlink inside a directory pointing back at
        // an ancestor of that same directory) cannot cause unbounded recursion.
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("cyclic")).unwrap();
        std::os::unix::fs::symlink(
            src.path().join("cyclic"),
            src.path().join("cyclic/self_loop"),
        )
        .unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "cyclic".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        assert_eq!(summary.errors, Vec::<String>::new());
        let loop_link = dest.path().join("cyclic/self_loop");
        assert!(
            std::fs::symlink_metadata(&loop_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_paths_creates_parent_directories_before_creating_a_nested_symlink() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("config")).unwrap();
        std::fs::write(src.path().join("config/local.env"), "X=1").unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "config/local.env".to_string(),
            mode: CopyPathMode::Symlink,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        let link = dest.path().join("config/local.env");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn sync_paths_refuses_to_follow_an_intermediate_symlink_planted_in_dest() {
        // `dest` is a `git worktree add` checkout of whatever branch the
        // caller asked for, which can be an attacker-influenced PR head_ref.
        // A malicious branch that commits a directory symlink named "config"
        // (pointing at some other writable location) must not redirect this
        // repo's own trusted `config/local/secrets.json` content out there —
        // this is the concrete escape a real intermediate-symlink check
        // defends against, not just the final-component check.
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let escape_target = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("config")).unwrap();
        std::fs::write(src.path().join("config/secrets.json"), "trusted-content").unwrap();
        // Simulate `git worktree add` having already checked out a branch
        // that committed `config` as a symlink to somewhere else entirely.
        std::os::unix::fs::symlink(escape_target.path(), dest.path().join("config")).unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "config/secrets.json".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("symlink"));
        assert!(
            !escape_target.path().join("secrets.json").exists(),
            "must not have written through the planted symlink to the escape target"
        );
        // The symlink itself must be left exactly as the malicious branch
        // committed it — not deleted, not replaced.
        assert!(
            std::fs::symlink_metadata(dest.path().join("config"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn sync_paths_records_error_for_missing_source_without_failing_the_rest() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(src.path().join("present.txt"), "x").unwrap();

        let specs = vec![
            SyncPathSpec {
                relative_path: "missing.txt".to_string(),
                mode: CopyPathMode::Copy,
            },
            SyncPathSpec {
                relative_path: "present.txt".to_string(),
                mode: CopyPathMode::Copy,
            },
        ];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].starts_with("missing.txt:"));
        assert!(dest.path().join("present.txt").exists());
    }

    #[test]
    fn sync_paths_never_overwrites_an_existing_destination() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(src.path().join("README.md"), "from source").unwrap();
        // Simulate a file `git worktree add` already checked out.
        std::fs::write(dest.path().join("README.md"), "already checked out").unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "README.md".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("already exists"));
        assert_eq!(
            std::fs::read_to_string(dest.path().join("README.md")).unwrap(),
            "already checked out",
            "must not clobber content the new worktree already has"
        );
    }

    #[test]
    fn sync_paths_rejects_parent_dir_traversal() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let specs = vec![SyncPathSpec {
            relative_path: "../escape.txt".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});
        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("escapes"));
    }

    #[test]
    fn sync_paths_rejects_absolute_path() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let specs = vec![SyncPathSpec {
            relative_path: "/etc/passwd".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});
        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("escapes"));
    }

    #[test]
    fn sync_paths_rejects_the_repository_root() {
        // A `copy_paths` entry of "." (or a value that normalizes to it) must
        // never trigger a full recursive copy of the source repo.
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        let specs = vec![SyncPathSpec {
            relative_path: ".".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});
        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("escapes"));
    }

    #[test]
    fn sync_paths_rejects_dot_git() {
        // A `copy_paths` entry of ".git" (or a path that walks through it)
        // must never overwrite the new worktree's own linked-worktree `.git`
        // file with a copy of the source repo's real `.git` directory.
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join(".git")).unwrap();
        std::fs::write(src.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let specs = vec![
            SyncPathSpec {
                relative_path: ".git".to_string(),
                mode: CopyPathMode::Copy,
            },
            SyncPathSpec {
                relative_path: ".git/HEAD".to_string(),
                mode: CopyPathMode::Copy,
            },
        ];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});
        assert_eq!(summary.copied, 0);
        assert_eq!(summary.errors.len(), 2);
        assert!(summary.errors.iter().all(|e| e.contains(".git")));
        assert!(!dest.path().join(".git").exists());
    }

    #[test]
    fn sync_paths_creates_parent_directories_for_a_nested_file() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::create_dir_all(src.path().join("config/local")).unwrap();
        std::fs::write(src.path().join("config/local/secrets.json"), "{}").unwrap();

        let specs = vec![SyncPathSpec {
            relative_path: "config/local/secrets.json".to_string(),
            mode: CopyPathMode::Copy,
        }];
        let summary = sync_paths(src.path(), dest.path(), &specs, |_, _| {});

        assert_eq!(summary.copied, 1);
        assert_eq!(
            std::fs::read_to_string(dest.path().join("config/local/secrets.json")).unwrap(),
            "{}"
        );
    }

    #[test]
    fn specs_from_copy_path_entries_maps_fields() {
        let entries = vec![CopyPathEntry {
            path: ".env".to_string(),
            mode: CopyPathMode::Symlink,
        }];
        let specs = specs_from_copy_path_entries(&entries);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].relative_path, ".env");
        assert_eq!(specs[0].mode, CopyPathMode::Symlink);
    }
}
