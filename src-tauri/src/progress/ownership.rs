use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Resolve a registered project/workspace path to the project that owns its
/// Progress database. The caller must already have an authoritative registered
/// project; this function deliberately has no focused-repository or CWD fallback.
pub fn resolve_owning_project(registered_project: Option<&str>) -> Result<PathBuf, String> {
    let registered = registered_project
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "project_required: register an authoritative project before reporting progress"
                .to_string()
        })?;
    resolve_owning_project_in(registered, &crate::config::load_repositories())
}

pub(crate) fn resolve_owning_project_in(
    registered_project: &str,
    repositories: &serde_json::Value,
) -> Result<PathBuf, String> {
    let start = canonical_existing_dir(Path::new(registered_project))?;
    let Some(repos) = repositories
        .get("repos")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(start);
    };

    let mut workspace_parents = HashMap::<PathBuf, PathBuf>::new();
    for (repo_path, repo) in repos {
        let repo_root = canonical_if_present(Path::new(repo_path));
        let Some(workspaces) = repo
            .get("workspaces")
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for workspace in workspaces.values() {
            let Some(workspace_path) = workspace
                .get("worktreePath")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let workspace_path = canonical_if_present(Path::new(workspace_path));
            let parent = workspace
                .get("parentRepoPath")
                .and_then(serde_json::Value::as_str)
                .map(Path::new)
                .map(canonical_if_present)
                .unwrap_or_else(|| repo_root.clone());
            // The main workspace of a repository records the repository itself
            // as its worktree, so it maps the project root onto the project
            // root. That self-edge carries no ownership information, and
            // following it below would look exactly like a two-node cycle —
            // which made every registered project report
            // `project_unavailable` and left the Progress panel showing
            // nothing but errors. Drop it here so the cycle check stays strict
            // for real cycles.
            if workspace_path != parent {
                workspace_parents.insert(workspace_path, parent);
            }
        }
    }

    let mut current = start;
    let mut visited = HashSet::new();
    while let Some(parent) = workspace_parents.get(&current) {
        if !visited.insert(current.clone()) {
            return Err(format!(
                "project_unavailable: managed workspace ownership cycle at '{}'",
                current.display()
            ));
        }
        current = parent.clone();
    }

    canonical_existing_dir(&current)
}

fn canonical_existing_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "project_unavailable: cannot access project root '{}': {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "project_unavailable: project root '{}' is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn canonical_if_present(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unbound_callers_fail_explicitly() {
        assert!(
            resolve_owning_project(None)
                .unwrap_err()
                .starts_with("project_required:")
        );
    }

    #[test]
    fn managed_and_nested_workspaces_resolve_to_the_primary_project() {
        let root = tempfile::tempdir().unwrap();
        let nested = tempfile::tempdir().unwrap();
        let leaf = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "nested": {
                            "worktreePath": nested.path(),
                            "parentRepoPath": root.path()
                        }
                    }
                },
                nested.path().to_string_lossy(): {
                    "workspaces": {
                        "leaf": {
                            "worktreePath": leaf.path(),
                            "parentRepoPath": nested.path()
                        }
                    }
                }
            }
        });

        // Ownership canonicalizes, so the owner is the resolved path — on macOS
        // `/var/…` is a symlink to `/private/var/…`.
        let canonical_root = root.path().canonicalize().unwrap();
        let owner = resolve_owning_project_in(&leaf.path().to_string_lossy(), &doc).unwrap();
        assert_eq!(owner, canonical_root);
    }

    #[test]
    fn a_main_workspace_owns_itself() {
        // Every registered repository has a main workspace whose `worktreePath`
        // is the repository root and whose `parentRepoPath` is absent, so the
        // parent falls back to that same root.
        let root = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "master": { "worktreePath": root.path(), "isMain": true }
                    }
                }
            }
        });

        assert_eq!(
            resolve_owning_project_in(&root.path().to_string_lossy(), &doc).unwrap(),
            root.path().canonicalize().unwrap()
        );
    }

    /// A COW workspace's journal belongs to the parent project, and reading it
    /// back through the workspace path finds the parent's entries — one journal,
    /// reached from either address.
    ///
    /// The database this used to guard against — a `.tuic/progress.sqlite3`
    /// copied into the workspace by the COW clone — cannot exist any more: the
    /// store writes one file in the config directory and nothing inside a
    /// repository. What is still worth proving is the resolution itself.
    #[test]
    fn a_cow_workspace_reads_and_writes_the_parent_journal() {
        use crate::progress::{NewProgressEntry, ProgressKind, ProgressStore};

        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let root = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                root.path().to_string_lossy(): {
                    "workspaces": {
                        "cow": {
                            "kind": "cow",
                            "worktreePath": workspace.path(),
                            "parentRepoPath": root.path()
                        }
                    }
                }
            }
        });

        let owner = resolve_owning_project_in(&workspace.path().to_string_lossy(), &doc).unwrap();
        assert_eq!(owner, root.path().canonicalize().unwrap());

        let store = ProgressStore::open().unwrap();
        let recorded = store
            .record(
                &owner.to_string_lossy(),
                &NewProgressEntry {
                    kind: ProgressKind::Done,
                    text: "The owning project retains its history.".to_string(),
                    step: None,
                    agent_name: None,
                },
            )
            .unwrap();
        let listed = store
            .list(&owner.to_string_lossy(), &Default::default())
            .unwrap();
        assert_eq!(listed.entries, vec![recorded]);
        assert!(
            !workspace.path().join(".tuic").exists(),
            "nothing is written inside the workspace"
        );
    }

    #[test]
    fn independent_projects_remain_separate() {
        let project = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_owning_project_in(&project.path().to_string_lossy(), &serde_json::json!({}))
                .unwrap(),
            project.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn ownership_cycles_fail_closed() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let doc = json!({
            "repos": {
                one.path().to_string_lossy(): { "workspaces": { "two": {
                    "worktreePath": two.path(), "parentRepoPath": one.path()
                }}},
                two.path().to_string_lossy(): { "workspaces": { "one": {
                    "worktreePath": one.path(), "parentRepoPath": two.path()
                }}}
            }
        });
        assert!(
            resolve_owning_project_in(&one.path().to_string_lossy(), &doc)
                .unwrap_err()
                .contains("ownership cycle")
        );
    }
}
