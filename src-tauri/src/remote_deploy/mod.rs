pub(crate) mod assets;
pub(crate) mod service;

use std::path::Path;
use std::time::Duration;

use crate::tunnels::classifier::ExitReason;
use crate::tunnels::exec::{scp_push_with_binaries, ssh_exec_with_binary};
use crate::tunnels::profile::TunnelProfile;

const REMOTE_DIR: &str = "~/.cache/tuic";
pub(crate) const REMOTE_BINARY: &str = "~/.cache/tuic/tuic-remote";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeployError {
    Uname(ExitReason),
    Asset(String),
    Push(ExitReason),
    Launch(String),
}

impl std::fmt::Display for DeployError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uname(reason) => write!(formatter, "remote platform probe failed: {reason:?}"),
            Self::Asset(reason) => write!(formatter, "release asset failed: {reason}"),
            Self::Push(reason) => write!(formatter, "remote binary push failed: {reason:?}"),
            Self::Launch(reason) => write!(formatter, "remote daemon launch failed: {reason}"),
        }
    }
}

impl std::error::Error for DeployError {}

// ---------------------------------------------------------------------------
// Remote command builders
//
// Every remote shell command this module (and `ssh_provision`, which shows them
// to the user before they run) sends is built here, from fixed text plus three
// values that cannot carry shell syntax: a `u16` port, a `u64` duration and an
// instance id validated as a lowercase DNS label. No connection field (host,
// user, identity file) ever reaches a remote shell — those go to `ssh` as argv
// after `--` (`tunnels::exec`). Secrets (the pairing token, a password) travel
// on stdin, never in a command line.
// ---------------------------------------------------------------------------

/// `""` for the default instance, `"-<id>"` for a named one — so two
/// connections that launch different instances on one host keep separate PID
/// and log files instead of stopping each other's daemon.
fn instance_suffix(instance: Option<&str>) -> Result<String, String> {
    match instance {
        None => Ok(String::new()),
        Some(id) => {
            crate::remote_connection::validate_remote_instance_id(id)?;
            Ok(format!("-{id}"))
        }
    }
}

/// `" --instance <id>"` or `""`, validated.
pub(crate) fn instance_arg(instance: Option<&str>) -> Result<String, String> {
    match instance {
        None => Ok(String::new()),
        Some(id) => {
            crate::remote_connection::validate_remote_instance_id(id)?;
            Ok(format!(" --instance {id}"))
        }
    }
}

/// PID file name, relative to [`REMOTE_DIR`].
pub(crate) fn pid_file(instance: Option<&str>) -> Result<String, String> {
    Ok(format!("tuic-remote{}.pid", instance_suffix(instance)?))
}

/// Log file name, relative to [`REMOTE_DIR`].
pub(crate) fn log_file(instance: Option<&str>) -> Result<String, String> {
    Ok(format!("tuic-remote{}.log", instance_suffix(instance)?))
}

/// Signal the process a PID file names — but only after proving it is still a
/// `tuic-remote`: the file must hold a plain number, and `ps` must report that
/// PID's executable as `tuic-remote`. A stale file whose PID now belongs to
/// anything else is never signalled, and nothing is ever killed by name
/// (`pkill`/`killall`). `on_kill` runs only after a signal was delivered.
fn verified_kill(pid_file: &str, on_kill: &str) -> String {
    format!(
        "P=$(cat {pid_file} 2>/dev/null); case \"$P\" in ''|*[!0-9]*) ;; *) C=$(ps -p \"$P\" -o comm= 2>/dev/null | tr -d ' '); case \"${{C##*/}}\" in tuic-remote) kill \"$P\" 2>/dev/null{on_kill};; esac;; esac"
    )
}

/// The remote's platform, for picking the release asset.
pub(crate) const UNAME_COMMAND: &str = "uname -sm";

/// The installed binary's SHA-256, compared with the pinned release asset's.
pub(crate) fn hash_command() -> String {
    format!(
        "mkdir -p {REMOTE_DIR} && if [ -f {REMOTE_BINARY} ]; then (sha256sum {REMOTE_BINARY} 2>/dev/null || shasum -a 256 {REMOTE_BINARY} 2>/dev/null || true); fi"
    )
}

