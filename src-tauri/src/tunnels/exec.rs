use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::agent::discover_agent_socket;
use super::classifier::{ExitReason, classify_exit};
use super::command::{
    build_ssh_base_args, build_ssh_env, build_ssh_test_args, ensure_ssh_control_dir,
};
use super::profile::TunnelProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) code: Option<i32>,
}

pub(crate) async fn ssh_exec(
    profile: &TunnelProfile,
    remote_command: &str,
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<ExecOutput, ExitReason> {
    ssh_exec_with_binary(profile, remote_command, stdin, timeout, Path::new("ssh")).await
}

pub(crate) async fn ssh_exec_with_binary(
    profile: &TunnelProfile,
    remote_command: &str,
    stdin: Option<&[u8]>,
    timeout: Duration,
    ssh_binary: &Path,
) -> Result<ExecOutput, ExitReason> {
    let mut args = build_ssh_base_args(profile);
    args.push("-T".to_string());
    args.push("--".to_string());
    args.push(format!("{}@{}", profile.ssh.user, profile.ssh.host));
    args.push(remote_command.to_string());

    run_process(ssh_binary, &args, stdin, timeout).await
}

/// Test Connection's one-shot SSH check (`command::build_ssh_test_args`):
/// connect, authenticate, run `true`, exit — through the same runner, agent
/// socket and exit classification as every other one-shot ssh here.
pub(crate) async fn ssh_check(
    ssh: &crate::ssh_connection::SshConnectionParams,
    timeout: Duration,
) -> Result<(), ExitReason> {
    ssh_check_with_binary(ssh, timeout, Path::new("ssh")).await
}

pub(crate) async fn ssh_check_with_binary(
    ssh: &crate::ssh_connection::SshConnectionParams,
    timeout: Duration,
    ssh_binary: &Path,
) -> Result<(), ExitReason> {
    run_process(ssh_binary, &build_ssh_test_args(ssh), None, timeout)
        .await
        .map(|_| ())
}

pub(crate) async fn scp_push(
    profile: &TunnelProfile,
    local: &Path,
    remote: &str,
    timeout: Duration,
) -> Result<(), ExitReason> {
    scp_push_with_binaries(
        profile,
        local,
        remote,
        timeout,
        Path::new("scp"),
        Path::new("ssh"),
    )
    .await
}

/// `user@host:path` for scp. scp splits the host from the path at the first
/// `:`, so an IPv6 host (the only host `SshConnectionParams::validate` lets
/// contain `:`) is bracketed: `user@[::1]:path`. ssh itself takes the bare
/// address, so only scp's destination brackets it.
fn scp_destination(user: &str, host: &str, path: &str) -> String {
    if crate::ssh_connection::is_ipv6_literal(host) {
        format!("{user}@[{host}]:{path}")
    } else {
        format!("{user}@{host}:{path}")
    }
}

pub(crate) async fn scp_push_with_binaries(
    profile: &TunnelProfile,
    local: &Path,
    remote: &str,
    timeout: Duration,
    scp_binary: &Path,
    ssh_binary: &Path,
) -> Result<(), ExitReason> {
    validate_remote_path(remote)?;
    let staged = format!("{remote}.tmp-{}", uuid::Uuid::new_v4());
    let mut args = build_ssh_base_args(profile);
    args.push("--".to_string());
    args.push(local.to_string_lossy().into_owned());
    args.push(scp_destination(
        &profile.ssh.user,
        &profile.ssh.host,
        &staged,
    ));

    run_process(scp_binary, &args, None, timeout).await?;

    let command = format!(
        "mv -f {} {} && chmod 755 {}",
        shell_path(&staged),
        shell_path(remote),
        shell_path(remote)
    );
    ssh_exec_with_binary(profile, &command, None, timeout, ssh_binary).await?;
    Ok(())
}

/// How long a timed-out one-shot's process group gets after SIGTERM before it
/// is SIGKILLed, and how long the reap of the direct child may take after
/// that. Both bound a cleanup step, never the behaviour under test.
const KILL_GRACE: Duration = Duration::from_secs(1);
const REAP_BOUND: Duration = Duration::from_secs(2);

/// Signals a one-shot's whole process group (Windows: its process tree) when
/// dropped while still armed — the cancellation path, where `run_process`'s
/// future is dropped mid-wait (an HTTP request torn down, an outer
/// `tokio::time::timeout`). `kill_on_drop` alone ends only the direct child,
/// so a `sh`/`ssh` that forked (a `ProxyCommand`, a backgrounded job) would
/// leave that grandchild running: the `sleep 3600` leak class in
/// src-tauri/AGENTS.md "Killing a Child Process".
struct ProcessGroupGuard(Option<u32>);

impl ProcessGroupGuard {
    /// The direct child has been reaped; only a straggler could remain, and
    /// the normal-exit path does not own those.
    fn disarm(&mut self) {
        self.0 = None;
    }

    fn kill_now(&mut self) {
        if let Some(pid) = self.0.take() {
            #[cfg(unix)]
            tuic_core::process_tree::kill_process_group(pid);
            #[cfg(windows)]
            tuic_core::process_tree::kill_process_tree(pid);
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.kill_now();
    }
}

/// End a timed-out one-shot and everything it started, then confirm the reap
/// of the direct child — every wait bounded. Unix: SIGTERM the group, a short
/// grace, SIGKILL the group. Windows: `taskkill /T /F`.
async fn kill_one_shot(child: &mut tokio::process::Child, guard: &mut ProcessGroupGuard) {
    #[cfg(unix)]
    if let Some(pid) = guard.0 {
        tuic_core::process_tree::terminate_process_group(pid);
        if tokio::time::timeout(KILL_GRACE, child.wait())
            .await
            .is_err()
        {
            tracing::debug!(
                source = "tunnel_exec",
                pid,
                "one-shot ignored SIGTERM; killing its process group"
            );
        }
    }
    // SIGKILL whatever is left of the group even when the leader exited on
    // SIGTERM: a member that ignores SIGTERM would otherwise survive it.
    guard.kill_now();
    let _ = child.start_kill();
    if tokio::time::timeout(REAP_BOUND, child.wait())
        .await
        .is_err()
    {
        tracing::warn!(
            source = "tunnel_exec",
            "timed-out one-shot was not reaped within the bound"
        );
    }
}

async fn run_process(
    binary: &Path,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<ExecOutput, ExitReason> {
    ensure_ssh_control_dir().map_err(|error| {
        ExitReason::Unknown(format!("failed to create SSH control directory: {error}"))
    })?;
    let agent_socket = discover_agent_socket();
    let env = build_ssh_env(agent_socket.as_deref());

    let mut command = Command::new(binary);
    command
        .args(args)
        .envs(
            env.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Its own process group, so a timeout or a cancellation can signal
    // everything it started, not just this one PID.
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn().map_err(|error| {
        ExitReason::Unknown(format!("failed to spawn {}: {error}", binary.display()))
    })?;
    let mut guard = ProcessGroupGuard(child.id());
    let deadline = tokio::time::Instant::now() + timeout;

    let writer = if let Some(payload) = stdin {
        let mut pipe = child
            .stdin
            .take()
            .ok_or_else(|| ExitReason::Unknown("ssh stdin pipe was not created".to_string()))?;
        let payload = payload.to_vec();
        Some(tokio::spawn(async move {
            pipe.write_all(&payload).await?;
            pipe.shutdown().await
        }))
    } else {
        None
    };
    let mut stdout_reader = drain(child.stdout.take());
    let mut stderr_reader = drain(child.stderr.take());
    let abort_all = |writer: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
                     stdout: &tokio::task::JoinHandle<Vec<u8>>,
                     stderr: &tokio::task::JoinHandle<Vec<u8>>| {
        if let Some(writer) = writer {
            writer.abort();
        }
        stdout.abort();
        stderr.abort();
    };

    let status = match tokio::time::timeout_at(deadline, child.wait()).await {
        Ok(result) => result.map_err(|error| {
            ExitReason::Unknown(format!("failed to wait for {}: {error}", binary.display()))
        })?,
        Err(_) => {
            kill_one_shot(&mut child, &mut guard).await;
            abort_all(writer, &stdout_reader, &stderr_reader);
            return Err(ExitReason::Timeout);
        }
    };

    // The pipes close once every process holding them is gone. A straggler
    // the one-shot left behind can keep them open past the deadline; it is
    // part of the group we started, so it is ended rather than waited on.
    let drained = tokio::time::timeout_at(deadline, async {
        let stdout = (&mut stdout_reader).await.unwrap_or_default();
        let stderr = (&mut stderr_reader).await.unwrap_or_default();
        (stdout, stderr)
    })
    .await;
    let (stdout, stderr) = match drained {
        Ok(output) => {
            guard.disarm();
            output
        }
        Err(_) => {
            guard.kill_now();
            abort_all(writer, &stdout_reader, &stderr_reader);
            return Err(ExitReason::Timeout);
        }
    };

    if let Some(writer) = writer {
        writer
            .await
            .map_err(|error| ExitReason::Unknown(format!("stdin writer failed: {error}")))?
            .map_err(|error| ExitReason::Unknown(format!("failed to write ssh stdin: {error}")))?;
    }

    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    let code = status.code();
    if !status.success() {
        return Err(classify_exit(&stderr, code));
    }

    Ok(ExecOutput {
        stdout,
        stderr,
        code,
    })
}

/// Read a child pipe to EOF on its own task, so the child can never block on
/// a full pipe while we wait for it.
fn drain<R>(pipe: Option<R>) -> tokio::task::JoinHandle<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut pipe, &mut buffer).await;
        }
        buffer
    })
}

