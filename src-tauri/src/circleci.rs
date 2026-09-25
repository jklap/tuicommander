#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CircleCiJob {
    pub(crate) vcs: String,
    pub(crate) org: String,
    pub(crate) repo: String,
    pub(crate) build_num: u64,
}

/// Where the active CircleCI token was found. The token itself never crosses a
/// transport boundary.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TokenSource {
    Vault,
    Env,
    CliConfig,
    None,
}

const MAX_FAILED_ACTIONS: usize = 5;
const MAX_LOG_BODY_BYTES: usize = 64 * 1024;
/// Bounds a half-open credential-bearing API connection without constraining S3 downloads.
const CIRCLECI_API_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Bounds the complete CircleCI API request so callers cannot leave healing active indefinitely.
const CIRCLECI_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn vcs_slug(segment: &str) -> Option<&'static str> {
    match segment {
        "gh" | "github" => Some("gh"),
        "bb" | "bitbucket" => Some("bb"),
        _ => None,
    }
}

pub(crate) fn parse_check_url(value: &str) -> Option<CircleCiJob> {
    let url = url::Url::parse(value).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let segments: Vec<_> = url.path_segments()?.collect();

    let (vcs, org, repo, build_num) = match (host.as_str(), segments.as_slice()) {
        ("circleci.com", [vcs, org, repo, build_num]) => (*vcs, *org, *repo, *build_num),
        (
            "app.circleci.com",
            [
                "pipelines",
                vcs,
                org,
                repo,
                _,
                "workflows",
                _,
                "jobs",
                build_num,
            ],
        ) => (*vcs, *org, *repo, *build_num),
        _ => return None,
    };

    if !is_safe_segment(org) || !is_safe_segment(repo) {
        return None;
    }
    Some(CircleCiJob {
        vcs: vcs_slug(vcs)?.to_string(),
        org: org.to_string(),
        repo: repo.to_string(),
        build_num: build_num.parse().ok()?,
    })
}

fn token_from_cli_config(config: &str) -> Option<String> {
    let config: serde_yaml::Value = serde_yaml::from_str(config).ok()?;
    let mapping = config.as_mapping()?;
    let host = mapping.get("host").and_then(serde_yaml::Value::as_str);
    if host.is_some_and(|host| host.trim_end_matches('/').ne("https://circleci.com")) {
        return None;
    }
    mapping.get("token").and_then(serde_yaml::Value::as_str).map(str::trim).filter(|token| !token.is_empty()).map(ToOwned::to_owned)
}

fn token_from_cli_config_file() -> Option<String> {
    let path = dirs::home_dir()?.join(".circleci").join("cli.yml");
    token_from_cli_config(&std::fs::read_to_string(path).ok()?)
}

pub(crate) fn resolve_token() -> Result<(Option<String>, TokenSource), String> {
    resolve_token_with(
        || crate::credentials::get(crate::credentials::Credential::CircleCiToken),
        std::env::var("CIRCLE_TOKEN").ok(),
        token_from_cli_config_file(),
    )
}

