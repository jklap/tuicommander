//! PTY advertisements are hints only, and require a current owned listener.
use crate::AppState;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
static URLS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"https?://(?:localhost|127\.0\.0\.1|\[::1\])(?::[0-9]+)?[^\s<>\"']*"#)
        .expect("URL hint regex")
});
pub(super) fn advertised(state: &AppState, roots: &[PathBuf], live: &[Value]) -> Vec<Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut found = Vec::new();
    for entry in state.session_maps.sessions.iter() {
        let sid = entry.key();
        if state
            .session_maps
            .last_output_ms
            .get(sid)
            .is_none_or(|timestamp| now.saturating_sub(timestamp.load(Ordering::Relaxed)) >= 60_000)
        {
            continue;
        }
        let cwd = {
            let session = entry.lock();
            session.cwd.clone()
        };
        if !cwd.as_ref().is_some_and(|cwd| {
            roots
                .iter()
                .any(|root| std::path::Path::new(cwd).starts_with(root))
        }) {
            continue;
        }
        let Some(buffer) = state.session_maps.output_buffers.get(sid) else {
            continue;
        };
        let bytes = buffer.lock().read_last(8192).0;
        let text = tuic_terminal::output_watchers::clean_line(&String::from_utf8_lossy(&bytes));
        found.extend(validated(&text, live));
    }
    found
}
fn validated(text: &str, live: &[Value]) -> Vec<Value> {
    URLS.find_iter(text).filter_map(|matched| {
        let text=matched.as_str().trim_end_matches([')',',',';']);let url=url::Url::parse(text).ok()?;
        if !matches!(url.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"|"::1")){return None;}
        let listener=live.iter().find(|listener|listener["source"]=="listener" && listener["url"].as_str().and_then(|url|url::Url::parse(url).ok()).is_some_and(|candidate|candidate.port_or_known_default()==url.port_or_known_default()))?;
        Some(json!({"url":text,"source":"fresh PTY","pid":listener["pid"],"worktreePath":listener["worktreePath"]}))
    }).collect()
}
#[cfg(all(test, unix))]
mod tests {
    use super::super::discovery::tests::{Server, offered, repository};
    use super::*;
    use std::sync::{Arc, atomic::AtomicU64};
    // Catches: discovery offering stale, foreign-host, or unowned PTY advertisements.
    #[tokio::test]
    async fn pty_urls_require_a_current_owned_listener_on_the_same_port() {
        let repo = repository();
        let server = Server::start(repo.path(), None);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "design-advertiser";
        crate::test_support::insert_recording_session(&state, sid);
        state.session_maps.sessions.get(sid).unwrap().lock().cwd =
            Some(repo.path().to_string_lossy().into_owned());
        let mut buffer = crate::OutputRingBuffer::new(8192);
        buffer.write(format!("http://127.0.0.1:{}/preview http://localhost:1/stale https://other.invalid:{}/ http://localhost.evil.invalid:{}/", server.port, server.port, server.port).as_bytes());
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), parking_lot::Mutex::new(buffer));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        state
            .session_maps
            .last_output_ms
            .insert(sid.into(), AtomicU64::new(now));
        let found = offered(state.clone(), repo.path()).await;
        let hints: Vec<_> = found
            .iter()
            .filter(|row| row["source"] == "fresh PTY")
            .collect();
        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints[0]["url"],
            format!("http://127.0.0.1:{}/preview", server.port)
        );
        state
            .session_maps
            .last_output_ms
            .get(sid)
            .unwrap()
            .store(0, Ordering::Relaxed);
        assert!(
            !offered(state.clone(), repo.path())
                .await
                .iter()
                .any(|row| row["source"] == "fresh PTY")
        );
        state
            .session_maps
            .last_output_ms
            .get(sid)
            .unwrap()
            .store(now, Ordering::Relaxed);
        drop(server);
        assert!(
            !offered(state, repo.path())
                .await
                .iter()
                .any(|row| row["source"] == "fresh PTY")
        );
    }
}
