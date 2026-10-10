//! Ordinary Chrome/Edge launch with a dedicated repository profile and native host.
use crate::AppState;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
const EXTENSION_ID: &str = "eefbgjmmpaimpeaflioojmdipmfchefo";
fn browser(choice: Option<&str>) -> Result<(String, PathBuf), String> {
    if let Some(choice) = choice
        && !matches!(choice, "chrome" | "edge")
    {
        return Err("Choose Chrome or Edge".into());
    }
    let candidates = super::browser::chrome_candidates();
    let matches = |path: &Path| {
        let name = path.to_string_lossy().to_lowercase();
        match choice {
            Some("edge") => name.contains("edge"),
            Some("chrome") => !name.contains("edge"),
            _ => true,
        }
    };
    let executable = candidates
        .into_iter()
        .find(|path| path.is_file() && matches(path))
        .ok_or("The chosen Chrome/Edge browser is not installed")?;
    let kind = if executable.to_string_lossy().to_lowercase().contains("edge") {
        "edge"
    } else {
        "chrome"
    };
    Ok((kind.into(), executable))
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn onboarding(profile: &Path, folder: &Path) -> Result<String, String> {
    let path = profile.join("design-mode.html");
    let html = format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Set up Design Mode</title><style>body{{font:16px system-ui;max-width:780px;margin:8vh auto;background:#1e1e1e;color:#ccc;line-height:1.6}}code{{overflow-wrap:anywhere;color:#59a8dd}}h1{{font-size:28px}}li{{margin:16px 0}}</style><h1>Design Mode</h1><p>Set up this repository’s dedicated browser profile once.</p><ol><li>Open <code>chrome://extensions</code> (Edge: <code>edge://extensions</code>).</li><li>Enable Developer mode, choose <b>Load unpacked</b>, and select:<br><code>{}</code></li><li>Click the TUICommander extension icon to open the sidebar. Choose a running dev server there, or enter your page URL in the address bar.</li></ol><p>The sidebar collects your picks and comments before you send them to an agent. Ordinary browsing works until you click Pick element.</p></html>",
        escape(&folder.to_string_lossy())
    );
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| "Cannot open Design Mode setup page".into())
}
fn installed(profile: &Path) -> bool {
    std::fs::read(profile.join("Default/Preferences"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|prefs| {
            prefs
                .pointer(&format!("/extensions/settings/{EXTENSION_ID}"))
                .cloned()
        })
        .is_some()
}
pub(crate) async fn launch(
    state: &Arc<AppState>,
    repo: &Path,
    session: &str,
    choice: Option<&str>,
) -> Result<Value, String> {
    let (kind, executable) = browser(choice)?;
    let profile = super::browser::profile_dir(repo).join(&kind);
    let bridge = crate::agent_mcp::locate_bridge_binary()
        .ok_or("Design Mode native host is missing; rebuild the bundled tuic-bridge sidecar")?;
    #[cfg(unix)]
    let endpoint = state
        .bound_socket_path
        .read()
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = tuic_ipc::PIPE_NAME.to_owned();
    if endpoint.is_empty() {
        return Err("This instance has no bound local IPC endpoint".into());
    }
    let output = tokio::process::Command::new(bridge)
        .arg("--design-install")
        .arg(&profile)
        .arg(repo)
        .arg(&endpoint)
        .arg(session)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Native host installation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let discovery = super::discovery::discover(state, repo).await;
    let selected = discovery
        .as_ref()
        .ok()
        .and_then(|value| value["servers"].as_array())
        .filter(|servers| servers.len() == 1)
        .and_then(|servers| servers[0]["url"].as_str())
        .map(str::to_owned);
    let setup = onboarding(&profile, &profile.join("tuic-design-extension"))?;
    let target = if installed(&profile) {
        selected.unwrap_or(setup)
    } else {
        setup
    };
    tokio::process::Command::new(executable)
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check"])
        .arg(&target)
        .spawn()
        .map_err(|e| e.to_string())?;
    let status = super::legacy::manager(state)
        .await
        .arm_extension(repo.to_string_lossy().into_owned(), session.to_owned())
        .await;
    Ok(
        json!({"repoPath":status.repo_path,"sessionId":status.session_id,"status":status.status,"browser":kind,"url":target,"extensionPath":profile.join("tuic-design-extension")}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    // Catches: a path containing HTML becoming executable script in first-run instructions.
    #[test]
    fn first_run_page_is_useful_without_a_server_and_escapes_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let url = onboarding(tmp.path(), Path::new("/repo/<script>\"&")).unwrap();
        let html = std::fs::read_to_string(tmp.path().join("design-mode.html")).unwrap();
        assert!(url.starts_with("file:"));
        assert!(html.contains("Load unpacked"));
        assert!(html.contains("&lt;script&gt;&quot;&amp;"));
        assert!(!html.contains("about:blank"));
    }
}
