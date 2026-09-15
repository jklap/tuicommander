//! `TUIC_*` environment context injected into worktree automation scripts
//! (Setup Script, Archive Script, Run Script) and Smart Prompt shell/headless
//! children.
//!
//! # Why derive from `cwd` rather than accept caller-supplied context
//!
//! [`ScriptContext::derive`] is a pure function of a filesystem path — it
//! never accepts repo/branch/worktree facts from a caller. This is
//! deliberate, not an oversight:
//!
//! 1. The Run Script is typed into a PTY whose spawn config
//!    (`state.rs::PtyConfig`) carries only `cwd`/`shell`/`env`/`tuic_session`
//!    — no branch, no base repo. Deriving from `cwd` is the only way that
//!    surface can ever get this context at all; once it exists, Setup and
//!    Archive get it for free and all three kinds agree by construction.
//! 2. `run_setup_script`'s HTTP counterpart (`POST /worktrees/run-script`) is
//!    remote shell execution. Caller-supplied context would let a remote
//!    client set e.g. `TUIC_MAIN_REPO_PATH=/anywhere`; deriving from the
//!    already-validated `cwd` closes that off.
//! 3. It keeps `run_setup_script`/`run_script_in_dir`'s signatures — and
//!    therefore the IPC/HTTP request shape — unchanged.
//!
//! # Unknown values: unset, never empty
//!
//! [`ScriptContext::pairs`] only ever pushes a `(name, value)` pair for a
//! fact that actually resolved — a detached-HEAD `branch`, an unconfigured
//! `base_ref`, etc. are simply absent from the child's environment rather
//! than present as an empty string. `set -u` and `${X?msg}` distinguish
//! unset from empty; an empty `TUIC_BRANCH` would silently produce
//! `git checkout ""` in a careless script, while an absent one fails loudly
//! on the right line.
//!
//! # These values are not secrets
//!
//! Every `TUIC_*` value here is a path or a branch name the script could
//! already learn by running `git` in its own cwd — this does not widen the
//! exfiltration surface `smart_prompt::ENV_ALLOWLIST` protects against
//! (`ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, etc.). A future security pass
//! should not "fix" this by stripping `TUIC_*` from the Smart Prompt path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which script surface a [`ScriptContext`] was derived for. Exposed to the
/// script itself as `TUIC_SCRIPT_KIND` so one script body can serve more than
/// one surface (e.g. a Setup Script reused as a Run Script).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptKind {
    Setup,
    Archive,
    Run,
    Prompt,
}

impl ScriptKind {
    fn as_str(self) -> &'static str {
        match self {
            ScriptKind::Setup => "setup",
            ScriptKind::Archive => "archive",
            ScriptKind::Run => "run",
            ScriptKind::Prompt => "prompt",
        }
    }
}

/// Derived `TUIC_*` context for a single script invocation. Build with
/// [`ScriptContext::derive`], then apply with [`ScriptContext::apply_std`],
/// [`ScriptContext::apply_pty`], or [`ScriptContext::as_map`] depending on
/// the target command type.
pub(crate) struct ScriptContext {
    kind: ScriptKind,
    /// Whether `cwd` is inside a git repo at all. When false, only the
    /// universal (non-repo-shaped) vars are emitted — this is the PTY
    /// degradation path for a plain, non-repo terminal.
    is_repo: bool,
    worktree_path: PathBuf,
    main_repo_path: PathBuf,
    branch: Option<String>,
    base_ref: Option<String>,
    base_branch: Option<String>,
}

impl ScriptContext {
    /// Derive context from `cwd` — the directory the script will actually
    /// run in (a worktree path, the main checkout, a subdirectory of either,
    /// or a plain non-repo dir). A subdirectory cwd (e.g. a Finder-service
    /// tab opened inside a worktree subfolder) is walked up to its owning
    /// worktree/repo root via `git::find_repo_root` first — `TUIC_*`
    /// describes that root, not the subdirectory, matching what `git
    /// rev-parse --show-toplevel` would report from the same cwd.
    pub(crate) fn derive(kind: ScriptKind, cwd: &Path) -> Self {
        let Some(repo_root) = crate::git::find_repo_root(cwd) else {
            let path = cwd.to_path_buf();
            return Self {
                kind,
                is_repo: false,
                worktree_path: path.clone(),
                main_repo_path: path,
                branch: None,
                base_ref: None,
                base_branch: None,
            };
        };

        let worktree_path = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone());
        let main_repo_path = crate::git::canonical_repo_root(&repo_root);
        let main_repo_str = main_repo_path.to_string_lossy().to_string();
        let branch = crate::git::read_branch_from_head(&repo_root);
        let base_ref = branch
            .as_deref()
            .and_then(|b| crate::worktree::get_branch_base(&main_repo_str, b));
        let base_branch = crate::config::resolve_effective_base_branch(&main_repo_str);