/// Start the daemon detached, bound to loopback, with the pairing token read
/// from stdin (`read -r T`) so it never appears in a process listing or a log.
/// A previous daemon of the same instance is stopped first, PID-verified.
pub(crate) fn launch_command(
    port: u16,
    survive_secs: u64,
    instance: Option<&str>,
) -> Result<String, String> {
    let pid = pid_file(instance)?;
    let log = log_file(instance)?;
    let instance = instance_arg(instance)?;
    let stop_previous = verified_kill(&pid, " || true");
    Ok(format!(
        "read -r T; cd {REMOTE_DIR} && if [ -f {pid} ]; then {stop_previous}; fi; if command -v setsid >/dev/null 2>&1; then TUIC_PAIRING_TOKEN=$T TUIC_PORT={port} setsid nohup ./tuic-remote{instance} --bind 127.0.0.1 --survive-secs {survive_secs} --no-agent-configs >{log} 2>&1 </dev/null & else TUIC_PAIRING_TOKEN=$T TUIC_PORT={port} nohup ./tuic-remote{instance} --bind 127.0.0.1 --survive-secs {survive_secs} --no-agent-configs >{log} 2>&1 </dev/null & fi; P=$!; echo $P > {pid}; unset T; (while kill -0 \"$P\" 2>/dev/null; do sleep 1; done; if [ \"$(cat {pid} 2>/dev/null)\" = \"$P\" ]; then rm -f {pid}; fi) >/dev/null 2>&1 </dev/null & sleep 1; kill -0 \"$P\" 2>/dev/null"
    ))
}

/// Stop the daemon a PID file names, PID-verified (see [`verified_kill`]).
/// Prints `KILLED` when a `tuic-remote` was signalled and nothing otherwise; a
/// stale or missing PID file is cleaned up and is not an error.
pub(crate) fn stop_command(instance: Option<&str>) -> Result<String, String> {
    let pid = pid_file(instance)?;
    let kill = verified_kill(&pid, " && echo KILLED");
    Ok(format!(
        "cd {REMOTE_DIR} 2>/dev/null || exit 0; if [ -f {pid} ]; then {kill}; rm -f {pid}; fi"
    ))
}

/// The tail of the daemon's log, shown when a launch fails.
fn log_tail_command(instance: Option<&str>) -> Result<String, String> {
    Ok(format!(
        "tail -n 5 {REMOTE_DIR}/{} 2>/dev/null || true",
        log_file(instance)?
    ))
}

/// Deploy and launch the ephemeral daemon for one remote instance
/// (`--instance <id>`; `None` = the default instance). There is deliberately
/// no instance-less variant: every caller has a connection, and a connection
/// with an Instance ID that went through the default instance's PID file
/// stopped or replaced another connection's daemon (Batch 32 review #1).
pub(crate) async fn deploy_ephemeral_for(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
    instance: Option<&str>,
) -> Result<(), DeployError> {
    deploy_ephemeral_inner(
        profile,
        port,
        token,
        survive_secs,
        instance,
        Path::new("ssh"),
        Path::new("scp"),
    )
    .await
}

/// Stop a named instance's ephemeral daemon. `Ok(true)` when a verified
/// `tuic-remote` was signalled, `Ok(false)` when there was nothing to stop.
pub(crate) async fn stop_ephemeral_for(
    profile: &TunnelProfile,
    instance: Option<&str>,
) -> Result<bool, ExitReason> {
    stop_ephemeral_with_binary(profile, instance, Path::new("ssh")).await
}

#[cfg(test)]
async fn deploy_ephemeral_with_binaries(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
    ssh_binary: &Path,
    scp_binary: &Path,
) -> Result<(), DeployError> {
    deploy_ephemeral_inner(
        profile,
        port,
        token,
        survive_secs,
        None,
        ssh_binary,
        scp_binary,
    )
    .await
}