fn resolve_token_with(
    vault: impl FnOnce() -> Result<Option<String>, String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> Result<(Option<String>, TokenSource), String> {
    resolve_token_from_vault_result(vault(), env, cli_config)
}

fn resolve_token_from_vault_result(
    vault: Result<Option<String>, String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> Result<(Option<String>, TokenSource), String> {
    let vault = vault.map_err(|error| format!("Failed to read CircleCI token: {error}"))?;
    Ok(resolve_token_from_sources(vault, env, cli_config))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_token_status() -> Result<serde_json::Value, String> {
    let (token, source) = resolve_token()?;
    Ok(serde_json::json!({ "configured": token.is_some(), "source": source }))
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_set_token(token: String) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("CircleCI token must not be empty; use Remove token instead".to_string());
    }
    crate::credentials::set(crate::credentials::Credential::CircleCiToken, &token)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn circleci_delete_token() -> Result<(), String> {
    crate::credentials::delete(crate::credentials::Credential::CircleCiToken)
}

fn resolve_token_from_sources(
    vault: Option<String>,
    env: Option<String>,
    cli_config: Option<String>,
) -> (Option<String>, TokenSource) {
    for (token, source) in [
        (vault, TokenSource::Vault),
        (env, TokenSource::Env),
        (cli_config, TokenSource::CliConfig),
    ] {
        if let Some(token) = token.map(|value| value.trim().to_string())
            && !token.is_empty()
        {
            return (Some(token), source);
        }
    }

    (None, TokenSource::None)
}

/// Return a bounded list of failed CircleCI action logs. CircleCI occasionally
/// reports a failed job without marking an action failed, so retain the last
/// fetchable action as a best-effort fallback for that case.
pub(crate) fn failed_actions(detail: &serde_json::Value) -> Vec<(String, String)> {
    let actions: Vec<(bool, String, String)> = detail
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|step| {
            let step_name = step
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("failed step")
                .to_string();
            step.get("actions")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |action| {
                    let output_url = action.get("output_url")?.as_str()?.trim();
                    (!output_url.is_empty()).then(|| {
                        (
                            action.get("failed").and_then(serde_json::Value::as_bool) == Some(true),
                            step_name.clone(),
                            output_url.to_string(),
                        )
                    })
                })
        })
        .collect();

    let failed: Vec<_> = actions
        .iter()
        .filter(|(failed, _, _)| *failed)
        .take(MAX_FAILED_ACTIONS)
        .map(|(_, step, output_url)| (step.clone(), output_url.clone()))
        .collect();
    if !failed.is_empty() {
        return failed;
    }

    (detail.get("status").and_then(serde_json::Value::as_str) == Some("failed"))
        .then(|| actions.last())
        .flatten()
        .map(|(_, step, output_url)| vec![(step.clone(), output_url.clone())])
        .unwrap_or_default()
}

/// Fetches a CircleCI v1.1 job and the pre-signed S3 logs for its failed
/// actions. CircleCI v2 does not expose step output, so keep this v1.1 detail
/// contained here for a future adapter replacement.
pub(crate) async fn fetch_job_log(
    client: &reqwest::Client,
    job: &CircleCiJob,
    token: &str,
) -> Result<String, String> {
    fetch_job_log_from_base(client, job, token, "https://circleci.com/api/v1.1/project/").await
}

async fn fetch_job_log_from_base(
    client: &reqwest::Client,
    job: &CircleCiJob,
    token: &str,
    api_base: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(api_base).map_err(|error| format!("Invalid CircleCI API base: {error}"))?;
    url.path_segments_mut()
        .expect("static URL can hold path segments")
        .pop_if_empty()
        .extend([
            job.vcs.as_str(),
            job.org.as_str(),
            job.repo.as_str(),
            &job.build_num.to_string(),
        ]);
    // The credential must never follow a provider-controlled redirect. S3 log
    // downloads below deliberately use the shared client without this header.
    let api_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CIRCLECI_API_CONNECT_TIMEOUT)
        .timeout(CIRCLECI_API_TIMEOUT)
        .build()
        .map_err(|error| format!("Failed to build CircleCI client: {error}"))?;
    let detail: serde_json::Value = api_client
        .get(url)
        .header("Circle-Token", token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("CircleCI request failed: {}", error.without_url()))?
        .error_for_status()
        .map_err(|error| format!("CircleCI API error: {}", error.without_url()))?
        .json()
        .await
        .map_err(|error| format!("Failed to parse CircleCI job: {error}"))?;

    let actions = failed_actions(&detail);
    if actions.is_empty() {
        return Err("CircleCI job contains no failed action logs".to_string());
    }

    let mut output = String::new();
    for (step, output_url) in actions {
        // output_url is a pre-signed S3 bearer URL: never attach Circle-Token
        // and never log it.
        let response = client
            .get(&output_url)
            .send()
            .await
            .map_err(|error| format!("CircleCI log fetch failed for {step}: {}", error.without_url()))?
            .error_for_status()
            .map_err(|error| format!("CircleCI log API error for {step}: {}", error.without_url()))?;
        if response.content_length().is_some_and(|length| length as usize > MAX_LOG_BODY_BYTES) {
            return Err(format!("CircleCI log for {step} exceeds {MAX_LOG_BODY_BYTES} bytes"));
        }
        use futures_util::StreamExt;
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("CircleCI log fetch failed for {step}: {}", error.without_url()))?;
            if bytes.len() + chunk.len() > MAX_LOG_BODY_BYTES {
                return Err(format!("CircleCI log for {step} exceeds {MAX_LOG_BODY_BYTES} bytes"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let chunks: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Failed to parse CircleCI log for {step}: {error}"))?;
        output.push_str(&format!("===== FAILED STEP: {step} =====\n"));
        for chunk in chunks {
            output.push_str(
                chunk
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            );
        }
        output.push('\n');
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::parse_check_url;

    #[test]
    fn parses_classic_circleci_job_url_with_query() {
        assert_eq!(
            parse_check_url("https://circleci.com/gh/acme/widget/42?utm_source=github"),
            Some(super::CircleCiJob {
                vcs: "gh".into(),
                org: "acme".into(),
                repo: "widget".into(),
                build_num: 42,
            })
        );
    }

    #[test]
    fn parses_app_urls_and_normalizes_vcs_slug() {
        for (vcs, expected) in [
            ("gh", "gh"),
            ("github", "gh"),
            ("bb", "bb"),
            ("bitbucket", "bb"),
        ] {
            let url = format!(
                "https://app.circleci.com/pipelines/{vcs}/acme/widget/77/workflows/workflow-id/jobs/19"
            );
            assert_eq!(
                parse_check_url(&url),
                Some(super::CircleCiJob {
                    vcs: expected.into(),
                    org: "acme".into(),
                    repo: "widget".into(),
                    build_num: 19,
                })
            );
        }
    }

    #[test]
    fn rejects_non_circleci_or_incomplete_job_urls() {
        for url in [
            "https://github.com/acme/widget/actions/runs/42",
            "https://app.circleci.com/pipelines/github/acme/widget/77/workflows/workflow-id",
            "https://circleci.com/gh/acme/widget/not-a-number",
            "https://app.circleci.com/pipelines/gitlab/acme/widget/77/workflows/workflow-id/jobs/19",
        ] {
            assert_eq!(parse_check_url(url), None, "{url}");
        }
    }

    #[test]
    fn reads_only_the_token_key_from_circleci_cli_config() {
        let config = "# CircleCI CLI config\napi: https://circleci.com\ntoken:  cli-read-only-token  \nother_token: ignored\n";

        assert_eq!(
            super::token_from_cli_config(config),
            Some("cli-read-only-token".into())
        );
    }

    #[test]
    fn resolves_tokens_in_vault_env_cli_config_order() {
        assert_eq!(
            super::resolve_token_from_sources(
                Some("vault-token".into()),
                Some("env-token".into()),
                Some("cli-token".into()),
            ),
            (Some("vault-token".into()), super::TokenSource::Vault)
        );
        assert_eq!(
            super::resolve_token_from_sources(
                None,
                Some("env-token".into()),
                Some("cli-token".into()),
            ),
            (Some("env-token".into()), super::TokenSource::Env)
        );
        assert_eq!(
            super::resolve_token_from_sources(None, None, Some("cli-token".into())),
            (Some("cli-token".into()), super::TokenSource::CliConfig)
        );
        assert_eq!(
            super::resolve_token_from_sources(None, None, None),
            (None, super::TokenSource::None)
        );
    }

    #[test]
    fn resolves_a_present_vault_token_through_the_injected_reader() {
        assert_eq!(
            super::resolve_token_with(|| Ok(Some("vault-token".into())), None, None),
            Ok((Some("vault-token".into()), super::TokenSource::Vault))
        );
    }

    #[test]
    fn vault_failure_is_not_reported_as_an_absent_token() {
        assert_eq!(
            super::resolve_token_from_vault_result(Err("keychain unavailable".into()), None, None),
            Err("Failed to read CircleCI token: keychain unavailable".into())
        );
    }

    #[test]
    fn selects_failed_actions_and_caps_the_s3_fan_out() {
        let detail: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/circleci/job.json")).unwrap();

        assert_eq!(
            super::failed_actions(&detail),
            vec![
                ("unit".into(), "https://logs.example/unit".into()),
                ("lint".into(), "https://logs.example/lint".into()),
                (
                    "integration".into(),
                    "https://logs.example/integration".into()
                ),
                ("e2e".into(), "https://logs.example/e2e".into()),
                ("package".into(), "https://logs.example/package".into()),
            ]
        );
    }

    #[test]
    fn falls_back_to_the_last_action_for_a_failed_job_without_flagged_actions() {
        let detail = serde_json::json!({
            "status": "failed",
            "steps": [
                { "name": "first", "actions": [{ "failed": false, "output_url": "https://logs.example/first" }] },
                { "name": "last", "actions": [{ "failed": false, "output_url": "https://logs.example/last" }] }
            ]
        });

        assert_eq!(
            super::failed_actions(&detail),
            vec![("last".into(), "https://logs.example/last".into())]
        );
    }

    #[tokio::test]
    async fn presigned_s3_download_never_receives_circleci_credentials() {
        use mockito::{Matcher, Server};

        let mut server = Server::new_async().await;
        let output_url = format!("{}/output", server.url());
        let _api = server
            .mock("GET", "/api/v1.1/project/gh/acme/widget/42")
            .match_header("circle-token", "secret")
            .with_status(200)
            .with_body(serde_json::json!({"steps":[{"name":"unit","actions":[{"failed":true,"output_url":output_url}]}]}).to_string())
            .create_async()
            .await;
        let output = server
            .mock("GET", "/output")
            .match_header("circle-token", Matcher::Missing)
            .match_header("authorization", Matcher::Missing)
            .with_status(200)
            .with_body("[{\"message\":\"failed\"}]")
            .create_async()
            .await;
        let job = super::CircleCiJob { vcs: "gh".into(), org: "acme".into(), repo: "widget".into(), build_num: 42 };

        assert!(super::fetch_job_log_from_base(&reqwest::Client::new(), &job, "secret", &(server.url() + "/api/v1.1/project/")).await.unwrap().contains("failed"));
        _api.assert_async().await;
        output.assert_async().await;
    }
}
