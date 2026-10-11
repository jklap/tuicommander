//! Account-aware API boundary shared by review threads and PR actions.
use crate::{github_account::GitHubAccount, state::AppState};
use serde_json::Value;

pub(crate) trait PrApi: Sync {
    fn graphql(
        &self,
        query: &str,
        variables: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send;
    fn rest(
        &self,
        method: &str,
        endpoint: &str,
        body: Value,
    ) -> impl std::future::Future<Output = Result<Value, String>> + Send;
}

pub(crate) struct RepoApi<'a> {
    pub state: &'a AppState,
    pub account: GitHubAccount,
    pub token: String,
    pub owner: String,
    pub repo: String,
}

impl<'a> RepoApi<'a> {
    pub async fn resolve(state: &'a AppState, path: &str) -> Result<Self, String> {
        let (account, token, owner, repo) =
            crate::github::resolve_repo_for_rest(state, path).await?;
        Ok(Self {
            state,
            account,
            token,
            owner,
            repo,
        })
    }
    pub fn variables(&self, number: i64) -> Value {
        serde_json::json!({"owner": self.owner, "repo": self.repo, "number": number})
    }
}

impl PrApi for RepoApi<'_> {
    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, String> {
        crate::github::graphql_with_retry(
            self.state,
            &self.account,
            query,
            variables,
            Some(&self.token),
        )
        .await
    }
    async fn rest(&self, method: &str, endpoint: &str, body: Value) -> Result<Value, String> {
        let path = format!("/repos/{}/{}/{}", self.owner, self.repo, endpoint);
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let request = crate::github::github_rest_request(
            &self.state.http_client,
            method.clone(),
            &self.account.host,
            &path,
            &self.token,
            crate::github::GITHUB_JSON,
        );
        let request = if method == reqwest::Method::GET {
            request
        } else {
            request.json(&body)
        };
        let response =
            crate::github::send_rest_with_breaker(self.state, &self.account, request).await?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!(
                "GitHub API failed ({status}): {}",
                String::from_utf8_lossy(&bytes)
            ));
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid GitHub response: {e}"))
    }
}