async fn deploy_ephemeral_inner(
    profile: &TunnelProfile,
    port: u16,
    token: &str,
    survive_secs: u64,
    instance: Option<&str>,
    ssh_binary: &Path,
    scp_binary: &Path,
) -> Result<(), DeployError> {
    if token.is_empty() || token.contains(['\r', '\n']) {
        return Err(DeployError::Launch("invalid pairing token".to_string()));
    }
    // Built before anything is contacted, so an invalid instance id fails
    // without a single ssh round trip.
    let launch_command =
        launch_command(port, survive_secs, instance).map_err(DeployError::Launch)?;
    let tail_command = log_tail_command(instance).map_err(DeployError::Launch)?;

    let uname = ssh_exec_with_binary(profile, UNAME_COMMAND, None, COMMAND_TIMEOUT, ssh_binary)
        .await
        .map_err(DeployError::Uname)?;
    let uname = uname.stdout.trim();
    let target = assets::require_target(uname).map_err(DeployError::Asset)?;
    let asset = assets::resolve_update_asset(target)
        .await
        .map_err(DeployError::Asset)?;
    let asset = asset.binary;

    let hash_command = hash_command();
    let remote_hash =
        ssh_exec_with_binary(profile, &hash_command, None, COMMAND_TIMEOUT, ssh_binary)
            .await
            .ok()
            .and_then(|output| output.stdout.split_whitespace().next().map(str::to_owned));

    if remote_hash.as_deref() != Some(asset.sha256.as_str()) {
        scp_push_with_binaries(
            profile,
            &asset.path,
            REMOTE_BINARY,
            PUSH_TIMEOUT,
            scp_binary,
            ssh_binary,
        )
        .await
        .map_err(DeployError::Push)?;
    }

    let stdin = format!("{token}\n");
    if let Err(reason) = ssh_exec_with_binary(
        profile,
        &launch_command,
        Some(stdin.as_bytes()),
        LAUNCH_TIMEOUT,
        ssh_binary,
    )
    .await
    {
        let tail = ssh_exec_with_binary(profile, &tail_command, None, COMMAND_TIMEOUT, ssh_binary)
            .await
            .map(|output| output.stdout)
            .unwrap_or_default();
        return Err(DeployError::Launch(format!(
            "{reason:?}; last remote log lines:\n{}",
            tail.trim_end()
        )));
    }

    Ok(())
}

