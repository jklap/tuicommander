use crate::workflows::CheckDefinition;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckReceipt {
    pub check_id: String,
    pub argv: Vec<String>,
    pub exit_code: i32,
    #[serde(default)]
    pub ref_name: String,
    pub commit: String,
    pub tree: String,
    pub duration_ms: u64,
}

pub(super) fn git_output(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .map_err(|error| format!("run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|error| format!("decode git output: {error}"))
}

pub(super) fn clean_artifact(path: &Path) -> Result<(String, String), String> {
    if !git_output(path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        return Err("workflow check requires a clean worktree".into());
    }
    let commit = git_output(path, &["rev-parse", "HEAD"])?;
    let tree = git_output(path, &["rev-parse", "HEAD^{tree}"])?;
    if commit.len() != 40 && commit.len() != 64 {
        return Err("invalid Git commit digest".into());
    }
    if tree.len() != commit.len() {
        return Err("invalid Git tree digest".into());
    }
    Ok((commit, tree))
}

/// Run a published check directly in a clean worktree and bind the result to
/// the exact commit and tree observed before and after it. A changing tree
/// yields no usable receipt, even when the command exits successfully.
pub fn execute_pinned_check(check: &CheckDefinition, path: &Path) -> Result<CheckReceipt, String> {
    let (commit, tree) = clean_artifact(path)?;
    let ref_name = git_output(path, &["symbolic-ref", "HEAD"])?;
    let executable = check
        .argv
        .first()
        .ok_or("workflow check has no executable")?;
    let start = Instant::now();
    let mut child = Command::new(executable)
        .args(check.argv.iter().skip(1))
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("start workflow check {}: {error}", check.id))?;
    let timeout = Duration::from_secs(u64::from(check.timeout_secs));
    let exit_code = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for workflow check: {error}"))?
        {
            break status.code().unwrap_or(-1);
        }
        if start.elapsed() >= timeout {
            child
                .kill()
                .map_err(|error| format!("stop timed-out workflow check: {error}"))?;
            child
                .wait()
                .map_err(|error| format!("reap timed-out workflow check: {error}"))?;
            break -1;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let duration_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    if clean_artifact(path)? != (commit.clone(), tree.clone())
        || git_output(path, &["symbolic-ref", "HEAD"])? != ref_name
    {
        return Err("workflow check changed the branch, commit, or tree".into());
    }
    Ok(CheckReceipt {
        check_id: check.id.clone(),
        argv: check.argv.clone(),
        exit_code,
        ref_name,
        commit,
        tree,
        duration_ms,
    })
}
