//! Putting the main WebView back on the app when its document disappears.
//!
//! On 2026-09-08 the Mac went to standby with the display off, twice, and
//! TUICommander came back showing a white window. The backend, the HTTP server
//! and all 20 PTY sessions were untouched. What had changed was the document in
//! the main frame:
//!
//! ```text
//! TOP {"href":"about:srcdoc","htmlLen":26,"bodyLen":0,"canvases":0,"appPresent":false}
//! ```
//!
//! 26 characters is `<html><body></body></html>`, the parse of an *empty*
//! srcdoc. The WebContent process had not crashed — the same pid served the
//! window before and after — so this is not the `about:blank` case the
//! `on_page_load` hook in `lib.rs` was written for. It happened during the
//! memory-pressure sweep macOS runs while asleep
//! (`JetsamEvent-2026-09-08-214808`: 285 MB free, a systemwide
//! `JETSAM_REASON_MEMORY_IDLE_EXIT` pass at 21:41), which suspends and resumes
//! the WebKit process. The app renders plugin panels and HTML previews in
//! `srcdoc` iframes, and one of those documents is what the main frame came back
//! holding.
//!
//! The recovery does not depend on knowing what WebKit did internally: put the
//! main frame back on the URL it was last healthy at.
//!
//! **`reload()` cannot do that.** There is no URL behind `about:srcdoc` to
//! reload — which is why `POST /debug/reload_webview` answered `{"ok":true}` and
//! left the window white for an hour. Only `navigate(boot_url)` works, and the
//! boot URL has to have been recorded *before* the frame was lost.
//!
//! The frontend heartbeat (`frontend_liveness`) does not cover this either. The
//! app is gone from the DOM rather than blocked, so it never beat once from the
//! blank document, and a frontend that never beat is deliberately not reported
//! as frozen — `tuic-remote` has no WebView at all. Absence of a heartbeat and
//! absence of the app are different faults; this module watches the second.

use std::time::Duration;

/// How often the main frame's URL is checked. The probe is a round trip through
/// the event loop, so it is kept well below the cost of the diagnostics tick;
/// fifteen seconds of white screen before self-healing is not worth polling
/// harder for.
const POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Let the window boot before judging it. A webview that has not committed its
/// first document yet can legitimately read as `about:blank`.
const STARTUP_DELAY: Duration = Duration::from_secs(30);

/// True when the main frame is no longer showing the app.
///
/// Every `about:` URL qualifies: `about:blank` after a WebContent crash,
/// `about:srcdoc` after the standby incident above. Nothing else can legitimately
/// be the top document — the navigation handler in `lib.rs` sends external links
/// to the system browser instead of loading them — and restricting the test to
/// the `about:` scheme means a healthy URL can never be mistaken for a lost one
/// and re-navigated in a loop.
pub(crate) fn is_lost(url: &str) -> bool {
    url.starts_with("about:")
}

#[cfg(feature = "desktop")]
mod desktop {
    use super::{POLL_INTERVAL, STARTUP_DELAY, is_lost};
    use crate::state::AppState;
    use std::sync::Arc;

    fn main_window(state: &Arc<AppState>) -> Option<tauri::WebviewWindow> {
        use tauri::Manager;
        state.app_handle.read().as_ref()?.get_webview_window("main")
    }

    /// Send the main frame back to the last URL it was healthy at.
    ///
    /// Shared by the automatic poller, the `on_page_load` crash hook and
    /// `POST /debug/reload_webview`, because all three want the same thing and
    /// only one of them can be tested by hand.
    pub(crate) fn navigate_home(state: &Arc<AppState>) -> serde_json::Value {
        let Some(window) = main_window(state) else {
            return serde_json::json!({"error": "main window not found"});
        };
        let target = state.webview_boot_url.read().clone();
        let Some(target) = target else {
            // No healthy URL was ever recorded, so there is nothing to aim at.
            // `reload` is the only move left and it is precisely the one that
            // does not work against a blank srcdoc — say so rather than report
            // a success the caller cannot verify.
            return match window.reload() {
                Ok(()) => serde_json::json!({
                    "ok": true,
                    "action": "reload",
                    "warning": "no boot URL recorded yet — reload does not recover a blank document",
                }),
                Err(e) => serde_json::json!({"error": format!("reload failed: {e}")}),
            };
        };
        match window.navigate(target.clone()) {
            Ok(()) => serde_json::json!({
                "ok": true,
                "action": "navigate",
                "url": target.as_str(),
            }),
            Err(e) => serde_json::json!({"error": format!("navigate failed: {e}")}),
        }
    }

