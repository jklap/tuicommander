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

    Some(CircleCiJob {
        vcs: vcs_slug(vcs)?.to_string(),
        org: org.to_string(),
        repo: repo.to_string(),
        build_num: build_num.parse().ok()?,
    })
}

fn token_from_cli_config(config: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix("token:")?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn token_from_cli_config_file() -> Option<String> {
    let path = dirs::home_dir()?.join(".circleci").join("cli.yml");
    token_from_cli_config(&std::fs::read_to_string(path).ok()?)
}

pub(crate) fn resolve_token() -> (Option<String>, TokenSource) {
    let vault = crate::credentials::get(crate::credentials::Credential::CircleCiToken)
        .ok()
        .flatten();
    let env = std::env::var("CIRCLE_TOKEN").ok();
    resolve_token_from_sources(vault, env, token_from_cli_config_file())
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
}
