//! Draft PR publication for event runs. The allowlist is checked here, at the
//! only place a PR is created, independently of how the run was admitted.
use super::model::EventIntake;

pub(crate) struct DraftPr<'a> {
    pub repository: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub issue_number: i64,
}

/// A branch name is passed as an option value; it must not look like an option
/// or carry whitespace and control characters.
fn plain_ref(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Create a draft PR with `gh` through `run_gh` (argv without the program name,
/// returns stdout). Nothing runs for a repository outside the allowlist.
pub(crate) fn publish_draft_pr(
    intake: &EventIntake,
    request: &DraftPr<'_>,
    run_gh: impl FnOnce(&[String]) -> Result<String, String>,
) -> Result<String, String> {
    if !intake.allows(request.repository) {
        return Err(format!(
            "Refusing to publish: {} is not in the event intake allowlist",
            request.repository
        ));
    }
    // A detached checkout reports HEAD, which names no branch to publish.
    if !plain_ref(request.head) || request.head == "HEAD" || !plain_ref(request.base) {
        return Err("Refusing to publish: invalid head or base branch".into());
    }
    let args = [
        "pr",
        "create",
        "--draft",
        "--repo",
        request.repository,
        "--head",
        request.head,
        "--base",
        request.base,
        "--title",
        &format!("Resolve issue #{}", request.issue_number),
        "--body",
        &format!(
            "Closes #{}\n\nDraft opened by a TUICommander event run.",
            request.issue_number
        ),
    ]
    .map(String::from);
    run_gh(&args)
}

/// The native `gh` runner: the instance's own environment supplies the auth.
pub(crate) fn run_gh_in(workspace: &std::path::Path, args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new(crate::agent::resolve_cli("gh"))
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|e| format!("gh unavailable: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "gh pr create failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Trimmed stdout of `git` in `workspace`; used to name the branch to publish.
pub(crate) fn run_git(workspace: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|e| format!("git unavailable: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!("git {} failed", args.join(" ")))
    }
}

#[cfg(test)]
mod tests;
