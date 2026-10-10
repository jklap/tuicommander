use super::*;
use crate::work_items::model::WorkItemTone;

fn link(id: &str) -> WorkItemLink {
    WorkItemLink {
        reference: WorkItemRef {
            provider_id: "jira".into(),
            account_id: "acme".into(),
            id: id.into(),
        },
        key: format!("ACME-{id}"),
        title: "Fix the thing".into(),
        url: format!("https://acme.atlassian.net/browse/ACME-{id}"),
        state: Some(WorkItemState {
            id: "3".into(),
            name: "In Progress".into(),
            tone: WorkItemTone::Info,
        }),
    }
}

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
}

const REPO: &str = "/work/repo";

/// catches: an in-memory link store — a fresh store (as after a restart) must
/// read back what the previous one wrote.
#[test]
fn link_survives_restart() {
    let dir = temp_dir();
    let path = dir.path().join("workitem_links.json");
    LinkStore::at_path(path.clone())
        .link(REPO, "feat/a", link("1"))
        .unwrap();

    let reloaded = LinkStore::at_path(path).list().unwrap();
    assert_eq!(reloaded[REPO]["feat/a"], link("1"));
}

/// catches: orphan links — removing a worktree through the shared removal seam
/// must drop its link and only its link.
#[test]
fn removed_worktree_drops_link() {
    let dir = temp_dir();
    let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let store = LinkStore::new();
    store.link(REPO, "feat/a", link("1")).unwrap();
    store.link(REPO, "feat/b", link("2")).unwrap();
    store.link("/work/other", "feat/a", link("3")).unwrap();

    let state = crate::state::tests_support::make_test_app_state();
    state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
        repo_path: REPO.into(),
        workspace_id: "ws-a".into(),
        branch: "feat/a".into(),
    });

    let links = LinkStore::new().list().unwrap();
    assert!(!links[REPO].contains_key("feat/a"));
    assert_eq!(links[REPO]["feat/b"], link("2"));
    assert_eq!(links["/work/other"]["feat/a"], link("3"));
}

/// catches: a corrupt file treated as empty, so the next link erases every
/// other link on disk.
#[test]
fn corrupt_links_file_is_not_overwritten() {
    let dir = temp_dir();
    let path = dir.path().join("workitem_links.json");
    std::fs::write(&path, "{not json").unwrap();

    let result = LinkStore::at_path(path.clone()).link(REPO, "feat/a", link("1"));

    assert!(result.is_err());
    let preserved = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
    assert!(
        preserved,
        "the unreadable document must survive for recovery"
    );
}

/// catches: unlink of the last branch leaving an empty repo entry, or reporting
/// a removal that did not happen.
#[test]
fn unlink_reports_whether_a_link_existed() {
    let dir = temp_dir();
    let store = LinkStore::at_path(dir.path().join("workitem_links.json"));
    store.link(REPO, "feat/a", link("1")).unwrap();

    assert!(store.unlink(REPO, "feat/a").unwrap());
    assert!(!store.unlink(REPO, "feat/a").unwrap());
    assert!(store.list().unwrap().is_empty());
}

/// catches: a link whose URL the badge would hand to a local protocol handler.
#[test]
fn link_rejects_non_http_url() {
    let dir = temp_dir();
    let store = LinkStore::at_path(dir.path().join("workitem_links.json"));
    for url in ["javascript:alert(1)", "file:///etc/passwd", "ACME-1"] {
        let mut bad = link("1");
        bad.url = url.into();
        assert!(store.link(REPO, "feat/a", bad).is_err(), "{url} accepted");
    }
    assert!(store.link("relative/repo", "feat/a", link("1")).is_err());
    assert!(store.list().unwrap().is_empty());
}

/// catches: a token embedded in a remote URL reaching plugin JavaScript.
#[test]
fn git_remotes_strip_credentials() {
    let dir = temp_dir();
    let repo = dir.path();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(
        repo.join(".git/config"),
        "[remote \"origin\"]\n\turl = https://x-access-token:ghp_secret@github.com/acme/app.git\n\
         [remote \"fork\"]\n\turl = git@github.com:me/app.git\n",
    )
    .unwrap();

    let remotes = git_remotes(repo);

    assert_eq!(
        remotes,
        vec![
            GitRemote {
                name: "origin".into(),
                url: "https://github.com/acme/app.git".into()
            },
            GitRemote {
                name: "fork".into(),
                url: "git@github.com:me/app.git".into()
            },
        ]
    );
}
