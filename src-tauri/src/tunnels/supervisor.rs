use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::agent::discover_agent_socket;
use super::backoff::BackoffCalculator;
use super::classifier::{ExitReason, classify_exit};
use super::command::{build_ssh_args, build_ssh_env};
use super::port::check_local_port;
#[cfg(unix)]
use super::port::kill_ssh_on_port;
use super::profile::{ForwardSpec, TunnelProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Starting,
    Connected,
    Reconnecting { attempt: u32, reason: String },
    Stopped { reason: String },
    Error { message: String },
}

pub struct TunnelSupervisor {
    profile: TunnelProfile,
    status: Arc<Mutex<TunnelStatus>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ssh_binary: PathBuf,
}

impl TunnelSupervisor {
    /// Start supervising an SSH tunnel for the given profile.
    ///
    /// `status_callback` is invoked on every status transition from a spawned
    /// tokio task — it must be `Send + 'static`.
    pub async fn start(
        profile: TunnelProfile,
        status_callback: impl Fn(TunnelStatus) + Send + 'static,
    ) -> Self {
        Self::start_with_binary(profile, PathBuf::from("ssh"), status_callback).await
    }

    /// Like `start`, but allows overriding the ssh binary path (for tests).
    pub(crate) async fn start_with_binary(
        mut profile: TunnelProfile,
        ssh_binary: PathBuf,
        status_callback: impl Fn(TunnelStatus) + Send + 'static,
    ) -> Self {
        let status = Arc::new(Mutex::new(TunnelStatus::Starting));

        // Validate profile.
        if let Err(e) = profile.validate() {
            let error_status = TunnelStatus::Error { message: e };
            *status.lock() = error_status.clone();
            status_callback(error_status);
            return Self {
                profile,
                status,
                shutdown_tx: None,
                ssh_binary,
            };
        }

        // Check port availability for all Local forwards.
        // If a port is in use, try to kill orphaned SSH processes holding it.
        for forward in &profile.forwards {
            if let ForwardSpec::Local { bind_port, .. } = forward
                && check_local_port(*bind_port).await.is_err()
            {
                #[cfg(unix)]
                {
                    kill_ssh_on_port(*bind_port).await;
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                if let Err(msg) = check_local_port(*bind_port).await {
                    let error_status = TunnelStatus::Error { message: msg };
                    *status.lock() = error_status.clone();
                    status_callback(error_status);
                    return Self {
                        profile,
                        status,
                        shutdown_tx: None,
                        ssh_binary,
                    };
                }
            }
        }

        status_callback(TunnelStatus::Starting);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let task_status = Arc::clone(&status);
        let task_profile = profile.clone();
        let task_binary = ssh_binary.clone();

        tokio::spawn(async move {
            supervision_loop(
                task_profile,
                task_binary,
                task_status,
                shutdown_rx,
                status_callback,
            )
            .await;
        });

        Self {
            profile,
            status,
            shutdown_tx: Some(shutdown_tx),
            ssh_binary,
        }
    }

    /// Request graceful shutdown of the supervised tunnel.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Return the current tunnel status.
    pub fn status(&self) -> TunnelStatus {
        self.status.lock().clone()
    }
}

fn set_status(status: &Mutex<TunnelStatus>, new: TunnelStatus, callback: &impl Fn(TunnelStatus)) {
    *status.lock() = new.clone();
    callback(new);
}

async fn supervision_loop(
    profile: TunnelProfile,
    ssh_binary: PathBuf,
    status: Arc<Mutex<TunnelStatus>>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    callback: impl Fn(TunnelStatus) + Send + 'static,
) {
    let agent_socket = discover_agent_socket();
    let mut backoff = BackoffCalculator::new();

    loop {
        let args = build_ssh_args(&profile);
        let env = build_ssh_env(agent_socket.as_deref());

        // Build command — skip argv[0] ("ssh") from args since we set the binary separately.
        let mut cmd = Command::new(&ssh_binary);
        cmd.args(&args[1..])
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Retry spawn briefly on transient OS errors (Linux ETXTBSY: race
        // between closing a write fd and execve on the same temp script).
        let mut child = 'spawn: {
            let mut last_err = None;
            for attempt in 0..3u8 {
                match cmd.spawn() {
                    Ok(c) => break 'spawn c,
                    Err(e) if is_retryable_spawn_error(&e) && attempt < 2 => {
                        last_err = Some(e);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
            }
            set_status(
                &status,
                TunnelStatus::Error {
                    message: format!("failed to spawn ssh: {}", last_err.unwrap()),
                },
                &callback,
            );
            return;
        };

        // Drain stderr concurrently on a background task, retaining only a
        // bounded tail for diagnostics. Without this, a chatty ssh fills the
        // OS pipe buffer (64KB on Linux, 16KB on macOS) and blocks on write()
        // forever — the child never exits and the status stays stuck at
        // Connected even though ssh is effectively hung.
        let stderr_tail = child.stderr.take().map(spawn_stderr_drainer);

        // Brief health check — if the process dies within 500ms it never connected.
        let health_check = tokio::time::sleep(Duration::from_millis(500));
        tokio::pin!(health_check);

        let died_early = tokio::select! {
            result = child.wait() => Some(result),
            () = &mut health_check => None,
            _ = &mut shutdown_rx => {
                graceful_kill(&mut child).await;
                set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                return;
            }
        };

        if let Some(wait_result) = died_early {
            // Process died during health check.
            let stderr = stderr_tail_snapshot(&stderr_tail);
            let code = wait_result.ok().and_then(|s| s.code());
            let reason = classify_exit(&stderr, code);
            if handle_exit(&reason, &mut backoff, &status, &callback) {
                // Retryable — wait backoff then loop.
                if let Some(delay) = backoff_delay(&mut backoff) {
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        _ = &mut shutdown_rx => {
                            set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                            return;
                        }
                    }
                } else {
                    set_status(
                        &status,
                        TunnelStatus::Stopped {
                            reason: "max retries exceeded".to_string(),
                        },
                        &callback,
                    );
                    return;
                }
                continue;
            }
            return;
        }

        // Process survived 500ms — consider it connected.
        backoff.reset();
        set_status(&status, TunnelStatus::Connected, &callback);

        // Wait for process exit or shutdown signal.
        let wait_result = tokio::select! {
            result = child.wait() => result,
            _ = &mut shutdown_rx => {
                graceful_kill(&mut child).await;
                set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                return;
            }
        };

        let stderr = stderr_tail_snapshot(&stderr_tail);
        let code = wait_result.ok().and_then(|s| s.code());
        let reason = classify_exit(&stderr, code);

        if handle_exit(&reason, &mut backoff, &status, &callback) {
            // Retryable — wait backoff then loop.
            if let Some(delay) = backoff_delay(&mut backoff) {
                tokio::select! {
                    () = tokio::time::sleep(delay) => {}
                    _ = &mut shutdown_rx => {
                        set_status(&status, TunnelStatus::Stopped { reason: "shutdown requested".to_string() }, &callback);
                        return;
                    }
                }
            } else {
                set_status(
                    &status,
                    TunnelStatus::Stopped {
                        reason: "max retries exceeded".to_string(),
                    },
                    &callback,
                );
                return;
            }
            continue;
        }

        // Non-retryable — already set by handle_exit.
        return;
    }
}

/// Returns `true` if the exit is retryable (caller should loop), `false` if
/// the supervisor should stop. Updates status accordingly.
fn handle_exit(
    reason: &ExitReason,
    backoff: &mut BackoffCalculator,
    status: &Mutex<TunnelStatus>,
    callback: &impl Fn(TunnelStatus),
) -> bool {
    if reason.is_retryable() {
        let attempt = backoff.attempts() + 1;
        let reason_str = format!("{reason:?}");
        set_status(
            status,
            TunnelStatus::Reconnecting {
                attempt,
                reason: reason_str,
            },
            callback,
        );
        true
    } else {
        let reason_str = format!("{reason:?}");
        set_status(
            status,
            TunnelStatus::Stopped { reason: reason_str },
            callback,
        );
        false
    }
}

/// Get the next backoff delay, or `None` if retries are exhausted.
fn backoff_delay(backoff: &mut BackoffCalculator) -> Option<Duration> {
    backoff.next_delay()
}

/// Cap on the retained stderr tail, in bytes. Diagnostics only need the most
/// recent output (e.g. the auth-failure or connection-refused message), not
/// the full chatty stream.
const STDERR_TAIL_LIMIT: usize = 8192;

/// Spawn a background task that continuously drains the child's stderr pipe
/// for as long as the process runs, keeping only a bounded tail.
///
/// This must run concurrently with the process, not after it exits: ssh's
/// stderr is a pipe with a small OS buffer (64KB on Linux, 16KB on macOS).
/// A chatty process fills it and blocks on write() until someone reads —
/// reading only after `child.wait()` returns means nobody ever reads while
/// the process is alive, so it can block forever and never exit.
fn spawn_stderr_drainer(mut stderr: tokio::process::ChildStderr) -> Arc<Mutex<Vec<u8>>> {
    let tail = Arc::new(Mutex::new(Vec::new()));
    let task_tail = Arc::clone(&tail);
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break, // EOF — pipe closed (process exited).
                Ok(n) => {
                    let mut guard = task_tail.lock();
                    guard.extend_from_slice(&buf[..n]);
                    if guard.len() > STDERR_TAIL_LIMIT {
                        let excess = guard.len() - STDERR_TAIL_LIMIT;
                        guard.drain(0..excess);
                    }
                }
                Err(e) => {
                    tracing::warn!(source = "tunnel_supervisor", error = %e, "Failed to read ssh stderr");
                    break;
                }
            }
        }
    });
    tail
}