async fn stop_ephemeral_with_binary(
    profile: &TunnelProfile,
    instance: Option<&str>,
    ssh_binary: &Path,
) -> Result<bool, ExitReason> {
    let command = stop_command(instance).map_err(ExitReason::Unknown)?;
    ssh_exec_with_binary(profile, &command, None, STOP_TIMEOUT, ssh_binary)
        .await
        .map(|output| output.stdout.lines().any(|line| line.trim() == "KILLED"))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::test_support::{fail_with_stderr_script, fake_ssh_script, system32_exe};
    use crate::tunnels::classifier::ExitReason;
    use crate::tunnels::profile::TunnelProfile;

    fn profile() -> TunnelProfile {
        TunnelProfile::new("deploy", "example.com", "alice")
    }

    fn cached_asset(config: &Path, bytes: &[u8]) -> String {
        let target = "x86_64-unknown-linux-gnu";
        let path = config
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"))
            .join(format!("tuic-remote-{target}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    fn remove_log(binary: &Path) {
        let _ = std::fs::remove_file(format!("{}.log", binary.display()));
        let _ = std::fs::remove_file(format!("{}.stdin", binary.display()));
        let _ = std::fs::remove_file(format!("{}.args", binary.display()));
    }

    fn scripted_ssh(name: &str, remote_hash: &str, launch_ok: bool) -> PathBuf {
        let launch = if launch_ok {
            "IFS= read -r token; printf '%s' \"$token\" > \"$0.stdin\"; exit 0"
        } else {
            "exit 1"
        };
        let posix = format!(
            "printf '%s\\n' \"$*\" >> \"$0.log\"\ncase \"$*\" in\n  *\"uname -sm\"*) printf 'Linux x86_64\\n'; exit 0;;\n  *\"sha256sum\"*) printf '{remote_hash}  tuic-remote\\n'; exit 0;;\n  *\"read -r T\"*) {launch};;\n  *\"tail -n 5\"*) printf 'line1\\nline2\\nline3\\nline4\\nAddress already in use\\n'; exit 0;;\nesac\nexit 0"
        );
        let windows_launch = if launch_ok {
            "set /p token=\r\n<nul set /p \"=!token!\">\"%~f0.stdin\"\r\nexit /b 0"
        } else {
            "exit /b 1"
        };
        let findstr = system32_exe("findstr.exe");
        let windows = format!(
            "setlocal EnableDelayedExpansion\r\nset \"last=\"\r\n:args\r\nif \"%~1\"==\"\" goto args_done\r\nset \"last=%~1\"\r\nshift /1\r\ngoto args\r\n:args_done\r\necho(!last!>>\"%~f0.log\"\r\necho(!last!>\"%~f0.args\"\r\n{findstr} /C:\"uname -sm\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo Linux x86_64& exit /b 0)\r\n{findstr} /C:\"sha256sum\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo {remote_hash}  tuic-remote& exit /b 0)\r\n{findstr} /C:\"read -r T\" \"%~f0.args\" >nul\r\nif not errorlevel 1 ({windows_launch})\r\n{findstr} /C:\"tail -n 5\" \"%~f0.args\" >nul\r\nif not errorlevel 1 (echo line1& echo line2& echo line3& echo line4& echo Address already in use& exit /b 0)\r\nexit /b 0"
        );
        fake_ssh_script(name, &posix, &windows)
    }

    #[tokio::test]
    async fn matching_remote_hash_skips_scp_and_token_stays_out_of_arguments() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let hash = cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_hash_match", &hash, true);
        let scp = fake_ssh_script(
            "deploy_scp_must_not_run",
            "printf called > \"$0.log\"; exit 0",
            "echo called > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);
        remove_log(&scp);

        deploy_ephemeral_with_binaries(&profile(), 9877, "pair-secret", 1_800, &ssh, &scp)
            .await
            .expect("deploy succeeds");

        assert!(!Path::new(&format!("{}.log", scp.display())).exists());
        let ssh_log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        // Rust's batch argument encoding doubles internal quotes. Decode that
        // fixture representation before checking the remote POSIX command.
        #[cfg(windows)]
        let ssh_log = ssh_log.replace("\"\"", "\"");
        assert!(!ssh_log.contains("pair-secret"));
        assert_eq!(
            std::fs::read_to_string(format!("{}.stdin", ssh.display())).unwrap(),
            "pair-secret"
        );
        assert!(ssh_log.contains("--survive-secs 1800"));
        assert!(ssh_log.contains("TUIC_PORT=9877"));
        assert!(ssh_log.contains("[ \"$(cat tuic-remote.pid 2>/dev/null)\" = \"$P\" ]"));
        assert!(ssh_log.contains("rm -f tuic-remote.pid"));
    }

    #[tokio::test]
    async fn mismatched_remote_hash_pushes_before_launch() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_hash_mismatch", "deadbeef", true);
        let scp = fake_ssh_script(
            "deploy_scp_runs",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);
        remove_log(&scp);

        deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &scp)
            .await
            .expect("deploy succeeds");

        let scp_log = std::fs::read_to_string(format!("{}.log", scp.display())).unwrap();
        assert!(scp_log.contains("tuic-remote.tmp-"));
        let ssh_log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        assert!(ssh_log.contains("$HOME/"));
        assert!(!ssh_log.contains("mv -f '~/.cache"));
    }

    #[tokio::test]
    async fn failing_steps_keep_their_deploy_error_variant() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        cached_asset(config.path(), b"binary");

        let failure = fail_with_stderr_script("Permission denied", 255);
        let uname_ssh = fake_ssh_script("deploy_uname_fails", &failure, &failure);
        let error =
            deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &uname_ssh, &uname_ssh)
                .await
                .unwrap_err();
        assert!(matches!(error, DeployError::Uname(ExitReason::AuthFailed)));

        let unsupported = fake_ssh_script(
            "deploy_asset_fails",
            "printf 'Plan9 mips\\n'; exit 0",
            "echo Plan9 mips\r\nexit /b 0",
        );
        let error = deploy_ephemeral_with_binaries(
            &profile(),
            9877,
            "token",
            60,
            &unsupported,
            &unsupported,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, DeployError::Asset(message) if message.contains("Plan9 mips")));

        let ssh = scripted_ssh("deploy_before_push_failure", "deadbeef", true);
        let push_failure = fail_with_stderr_script("Connection refused", 255);
        let scp = fake_ssh_script("deploy_push_fails", &push_failure, &push_failure);
        let error = deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &scp)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DeployError::Push(ExitReason::ConnectionRefused)
        ));
    }

    #[tokio::test]
    async fn launch_failure_includes_the_last_five_log_lines() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let hash = cached_asset(config.path(), b"binary");
        let ssh = scripted_ssh("deploy_launch_fails", &hash, false);

        let error = deploy_ephemeral_with_binaries(&profile(), 9877, "token", 60, &ssh, &ssh)
            .await
            .unwrap_err();

        assert!(
            matches!(error, DeployError::Launch(message) if message.contains("Address already in use") && message.contains("line1"))
        );
    }

    #[tokio::test]
    async fn stop_ephemeral_uses_the_pid_file_and_is_bounded() {
        let ssh = fake_ssh_script(
            "deploy_stop",
            "printf '%s\\n' \"$*\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);

        stop_ephemeral_with_binary(&profile(), None, &ssh)
            .await
            .expect("best-effort stop command runs");

        let args = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        assert!(args.contains("tuic-remote.pid"));
        assert!(args.contains("kill"));
        assert_eq!(STOP_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn default_instance_keeps_the_original_file_names() {
        assert_eq!(pid_file(None).unwrap(), "tuic-remote.pid");
        assert_eq!(log_file(None).unwrap(), "tuic-remote.log");
        assert_eq!(instance_arg(None).unwrap(), "");
        let launch = launch_command(9877, 1_800, None).unwrap();
        assert!(launch.contains("./tuic-remote --bind 127.0.0.1"));
        assert!(launch.starts_with("read -r T;"));
    }

    #[test]
    fn a_named_instance_gets_its_own_pid_log_and_flag() {
        let launch = launch_command(9877, 60, Some("dev-box")).unwrap();
        assert!(launch.contains("./tuic-remote --instance dev-box --bind 127.0.0.1"));
        assert!(launch.contains("echo $P > tuic-remote-dev-box.pid"));
        assert!(launch.contains(">tuic-remote-dev-box.log"));
        let stop = stop_command(Some("dev-box")).unwrap();
        assert!(stop.contains("tuic-remote-dev-box.pid"));
        assert!(!stop.contains("tuic-remote.pid"));
    }

    #[test]
    fn hostile_instance_ids_never_reach_a_command() {
        for hostile in ["a;reboot", "$(id)", "a b", "A", "default", "-x", "a`id`"] {
            assert!(
                launch_command(9877, 60, Some(hostile)).is_err(),
                "{hostile}"
            );
            assert!(stop_command(Some(hostile)).is_err(), "{hostile}");
            assert!(instance_arg(Some(hostile)).is_err(), "{hostile}");
        }
    }

    /// The token is only ever read from stdin; no builder puts a secret in a
    /// command line, and the stop is never by name.
    #[test]
    fn commands_carry_no_secret_and_never_kill_by_name() {
        let launch = launch_command(9877, 60, None).unwrap();
        assert!(launch.contains("TUIC_PAIRING_TOKEN=$T"));
        assert!(launch.contains("unset T"));
        for command in [launch, stop_command(None).unwrap()] {
            assert!(!command.contains("pkill"), "{command}");
            assert!(!command.contains("killall"), "{command}");
            assert!(
                !command.contains("kill $(cat"),
                "unverified kill: {command}"
            );
        }
    }

    #[test]
    fn stop_verifies_the_pid_before_signalling() {
        let stop = stop_command(None).unwrap();
        let digits = stop.find("*[!0-9]*").expect("PID must be checked numeric");
        let comm = stop.find("-o comm=").expect("PID must be checked by ps");
        let name = stop
            .find("tuic-remote)")
            .expect("process must be tuic-remote");
        let kill = stop.find("kill \"$P\"").expect("kill by verified PID");
        assert!(digits < kill && comm < kill && name < kill, "{stop}");
    }

    /// Runs the real stop command in a throwaway `$HOME`: a PID file pointing at
    /// a live process that is NOT `tuic-remote` (a stale PID reused by
    /// something else) is never signalled; one pointing at a real
    /// `tuic-remote` is.
    #[cfg(unix)]
    #[test]
    fn the_stop_command_spares_a_stale_pid_and_stops_a_real_daemon() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".cache/tuic");
        std::fs::create_dir_all(&dir).unwrap();
        let run_stop = || {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(stop_command(None).unwrap())
                .env("HOME", home.path())
                .output()
                .unwrap()
        };
        let alive = |child: &mut std::process::Child| child.try_wait().unwrap().is_none();

        let mut stranger = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        std::fs::write(dir.join("tuic-remote.pid"), stranger.id().to_string()).unwrap();
        let output = run_stop();
        assert!(!String::from_utf8_lossy(&output.stdout).contains("KILLED"));
        std::thread::sleep(Duration::from_millis(100));
        assert!(alive(&mut stranger), "a stale PID's process must be spared");
        assert!(
            !dir.join("tuic-remote.pid").exists(),
            "stale PID file removed"
        );
        let _ = stranger.kill();
        let _ = stranger.wait();

        let sleep = ["/bin/sleep", "/usr/bin/sleep"]
            .into_iter()
            .find(|p| Path::new(p).exists())
            .expect("a sleep binary");
        let fake_daemon = home.path().join("tuic-remote");
        std::fs::copy(sleep, &fake_daemon).unwrap();
        let mut daemon = std::process::Command::new(&fake_daemon)
            .arg("30")
            .spawn()
            .unwrap();
        std::fs::write(dir.join("tuic-remote.pid"), daemon.id().to_string()).unwrap();
        let output = run_stop();
        assert!(String::from_utf8_lossy(&output.stdout).contains("KILLED"));
        let status = daemon.wait().unwrap();
        assert!(!status.success(), "the verified daemon was signalled");

        std::fs::write(dir.join("tuic-remote.pid"), "1; touch pwned").unwrap();
        run_stop();
        assert!(!home.path().join("pwned").exists());
        assert!(!dir.join("pwned").exists());
    }

    /// Hostile host and user values are argv to `ssh`, after `--`: they can be
    /// neither an ssh option nor shell syntax, because no shell is involved on
    /// this side and the remote command never contains them.
    #[tokio::test]
    async fn hostile_connection_fields_stay_after_the_end_of_options_marker() {
        let ssh = fake_ssh_script(
            "deploy_stop_hostile_fields",
            "printf '%s\\n' \"$@\" > \"$0.log\"; exit 0",
            "echo %* > \"%~f0.log\"\r\nexit /b 0",
        );
        remove_log(&ssh);
        let hostile = TunnelProfile::new("x", "-oProxyCommand=touch_pwned", "a;$(id)");
        stop_ephemeral_with_binary(&hostile, None, &ssh)
            .await
            .expect("stop runs");
        let log = std::fs::read_to_string(format!("{}.log", ssh.display())).unwrap();
        #[cfg(unix)]
        {
            let args: Vec<&str> = log.lines().collect();
            let marker = args.iter().position(|a| *a == "--").expect("-- marker");
            assert_eq!(args[marker + 1], "a;$(id)@-oProxyCommand=touch_pwned");
            let remote = args[marker + 2];
            assert!(!remote.contains("ProxyCommand") && !remote.contains("$(id)"));
        }
        #[cfg(windows)]
        assert!(log.contains("-- "));
    }
}
