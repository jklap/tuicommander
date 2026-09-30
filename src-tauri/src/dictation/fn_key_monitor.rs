//! Monitors the Fn/Globe key on macOS via NSEvent local event monitor.
//!
//! Uses `addLocalMonitorForEventsMatchingMask:handler:` to observe `flagsChanged`
//! events within the app process. Only fires when the window is focused.
//! No accessibility permissions required.
//!
//! Keys that arrive as ordinary `keyDown` events but never reach the WebView
//! (Ctrl+Tab, F13-F20) are handled by `crate::native_keys` instead.

/// Install the Fn key local event monitor. Emits "fn-key-down" and "fn-key-up"
/// Tauri events (scoped to the main window) when the Fn/Globe modifier flag toggles.
///
/// Must be called from the main thread (Tauri setup runs on main thread).
// DEFERRED (2026-09-06) — story #672-c1a3 asked for this monitor to start
// only when dictation is enabled and stop when disabled, instead of running
// for the whole app lifetime. `addLocalMonitorForEventsMatchingMask:` is
// AppKit main-thread-only; today `install()` runs once, at setup, on the main
// thread by construction. Toggling it dynamically from a settings change
// (which fires off the main thread) needs a real main-thread dispatch and
// hardware testing to confirm the monitor can be safely removed/re-added
// without racing an in-flight event callback — not something to guess at
// blind. Left as an always-on monitor.
#[cfg(target_os = "macos")]
pub fn install(app_handle: tauri::AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;
    use std::sync::atomic::Ordering;
    #[cfg(feature = "desktop")]
    use tauri::{Emitter, Manager};

    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: event is a valid NSEvent pointer provided by AppKit for the
        // duration of the block invocation.
        let event_ref = unsafe { event.as_ref() };
        let flags = event_ref.modifierFlags();
        let fn_down = flags.contains(NSEventModifierFlags::Function);
        let dictation = app_handle.state::<super::DictationState>();
        let was_down = dictation.fn_down.swap(fn_down, Ordering::AcqRel);

        // Scope events to main window only — prevents plugin iframes from
        // observing dictation timing.
        if fn_down && !was_down {
            tracing::info!(
                source = "dictation",
                key = "Fn",
                edge = "down",
                origin = "native-monitor",
                unix_ms = unix_ms(),
                "Push-to-talk key edge"
            );
            let _ = app_handle.emit_to(tauri::EventTarget::labeled("main"), "fn-key-down", ());
        } else if !fn_down && was_down {
            tracing::info!(
                source = "dictation",
                key = "Fn",
                edge = "up",
                origin = "native-monitor",
                unix_ms = unix_ms(),
                "Push-to-talk key edge"
            );
            dictation.request_native_stop("fn-key-up");
            let _ = app_handle.emit_to(tauri::EventTarget::labeled("main"), "fn-key-up", ());
        }

        // Return the event unchanged to let it propagate normally.
        event.as_ptr()
    });

    // SAFETY: we pass NSEventMask::FlagsChanged and a valid block.
    // The returned monitor is retained by AppKit for the lifetime of the app;
    // we don't store it since the monitor should live until app exit.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::FlagsChanged, &block)
    };

    if monitor.is_some() {
        // Monitor is retained by AppKit — it will live until removeMonitor or app exit.
        // We intentionally leak it since we want it active for the entire app lifetime.
        tracing::info!(source = "dictation", "Fn key monitor installed");
    } else {
        tracing::warn!(source = "dictation", "Failed to install Fn key monitor");
    }
}

#[cfg(target_os = "macos")]
pub fn release_on_focus_loss(app_handle: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};

    let dictation = app_handle.state::<super::DictationState>();
    if dictation.fn_down.swap(false, Ordering::AcqRel) {
        tracing::info!(
            source = "dictation",
            key = "Fn",
            edge = "up",
            origin = "focus-loss",
            unix_ms = unix_ms(),
            "Push-to-talk key edge"
        );
        dictation.request_native_stop("focus-loss");
        let _ = app_handle.emit_to(tauri::EventTarget::labeled("main"), "fn-key-up", ());
    }
}

#[cfg(target_os = "macos")]
fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn install(_app_handle: tauri::AppHandle) {}
