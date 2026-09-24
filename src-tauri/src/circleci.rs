#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CircleCiJob {
    pub(crate) vcs: String,
    pub(crate) org: String,
    pub(crate) repo: String,
    pub(crate) build_num: u64,
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
}
