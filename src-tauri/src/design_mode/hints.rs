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
#[cfg(test)]
mod tests {
    use super::*;
    // Catches: a printed stale or foreign URL becoming an actionable server.
    #[test]
    fn pty_urls_require_a_current_owned_listener_on_the_same_port() {
        let live = vec![json!({"url":"http://localhost:19340/","source":"listener","pid":83008})];
        let hints = validated(include_str!("fixtures/mac-vite-output.txt"), &live);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["url"], "http://127.0.0.1:19340/");
        assert!(validated("https://other.invalid:19340/ http://localhost.evil.invalid:19340/ http://localhost:9999/",&live).is_empty());
    }
}
