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
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Set up Design Mode</title><style>body{{font:16px system-ui;max-width:780px;margin:8vh auto;background:#1e1e1e;color:#ccc;line-height:1.6}}code{{overflow-wrap:anywhere;color:#59a8dd}}h1{{font-size:28px}}li{{margin:16px 0}}</style><h1>Design Mode</h1><p>Set up this repository’s dedicated browser profile once.</p><ol><li>Open <code>chrome://extensions</code> (Edge: <code>edge://extensions</code>).</li><li>Enable Developer mode, choose <b>Load unpacked</b>, and select:<br><code>{}</code></li><li>Open the Extensions (puzzle) menu and pin <b>TUICommander Design Mode</b>. Click its icon to open the sidebar, or press <b>Option+Shift+D</b> (Windows/Linux: <b>Alt+Shift+D</b>). Choose a running dev server there, or enter your page URL in the address bar.</li></ol><p>The sidebar collects your picks and comments before you send them to an agent. Ordinary browsing works until you click Pick element.</p></html>",
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
    url: Option<&str>,
) -> Result<Value, String> {
    validate_url(url)?;
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
    let target = if let Some(url) = url {
        url.to_owned()
    } else if installed(&profile) {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // Catches: launch choosing about:blank/no setup page, or writing an unescaped profile path.
    #[tokio::test]
    async fn first_run_page_is_useful_without_a_server_and_escapes_paths() {
        const ROLE: &str = "TUIC_DESIGN_LAUNCH_TEST";
        if let Some(root) = std::env::var_os(ROLE) {
            let root = PathBuf::from(root);
            let _config = crate::config::set_config_dir_override(root.join("<script>\"&"));
            let state = Arc::new(crate::state::tests_support::make_test_app_state());
            *state.bound_socket_path.write() = root.join("socket");
            let bridge = root.join(".cargo/bin/tuic-bridge");
            assert_eq!(crate::agent_mcp::locate_bridge_binary(), Some(bridge));
            let response = launch(
                &state,
                &root.join("repo"),
                "launching-agent",
                Some("chrome"),
                std::env::var("TUIC_DESIGN_LAUNCH_URL").ok().as_deref(),
            )
            .await
            .unwrap();
            let target = response["url"].as_str().unwrap();
            if let Ok(requested) = std::env::var("TUIC_DESIGN_LAUNCH_URL") {
                // Catches: onboarding/discovery replacing an explicit URL on first start.
                assert_eq!(target, requested);
                assert_eq!(response["sessionId"], "launching-agent");
                assert_eq!(response["status"], "armed");
            } else {
                let path = url::Url::parse(target).unwrap().to_file_path().unwrap();
                let html = std::fs::read_to_string(path).unwrap();
                assert!(target.starts_with("file:"));
                assert!(html.contains("Load unpacked"));
                assert!(html.contains("&lt;script&gt;&quot;&amp;"));
                assert!(!html.contains("about:blank"));
            }
            let captured = root.join("browser-args");
            while !captured.exists() {
                tokio::task::yield_now().await;
            }
            let args = std::fs::read_to_string(captured).unwrap();
            assert_eq!(args.lines().last(), Some(target));
            assert!(args.contains("--user-data-dir="));
            return;
        }
        // Environment-based browser/sidecar discovery is isolated from other libtest threads.
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".cargo/bin")).unwrap();
        std::fs::create_dir(root.join("repo")).unwrap();
        super::super::discovery::tests::git(&root.join("repo"), &["init"]);
        let bridge = root.join(".cargo/bin/tuic-bridge");
        std::fs::write(
            &bridge,
            "#!/bin/sh\nmkdir -p \"$2/tuic-design-extension\"\n",
        )
        .unwrap();
        let browser = root.join("chrome-test");
        std::fs::write(&browser, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TUIC_DESIGN_LAUNCH_TEST/browser-args.pending\"\nmv \"$TUIC_DESIGN_LAUNCH_TEST/browser-args.pending\" \"$TUIC_DESIGN_LAUNCH_TEST/browser-args\"\n").unwrap();
        for path in [&bridge, &browser] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for requested in [None, Some("http://localhost:5173/page?x=1#element")] {
            let _ = std::fs::remove_file(root.join("browser-args"));
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            if let Some(requested) = requested {
                command.env("TUIC_DESIGN_LAUNCH_URL", requested);
            }
            let output = command
            .args(["--exact", "design_mode::launcher::tests::first_run_page_is_useful_without_a_server_and_escapes_paths", "--nocapture"])
            .env(ROLE, root).env("HOME", root).env("TUIC_DESIGN_CHROME", &browser)
            .output().await.unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Check the navigation contract before any native host or browser side effects.
pub(crate) fn validate_url(url: Option<&str>) -> Result<(), String> {
    let Some(url) = url else {
        return Ok(());
    };
    let parsed = url::Url::parse(url)
        .map_err(|_| "Design Mode URL must be an absolute HTTP or HTTPS URL")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Design Mode URL must be an absolute HTTP or HTTPS URL".into());
    }
    Ok(())
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn url_contract_rejects_non_web_navigation_before_launch() {
        // Catches: file/custom schemes or empty URL reaching a browser launch.
        for url in [
            "",
            "localhost:3000",
            "file:///tmp/page.html",
            "javascript:alert(1)",
            "ftp://example.com",
        ] {
            assert!(validate_url(Some(url)).is_err(), "{url}");
        }
        for url in [
            "http://localhost:3000/page?x=1#target",
            "https://example.com/path",
            "http://127.0.0.1:5173",
        ] {
            assert!(validate_url(Some(url)).is_ok(), "{url}");
        }
        assert!(validate_url(None).is_ok());
    }
}
