use super::*;
use crate::github_account::{GitHubAccount, GitHubHost};
use mockito::{Matcher, Server};

// Catches: bearer auth leaks to redirected blob storage; GHE uses ambient credentials.
#[tokio::test]
async fn ghe_log_redirect_strips_authorization_and_retains_only_the_utf8_tail() {
    let mut github = Server::new_async().await;
    let mut blob = Server::new_async().await;
    let target = format!("{}/blob", blob.url().replace("127.0.0.1", "localhost"));
    let redirect = github
        .mock("GET", "/repos/team/project/actions/jobs/7/logs")
        .match_header("authorization", "Bearer named-ghe-token")
        .match_header("x-github-api-version", "2022-11-28")
        .with_status(302)
        .with_header("location", &target)
        .create_async()
        .await;
    let log = format!("{}é\nlast failure\n", "prefix\n".repeat(2000));
    let storage = blob
        .mock("GET", "/blob")
        .match_header("authorization", Matcher::Missing)
        .with_body(log)
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    *state.github.token.write() = Some("ambient-must-not-be-used".into());
    let account = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
    let api = RepoApi {
        state: &state,
        account,
        token: "named-ghe-token".into(),
        owner: "team".into(),
        repo: "project".into(),
    };
    let mut request = api.text_request("actions/jobs/7/logs").build().unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://ghe.acme.com/api/v3/repos/team/project/actions/jobs/7/logs"
    );
    // Redirect only the connection target to the local API simulator; keep the
    // exact production method and headers built for the repo's named GHE account.
    *request.url_mut() = format!("{}/repos/team/project/actions/jobs/7/logs", github.url())
        .parse()
        .unwrap();
    let request = reqwest::RequestBuilder::from_parts(state.http_client.clone(), request);
    let tail = download_text_tail(&state, &api.account, request, 14)
        .await
        .unwrap();
    assert_eq!(tail, "\nlast failure\n");
    redirect.assert_async().await;
    storage.assert_async().await;
}

// Catches: log download buffers the error prefix instead of preserving bounded diagnostics.
#[tokio::test]
async fn text_download_error_keeps_status_and_sanitized_bounded_tail() {
    let mut server = Server::new_async().await;
    let response = server
        .mock("GET", "/logs")
        .with_status(404)
        .with_body(format!("{}\x1b[31mgone\x1b[0m", "prefix".repeat(2000)))
        .create_async()
        .await;
    let state = crate::state::tests_support::make_test_app_state();
    let account = GitHubAccount::ghe_pat(GitHubHost::new("ghe.acme.com").unwrap(), None);
    let request = state.http_client.get(format!("{}/logs", server.url()));
    let error = download_text_tail(&state, &account, request, 32)
        .await
        .unwrap_err();
    assert!(error.starts_with("GitHub API failed (404 Not Found): "));
    assert!(error.ends_with("gone"));
    assert!(!error.contains('\x1b'));
    assert!(error.len() < 80);
    response.assert_async().await;
}
