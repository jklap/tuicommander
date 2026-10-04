// DEFERRED (2026-09-23) — Main-target inspect mode may not reach cross-origin
// OOPIFs. Wait for a real use case before attaching inspect mode to subtargets.

use chromiumoxide::{Browser, BrowserConfig, handler::Handler};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(crate) fn profile_dir(repo_root: &Path) -> PathBuf {
    let key = hex::encode(Sha256::digest(repo_root.to_string_lossy().as_bytes()));
    crate::config::config_dir()
        .join("design-mode")
        .join(&key[..16])
}

pub(crate) fn read_active_port(profile: &Path) -> Result<u16, String> {
    let text = std::fs::read_to_string(profile.join("DevToolsActivePort"))
        .map_err(|error| error.to_string())?;
    let first = text.lines().next().ok_or("Empty DevToolsActivePort")?;
    let port = first
        .parse::<u16>()
        .map_err(|_| "Invalid DevToolsActivePort".to_string())?;
    if port == 0 {
        return Err("Invalid DevToolsActivePort".into());
    }
    Ok(port)
}

pub(crate) fn chrome_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(explicit) = std::env::var_os("TUIC_DESIGN_CHROME") {
        paths.push(PathBuf::from(explicit));
    }
    #[cfg(target_os = "macos")]
    {
        for app in [
            "Google Chrome",
            "Chromium",
            "Microsoft Edge",
            "Google Chrome Canary",
        ] {
            paths.push(PathBuf::from(format!(
                "/Applications/{app}.app/Contents/MacOS/{app}"
            )));
        }
    }
    #[cfg(target_os = "linux")]
    {
        for binary in [
            "google-chrome-stable",
            "google-chrome",
            "chromium",
            "chromium-browser",
        ] {
            for folder in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
                paths.push(folder.join(binary));
            }
        }
        paths.push(PathBuf::from("/opt/google/chrome/chrome"));
    }
    #[cfg(target_os = "windows")]
    {
        for root in ["HKCU", "HKLM"] {
            let key =
                format!(r"{root}\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe");
            if let Ok(output) = std::process::Command::new("reg")
                .args(["query", &key, "/ve"])
                .output()
                && output.status.success()
            {
                let listing = String::from_utf8_lossy(&output.stdout);
                for line in listing.lines() {
                    if let Some((_, path)) = line.split_once("REG_SZ") {
                        paths.push(PathBuf::from(path.trim()));
                    }
                }
            }
        }
        for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            if let Some(dir) = std::env::var_os(root) {
                paths.push(PathBuf::from(dir).join("Google/Chrome/Application/chrome.exe"));
            }
        }
    }
    paths
}

