use super::*;

// Catches: rejecting a partial GraphQL response, or dropping healthy aliases with a denied sibling.
#[tokio::test]
async fn recorded_not_found_keeps_accessible_prs_and_issues() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/github-partial-monitoring-2336.json")).unwrap();
    let body = fixture["response"].clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = axum::Router::new().route(
        "/graphql",
        axum::routing::post(move |axum::Json(_request): axum::Json<serde_json::Value>| {
            let body = body.clone();
            async move { axum::Json(body) }
        }),
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let response = graphql_request(
        &client,
        "test-only-credential",
        &format!("http://{address}/graphql"),
        fixture["request"]["query"].as_str().unwrap(),
        &serde_json::Value::Null,
    )
    .await
    .unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();

    let state = crate::state::tests_support::make_test_app_state();
    let account = github_com_account(&state);
    let repos = vec![
        ("accessible".into(), "sstraus".into(), "tuicommander".into()),
        (
            "denied".into(),
            "sstraus".into(),
            "tuic-2336-nonexistent-fixture".into(),
        ),
    ];
    let aliases = vec![
        ("r0".into(), "accessible".into()),
        ("r1".into(), "denied".into()),
    ];
    let mut prs = std::collections::HashMap::new();
    let mut issues = std::collections::HashMap::new();
    adopt_batch_response(
        &state,
        &account,
        &repos,
        &aliases,
        false,
        true,
        "",
        &response,
        &mut prs,
        &mut issues,
    )
    .await
    .unwrap();
    assert_eq!(prs["accessible"].len(), 1);
    assert_eq!(prs["accessible"][0].number, 128);
    assert_eq!(issues["accessible"].len(), 1);
    assert_eq!(issues["accessible"][0].number, 121);
    assert!(!prs.contains_key("denied"));
    assert!(!issues.contains_key("denied"));
    assert_eq!(
        state.git_cache.github_status.get("accessible").unwrap()[0].number,
        128
    );
    assert_eq!(state.git_cache.github_repo_access_errors.len(), 1);
    assert!(
        state
            .git_cache
            .github_repo_cooldown
            .contains_key(&cooldown_key(
                &account,
                "sstraus",
                "tuic-2336-nonexistent-fixture"
            ))
    );
}

// Catches: coupling routine PR/issue monitoring to optional Projects permission.
#[test]
fn batch_poll_never_selects_projects_fields() {
    let repos = vec![("local".into(), "sstraus".into(), "tuicommander".into())];
    for merged in [false, true] {
        for drafts in [false, true] {
            for filter in ["disabled", "all", "assigned", "created", "mentioned"] {
                let (query, _) =
                    build_unified_batch_query(&repos, merged, filter, "sstraus", drafts);
                assert!(
                    query.contains("pullRequests("),
                    "missing PR selection: {query}"
                );
                assert_eq!(query.contains("issues("), filter != "disabled");
                assert!(
                    !query.to_ascii_lowercase().contains("project"),
                    "Projects selection leaked into BatchPoll: {query}"
                );
            }
        }
    }
}