    /// One poll. Returns the lost document's URL, or `None` while the frame
    /// holds the app.
    ///
    /// Probes and nothing else: [`spawn`] does the logging and the recovery,
    /// because it is the only place that knows whether this is a new loss or
    /// the same one fifteen seconds later.
    ///
    /// `WebviewWindow::url` posts a message to the event loop and blocks on the
    /// reply, so this must not run on the main thread (it would wait for itself)
    /// nor on the diagnostics thread (a wedged event loop would take the CPU
    /// watchdog down with it). Hence the dedicated thread in [`spawn`].
    fn lost_document(state: &Arc<AppState>) -> Option<String> {
        let window = main_window(state)?; // No window yet — nothing to recover.
        let url = window.url().ok()?; // A failed probe is not a lost document.
        if !is_lost(url.as_str()) {
            *state.webview_boot_url.write() = Some(url);
            return None;
        }
        Some(url.to_string())
    }

    /// Ceiling on the retry schedule, in polls — four minutes at `POLL_INTERVAL`.
    /// Bounded rather than latched: a navigate can fail transiently, and a latch
    /// that stopped retrying would leave the window white for good.
    const MAX_RECOVERY_BACKOFF_POLLS: u32 = 16;

    pub(crate) fn spawn(state: Arc<AppState>) {
        std::thread::Builder::new()
            .name("webview-recovery".into())
            .spawn(move || {
                std::thread::sleep(STARTUP_DELAY);
                // The log lines are latched separately from the retries, and
                // they are the thing that must not repeat: an error and an info
                // every fifteen seconds alternate, so they defeat the ring
                // buffer's adjacent-entry coalescing and take two of its 1000
                // slots per poll. A window left white overnight evicted every
                // other diagnostic in the buffer — including whatever explained
                // the loss.
                let mut announced_loss = false;
                let mut polls_until_retry = 0u32;
                let mut backoff_polls = 1u32;
                loop {
                    std::thread::sleep(POLL_INTERVAL);
                    let Some(url) = lost_document(&state) else {
                        if announced_loss {
                            tracing::info!(
                                source = "webview",
                                "Main WebView is back on the app after a lost document"
                            );
                        }
                        announced_loss = false;
                        polls_until_retry = 0;
                        backoff_polls = 1;
                        continue;
                    };
                    if !announced_loss {
                        announced_loss = true;
                        tracing::error!(
                            source = "webview",
                            url = %url,
                            "Main WebView lost its document — the app is not in the DOM and the \
                             window is white. Navigating back to the app; PTY sessions are \
                             unaffected.",
                        );
                    }
                    if polls_until_retry > 0 {
                        polls_until_retry -= 1;
                        continue;
                    }
                    let outcome = navigate_home(&state);
                    tracing::info!(
                        source = "webview",
                        outcome = %outcome,
                        "WebView recovery attempted"
                    );
                    polls_until_retry = backoff_polls;
                    backoff_polls = (backoff_polls * 2).min(MAX_RECOVERY_BACKOFF_POLLS);
                }
            })
            .expect("failed to spawn webview-recovery thread");
    }
}

#[cfg(feature = "desktop")]
pub(crate) use desktop::{navigate_home, spawn};

#[cfg(not(feature = "desktop"))]
pub(crate) fn navigate_home(_state: &std::sync::Arc<crate::state::AppState>) -> serde_json::Value {
    serde_json::json!({"error": "webview recovery requires the desktop feature"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_observed_blank_documents_are_lost() {
        // What the main frame actually held on 2026-09-08.
        assert!(is_lost("about:srcdoc"));
        // What a WebContent crash leaves behind, handled since before that.
        assert!(is_lost("about:blank"));
    }

    #[test]
    fn a_serving_app_is_not_lost() {
        // Dev, production, and an in-app route: none of these may ever be
        // re-navigated, or the poller would reload the app every 15 seconds.
        assert!(!is_lost("http://127.0.0.1:1421/"));
        assert!(!is_lost("tauri://localhost/"));
        assert!(!is_lost("http://127.0.0.1:1421/#/settings"));
        assert!(!is_lost("http://tauri.localhost/index.html"));
    }

    #[test]
    fn a_url_that_merely_mentions_about_is_not_lost() {
        // The check is on the scheme, not on the text: a route called "about"
        // is a page of the app, not a lost frame.
        assert!(!is_lost("http://127.0.0.1:1421/about"));
        assert!(!is_lost("tauri://localhost/#/about:srcdoc"));
    }
}