fn validate_remote_path(path: &str) -> Result<(), ExitReason> {
    let suffix = path.strip_prefix("~/").unwrap_or(path);
    if suffix.is_empty()
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
    {
        return Err(ExitReason::Unknown(
            "remote path contains unsupported characters".to_string(),
        ));
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_path(value: &str) -> String {
    value.strip_prefix("~/").map_or_else(
        || shell_quote(value),
        |suffix| format!("$HOME/{}", shell_quote(suffix)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fail_with_stderr_script, fake_ssh_script, sleep_script};
    use crate::tunnels::classifier::ExitReason;
    use crate::tunnels::profile::TunnelProfile;

    fn profile() -> TunnelProfile {
        TunnelProfile::new("test", "example.com", "alice")
    }

    #[tokio::test]
    async fn ssh_exec_delivers_stdin_and_captures_both_streams() {
        let ssh = fake_ssh_script(
            "exec_captures_streams",
            "read payload; printf 'got:%s' \"$payload\"; printf 'warning' >&2; exit 0",
            "set /p payload=\r\necho got:%payload%\r\necho warning 1>&2\r\nexit /b 0",
        );

        let output = ssh_exec_with_binary(
            &profile(),
            "ignored-command",
            Some(b"secret\n"),
            Duration::from_secs(5),
            &ssh,
        )
        .await
        .expect("ssh exec succeeds");

        assert!(output.stdout.contains("got:secret"));
        assert!(output.stderr.contains("warning"));
        assert_eq!(output.code, Some(0));
    }

    #[test]
    fn scp_destination_brackets_only_an_ipv6_host() {
        assert_eq!(
            scp_destination("u", "host.example", "p"),
            "u@host.example:p"
        );
        assert_eq!(scp_destination("u", "10.0.0.1", "p"), "u@10.0.0.1:p");
        assert_eq!(
            scp_destination("u", "2001:db8::1", "p"),
            "u@[2001:db8::1]:p"
        );
        assert_eq!(
            scp_destination("u", "fe80::1%en0", "p"),
            "u@[fe80::1%en0]:p"
        );
    }

    /// Catches: the one-shot ssh and scp destinations lacking `--`, so a
    /// profile user starting with `-` is read as an option.
    #[tokio::test]
    async fn one_shot_ssh_and_scp_put_the_destination_after_double_dash() {
        let script = "printf '%s\\n' \"$@\" > \"$0.log\"; exit 0";
        let batch = "setlocal EnableDelayedExpansion\r\ntype nul > \"%~f0.log\"\r\n:args\r\nif \"%~1\"==\"\" exit /b 0\r\nset \"arg=%~1\"\r\necho(!arg!>>\"%~f0.log\"\r\nshift /1\r\ngoto args";
        let ssh = fake_ssh_script("exec_double_dash_ssh", script, batch);
        let scp = fake_ssh_script("exec_double_dash_scp", script, batch);
        let _ = std::fs::remove_file(format!("{}.log", ssh.display()));
        let _ = std::fs::remove_file(format!("{}.log", scp.display()));
        let mut hostile = profile();
        hostile.ssh.user = "-oProxyCommand=evil".to_string();
        let local_dir = tempfile::tempdir().expect("temp dir");
        let local = local_dir.path().join("bin");
        std::fs::write(&local, b"x").expect("write local");

        scp_push_with_binaries(
            &hostile,
            &local,
            ".cache/tuic/bin",
            Duration::from_secs(5),
            &scp,
            &ssh,
        )
        .await
        .expect("push succeeds");

        let scp_log = std::fs::read_to_string(format!("{}.log", scp.display())).unwrap();
        let scp_args: Vec<_> = scp_log.lines().collect();
        let local_arg = local.to_string_lossy();
        assert!(
            scp_args.windows(3).any(|args| args[0] == "--"
                && args[1] == local_arg
                && args[2].starts_with("-oProxyCommand=evil@example.com:.cache/tuic/bin.tmp-")),
            "scp must take `--`, the local path, then the destination: {scp_log}"
        );
        let ssh_log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        let ssh_args: Vec<_> = ssh_log.lines().collect();
        assert!(
            ssh_args
                .windows(3)
                .any(|args| args == ["-T", "--", "-oProxyCommand=evil@example.com"]),
            "{ssh_log}"
        );
    }

    #[tokio::test]
    async fn ssh_exec_classifies_a_failed_process() {
        let failure = fail_with_stderr_script("Permission denied", 255);
        let ssh = fake_ssh_script("exec_auth_failure", &failure, &failure);

        let error = ssh_exec_with_binary(
            &profile(),
            "ignored-command",
            None,
            Duration::from_secs(5),
            &ssh,
        )
        .await
        .expect_err("failure is classified");

        assert_eq!(error, ExitReason::AuthFailed);
    }

    #[tokio::test]
    async fn ssh_exec_enforces_its_execution_timeout() {
        let sleep = sleep_script();
        let ssh = fake_ssh_script("exec_timeout", &sleep, &sleep);

        let error = ssh_exec_with_binary(
            &profile(),
            "ignored-command",
            None,
            Duration::from_millis(100),
            &ssh,
        )
        .await
        .expect_err("mute process times out");

        assert_eq!(error, ExitReason::Timeout);
    }

    /// A fake ssh that forks a grandchild (`sleep 60 &`), records its PID,
    /// and waits on it — the shape of a real `ssh` with a `ProxyCommand`.
    #[cfg(unix)]
    fn forking_ssh(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let marker = crate::test_support::test_temp_root().join(format!("{name}.grandchild.pid"));
        let _ = std::fs::remove_file(&marker);
        let posix = format!("sleep 60 & echo $! > '{}'; wait", marker.display());
        (fake_ssh_script(name, &posix, "exit /b 0"), marker)
    }

    /// The grandchild's PID, once the fake ssh has written it. Setup, not the
    /// behaviour under test, so the bound is generous.
    #[cfg(unix)]
    async fn grandchild_pid(marker: &std::path::Path) -> i32 {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Ok(text) = std::fs::read_to_string(marker)
                && let Ok(pid) = text.trim().parse()
            {
                return pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake ssh never recorded its grandchild PID at {}",
                marker.display()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Assert `pid` is gone (it is reparented to init/launchd and reaped
    /// there, so allow a moment), killing it if it survived.
    #[cfg(unix)]
    async fn assert_process_gone(pid: i32) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            // SAFETY: signal 0 only probes for existence.
            if unsafe { libc::kill(pid, 0) } != 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // SAFETY: a test-owned `sleep` we deliberately started.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        panic!("the one-shot's grandchild {pid} survived the kill");
    }

    /// Catches: a timed-out one-shot killing only the direct child, so the
    /// `sleep` it forked keeps running (the `sleep 3600` leak class).
    #[cfg(unix)]
    #[tokio::test]
    async fn a_timed_out_one_shot_kills_the_grandchild_it_forked() {
        let (ssh, marker) = forking_ssh("exec_timeout_forks_grandchild");

        let error = ssh_exec_with_binary(
            &profile(),
            "ignored-command",
            None,
            Duration::from_secs(2),
            &ssh,
        )
        .await
        .expect_err("a process that never exits times out");

        assert_eq!(error, ExitReason::Timeout);
        let pid = grandchild_pid(&marker).await;
        assert_process_gone(pid).await;
    }

    /// Catches: a cancelled one-shot (its future dropped mid-wait) relying on
    /// `kill_on_drop`, which ends only the direct child.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_cancelled_one_shot_kills_the_grandchild_it_forked() {
        let (ssh, marker) = forking_ssh("exec_cancel_forks_grandchild");
        let task = tokio::spawn({
            let ssh = ssh.clone();
            async move {
                ssh_exec_with_binary(
                    &profile(),
                    "ignored-command",
                    None,
                    Duration::from_secs(120),
                    &ssh,
                )
                .await
            }
        });

        let pid = grandchild_pid(&marker).await;
        task.abort();
        let _ = task.await;
        assert_process_gone(pid).await;
    }

    #[tokio::test]
    async fn scp_push_stages_then_promotes_the_remote_file() {
        let scp = fake_ssh_script(
            "scp_stages_upload",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        let ssh = fake_ssh_script(
            "scp_promotes_upload",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        let local_dir = tempfile::tempdir().expect("temp local file directory");
        let local = local_dir.path().join("tuic-remote");
        std::fs::write(&local, b"binary").expect("write local file");
        let _ = std::fs::remove_file(format!("{}.log", scp.display()));
        let _ = std::fs::remove_file(format!("{}.log", ssh.display()));

        scp_push_with_binaries(
            &profile(),
            &local,
            ".cache/tuic/tuic-remote",
            Duration::from_secs(5),
            &scp,
            &ssh,
        )
        .await
        .expect("push succeeds");

        let scp_args =
            std::fs::read_to_string(format!("{}.log", scp.display())).expect("scp invocation log");
        assert!(scp_args.contains(".cache/tuic/tuic-remote.tmp-"));
        assert!(!scp_args.ends_with(".cache/tuic/tuic-remote\n"));
        let ssh_args =
            std::fs::read_to_string(format!("{}.log", ssh.display())).expect("ssh invocation log");
        assert!(ssh_args.contains("mv -f"));
        assert!(ssh_args.contains("chmod 755"));
        assert!(ssh_args.contains(".cache/tuic/tuic-remote.tmp-"));
    }

    #[tokio::test]
    async fn failed_scp_never_promotes_the_remote_file() {
        let failure = fail_with_stderr_script("Connection refused", 255);
        let scp = fake_ssh_script("scp_fails", &failure, &failure);
        let ssh = fake_ssh_script(
            "scp_must_not_promote",
            "printf called > \"$0.log\"; exit 0",
            "echo called > \"%~f0.log\"\r\nexit /b 0",
        );
        let local_dir = tempfile::tempdir().expect("temp local file directory");
        let local = local_dir.path().join("tuic-remote");
        std::fs::write(&local, b"binary").expect("write local file");
        let ssh_log = format!("{}.log", ssh.display());
        let _ = std::fs::remove_file(&ssh_log);

        let error = scp_push_with_binaries(
            &profile(),
            &local,
            ".cache/tuic/tuic-remote",
            Duration::from_secs(5),
            &scp,
            &ssh,
        )
        .await
        .expect_err("scp failure is returned");

        assert_eq!(error, ExitReason::ConnectionRefused);
        assert!(!std::path::Path::new(&ssh_log).exists());
    }
}