/// Snapshot whatever stderr tail has been drained so far, for diagnostics.
fn stderr_tail_snapshot(tail: &Option<Arc<Mutex<Vec<u8>>>>) -> String {
    match tail {
        Some(tail) => String::from_utf8_lossy(&tail.lock()).into_owned(),
        None => String::new(),
    }
}

/// Send SIGTERM, wait 5s, escalate to SIGKILL.
async fn graceful_kill(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        match i32::try_from(id) {
            Ok(pid) => {
                // SAFETY: `pid` is from `child.id()` which returns the OS PID of
                // a child process we spawned and have not yet waited on.
                // SIGTERM has no preconditions beyond a valid PID.
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
            Err(_) => {
                tracing::warn!(
                    source = "tunnel_supervisor",
                    raw_pid = id,
                    "PID overflows i32, escalating to SIGKILL"
                );
                let _ = child.kill().await;
                return;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill().await;
    }

    // Wait up to 5s for clean exit after SIGTERM, then escalate.
    #[cfg(unix)]
    tokio::select! {
        _ = child.wait() => {}
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            let _ = child.kill().await;
        }
    }
}

/// ETXTBSY (26 on Linux) — exec on a file still open for writing.
fn is_retryable_spawn_error(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::ETXTBSY)
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::path::Path;
    use tokio::net::TcpListener;

    /// Env var that makes a fake ssh script exit before running its behavior.
    /// Set only by the warm-up exec in [`fake_ssh_script`]; the supervisor
    /// never sets it, so a supervised spawn always runs the real behavior.
    const WARMUP_VAR: &str = "TUIC_FAKE_SSH_WARMUP";

    /// Write a fake ssh script to a **stable, reused** path and make sure the OS
    /// has already vetted it for execution.
    ///
    /// The reuse is the point, and it is not a micro-optimisation. On a machine
    /// with exec-time code scanning (macOS `syspolicyd` plus an endpoint-security
    /// agent) the *first* exec of a freshly written executable blocks while it is
    /// scanned — measured here at 6s to 102s, in every directory tried, with no
    /// relation to test-suite load. Every later exec of the *same* file is ~6ms.
    /// A per-run temp file therefore paid that scan inside the test's own timing
    /// window, on every run, and the four supervisor tests failed whenever the
    /// scan outlasted their poll bound — reproducibly, with the suite otherwise
    /// idle. Keying the file by test name makes the scan a one-off per machine.
    ///
    /// The warm-up exec below pays that one-off *before* the caller starts a
    /// supervisor, so no assertion ever races the scanner. It only runs when the
    /// file was actually created or rewritten; an unchanged file is already
    /// vetted, so the whole helper costs a read and a compare.
    ///
    /// `name` must be unique per behavior — it is the cache key. The content is
    /// compared on every call, so editing a behavior rewrites (and re-warms) the
    /// script instead of silently reusing the old one.
    fn fake_ssh_script(name: &str, behavior: &str) -> PathBuf {
        // Under `target/`, so it is gitignored and survives between runs.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/fake-ssh");
        std::fs::create_dir_all(&dir).expect("create fake-ssh dir");
        let path = dir.join(format!("{name}.sh"));
        let desired = format!("#!/bin/sh\n[ -n \"${WARMUP_VAR}\" ] && exit 0\n{behavior}\n");

        if std::fs::read_to_string(&path).is_ok_and(|found| found == desired) {
            return path;
        }

        // Write beside the target and rename over it, so a second run of this
        // test never execs a half-written script.
        let staging = dir.join(format!("{name}.sh.{}", std::process::id()));
        std::fs::write(&staging, &desired).expect("write fake ssh script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
                .expect("chmod fake ssh script");
        }
        std::fs::rename(&staging, &path).expect("install fake ssh script");

        let _ = std::process::Command::new(&path)
            .env(WARMUP_VAR, "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        path
    }

    fn test_profile() -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test-tunnel".to_string(),
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            identity_file: None,
            forwards: Vec::new(),
            options: super::super::profile::ProfileOptions::default(),
            auto_connect: false,
        }
    }

    /// Collect statuses via a shared vec behind Arc<Mutex<_>>.
    fn status_collector() -> (
        impl Fn(TunnelStatus) + Send + 'static,
        Arc<Mutex<Vec<TunnelStatus>>>,
    ) {
        let statuses: Arc<Mutex<Vec<TunnelStatus>>> = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&statuses);
        let cb = move |st: TunnelStatus| {
            s.lock().push(st);
        };
        (cb, statuses)
    }

    /// Poll until the supervisor settles on `Stopped`.
    ///
    /// The bound covers the supervisor's own state machine and nothing else:
    /// the 500ms health check, the child's exit, and at worst the first two
    /// backoffs (~1s and ~2s). It is not sized for process startup — that cost
    /// is paid up front by [`fake_ssh_script`], deliberately, because it is the
    /// one term here that the OS can stretch without limit. Keep it that way:
    /// if this bound ever needs raising, the cause is a supervisor change or a
    /// new unwarmed executable, not a busy machine.
    async fn wait_for_stopped(supervisor: &TunnelSupervisor) -> TunnelStatus {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut status = supervisor.status();
        while !matches!(status, TunnelStatus::Stopped { .. })
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
            status = supervisor.status();
        }
        status
    }

    #[tokio::test]
    async fn spawn_clean_exit() {
        let script = fake_ssh_script("spawn_clean_exit", "sleep 0.2; exit 0");
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // Poll for the terminal state rather than sampling once after a fixed
        // sleep. The script exits at ~0.2s, but whether that lands inside the
        // 500ms health check is genuinely racy, so Stopped may arrive directly
        // or via Connected — both are correct and only the terminal state is.
        let final_status = wait_for_stopped(&sup).await;

        let history = statuses.lock().clone();
        // Should see Starting, then either Connected or Stopped (process exits
        // quickly — if it exits before 500ms health check, it goes straight to
        // Stopped; if after, Connected then Stopped).
        assert!(!history.is_empty(), "should have status updates");

        // Final status should be Stopped with a non-error reason.
        match &final_status {
            TunnelStatus::Stopped { .. } => {} // expected
            other => panic!("expected Stopped, got {other:?}"),
        }

        sup.stop(); // idempotent
    }

    #[tokio::test]
    async fn auth_failure_no_retry() {
        let script = fake_ssh_script(
            "auth_failure_no_retry",
            r#"echo "Permission denied (publickey)." >&2; exit 255"#,
        );
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // The script exits immediately, but the stderr drainer has to deliver
        // "Permission denied" before classify_exit can call it AuthFailed. Wait
        // for the terminal state instead of sampling at a fixed offset.
        let final_status = wait_for_stopped(&sup).await;

        let history = statuses.lock().clone();

        // Must NOT contain Reconnecting — auth failures are not retryable.
        let has_reconnecting = history
            .iter()
            .any(|s| matches!(s, TunnelStatus::Reconnecting { .. }));
        assert!(
            !has_reconnecting,
            "auth failure should not trigger reconnect, history: {history:?}"
        );

        // Final status should be Stopped with AuthFailed reason.
        match &final_status {
            TunnelStatus::Stopped { reason } => {
                assert!(
                    reason.contains("AuthFailed"),
                    "reason should mention AuthFailed, got: {reason}"
                );
            }
            other => panic!("expected Stopped, got {other:?}"),
        }

        sup.stop();
    }

    #[tokio::test]
    async fn network_error_retries() {
        // Script that prints "Connection refused" and exits — supervisor should retry.
        let script = fake_ssh_script(
            "network_error_retries",
            r#"echo "ssh: connect to host example.com port 22: Connection refused" >&2; exit 255"#,
        );
        let (cb, statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // The first two backoffs are roughly one and two seconds, and each
        // retry re-execs the script, so poll the observed transitions rather
        // than sleeping for a total nobody can predict exactly.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while statuses
            .lock()
            .iter()
            .filter(|status| matches!(status, TunnelStatus::Reconnecting { .. }))
            .count()
            < 2
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let history = statuses.lock().clone();

        // Should contain at least one Reconnecting status.
        let reconnect_count = history
            .iter()
            .filter(|s| matches!(s, TunnelStatus::Reconnecting { .. }))
            .count();
        assert!(
            reconnect_count >= 2,
            "expected at least 2 reconnect attempts, got {reconnect_count}, history: {history:?}"
        );

        // Verify attempt numbers increase.
        let attempts: Vec<u32> = history
            .iter()
            .filter_map(|s| {
                if let TunnelStatus::Reconnecting { attempt, .. } = s {
                    Some(*attempt)
                } else {
                    None
                }
            })
            .collect();
        for window in attempts.windows(2) {
            assert!(
                window[1] > window[0],
                "attempt numbers should increase: {attempts:?}"
            );
        }

        sup.stop();
    }

    #[tokio::test]
    async fn graceful_shutdown() {
        // Script that sleeps forever.
        let script = fake_ssh_script("graceful_shutdown", "sleep 3600");
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        // Wait for health check to pass.
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Should be Connected.
        assert_eq!(sup.status(), TunnelStatus::Connected);

        // Request shutdown.
        sup.stop();

        // The implementation has a five-second grace period before it escalates
        // to SIGKILL; poll across that boundary rather than guessing which side
        // of it the child exits on.
        let final_status = wait_for_stopped(&sup).await;
        match &final_status {
            TunnelStatus::Stopped { reason } => {
                assert!(
                    reason.contains("shutdown"),
                    "reason should mention shutdown, got: {reason}"
                );
            }
            other => panic!("expected Stopped after shutdown, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn port_in_use_error_before_spawn() {
        // Bind a port so it's occupied.
        let addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let listener = TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let mut profile = test_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: port,
            remote_host: "remote.example.com".to_string(),
            remote_port: 80,
        }];

        let script = fake_ssh_script("port_in_use_error_before_spawn", "exit 0");
        let (cb, _statuses) = status_collector();

        let sup = TunnelSupervisor::start_with_binary(profile, script.to_path_buf(), cb).await;

        // Should immediately be in Error state — no spawn.
        let status = sup.status();
        match &status {
            TunnelStatus::Error { message } => {
                assert!(
                    message.contains("already in use"),
                    "message should mention port in use, got: {message}"
                );
            }
            other => panic!("expected Error for port in use, got {other:?}"),
        }

        drop(listener); // release the port
    }

    #[tokio::test]
    async fn chatty_stderr_does_not_stall() {
        // Emit >64KB of stderr, then exit cleanly. If stderr isn't drained
        // concurrently while the process runs, the OS pipe buffer (64KB on
        // Linux, 16KB on macOS) fills, the child blocks forever on write(),
        // and the tunnel never reaches Stopped — it stalls at Connected.
        let script = fake_ssh_script(
            "chatty_stderr_does_not_stall",
            "yes x | head -c 100000 1>&2; exit 0",
        );
        let (cb, _statuses) = status_collector();

        let mut sup =
            TunnelSupervisor::start_with_binary(test_profile(), script.to_path_buf(), cb).await;

        let final_status = wait_for_stopped(&sup).await;

        match &final_status {
            TunnelStatus::Stopped { .. } => {} // expected: exited promptly, no stall
            other => {
                panic!("expected Stopped (chatty stderr must not stall the tunnel), got {other:?}")
            }
        }

        sup.stop();
    }
}
