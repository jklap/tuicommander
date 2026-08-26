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
//! 2. `$TUIC_PTY_TTY` — the pty device path TUICommander stamps onto every
//!    child it spawns (`src-tauri/src/pty.rs::spawn_pty_pair_with_retry`).
//!    This is the primary mechanism, not a fallback: TUICommander already
//!    knows the exact device, so there's nothing to infer. It exists because
//!    ancestry-walking cannot work at all for a TUICommander-spawned agent —
//!    see the note on step 3 below.
//! 3. The parent process's controlling tty (platform-specific; see below).
//!    This only matters for agents *not* spawned by TUICommander (or an old
//!    binary that predates step 2). Empirically confirmed this walk is
//!    currently one hop too shallow for Claude Code specifically: Claude Code
//!    spawns the hook's own immediate parent shell already detached from any
//!    controlling terminal, so `getppid()` from the hook lands on that
//!    detached shell rather than on `claude` itself, which does have one.
//! 4. `/dev/tty` — this process's own controlling tty, if it has one. Mirrors
//!    the original shell fallback (`case … *) __t="/dev/tty" ;; esac`), which
//!    fires when the parent-tty lookup comes back empty. Note `/dev/tty`
//!    typically *exists* as a device node even in a fully detached process, so
//!    this step routinely returns `Some` — it's the later `open()` in
//!    `emit.rs` that fails (`ENXIO`), not this resolution step.
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

    if let Ok(path) = std::env::var("TUIC_PTY_TTY")
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

/// Bound on how many ancestors [`resolve_via_ancestors`] will walk. One hop
/// covers the Claude Code case measured directly (hook → detached shell →
/// `claude`, which has the ctty); a few extra hops give headroom for an
/// agent that wraps its hook invocation in more layers without letting a
/// misbehaving `ppid_of` (e.g. a pid that reports itself as its own parent)
/// spin forever.
#[cfg(unix)]
const MAX_ANCESTOR_HOPS: usize = 4;

