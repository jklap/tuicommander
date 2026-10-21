//! Checkout-scoped status for sidebar monitoring, never removal preflights.
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tuic_git::git::DiffStats;

type Status = (DiffStats, Result<usize, String>);

struct Entry {
    base: PathBuf,
    admin: PathBuf,
    state: Mutex<State>,
    compute: Mutex<()>,
}

#[derive(Default)]
struct State {
    watching: bool,
    generation: u64,
    branch: Option<String>,
    #[cfg(test)]
    computations: usize,
}

pub(crate) struct StatusCache {
    entries: moka::sync::Cache<PathBuf, Arc<Entry>>,
}

impl Default for StatusCache {
    fn default() -> Self {
        Self {
            entries: moka::sync::Cache::new(256),
        }
    }
}

pub(crate) static CACHE: LazyLock<StatusCache> = LazyLock::new(StatusCache::default);

impl StatusCache {
    pub(crate) fn set_watching(&self, base: &Path, checkout: &Path, watching: bool) {
        let Some(admin) = crate::git::resolve_git_dir(checkout) else {
            return;
        };
        let entry = self.entries.get_with(checkout.to_path_buf(), || {
            Arc::new(Entry {
                base: base.to_path_buf(),
                admin,
                state: Mutex::new(State::default()),
                compute: Mutex::new(()),
            })
        });
        let mut state = entry.state.lock();
        state.generation = state.generation.wrapping_add(1);
        state.watching = watching;
        state.branch = tuic_git::git::read_branch_from_head(checkout);
    }

    pub(crate) fn stop(&self, base: &Path) {
        for (_, entry) in self.entries.iter().filter(|(_, entry)| entry.base == base) {
            let mut state = entry.state.lock();
            state.watching = false;
            state.generation = state.generation.wrapping_add(1);
        }
    }

    /// Raw events must arrive here before ignore/classification filters.
    pub(crate) fn event(&self, base: &Path, event: &notify::Event) {
        if event.need_rescan() {
            self.stop(base);
            return;
        }
        if crate::repo_watcher::is_ignorable_access(&event.kind) {
            return;
        }
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.base == base)
            .collect();
        let common = base.join(".git");
        for path in &event.paths {
            // Nested linked worktrees own their paths before the main root does.
            let owner = entries
                .iter()
                .filter(|(root, _)| path.starts_with(root.as_path()))
                .max_by_key(|(root, _)| root.components().count())
                .map(|(root, _)| root.as_path());
            for (root, entry) in &entries {
                let mut state = entry.state.lock();
                let shared = path == &common
                    || path == &common.join("config")
                    || path.starts_with(common.join("info"))
                    || path == &common.join("packed-refs");
                let branch = state
                    .branch
                    .as_ref()
                    .is_some_and(|branch| path == &common.join("refs/heads").join(branch));
                let admin = path
                    .strip_prefix(&entry.admin)
                    .ok()
                    .is_some_and(|rel| rel.as_os_str().is_empty() || rel.components().count() == 1);
                let working = owner == Some(root.as_path())
                    && !path.starts_with(&common)
                    && !path.starts_with(&entry.admin);
                if shared || branch || admin || working {
                    state.generation = state.generation.wrapping_add(1);
                }
                if path == root.as_path() || path == &entry.admin || path == &common {
                    state.watching = false;
                }
            }
        }
    }

    pub(crate) fn read(&self, checkout: &Path) -> Status {
        if let Some(entry) = self.entries.get(checkout) {
            let _compute = entry.compute.lock();
            #[cfg(test)]
            {
                entry.state.lock().computations += 1;
            }
        }
        tuic_git::git_reads::git_reads().monitoring_status(checkout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::Watcher;
    use tuic_git::test_fixtures::setup_test_repo_with_commit;

    fn run_git(repo: &Path, args: &[&str]) {
        tuic_git::git_cli::git_cmd(repo).args(args).run().unwrap();
    }

    /// Catches unchanged linked checkouts being rescanned for a main-only commit.
    #[test]
    fn monitoring_status_snapshot_keeps_unchanged_sibling_out_of_refresh() {
        let (_guard, repo) = setup_test_repo_with_commit();
        run_git(&repo, &["branch", "-M", "main"]);
        let repo = repo.canonicalize().unwrap();
        let sibling_dir = tempfile::tempdir().unwrap();
        let sibling = sibling_dir.path().canonicalize().unwrap().join("linked");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "status-sibling",
                sibling.to_str().unwrap(),
            ],
        );
        let cache = Arc::new(StatusCache::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let callback_cache = Arc::clone(&cache);
        let callback_repo = repo.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                let event = event.unwrap();
                callback_cache.event(&callback_repo, &event);
                let _ = tx.send(event);
            })
            .unwrap();
        // Unlike Linux production's pruned watches, this small fixture is fully recursive.
        watcher
            .watch(&repo, notify::RecursiveMode::Recursive)
            .unwrap();
        watcher
            .watch(&sibling, notify::RecursiveMode::Recursive)
            .unwrap();
        cache.set_watching(&repo, &repo, true);
        cache.set_watching(&repo, &sibling, true);
        // Prove stream readiness with a delivered real write, not an arming sleep.
        let marker = repo.join("watch-ready.txt");
        std::fs::write(&marker, "ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            if event.paths.iter().any(|p| p == &marker) {
                break;
            }
        }
        let first = cache.read(&sibling);
        assert_eq!(first.1.unwrap(), 0);
        run_git(
            &repo,
            &["commit", "--allow-empty", "-m", "main-only commit"],
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            if event
                .paths
                .iter()
                .any(|p| p == &repo.join(".git/refs/heads/main"))
            {
                break;
            }
        }
        let before = cache
            .entries
            .get(&sibling)
            .unwrap()
            .state
            .lock()
            .computations;
        let second = cache.read(&sibling);
        assert_eq!(second.1.unwrap(), 0);
        let after = cache
            .entries
            .get(&sibling)
            .unwrap()
            .state
            .lock()
            .computations;
        run_git(&repo, &["worktree", "remove", sibling.to_str().unwrap()]);
        assert_eq!(
            after - before,
            0,
            "unchanged sibling repeated a status computation"
        );
    }
}