pub(crate) async fn launch_or_attach(repo_root: &Path) -> Result<(Browser, Handler), String> {
    let profile = profile_dir(repo_root);
    if let Ok(port) = read_active_port(&profile)
        && let Ok(attached) = Browser::connect(format!("http://127.0.0.1:{port}")).await
    {
        return Ok(attached);
    }
    let chrome = chrome_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "No Chrome or Chromium executable found".to_string())?;
    std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    let config = BrowserConfig::builder()
        .with_head()
        .chrome_executable(chrome)
        .user_data_dir(profile)
        .port(0)
        .args([
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            "--no-first-run",
            "--no-default-browser-check",
        ])
        .build()
        .map_err(|error| error.to_string())?;
    Browser::launch(config)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chromiumoxide::cdp::browser_protocol::{dom, input, overlay};
    use chromiumoxide::{Browser, BrowserConfig};
    use futures_util::StreamExt;

    #[test]
    fn profile_directory_is_stable_and_repo_scoped() {
        let first = profile_dir(Path::new("/repo/a"));
        assert_eq!(first, profile_dir(Path::new("/repo/a")));
        assert_ne!(first, profile_dir(Path::new("/repo/b")));
    }

    #[test]
    fn active_port_rejects_garbage_and_zero() {
        let dir = tempfile::tempdir().unwrap();
        let port_file = dir.path().join("DevToolsActivePort");
        std::fs::write(&port_file, "49152\n/devtools/browser/uuid\n").unwrap();
        assert_eq!(read_active_port(dir.path()).unwrap(), 49152);
        for invalid in ["", "not-a-port\n", "0\n", "65536\n"] {
            std::fs::write(&port_file, invalid).unwrap();
            assert!(read_active_port(dir.path()).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn candidates_include_supported_mac_browsers() {
        let candidates = chrome_candidates();
        #[cfg(target_os = "macos")]
        for name in [
            "Google Chrome",
            "Chromium",
            "Microsoft Edge",
            "Google Chrome Canary",
        ] {
            assert!(
                candidates
                    .iter()
                    .any(|path| path.to_string_lossy().contains(&format!("{name}.app")))
            );
        }
        #[cfg(target_os = "linux")]
        assert!(
            candidates
                .iter()
                .any(|path| path.ends_with("/opt/google/chrome/chrome"))
        );
        #[cfg(target_os = "windows")]
        assert!(
            candidates
                .iter()
                .any(|path| path.to_string_lossy().ends_with("chrome.exe"))
        );
    }

    #[tokio::test]
    #[ignore = "needs installed Chrome and a headed display"]
    async fn headed_inspect_selects_node_without_firing_page_click() {
        let chrome = std::env::var_os("TUIC_DESIGN_CHROME")
            .expect("TUIC_DESIGN_CHROME must point to an installed Chrome executable");
        let profile = tempfile::tempdir().expect("temporary Chrome profile");
        let config = BrowserConfig::builder()
            .with_head()
            .chrome_executable(chrome)
            .user_data_dir(profile.path())
            .port(0)
            .build()
            .expect("headed Chrome config");
        let (mut browser, mut handler) = Browser::launch(config).await.expect("Chrome launches");
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        let page = browser.new_page("about:blank").await.expect("test page");
        page.goto("data:text/html,<button id='pick' style='width:200px;height:80px' onclick='window.clicked=true'>Pick</button>")
            .await.expect("navigate test page");
        let port_file = profile.path().join("DevToolsActivePort");
        assert!(
            port_file.is_file(),
            "Chrome exposes its CDP port in the dedicated profile"
        );
        let port_text = std::fs::read_to_string(port_file).expect("active port contents");
        let port: u16 = port_text
            .lines()
            .next()
            .expect("port line")
            .parse()
            .expect("numeric CDP port");
        let (connected, mut connected_handler) =
            Browser::connect(format!("http://127.0.0.1:{port}"))
                .await
                .expect("attach to launched Chrome");
        let connected_task =
            tokio::spawn(async move { while connected_handler.next().await.is_some() {} });
        assert!(
            connected.version().await.is_ok(),
            "attached browser responds"
        );

        let mut picks = page
            .event_listener::<overlay::EventInspectNodeRequested>()
            .await
            .expect("inspect listener");
        page.execute(dom::EnableParams::default())
            .await
            .expect("DOM enabled");
        page.execute(overlay::EnableParams::default())
            .await
            .expect("Overlay enabled");
        page.execute(overlay::SetInspectModeParams {
            mode: overlay::InspectMode::SearchForNode,
            highlight_config: Some(overlay::HighlightConfig {
                show_info: Some(true),
                ..Default::default()
            }),
        })
        .await
        .expect("inspect mode armed");
        for kind in [
            input::DispatchMouseEventType::MouseMoved,
            input::DispatchMouseEventType::MousePressed,
            input::DispatchMouseEventType::MouseReleased,
        ] {
            page.execute(input::DispatchMouseEventParams {
                r#type: kind,
                x: 40.0,
                y: 35.0,
                button: Some(input::MouseButton::Left),
                ..Default::default()
            })
            .await
            .expect("mouse event");
        }
        let pick = tokio::time::timeout(std::time::Duration::from_secs(15), picks.next())
            .await
            .expect("inspect event arrived")
            .expect("inspect stream open");
        assert!(pick.backend_node_id.0 > 0);
        let clicked = page
            .evaluate("Boolean(window.clicked)")
            .await
            .expect("read page state");
        assert_eq!(clicked.value(), Some(&serde_json::json!(false)));
        browser.close().await.expect("close Chrome");
        let _ = browser.kill().await;
        handler_task.abort();
        connected_task.abort();
    }
}
