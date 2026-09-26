//! Git domain: subprocess and gix reads, branch and working-tree operations,
//! linked-worktree lifecycle and copy-on-write warming. No Tauri, no `AppState`:
//! the app crate owns the command wrappers, the git caches and event emission.

pub mod cow;
pub mod git;
pub mod git_cli;
pub mod git_graph;
pub mod git_locks;
pub mod git_reads;
pub mod worktree;

#[cfg(any(test, feature = "test-support"))]
pub mod test_fixtures;
