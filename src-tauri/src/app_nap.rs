//! Keeps macOS App Nap off for the whole process (story 1638-c94c).
//!
//! App Nap applies to an app whose windows are not visible and that holds no
//! power assertion. It moves every thread to the background band: CPU starved,
//! timers coalesced, disk I/O throttled. TUIC is a server as much as a window:
//! PTY readers, the HTTP/MCP API for agents and remote clients, and the
//! heartbeat all live in this process. Measured on 2026-10-20 in an occluded
//! agent-test instance under agent load: ~100 threads at priority 4, the main
//! thread blocked 40 s in `rename(2)`, the backend log silent for 40 s, and a
//! 41.6 s heartbeat gap. The daily app only escaped while `block_sleep` held a
//! power assertion for a busy agent.
//!
//! `UserInitiatedAllowingIdleSystemSleep` disables App Nap but still lets the
//! machine idle-sleep, so the sleep-prevention setting keeps its meaning.

/// Begin a process-lifetime activity that exempts this app from App Nap.
///
/// The activity is never ended: it must cover the process lifetime, and the
/// system releases it at exit.
#[cfg(target_os = "macos")]
pub fn disable() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

    let reason = NSString::from_str("Terminal sessions and local API stay live while hidden");
    let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
        NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
        &reason,
    );
    std::mem::forget(activity);
    tracing::info!(
        source = "app-nap",
        "App Nap disabled for the process lifetime"
    );
}

/// No-op on non-macOS platforms (App Nap is macOS-only).
#[cfg(not(target_os = "macos"))]
pub fn disable() {}
