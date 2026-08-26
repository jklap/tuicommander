//! Resolve the write target for OSC 7770 emission: the *parent* process's
//! controlling tty, not this process's own stdio.
//!
//! Claude Code (and the other agents this drives) captures a hook's stdout to
//! read its own protocol out of it, so writing to our own fd 1/2 would never
//! reach the terminal. We need the tty the agent itself is attached to — which
//! is this process's parent — resolved natively instead of shelling out to
//! `ps -o tty= -p $PPID` (the original implementation's approach, replicated
//! here without the subprocess).
//!
//! Resolution order:
//! 1. `$TUIC_HOOK_TTY` — test seam, used verbatim. Never set in production.
//! 2. The parent process's controlling tty (platform-specific; see below).
//! 3. `/dev/tty` — this process's own controlling tty, if it has one. Mirrors
//!    the original shell fallback (`case … *) __t="/dev/tty" ;; esac`), which
//!    fires when the parent-tty lookup comes back empty.
//!
//! Silent failure throughout: if nothing resolves, callers get `None` and the
//! hook is a no-op. A hook must never error or block the agent.

use std::path::PathBuf;

pub fn resolve() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("TUIC_HOOK_TTY")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }

    #[cfg(unix)]
    {
        let ppid = unsafe { libc::getppid() };
        if let Some(path) = platform::parent_tty(ppid) {
            return Some(path);
        }
    }

    fallback_own_tty()
}

#[cfg(unix)]
fn fallback_own_tty() -> Option<PathBuf> {
    let path = PathBuf::from("/dev/tty");
    path.exists().then_some(path)
}

#[cfg(windows)]
fn fallback_own_tty() -> Option<PathBuf> {
    // Best-effort only — hook instrumentation stays disabled on Windows until
    // validated against a real Claude Code session there. `CONOUT$` is the
    // Windows analogue of `/dev/tty`: the calling process's own console.
    Some(PathBuf::from("CONOUT$"))
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::PathBuf;

    /// Linux: the parent's controlling tty is (almost always) whatever its
    /// stdio fds point at. Prefer stderr, then stdout, then stdin — mirroring
    /// the original's rationale that a hook's *own* stdout is captured by the
    /// agent, so checking the parent's least-likely-redirected fd first is
    /// the closest native analogue to `ps -o tty=`, which reports the
    /// session's ctty regardless of fd redirection. If none of the parent's
    /// fds are a tty (e.g. all three redirected), this legitimately finds
    /// nothing and `resolve()` falls through to `/dev/tty`.
    pub(super) fn parent_tty(ppid: libc::pid_t) -> Option<PathBuf> {
        for fd in [2, 1, 0] {
            let link = format!("/proc/{ppid}/fd/{fd}");
            if let Ok(target) = std::fs::read_link(&link)
                && let Some(s) = target.to_str()
                && (s.starts_with("/dev/pts/") || s.starts_with("/dev/tty"))
            {
                return Some(target);
            }
        }
        None
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    /// macOS: `proc_pidinfo(PROC_PIDTBSDINFO)` gives `e_tdev` — the parent's
    /// controlling tty as a raw device number (0 or an all-ones sentinel
    /// means "no ctty"). There is no libproc call from device number to path,
    /// so resolve it by scanning `/dev` for the character-special entry whose
    /// `st_rdev` matches — bounded (`/dev` holds on the order of a few
    /// hundred entries) and avoids hand-rolling the legacy `kinfo_proc`/
    /// `sysctl(KERN_PROC_PID)` struct layout, which Apple has changed across
    /// releases; `proc_pidinfo` is the modern, stable libproc API.
    pub(super) fn parent_tty(ppid: libc::pid_t) -> Option<PathBuf> {
        let tdev = parent_tdev(ppid)?;
        find_dev_node_by_rdev(tdev)
    }

    fn parent_tdev(ppid: libc::pid_t) -> Option<u64> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let ret = unsafe {
            libc::proc_pidinfo(
                ppid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&raw mut info).cast::<libc::c_void>(),
                size as libc::c_int,
            )
        };
        if ret as usize != size {
            return None; // process gone, denied, or truncated read
        }
        // e_tdev is a 32-bit dev_t (major/minor packed); 0 and -1 both mean
        // "no controlling terminal" in practice across BSD/XNU.
        let tdev = u64::from(info.e_tdev);
        (tdev != 0 && info.e_tdev != u32::MAX).then_some(tdev)
    }

    fn find_dev_node_by_rdev(target_rdev: u64) -> Option<PathBuf> {
        let entries = std::fs::read_dir("/dev").ok()?;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_char_device() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.rdev() == target_rdev {
                return Some(entry.path());
            }
        }
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    // Any other unix (e.g. BSDs, if this ever builds there): no native
    // parent-tty lookup implemented — falls through to `/dev/tty`.
    pub(super) fn parent_tty(_ppid: libc::pid_t) -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both cases live in one test (rather than two `#[test]` fns) because
    // they mutate the same process-global env var and the test harness runs
    // tests in parallel within a binary — two fns would race each other's
    // set/remove. `#[serial_test::serial]` (bare, no key) additionally
    // serializes against `emit::tests`' two TUIC_HOOK_TTY-touching tests,
    // which mutate the same env var from a different module in this binary.
    #[test]
    #[serial_test::serial]
    fn tuic_hook_tty_env_seam() {
        // SAFETY: test-only, sequential within this fn, restored after each case.
        unsafe { std::env::set_var("TUIC_HOOK_TTY", "/tmp/does-not-need-to-exist") };
        let resolved = resolve();
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        assert_eq!(
            resolved,
            Some(PathBuf::from("/tmp/does-not-need-to-exist")),
            "a set TUIC_HOOK_TTY must win over every other resolution path"
        );

        unsafe { std::env::set_var("TUIC_HOOK_TTY", "") };
        let resolved = resolve();
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        // Falls through to real resolution — just must not be Some("").
        assert_ne!(
            resolved,
            Some(PathBuf::new()),
            "an empty TUIC_HOOK_TTY must be treated as unset, not as a literal empty path"
        );
    }
}
