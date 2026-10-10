//! Live listener discovery. Ownership uses path components, never string prefixes.
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Listener {
    pid: u32,
    name: String,
    port: u16,
}
fn listeners(output: &str) -> Vec<Listener> {
    let mut pid = 0;
    let mut name = String::new();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix('c') {
            name = value.to_owned();
        } else if let Some(value) = line.strip_prefix('n') {
            let Some((host, port)) = value.rsplit_once(':') else {
                continue;
            };
            if !matches!(host, "127.0.0.1" | "[::1]" | "*" | "0.0.0.0" | "[::]") {
                continue;
            }
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            if port > 0 && pid > 0 && seen.insert((pid, port)) {
                found.push(Listener {
                    pid,
                    name: name.clone(),
                    port,
                });
            }
        }
    }
    found
}
fn cwd_map(output: &str) -> HashMap<u32, PathBuf> {
    let mut pid = 0;
    let mut paths = HashMap::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix('n') {
            paths.insert(pid, PathBuf::from(value));
        }
    }
    paths
}
fn owner<'a>(cwd: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root| cwd.starts_with(root))
        .max_by_key(|root| root.components().count())
}
fn candidates(listing: &str, cwds: &str, roots: &[PathBuf]) -> Vec<Value> {
    let paths = cwd_map(cwds);
    listeners(listing).into_iter().filter_map(|listener| {
        let cwd=paths.get(&listener.pid)?;let root=owner(cwd,roots)?;
        Some(json!({"url":format!("http://localhost:{}/",listener.port),"source":"listener","pid":listener.pid,"process":listener.name,"worktreePath":root,"cwd":cwd}))
    }).collect()
}
fn command_owners(output: &str, roots: &[PathBuf]) -> HashMap<u32, PathBuf> {
    let mut owners = HashMap::new();
    for line in output.lines() {
        let mut words = line.split_whitespace();
        let Some(pid) = words.next().and_then(|word| word.parse().ok()) else {
            continue;
        };
        let deepest = words
            .filter_map(|word| owner(Path::new(word.trim_matches(['\"', '\''])), roots))
            .max_by_key(|root| root.components().count());
        if let Some(root) = deepest {
            owners.insert(pid, root.clone());
        }
    }
    owners
}
async fn command(binary: &str, args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new(crate::cli::resolve_cli(binary));
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output())
        .await
        .map_err(|_| format!("{binary} discovery timed out"))?
        .map_err(|e| format!("{binary}: {e}"))?;
    // lsof exits 1 when no process matches; that is an empty set, not a scan error.
    if !(output.status.success()
        || binary == "lsof" && output.status.code() == Some(1) && output.stderr.is_empty())
    {
        return Err(format!(
            "{binary}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}
fn settings(repo: &Path) -> Option<crate::config::RepoSettingsEntry> {
    crate::config::load_repo_settings()
        .repos
        .into_iter()
        .find_map(|(path, value)| {
            (crate::git::canonical_repo_root(Path::new(&path))
                == crate::git::canonical_repo_root(repo))
            .then_some(value)
        })
}
/// Only trusted TUIC settings supply scripts; .tuic.json cannot silently execute code.
pub(super) fn run_script(repo: &Path) -> Option<String> {
    settings(repo)
        .and_then(|entry| entry.run_script)
        .or_else(|| Some(crate::config::load_repo_defaults().run_script))
        .filter(|script| !script.trim().is_empty())
}
pub(crate) async fn discover(state: &crate::AppState, repo: &Path) -> Result<Value, String> {
    let repo = repo.canonicalize().map_err(|e| e.to_string())?;
    let path = repo.to_str().ok_or("Repository path is not UTF-8")?;
    let worktrees = command("git", &["-C", path, "worktree", "list", "--porcelain"]).await?;
    let roots: Vec<_> = worktrees
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect();
    let mut servers = Vec::new();
    if let Some(url) = settings(&repo)
        .and_then(|entry| entry.dev_server_url)
        .filter(|url| !url.trim().is_empty())
    {
        let parsed = url::Url::parse(&url).map_err(|_| "Invalid configured dev-server URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Configured URL must use HTTP or HTTPS".into());
        }
        servers.push(json!({"url":url,"source":"configured"}));
    }
    #[cfg(unix)]
    {
        let listing = command("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"]).await?;
        let ids = listeners(&listing)
            .iter()
            .map(|listener| listener.pid.to_string())
            .collect::<Vec<_>>()
            .join(",");
        if !ids.is_empty() {
            let mut cwds = command("lsof", &["-a", "-p", &ids, "-d", "cwd", "-Fn"]).await?;
            let known = cwd_map(&cwds);
            let commands = command("ps", &["-p", &ids, "-o", "pid=,command="]).await?;
            for (pid, root) in command_owners(&commands, &roots) {
                if known
                    .get(&pid)
                    .is_none_or(|cwd| owner(cwd, &roots).is_none())
                {
                    cwds.push_str(&format!("p{pid}\nfcwd\nn{}\n", root.display()));
                }
            }
            servers.extend(candidates(&listing, &cwds, &roots));
        }
    }
    let hints = super::hints::advertised(state, &roots, &servers);
    servers.extend(hints);
    let mut seen = HashSet::new();
    servers.retain(|server| seen.insert(server["url"].as_str().unwrap_or_default().to_owned()));
    Ok(
        json!({"servers":servers,"canStart":run_script(&repo).is_some(),"scanNotice":if cfg!(windows){Some("Live listener attribution is unavailable on Windows; use the configured URL or address bar.")}else{None}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: confusing two live servers, stale listeners, nested roots and path-like substrings.
    #[test]
    fn recorded_mac_listeners_use_deepest_cwd_and_require_a_live_owner() {
        let listing = include_str!("fixtures/mac-listeners.txt");
        let cwds = include_str!("fixtures/mac-cwds.txt");
        let parent = PathBuf::from(
            "/Users/stefano.straus/Gits/personal/tuicommander__wt/feat-design-mode-v2-1672/.tmp/design-v2/discovery",
        );
        let roots = vec![parent.clone(), parent.join("app")];
        let found = candidates(listing, cwds, &roots);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0]["url"], "http://localhost:19342/");
        assert_eq!(found[1]["worktreePath"], json!(roots[1]));
        assert!(
            candidates(listing, "", &roots).is_empty(),
            "dead/unattributed listener cannot become a URL"
        );
        assert!(
            candidates(
                listing,
                &cwds.replace("/discovery", "/discovery-other"),
                &roots
            )
            .is_empty(),
            "path substring is not ownership"
        );
    }
    // Catches: another worktree disappearing, and command-line path substrings claiming ownership.
    #[test]
    fn recorded_vite_processes_distinguish_worktrees_and_command_path_boundaries() {
        let root = PathBuf::from("/Users/stefano.straus/Gits/personal/tuicommander__wt");
        let roots = vec![
            root.join("feat-design-mode-v2-1672"),
            root.join("fix-1369-answers-history"),
        ];
        let found = candidates(
            include_str!("fixtures/mac-worktrees-listeners.txt"),
            include_str!("fixtures/mac-worktrees-cwds.txt"),
            &roots,
        );
        assert_eq!(found.len(), 2);
        let commands = include_str!("fixtures/mac-worktrees-commands.txt");
        assert_eq!(
            command_owners(commands, &roots).len(),
            1,
            "only the absolute Vite argv path is usable as a fallback"
        );
        assert!(
            command_owners(
                &commands.replace("feat-design-mode-v2-1672", "feat-design-mode-v2-1672-other"),
                &roots
            )
            .is_empty()
        );
    }
}
