//! Git repository fixtures shared by this crate's tests and by the app crate's
//! tests of the `AppState`-coupled worktree and cache paths. Compiled for this
//! crate's own tests and, elsewhere, only behind the `test-support` feature,
//! which consumers enable from `[dev-dependencies]`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::git_cli::git_cmd;
use crate::worktree::{WorktreeConfig, create_worktree_internal};

/// `git init` a fixture repo at `dir` and prove it is its own toplevel before
/// anything else runs there. A failed or odd init must stop the test here: the
/// next fixture command would otherwise run in whatever repository encloses
/// `dir` (once, the real checkout, whose branches it renamed).
///
/// # Panics
///
/// `git init` fails, or `git rev-parse --show-toplevel` in `dir` is not `dir`.
pub fn init_fixture_repo(dir: &Path) {
    // `--template=`: no hooks/*.sample copies, even under a bare `cargo test`
    // that skips the harness's `GIT_TEMPLATE_DIR` (see src-tauri/AGENTS.md).
    let out = std::process::Command::new("git")
        .args(["init", "-q", "--template="])
        .current_dir(dir)
        .output()
        .unwrap_or_else(|err| panic!("run git init for fixture repo {}: {err}", dir.display()));
    assert!(
        out.status.success(),
        "git init failed for fixture repo {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    assert_own_toplevel(dir);
}

/// Panic unless `dir` is the toplevel of its own git repository.
pub fn assert_own_toplevel(dir: &Path) {
    let want = dir
        .canonicalize()
        .unwrap_or_else(|err| panic!("canonicalize fixture dir {}: {err}", dir.display()));
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .unwrap_or_else(|err| panic!("run git rev-parse in {}: {err}", dir.display()));
    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let got = Path::new(&got).canonicalize().ok();
    assert!(
        out.status.success() && got.as_deref() == Some(want.as_path()),
        "{} is not its own git toplevel (git says {:?}: {}); refusing to run fixture git commands there",
        dir.display(),
        got,
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

pub fn setup_test_repo() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path();

    init_fixture_repo(repo_path);
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

/// Build a worktree on `branch` and return its path. Optionally commit a file
/// so the branch is genuinely ahead of the base branch.
pub fn worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
    let repo_path = repo.to_string_lossy().to_string();
    let config = WorktreeConfig {
        task_name: branch.to_string(),
        base_repo: repo_path,
        branch: Some(branch.to_string()),
        create_branch: true,
    };
    let wt =
        create_worktree_internal(&repo.join("worktrees"), &config, None).expect("create worktree");
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
pub fn base_branch_of(repo: &Path) -> String {
    git_cmd(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .run()
        .expect("rev-parse HEAD")
        .stdout
        .trim()
        .to_string()
}

/// A worktree on `branch` with an uncommitted file in it, plus the commit
/// count the branch is ahead of base. Returns the worktree path.
pub fn dirty_worktree_with(repo: &Path, branch: &str, commit: bool) -> PathBuf {
    let wt = worktree_with(repo, branch, commit);
    fs::write(wt.join("scratch.txt"), "hours of uncommitted work").expect("write scratch");
    wt
}

/// Helper: create a temp git repo with an initial commit.
pub fn setup_test_repo_with_commit() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    init_fixture_repo(&path);
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("config email");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["config", "user.name", "Test"])
        .output()
        .expect("config name");
    // Create an initial file and commit
    std::fs::write(path.join("initial.txt"), "hello").expect("write initial");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["add", "initial.txt"])
        .output()
        .expect("add");
    std::process::Command::new("git")
        .current_dir(&path)
        .args(["commit", "-m", "initial"])
        .output()
        .expect("commit");
    (dir, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every git hook file a fixture repo holds. A fixture must start with none:
    /// git's template would copy its `hooks/*.sample` files into each one.
    fn hook_files(repo: &Path) -> Vec<String> {
        match fs::read_dir(repo.join(".git/hooks")) {
            Ok(entries) => entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// A fixture dir whose `git init` fails, inside another repository: the
    /// helper must panic instead of leaving later commands to hit the outer one.
    #[test]
    #[should_panic(expected = "fixture repo")]
    fn init_fixture_repo_panics_when_git_init_fails() {
        let outer = TempDir::new().unwrap();
        init_fixture_repo(outer.path());
        let dir = outer.path().join("fixture");
        fs::create_dir_all(&dir).unwrap();
        // Not a gitfile: `git init` refuses it.
        fs::write(dir.join(".git"), "garbage\n").unwrap();
        init_fixture_repo(&dir);
    }

    #[test]
    #[should_panic(expected = "is not its own git toplevel")]
    fn assert_own_toplevel_rejects_a_dir_inside_another_repository() {
        let outer = TempDir::new().unwrap();
        init_fixture_repo(outer.path());
        let sub = outer.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        assert_own_toplevel(&sub);
    }

    #[test]
    fn shared_fixture_repos_skip_the_git_template() {
        let repo = setup_test_repo();
        assert_eq!(hook_files(repo.path()), Vec::<String>::new());
        let (_dir, path) = setup_test_repo_with_commit();
        assert_eq!(hook_files(&path), Vec::<String>::new());
    }

    /// A plain `git init`, as most test call sites run it, gets an empty
    /// template from the repo's test harness (nextest setup script or
    /// scripts/with-test-tmp.sh, both of which set `TUIC_TEST_TMP_ROOT`).
    #[test]
    fn plain_git_init_under_the_test_harness_skips_the_git_template() {
        if std::env::var_os("TUIC_TEST_TMP_ROOT").is_none() {
            eprintln!("not under the repo's test harness (bare `cargo test`): nothing to check");
            return;
        }
        let template = std::env::var_os("GIT_TEMPLATE_DIR")
            .expect("the test harness must export GIT_TEMPLATE_DIR");
        assert!(
            fs::read_dir(&template)
                .expect("template dir exists")
                .next()
                .is_none(),
            "GIT_TEMPLATE_DIR must be empty"
        );
        let dir = TempDir::new().unwrap();
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(hook_files(dir.path()), Vec::<String>::new());
    }
}