/// Walk upward from `start_pid` through parent pids, stopping at the first
/// ancestor that has a controlling tty. `ctty_of` and `ppid_of` are injected
/// so this walk is unit-testable against a fake process table — no real
/// process inspection happens in this function.
///
/// Stops after [`MAX_ANCESTOR_HOPS`], when `ppid_of` returns `None` (no
/// further ancestor, or permission denied), or if a pid repeats (a cycle,
/// which a real process tree should never produce, but a fake or adversarial
/// `ppid_of` might).
#[cfg(unix)]
fn resolve_via_ancestors(
    start_pid: i32,
    ctty_of: impl Fn(i32) -> Option<PathBuf>,
    ppid_of: impl Fn(i32) -> Option<i32>,
) -> Option<PathBuf> {
    let mut pid = start_pid;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..MAX_ANCESTOR_HOPS {
        if !seen.insert(pid) {
            return None;
        }
        if let Some(path) = ctty_of(pid) {
            return Some(path);
        }
        pid = ppid_of(pid)?;
    }
    None
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::PathBuf;

    /// Linux: a process's controlling tty is (almost always) whatever its
    /// stdio fds point at. Prefer stderr, then stdout, then stdin — mirroring
    /// the original's rationale that a hook's *own* stdout is captured by the
    /// agent, so checking the least-likely-redirected fd first is the closest
    /// native analogue to `ps -o tty=`, which reports the session's ctty
    /// regardless of fd redirection.
    fn ctty_of(pid: i32) -> Option<PathBuf> {
        for fd in [2, 1, 0] {
            let link = format!("/proc/{pid}/fd/{fd}");
            if let Ok(target) = std::fs::read_link(&link)
                && let Some(s) = target.to_str()
                && (s.starts_with("/dev/pts/") || s.starts_with("/dev/tty"))
            {
                return Some(target);
            }
        }
        None
    }

    /// `/proc/<pid>/status`'s `PPid:` field. Preferred over parsing
    /// `/proc/<pid>/stat`'s positional 4th field, which requires skipping a
    /// `(comm)` block that can itself contain spaces or parens.
    fn ppid_of(pid: i32) -> Option<i32> {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|rest| rest.trim().parse().ok())
    }

    /// Walk ancestors starting at `ppid` (the hook's immediate parent) for the
    /// first one with a controlling tty. If none of the parent's own fds are
    /// a tty (e.g. all three redirected) and no ancestor has one either, this
    /// legitimately finds nothing and `resolve()` falls through to `/dev/tty`.
    pub(super) fn parent_tty(ppid: libc::pid_t) -> Option<PathBuf> {
        super::resolve_via_ancestors(ppid, ctty_of, ppid_of)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::PathBuf;

    /// One `proc_pidinfo(PROC_PIDTBSDINFO)` call yields both the fields the
    /// ancestor walk needs (`e_tdev`, `pbi_ppid`), so fetch it once per hop
    /// rather than making two syscalls where one suffices.
    fn bsdinfo_of(pid: libc::pid_t) -> Option<libc::proc_bsdinfo> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let ret = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&raw mut info).cast::<libc::c_void>(),
                size as libc::c_int,
            )
        };
        (ret as usize == size).then_some(info) // else: process gone, denied, or truncated read
    }

    /// `e_tdev` is the process's controlling tty as a raw device number (0 or
    /// an all-ones sentinel means "no ctty"). There is no libproc call from
    /// device number to path, so resolve it by scanning `/dev` for the
    /// character-special entry whose `st_rdev` matches — bounded (`/dev`
    /// holds on the order of a few hundred entries) and avoids hand-rolling
    /// the legacy `kinfo_proc`/`sysctl(KERN_PROC_PID)` struct layout, which
    /// Apple has changed across releases; `proc_pidinfo` is the modern,
    /// stable libproc API.
    fn ctty_of(pid: i32) -> Option<PathBuf> {
        let info = bsdinfo_of(pid as libc::pid_t)?;
        let tdev = u64::from(info.e_tdev);
        let tdev = (tdev != 0 && info.e_tdev != u32::MAX).then_some(tdev)?;
        find_dev_node_by_rdev(tdev)
    }

    fn ppid_of(pid: i32) -> Option<i32> {
        let info = bsdinfo_of(pid as libc::pid_t)?;
        let ppid = info.pbi_ppid as i32;
        (ppid > 0).then_some(ppid)
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

    /// Walk ancestors starting at `ppid` (the hook's immediate parent) for the
    /// first one with a controlling tty.
    pub(super) fn parent_tty(ppid: libc::pid_t) -> Option<PathBuf> {
        super::resolve_via_ancestors(ppid, ctty_of, ppid_of)
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

    /// `TUIC_PTY_TTY` — the primary resolution mechanism (F1) — sits between
    /// the `TUIC_HOOK_TTY` test seam and the ancestor walk. Same env-mutation
    /// hazard as the seam above, so this joins the same bare serial group
    /// (see the module-level rationale on `tuic_hook_tty_env_seam`).
    #[test]
    #[serial_test::serial]
    fn tuic_pty_tty_env_seam() {
        // Used when set and TUIC_HOOK_TTY is unset.
        unsafe { std::env::set_var("TUIC_PTY_TTY", "/tmp/pty-tty-does-not-need-to-exist") };
        let resolved = resolve();
        unsafe { std::env::remove_var("TUIC_PTY_TTY") };
        assert_eq!(
            resolved,
            Some(PathBuf::from("/tmp/pty-tty-does-not-need-to-exist")),
            "a set TUIC_PTY_TTY must be used when there is no TUIC_HOOK_TTY override"
        );

        // Empty string is treated as unset, same rule as TUIC_HOOK_TTY.
        unsafe { std::env::set_var("TUIC_PTY_TTY", "") };
        let resolved = resolve();
        unsafe { std::env::remove_var("TUIC_PTY_TTY") };
        assert_ne!(
            resolved,
            Some(PathBuf::new()),
            "an empty TUIC_PTY_TTY must be treated as unset, not as a literal empty path"
        );

        // TUIC_HOOK_TTY still wins when both are set.
        unsafe { std::env::set_var("TUIC_PTY_TTY", "/tmp/pty-tty-loses") };
        unsafe { std::env::set_var("TUIC_HOOK_TTY", "/tmp/hook-tty-wins") };
        let resolved = resolve();
        unsafe { std::env::remove_var("TUIC_PTY_TTY") };
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        assert_eq!(
            resolved,
            Some(PathBuf::from("/tmp/hook-tty-wins")),
            "the TUIC_HOOK_TTY test seam must still win over TUIC_PTY_TTY"
        );
    }

    /// [`resolve_via_ancestors`] against a fake process table — no real
    /// process inspection, so these cover the walk logic deterministically on
    /// every platform/CI environment regardless of what ctty (if any) the
    /// test runner itself happens to have.
    #[cfg(unix)]
    mod ancestor_walk {
        use super::*;
        use std::collections::HashMap;

        /// A fake process table: pid -> (ctty, ppid). Missing entries have no
        /// further ancestor (`ppid_of` returns `None`).
        struct FakeTable(HashMap<i32, (Option<PathBuf>, Option<i32>)>);

        impl FakeTable {
            fn ctty_of(&self, pid: i32) -> Option<PathBuf> {
                self.0.get(&pid).and_then(|(ctty, _)| ctty.clone())
            }
            fn ppid_of(&self, pid: i32) -> Option<i32> {
                self.0.get(&pid).and_then(|(_, ppid)| *ppid)
            }
        }

        #[test]
        fn finds_a_ctty_on_the_immediate_parent() {
            let table = FakeTable(HashMap::from([(
                1,
                (Some(PathBuf::from("/dev/ttys000")), None),
            )]));
            let resolved = resolve_via_ancestors(1, |p| table.ctty_of(p), |p| table.ppid_of(p));
            assert_eq!(resolved, Some(PathBuf::from("/dev/ttys000")));
        }

        /// The measured real-world case this whole fix exists for: the
        /// immediate parent (the hook shell Claude Code spawns) is detached,
        /// but its own parent (`claude`) has a ctty.
        #[test]
        fn steps_past_a_detached_parent_to_a_grandparent_with_a_ctty() {
            let table = FakeTable(HashMap::from([
                (1, (None, Some(2))),                             // detached shell
                (2, (Some(PathBuf::from("/dev/ttys000")), None)), // claude
            ]));
            let resolved = resolve_via_ancestors(1, |p| table.ctty_of(p), |p| table.ppid_of(p));
            assert_eq!(resolved, Some(PathBuf::from("/dev/ttys000")));
        }

        #[test]
        fn returns_none_when_no_ancestor_within_reach_has_a_ctty() {
            let table = FakeTable(HashMap::from([
                (1, (None, Some(2))),
                (2, (None, Some(3))),
                (3, (None, None)), // chain ends, still nothing
            ]));
            let resolved = resolve_via_ancestors(1, |p| table.ctty_of(p), |p| table.ppid_of(p));
            assert_eq!(resolved, None);
        }

        #[test]
        fn stops_at_the_hop_bound_rather_than_walking_forever() {
            // A ctty exists, but one hop past MAX_ANCESTOR_HOPS — must not be found.
            let mut entries = HashMap::new();
            for pid in 1..=(MAX_ANCESTOR_HOPS as i32 + 1) {
                entries.insert(pid, (None, Some(pid + 1)));
            }
            entries.insert(
                MAX_ANCESTOR_HOPS as i32 + 2,
                (Some(PathBuf::from("/dev/ttys000")), None),
            );
            let table = FakeTable(entries);
            let resolved = resolve_via_ancestors(1, |p| table.ctty_of(p), |p| table.ppid_of(p));
            assert_eq!(
                resolved, None,
                "a ctty beyond MAX_ANCESTOR_HOPS must not be found"
            );
        }

        #[test]
        fn a_ppid_cycle_terminates_instead_of_looping_forever() {
            // pid 1 -> 2 -> 1 -> ... ; neither has a ctty. A naive walk with
            // no cycle guard would spin until the hop bound anyway (still
            // bounded), but this pins the *earlier*, explicit exit.
            let table = FakeTable(HashMap::from([(1, (None, Some(2))), (2, (None, Some(1)))]));
            let resolved = resolve_via_ancestors(1, |p| table.ctty_of(p), |p| table.ppid_of(p));
            assert_eq!(resolved, None);
        }
    }
}
