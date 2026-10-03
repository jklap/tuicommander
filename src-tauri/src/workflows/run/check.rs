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

pub(super) fn require_merge_tree_support(path: &Path) -> Result<(), String> {
    let version = git_output(path, &["--version"])?;
    let mut components = version
        .strip_prefix("git version ")
        .unwrap_or("")
        .split('.');
    let major = components
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    let minor = components
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => require_merge_tree_git_version(major, minor),
        _ => Err(format!(
            "git >= 2.38 required; cannot verify installed version: {version}"
        )),
    }
}

fn require_merge_tree_git_version(major: u32, minor: u32) -> Result<(), String> {
    if (major, minor) < (2, 38) {
        Err(format!(
            "git >= 2.38 required for workflow merge-tree verification (detected {major}.{minor})"
        ))
    } else {
        Ok(())
    }
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
    let mut command = Command::new(executable);
    command
        .args(check.argv.iter().skip(1))
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Own process group, so a timeout can stop the runner's descendants too.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
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
            #[cfg(unix)]
            // SAFETY: killpg only signals the group led by our own child; an
            // already empty group fails with ESRCH, which is ignored.
            unsafe {
                libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
            }
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

#[cfg(test)]
mod git_version_tests {
    use super::*;

    #[test]
    fn old_git_is_refused_with_the_required_version_not_conflict_review() {
        // catches: an unsupported merge-tree command being blamed on conflict resolution.
        let error = require_merge_tree_git_version(2, 37).unwrap_err();
        assert!(error.contains("git >= 2.38 required"), "{error}");
        assert!(!error.contains("human review"), "{error}");
        assert!(require_merge_tree_git_version(1, 99).is_err());
        assert!(require_merge_tree_git_version(2, 38).is_ok());
        assert!(require_merge_tree_git_version(2, 55).is_ok());
        assert!(require_merge_tree_git_version(3, 0).is_ok());
    }
}
