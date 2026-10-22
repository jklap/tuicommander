//! Checkout-scoped status for sidebar monitoring, never removal preflights.
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tuic_git::git::DiffStats;

type Status = (DiffStats, Result<usize, String>);
const SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(300);

struct Snapshot {
    signature: u64,
    generation: u64,
    at: std::time::Instant,
    value: Status,
}

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
    snapshot: Option<Snapshot>,
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
        if let Some(entry) = self.entries.get(checkout) {
            let mut state = entry.state.lock();
            state.watching = false;
            state.generation = state.generation.wrapping_add(1);
            state.snapshot = None;
        }
        if !watching {
            return;
        }
        let Some(admin) = crate::git::resolve_git_dir(checkout).and_then(|p| p.canonicalize().ok())
        else {
            return;
        };
        let common = base.join(".git");
        if common.canonicalize().ok().as_ref() != Some(&common) {
            return; // Recursive working-tree watches do not cover an external Git symlink.
        }
        if checkout != base {
            let backlink = std::fs::read_to_string(admin.join("gitdir"))
                .ok()
                .and_then(|p| PathBuf::from(p.trim()).canonicalize().ok());
            if admin.parent().and_then(Path::parent) != Some(common.as_path())
                || backlink != checkout.join(".git").canonicalize().ok()
            {
                return; // Unknown linked-admin mappings cannot authorize reuse.
            }
        }
        // Registration happens only after actual watch setup, not on every refresh.
        // Replace the mapping when a checkout was removed/recreated or moved.
        self.entries.insert(
            checkout.to_path_buf(),
            Arc::new(Entry {
                base: base.to_path_buf(),
                admin,
                state: Mutex::new(State {
                    watching,
                    branch: tuic_git::git::read_branch_from_head(checkout),
                    ..State::default()
                }),
                compute: Mutex::new(()),
            }),
        );
    }

    pub(crate) fn stop(&self, base: &Path) {
        for (_, entry) in self.entries.iter().filter(|(_, entry)| entry.base == base) {
            let mut state = entry.state.lock();
            state.watching = false;
            state.generation = state.generation.wrapping_add(1);
        }
    }

    /// Raw events must arrive here before ignore/classification filters.
    pub(crate) fn event(&self, base: &Path, event: &notify::Event) -> bool {
        if event.need_rescan() || event.paths.is_empty() {
            self.stop(base);
            return false;
        }
        if crate::repo_watcher::is_ignorable_access(&event.kind) {
            return true;
        }
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.base == base)
            .collect();
        let common = base.join(".git");
        for path in &event.paths {
            // Replacement and unmapped paths cannot retain watch authority. A
            // new registration/restart must establish the mapping again.
            let lost = entries.iter().any(|(root, entry)| {
                path == root.as_path()
                    || path == &entry.admin
                    || path == &root.join(".git")
                    || path == &entry.admin.join("gitdir")
            }) || path == &common
                || (!path.starts_with(&common)
                    && !entries
                        .iter()
                        .any(|(root, _)| path.starts_with(root.as_path())));
            // New admin directories belong to inventory growth, not watch loss.
            // Their checkout has no snapshot until sync arms its recursive watch.
            if lost {
                self.stop(base);
                return false;
            }
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
            }
        }
        true
    }

    pub(crate) fn invalidate_checkout(&self, checkout: &Path) {
        if let Some(entry) = self.entries.get(checkout) {
            let mut state = entry.state.lock();
            state.generation = state.generation.wrapping_add(1);
            state.snapshot = None;
        }
    }

    pub(crate) fn clear(&self) {
        for (checkout, _) in self.entries.iter() {
            self.invalidate_checkout(&checkout);
        }
    }

    pub(crate) fn read(&self, checkout: &Path) -> Status {
        self.read_at(checkout, std::time::Instant::now())
    }

    fn read_at(&self, checkout: &Path, now: std::time::Instant) -> Status {
        self.read_with(checkout, now, || {})
    }

    fn read_with(
        &self,
        checkout: &Path,
        now: std::time::Instant,
        after_load: impl FnOnce(),
    ) -> Status {
        let Some(entry) = self.entries.get(checkout) else {
            return tuic_git::git_reads::git_reads().monitoring_status(checkout);
        };
        let _compute = entry.compute.lock();
        let (watching, generation) = {
            let state = entry.state.lock();
            (state.watching, state.generation)
        };
        let signature = watching
            .then(|| tuic_git::git_reads::monitoring_status_signature(checkout))
            .flatten();
        if let Some(signature) = signature {
            let state = entry.state.lock();
            if state.watching
                && state.generation == generation
                && let Some(snapshot) = &state.snapshot
                && snapshot.signature == signature
                && snapshot.generation == generation
                && now.saturating_duration_since(snapshot.at) < SNAPSHOT_TTL
            {
                return snapshot.value.clone();
            }
        }
        #[cfg(test)]
        {
            entry.state.lock().computations += 1;
        }
        let (diff, dirty, complete) =
            tuic_git::git_reads::git_reads().monitoring_status_with_cacheability(checkout);
        let value = (diff, dirty);
        after_load();
        let branch = tuic_git::git::read_branch_from_head(checkout);
        let after =
            signature.and_then(|_| tuic_git::git_reads::monitoring_status_signature(checkout));
        let mut state = entry.state.lock();
        state.branch = branch;
        if complete
            && value.1.is_ok()
            && state.watching
            && state.generation == generation
            && let Some(signature) = signature
            && after == Some(signature)
        {
            state.snapshot = Some(Snapshot {
                signature,
                generation,
                at: now,
                value: value.clone(),
            });
        } else {
            state.snapshot = None;
        }
        value
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
        std::fs::create_dir(repo.join("target")).unwrap();
        std::fs::write(repo.join("target/tracked.txt"), "one\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        run_git(&repo, &["add", ".gitignore"]);
        run_git(&repo, &["add", "-f", "target/tracked.txt"]);
        run_git(&repo, &["commit", "-m", "tracked excluded watcher fixture"]);
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
            if !crate::repo_watcher::is_ignorable_access(&event.kind)
                && event.paths.iter().any(|p| p == &marker)
            {
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
            if !crate::repo_watcher::is_ignorable_access(&event.kind)
                && event
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
        let tracked = sibling.join("target/tracked.txt");
        std::fs::write(&tracked, "one\ntwo\n").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            if !crate::repo_watcher::is_ignorable_access(&event.kind)
                && event.paths.contains(&tracked)
            {
                break;
            }
        }
        assert_cli_status(&cache, &sibling);
        assert_eq!(computations(&cache, &sibling), after + 1);
        run_git(&sibling, &["add", "-f", "target/tracked.txt"]);
        let index = crate::git::resolve_git_dir(&sibling).unwrap().join("index");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let event = rx
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            if !crate::repo_watcher::is_ignorable_access(&event.kind)
                && event.paths.contains(&index)
            {
                break;
            }
        }
        assert_cli_status(&cache, &sibling);
        run_git(&sibling, &["reset", "--hard", "HEAD"]);
        std::fs::write(sibling.join("late-preflight.txt"), "late").unwrap();
        assert!(
            tuic_git::worktree::inspect_workspace_lifecycle(&repo, "status-sibling")
                .dirty_files
                .unwrap()
                > 0
        );
        std::fs::remove_file(sibling.join("late-preflight.txt")).unwrap();
        run_git(&repo, &["worktree", "remove", sibling.to_str().unwrap()]);
        assert_eq!(
            after - before,
            0,
            "unchanged sibling repeated a status computation"
        );
    }
    fn changed(cache: &StatusCache, repo: &Path, path: &Path) {
        cache.event(
            repo,
            &notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
                .add_path(path.to_path_buf()),
        );
    }

    fn computations(cache: &StatusCache, repo: &Path) -> usize {
        cache.entries.get(repo).unwrap().state.lock().computations
    }

    fn assert_cli_status(cache: &StatusCache, repo: &Path) {
        use tuic_git::git_reads::{CliGitReads, GitReads};
        let (diff, dirty) = cache.read(repo);
        assert_eq!(
            serde_json::to_value(diff).unwrap(),
            serde_json::to_value(CliGitReads.diff_stats(repo, None)).unwrap()
        );
        let out = tuic_git::git_cli::git_cmd(repo)
            .args([
                "--no-optional-locks",
                "status",
                "--porcelain",
                "--untracked-files=all",
            ])
            .run()
            .unwrap();
        assert_eq!(dirty.unwrap(), out.stdout.lines().count());
    }

    /// Catches stale snapshots after tracked/ignored, rename, untracked and index writes.
    #[test]
    fn monitoring_status_raw_events_cover_status_inputs_before_ignore_filtering() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        std::fs::create_dir(repo.join("target")).unwrap();
        std::fs::write(repo.join("target/tracked.txt"), "one\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        run_git(&repo, &["add", ".gitignore"]);
        run_git(&repo, &["add", "-f", "target/tracked.txt"]);
        run_git(&repo, &["commit", "-m", "tracked excluded fixture"]);
        let cache = StatusCache::default();
        cache.set_watching(&repo, &repo, true);
        assert_cli_status(&cache, &repo);
        assert_cli_status(&cache, &repo);
        assert_eq!(computations(&cache, &repo), 1);
        let tracked = repo.join("target/tracked.txt");
        std::fs::write(&tracked, "one\ntwo\n").unwrap();
        changed(&cache, &repo, &tracked);
        assert_cli_status(&cache, &repo);
        assert_eq!(computations(&cache, &repo), 2);
        run_git(&repo, &["add", "-f", "target/tracked.txt"]);
        changed(&cache, &repo, &repo.join(".git/index"));
        assert_cli_status(&cache, &repo);
        std::fs::rename(&tracked, repo.join("target/renamed.txt")).unwrap();
        cache.event(
            &repo,
            &notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )))
            .add_path(tracked)
            .add_path(repo.join("target/renamed.txt")),
        );
        assert_cli_status(&cache, &repo);
        std::fs::write(repo.join("untracked.txt"), "new\n").unwrap();
        changed(&cache, &repo, &repo.join("untracked.txt"));
        assert_cli_status(&cache, &repo);
        assert_eq!(computations(&cache, &repo), 5);
    }

    /// Catches cache reuse after lost watching, replacement, explicit writes or TTL expiry.
    #[test]
    fn monitoring_status_watch_loss_and_expiry_force_actual_fresh_scans() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        let cache = StatusCache::default();
        cache.set_watching(&repo, &repo, true);
        let now = std::time::Instant::now();
        assert_eq!(cache.read_at(&repo, now).1.unwrap(), 0);
        cache
            .read_at(
                &repo,
                now + SNAPSHOT_TTL - std::time::Duration::from_nanos(1),
            )
            .1
            .unwrap();
        assert_eq!(computations(&cache, &repo), 1);
        cache.read_at(&repo, now + SNAPSHOT_TTL).1.unwrap();
        assert_eq!(computations(&cache, &repo), 2);
        cache.invalidate_checkout(&repo);
        cache.read(&repo).1.unwrap();
        assert_eq!(computations(&cache, &repo), 3);
        // Recorded notify inotify overflow shape: Other + Rescan, no paths.
        let event =
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
        assert!(!cache.event(&repo, &event));
        std::fs::write(repo.join("lost.txt"), "lost").unwrap();
        assert_cli_status(&cache, &repo);
        cache.read(&repo).1.unwrap();
        assert_eq!(computations(&cache, &repo), 5);
        cache.set_watching(&repo, &repo, true);
        cache.read(&repo).1.unwrap();
        // Replace the checkout with a clean clone at the same pathname.
        let old = tempfile::tempdir().unwrap();
        let original = old.path().join("original");
        std::fs::rename(&repo, &original).unwrap();
        run_git(
            old.path(),
            &["clone", original.to_str().unwrap(), repo.to_str().unwrap()],
        );
        // Directory replacement retires the watcher, not just one cached value.
        changed(&cache, &repo, &repo);
        assert_eq!(cache.read(&repo).1.unwrap(), 0);
        cache.read(&repo).1.unwrap();
        assert_eq!(computations(&cache, &repo), 3);
        cache.set_watching(&repo, &repo, true);
        cache.read(&repo).1.unwrap();
        cache.stop(&repo);
        cache.read(&repo).1.unwrap();
        cache.read(&repo).1.unwrap();
        assert_eq!(computations(&cache, &repo), 3);
    }

    /// Catches publishing a status generation that an event superseded during computation.
    #[test]
    fn monitoring_status_does_not_publish_superseded_generation() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        let cache = StatusCache::default();
        cache.set_watching(&repo, &repo, true);
        cache
            .read_with(&repo, std::time::Instant::now(), || {
                let path = repo.join("during-load.txt");
                std::fs::write(&path, "arrived").unwrap();
                changed(&cache, &repo, &path);
            })
            .1
            .unwrap();
        assert_eq!(cache.read(&repo).1.unwrap(), 1);
        cache.read(&repo).1.unwrap();
        assert_eq!(computations(&cache, &repo), 2);
    }

    /// Catches out-of-tree ignore changes bypassing watcher-only invalidation.
    #[test]
    fn monitoring_status_revalidates_global_status_sources() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        let sources = tempfile::tempdir().unwrap();
        let ignore = sources.path().join("ignore");
        std::fs::write(&ignore, "hidden.txt\n").unwrap();
        run_git(
            &repo,
            &["config", "core.excludesFile", ignore.to_str().unwrap()],
        );
        std::fs::write(repo.join("hidden.txt"), "hidden").unwrap();
        let cache = StatusCache::default();
        cache.set_watching(&repo, &repo, true);
        assert_eq!(cache.read(&repo).1.unwrap(), 0);
        std::fs::write(&ignore, "different-long-name.txt\n").unwrap();
        assert_eq!(cache.read(&repo).1.unwrap(), 1);
        assert_cli_status(&cache, &repo);
        assert_eq!(computations(&cache, &repo), 2);
        run_git(&repo, &["config", "core.sparseCheckout", "true"]);
        cache.read(&repo).1.unwrap();
        cache.read(&repo).1.unwrap();
        assert_eq!(
            computations(&cache, &repo),
            4,
            "unsupported status was cached"
        );
    }
    /// Catches parallel refreshes repeating the same checkout scan.
    #[test]
    fn monitoring_status_single_flight_shares_one_actual_scan() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        let cache = StatusCache::default();
        cache.set_watching(&repo, &repo, true);
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    assert_eq!(cache.read(&repo).1.unwrap(), 0);
                });
            }
        });
        assert_eq!(computations(&cache, &repo), 1);
    }
    /// Catches accepting an admin directory whose backlink names another checkout.
    #[test]
    fn monitoring_status_rejects_unproven_linked_admin_mapping() {
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        let holder = tempfile::tempdir().unwrap();
        let sibling = holder.path().canonicalize().unwrap().join("linked");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "mapping",
                sibling.to_str().unwrap(),
            ],
        );
        let admin = crate::git::resolve_git_dir(&sibling).unwrap();
        let backlink = std::fs::read(admin.join("gitdir")).unwrap();
        std::fs::write(admin.join("gitdir"), repo.join(".git").to_str().unwrap()).unwrap();
        let cache = StatusCache::default();
        cache.set_watching(&repo, &sibling, true);
        assert_eq!(cache.read(&sibling).1.unwrap(), 0);
        std::fs::write(sibling.join("unwatched.txt"), "new").unwrap();
        assert_eq!(cache.read(&sibling).1.unwrap(), 1);
        assert!(cache.entries.get(&sibling).is_none());
        std::fs::write(admin.join("gitdir"), backlink).unwrap();
        run_git(
            &repo,
            &["worktree", "remove", "--force", sibling.to_str().unwrap()],
        );
    }
    #[test]
    fn monitoring_status_inventory_add_keeps_unchanged_sibling_reusable() {
        inventory_add_reuses_sibling(false);
    }

    /// Catches treating a new admin path as watcher loss after a bounded-cache eviction.
    #[test]
    fn monitoring_status_inventory_add_after_main_entry_eviction_keeps_sibling_reusable() {
        inventory_add_reuses_sibling(true);
    }

    fn inventory_add_reuses_sibling(evict_main: bool) {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (_guard, repo) = setup_test_repo_with_commit();
        let repo = repo.canonicalize().unwrap();
        run_git(&repo, &["branch", "-M", "main"]);
        let holder_guard = tempfile::tempdir().unwrap();
        let holder = holder_guard.path().canonicalize().unwrap();
        let sibling = holder.join("existing");
        let added = holder.join("added");
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "inventory-existing",
                sibling.to_str().unwrap(),
            ],
        );
        let cache = Arc::new(StatusCache::default());
        let complete = Arc::new(AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        let callback_cache = Arc::clone(&cache);
        let callback_complete = Arc::clone(&complete);
        let callback_repo = repo.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                let event = event.unwrap();
                // Same authority revocation as the production callback.
                if !callback_cache.event(&callback_repo, &event) {
                    callback_complete.store(false, Ordering::Release);
                }
                let _ = tx.send(event);
            })
            .unwrap();
        watcher
            .watch(&repo, notify::RecursiveMode::Recursive)
            .unwrap();
        watcher
            .watch(&sibling, notify::RecursiveMode::Recursive)
            .unwrap();
        cache.set_watching(&repo, &repo, true);
        cache.set_watching(&repo, &sibling, true);
        assert_eq!(cache.read(&sibling).1.unwrap(), 0);
        let before = computations(&cache, &sibling);
        if evict_main {
            // Exercise the same absence that max_capacity eviction permits.
            // Retiring a snapshot entry does not retire its OS watch.
            cache.entries.invalidate(&repo);
        }
        run_git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "inventory-added",
                added.to_str().unwrap(),
            ],
        );
        let new_admin = crate::git::resolve_git_dir(&added).unwrap();
        let wait_for = |path: &Path| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                let event = rx
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .unwrap_or_else(|error| {
                        panic!("no mutating event for {}: {error}", path.display())
                    });
                if !crate::repo_watcher::is_ignorable_access(&event.kind)
                    && event.paths.iter().any(|p| p == path)
                {
                    break;
                }
            }
        };
        // The existing worktrees parent is already watched. Newly created
        // children may predate inotify's recursive-watch installation, so the
        // authoritative inventory signal is creation of the admin directory.
        wait_for(&new_admin);
        // Inventory sync arms only the newly discovered root, never re-arms old ones.
        watcher
            .watch(&added, notify::RecursiveMode::Recursive)
            .unwrap();
        cache.set_watching(&repo, &added, complete.load(Ordering::Acquire));
        run_git(
            &repo,
            &["commit", "--allow-empty", "-m", "after inventory growth"],
        );
        wait_for(&repo.join(".git/refs/heads/main"));
        assert_eq!(cache.read(&sibling).1.unwrap(), 0);
        let after = computations(&cache, &sibling);
        run_git(&repo, &["worktree", "remove", added.to_str().unwrap()]);
        run_git(&repo, &["worktree", "remove", sibling.to_str().unwrap()]);
        assert_eq!(
            after - before,
            0,
            "inventory addition retired unchanged sibling watching"
        );
    }
}
