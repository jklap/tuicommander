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
    }
}
