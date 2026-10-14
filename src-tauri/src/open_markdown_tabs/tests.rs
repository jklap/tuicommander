use super::*;
use serde_json::json;

fn config() -> serde_json::Value {
    json!({"repos": {"/repo": {"workspaces": {"feature": {"worktreePath": "/worktrees/feature"}}}, "/repo/nested": {}, "/other": {}}})
}
fn tab(id: &str, repo: &str, file: &str, root: Option<&str>) -> DesktopMarkdownTab {
    DesktopMarkdownTab {
        id: id.into(),
        repo_path: repo.into(),
        file_path: file.into(),
        fs_root: root.map(str::to_owned),
    }
}

// Catches: leaking another repository or accidentally assigning a nested session to its parent.
#[test]
fn menu_scopes_titles_and_relative_paths_to_the_deepest_registered_repository() {
    let tabs = vec![
        tab("a", "/repo", "docs/guide.md", None),
        tab("b", "/other", "private.md", None),
        tab("c", "/repo/nested", "README.md", None),
    ];
    let list = select_tabs(&tabs, "/repo/src", &config()).unwrap();
    assert_eq!(
        serde_json::to_value(list).unwrap(),
        json!([{"id":"a","title":"guide.md","relative_path":"docs/guide.md","absolute_path":"/repo/docs/guide.md","fs_root":"/repo"}])
    );
    let nested = select_tabs(&tabs, "/repo/nested/src", &config()).unwrap();
    assert_eq!(
        nested.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec!["c"]
    );
    assert!(select_tabs(&tabs, "/repo-other", &config()).is_err());
}

// Catches: linked-worktree sessions missing their repository tabs or opening a different checkout's file.
#[test]
fn worktree_tabs_keep_their_own_file_root_in_the_mobile_viewer_address() {
    let tabs = vec![
        tab("main", "/repo", "README.md", None),
        tab(
            "feature",
            "/repo",
            "/worktrees/feature/docs/guide.md",
            Some("/worktrees/feature"),
        ),
    ];
    let list = select_tabs(&tabs, "/worktrees/feature/src", &config()).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[1].absolute_path, "/worktrees/feature/docs/guide.md");
    assert_eq!(list[1].relative_path, "docs/guide.md");
    assert_eq!(list[1].fs_root, "/worktrees/feature");
}

// Catches: exposing traversal, sibling-repository files, unregistered roots, or non-Markdown tabs.
#[test]
fn menu_omits_files_outside_registered_roots_and_non_markdown_files() {
    let tabs = vec![
        tab("a", "/repo", "../secret.md", None),
        tab("b", "/repo", "/repo-other/secret.md", None),
        tab("c", "/repo", "src/main.ts", None),
        tab("d", "/repo", "secret.md", Some("/unregistered")),
        tab("e", "/unregistered", "secret.md", None),
        tab("ok", "/repo", "docs/UPPER.MD", None),
    ];
    let list = select_tabs(&tabs, "/repo", &config()).unwrap();
    assert_eq!(
        list.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec!["ok"]
    );
}

// Catches: host-only absolute path checks losing Windows tabs when the backend runs on Unix.
#[test]
fn windows_file_paths_have_relative_menu_labels_and_absolute_viewer_addresses() {
    let cfg = json!({"repos": {"C:\\repo": {}}});
    let tabs = vec![tab("a", "C:\\repo", "C:\\repo\\docs\\guide.md", None)];
    let list = select_tabs(&tabs, "C:\\repo\\src", &cfg).unwrap();
    assert_eq!(list[0].relative_path, "docs/guide.md");
    assert_eq!(list[0].absolute_path, "C:/repo/docs/guide.md");
}

// Catches: closes leaving a stale snapshot, missing invalidation, or content leaking into the event.
#[test]
fn desktop_open_and_close_publish_invalidation_and_replace_the_snapshot() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut events = state.event_bus.subscribe();
    let tabs = vec![tab("a", "/repo", "guide.md", None)];
    publish(&state, tabs.clone()).unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(
        crate::mcp_http::sse_routes::event_type_name_for_test(&event),
        "open-markdown-tabs-changed"
    );
    assert_eq!(
        crate::mcp_http::sse_routes::event_payload_for_test(&event),
        json!({})
    );
    assert_eq!(
        select_tabs(&state.open_markdown_tabs.read(), "/repo", &config())
            .unwrap()
            .len(),
        1
    );
    publish(&state, tabs).unwrap();
    assert!(
        events.try_recv().is_err(),
        "unchanged publications must not emit redundant events"
    );
    publish(&state, vec![]).unwrap();
    assert!(matches!(
        events.try_recv().unwrap(),
        AppEvent::OpenMarkdownTabsChanged
    ));
    assert!(
        select_tabs(&state.open_markdown_tabs.read(), "/repo", &config())
            .unwrap()
            .is_empty()
    );
}