        Self {
            kind,
            is_repo: true,
            worktree_path,
            main_repo_path,
            branch,
            base_ref,
            base_branch,
        }
    }

    /// The single source of truth for the variable list. Returns a `Vec`
    /// (not a `HashMap`) so iteration order — and therefore test output — is
    /// deterministic. Every entry here must follow the `TUIC_<UPPERCASE>`
    /// name convention: `contextVariablesParity.test.ts` (frontend) checks
    /// this mechanically against the Smart Prompts variable registry.
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![
            ("TUIC_SCRIPT_KIND", self.kind.as_str().to_string()),
            ("TUIC_APP_VERSION", env!("CARGO_PKG_VERSION").to_string()),
            (
                "TUIC_CONFIG_DIR",
                crate::config::config_dir().to_string_lossy().to_string(),
            ),
        ];

        if !self.is_repo {
            return out;
        }

        out.push((
            "TUIC_WORKTREE_PATH",
            self.worktree_path.to_string_lossy().to_string(),
        ));
        if let Some(name) = self.worktree_path.file_name().and_then(|n| n.to_str()) {
            out.push(("TUIC_WORKTREE_NAME", name.to_string()));
        }
        if let Some(parent) = self.worktree_path.parent() {
            out.push(("TUIC_WORKTREES_DIR", parent.to_string_lossy().to_string()));
        }
        out.push((
            "TUIC_MAIN_REPO_PATH",
            self.main_repo_path.to_string_lossy().to_string(),
        ));
        if let Some(name) = self.main_repo_path.file_name().and_then(|n| n.to_str()) {
            out.push(("TUIC_REPO_NAME", name.to_string()));
        }
        out.push((
            "TUIC_IS_WORKTREE",
            if self.worktree_path != self.main_repo_path {
                "true"
            } else {
                "false"
            }
            .to_string(),
        ));
        if let Some(branch) = &self.branch {
            out.push(("TUIC_BRANCH", branch.clone()));
        }
        if let Some(base_ref) = &self.base_ref {
            out.push(("TUIC_BASE_REF", base_ref.clone()));
        }
        if let Some(base_branch) = &self.base_branch {
            out.push(("TUIC_BASE_BRANCH", base_branch.clone()));
        }

        out
    }

    /// Apply to a plain `std::process::Command` (Setup Script, Archive
    /// Script) — keeps full parent-env inheritance (no `env_clear`, these
    /// scripts are user-authored in Settings and trusted) and additionally
    /// enriches `PATH` the same way every git subprocess already gets
    /// (`cli::enriched_path`), since these scripts previously got neither.
    pub(crate) fn apply_std(&self, cmd: &mut std::process::Command) {
        for (k, v) in self.pairs() {
            cmd.env(k, v);
        }
        cmd.env("PATH", crate::cli::enriched_path());
    }

    /// Apply to a PTY spawn (Run Script, via the interactive terminal).
    pub(crate) fn apply_pty(&self, cmd: &mut portable_pty::CommandBuilder) {
        for (k, v) in self.pairs() {
            cmd.env(k, v);
        }
    }

    /// As a map, for `smart_prompt::apply_clean_env`'s `ctx` slot (headless
    /// and shell Smart Prompt children, which build env via `HashMap` on top
    /// of an allowlist rather than a plain `std::process::Command`).
    pub(crate) fn as_map(&self) -> HashMap<String, String> {
        self.pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .expect("git command");
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "Test"]);
        fs::write(dir.path().join("README.md"), "# Test").expect("write file");
        run(&["add", "."]);
        run(&["commit", "-m", "Initial commit"]);
        dir
    }

    #[test]
    fn non_git_cwd_emits_only_universal_vars() {
        let dir = TempDir::new().expect("temp dir");
        let ctx = ScriptContext::derive(ScriptKind::Setup, dir.path());
        let map = ctx.as_map();

        assert!(map.contains_key("TUIC_SCRIPT_KIND"));
        assert!(map.contains_key("TUIC_APP_VERSION"));
        assert!(map.contains_key("TUIC_CONFIG_DIR"));
        assert!(
            !map.contains_key("TUIC_MAIN_REPO_PATH"),
            "a non-repo cwd must not get repo-shaped vars"
        );
        assert!(!map.contains_key("TUIC_BRANCH"));
        assert!(!map.contains_key("TUIC_WORKTREE_PATH"));
    }

    #[test]
    fn derive_on_the_main_checkout_reports_itself_as_main_repo() {
        let repo = setup_test_repo();
        let ctx = ScriptContext::derive(ScriptKind::Setup, repo.path());
        let map = ctx.as_map();

        let canonical = repo.path().canonicalize().unwrap();
        assert_eq!(
            map.get("TUIC_MAIN_REPO_PATH").map(PathBuf::from),
            Some(canonical.clone())
        );
        assert_eq!(
            map.get("TUIC_WORKTREE_PATH").map(PathBuf::from),
            Some(canonical)
        );
        assert_eq!(
            map.get("TUIC_IS_WORKTREE").map(String::as_str),
            Some("false")
        );
        assert_eq!(map.get("TUIC_BRANCH").map(String::as_str), Some("main"));
    }

    #[test]
    fn derive_from_a_subdirectory_reports_the_repo_root_not_the_subdirectory() {
        // Reachable in practice via the Finder Service ("New TUICommander Tab
        // Here" on a subfolder) or any terminal `cd`'d into a subdirectory
        // before a Run Script command executes — cwd need not be the repo
        // root. `git::resolve_git_dir` only ever checks the exact path given,
        // so without walking up (`git::find_repo_root`) this would silently
        // fall into the non-repo branch and emit zero TUIC_* context vars.
        let repo = setup_test_repo();
        let sub = repo.path().join("src").join("components");
        std::fs::create_dir_all(&sub).expect("mkdir nested subdirectory");

        let ctx = ScriptContext::derive(ScriptKind::Setup, &sub);
        let map = ctx.as_map();

        let canonical_root = repo.path().canonicalize().unwrap();
        assert_eq!(
            map.get("TUIC_WORKTREE_PATH").map(PathBuf::from),
            Some(canonical_root.clone()),
            "TUIC_WORKTREE_PATH must be the repo root, not the subdirectory cwd"
        );
        assert_eq!(
            map.get("TUIC_MAIN_REPO_PATH").map(PathBuf::from),
            Some(canonical_root)
        );
        assert_eq!(map.get("TUIC_BRANCH").map(String::as_str), Some("main"));
    }

    #[test]
    fn derive_finds_the_main_checkout_from_a_linked_worktree() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "feature-x".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature-x".to_string()),
            create_branch: true,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let ctx = ScriptContext::derive(ScriptKind::Setup, &wt.path);
        let map = ctx.as_map();

        let main_canonical = repo.path().canonicalize().unwrap();
        assert_eq!(
            map.get("TUIC_MAIN_REPO_PATH").map(PathBuf::from),
            Some(main_canonical),
            "a linked worktree's TUIC_MAIN_REPO_PATH must point at the main checkout, not itself"
        );
        assert_eq!(
            map.get("TUIC_WORKTREE_PATH").map(PathBuf::from),
            Some(wt.path.canonicalize().unwrap())
        );
        assert_eq!(
            map.get("TUIC_WORKTREE_NAME").map(String::as_str),
            Some("feature-x")
        );
        assert_eq!(
            map.get("TUIC_IS_WORKTREE").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            map.get("TUIC_BRANCH").map(String::as_str),
            Some("feature-x")
        );
    }

    #[test]
    fn worktrees_dir_is_the_worktree_paths_parent() {
        let repo = setup_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "wt-parent-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: None,
            create_branch: false,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let ctx = ScriptContext::derive(ScriptKind::Setup, &wt.path);
        let map = ctx.as_map();

        assert_eq!(
            map.get("TUIC_WORKTREES_DIR").map(PathBuf::from),
            Some(worktrees_dir.canonicalize().unwrap())
        );
    }

    #[test]
    fn derive_omits_branch_on_detached_head() {
        let repo = setup_test_repo();
        let sha = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo.path())
            .output()
            .expect("rev-parse");
        let sha = String::from_utf8_lossy(&sha.stdout).trim().to_string();
        let out = std::process::Command::new("git")
            .args(["checkout", &sha])
            .current_dir(repo.path())
            .output()
            .expect("checkout");
        assert!(out.status.success());

        let ctx = ScriptContext::derive(ScriptKind::Setup, repo.path());
        let map = ctx.as_map();
        assert!(
            !map.contains_key("TUIC_BRANCH"),
            "detached HEAD must omit TUIC_BRANCH entirely, not emit it empty"
        );
        // Still a repo — other repo-shaped vars remain present.
        assert!(map.contains_key("TUIC_MAIN_REPO_PATH"));
    }

    #[test]
    fn derive_reads_the_persisted_base_ref() {
        let repo = setup_test_repo();
        crate::worktree::set_branch_base(&repo.path().to_string_lossy(), "main", "origin/develop")
            .expect("set branch base");

        let ctx = ScriptContext::derive(ScriptKind::Setup, repo.path());
        let map = ctx.as_map();
        assert_eq!(
            map.get("TUIC_BASE_REF").map(String::as_str),
            Some("origin/develop")
        );
    }

    #[test]
    fn derive_omits_base_ref_when_none_persisted() {
        let repo = setup_test_repo();
        let ctx = ScriptContext::derive(ScriptKind::Setup, repo.path());
        let map = ctx.as_map();
        assert!(!map.contains_key("TUIC_BASE_REF"));
    }

    #[test]
    fn script_kind_is_distinct_for_each_of_the_four_kinds() {
        let repo = setup_test_repo();
        let kinds = [
            ScriptKind::Setup,
            ScriptKind::Archive,
            ScriptKind::Run,
            ScriptKind::Prompt,
        ];
        let mut seen = std::collections::HashSet::new();
        for kind in kinds {
            let ctx = ScriptContext::derive(kind, repo.path());
            let value = ctx.as_map()["TUIC_SCRIPT_KIND"].clone();
            assert!(
                seen.insert(value.clone()),
                "TUIC_SCRIPT_KIND value {value:?} reused across kinds"
            );
        }
    }
}
