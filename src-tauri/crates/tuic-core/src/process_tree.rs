//! Signalling a spawned child together with everything it started.
//!
//! Killing only the direct child of a `sh -c` script orphans whatever the
//! script started (`npm install`'s own workers, a backgrounded job): the
//! shell dies, its children are reparented and keep running. The fix is to
//! start the child as the leader of its own process group
//! (`CommandExt::process_group(0)`) and signal the GROUP; on Windows, to end
//! the process tree with `taskkill /T`.

/// Send SIGTERM to every process in group `pgid`. `false` when nothing was
/// signalled (no such group, or `pgid` is not a safe target).
#[cfg(unix)]
pub fn terminate_process_group(pgid: u32) -> bool {
    signal_process_group(pgid, libc::SIGTERM)
}

/// Send SIGKILL to every process in group `pgid`.
#[cfg(unix)]
pub fn kill_process_group(pgid: u32) -> bool {
    signal_process_group(pgid, libc::SIGKILL)
}

/// Whether group `pgid` still has a member (signal 0 probes, delivers nothing).
#[cfg(unix)]
pub fn process_group_exists(pgid: u32) -> bool {
    signal_process_group(pgid, 0)
}

#[cfg(unix)]
fn signal_process_group(pgid: u32, signal: libc::c_int) -> bool {
    let Ok(pgid) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    // 0 is the caller's own group and 1 is init's: never a child we spawned.
    if pgid <= 1 {
        return false;
    }
    // SAFETY: kill(2) with a negative pid (a process group) and a plain signal
    // number reads and writes no memory.
    unsafe { libc::kill(-pgid, signal) == 0 }
}

/// End `pid` and every process it started (`taskkill /T /F`). Best effort.
#[cfg(windows)]
pub fn kill_process_tree(pid: u32) -> bool {
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::cli::apply_no_window(&mut cmd);
    cmd.status().is_ok_and(|status| status.success())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn unsafe_group_ids_are_never_signalled() {
        assert!(!terminate_process_group(0));
        assert!(!kill_process_group(1));
        assert!(!kill_process_group(u32::MAX));
        assert!(!process_group_exists(0));
        assert!(!process_group_exists(1));
    }

    // Catches: the probe reporting a live group as gone (the supervisor would
    // stop waiting and skip the SIGKILL) or a dead one as live.
    #[test]
    fn group_probe_tracks_a_real_group_until_it_is_killed() {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .process_group(0)
            .spawn()
            .expect("spawn sh");
        let pgid = child.id();
        assert!(process_group_exists(pgid));
        assert!(kill_process_group(pgid));
        child.wait().expect("reap");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while process_group_exists(pgid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!process_group_exists(pgid));
    }
}
