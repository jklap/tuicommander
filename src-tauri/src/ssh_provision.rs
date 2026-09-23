//! SSH remote daemon provisioning: probe whether `tuic-remote` is already
//! running on a remote host, install or update its binary if needed, start
//! or stop it, and set its initial password if it's never been configured.
//!
//! Story: SSH Tunnels + Remote Servers consolidation, Phase 5
//! ("Remote daemon provisioning").
//!
//! ## Security posture
//!
//! This is the plan's highest-risk section: it can download and execute a
//! binary on a remote host, and write credentials to it. **Every
//! state-changing action here is gated by the CALLER (the frontend) on an
//! explicit user confirmation before it's invoked — this module has no
//! confirmation UI of its own and performs whatever it's asked immediately.**
//! It never introduces a new trust boundary: every command runs over the SSH
//! credential (identity file / agent) the user already configured for this
//! connection, the same one a real tunnel or Test Connection would use.
//!
//! The documented manual `tuic-remote` install
//! (`docs/user-guide/remote-access.md`) is a bare `curl` with no signature or
//! checksum verification. `download_artifact` opportunistically verifies
//! against a `.sha256` sidecar file if the release happens to publish one for
//! this asset; if not (unknown at the time of writing whether it does), this
//! automation sits at exactly the same trust level as the already-documented
//! manual process — not a new regression, but not a stronger guarantee either.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::ssh_connection::SshConnectionParams;
use crate::tunnels::command::{build_ssh_env, build_ssh_remote_command_args};

const REMOTE_BIN_DIR: &str = "~/.tuicommander/bin";
const REMOTE_BIN_PATH: &str = "~/.tuicommander/bin/tuic-remote";
const REMOTE_LOG_PATH: &str = "~/.tuicommander/tuic-remote.log";
const SSH_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------------------
// Release artifacts
// ---------------------------------------------------------------------------

/// One of the platform/arch combinations `tuic-remote` ships a release
/// binary for — see `docs/user-guide/remote-access.md`'s install table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteArtifact {
    LinuxX64,
    LinuxArm64,
    MacosArm64,
    WindowsX64,
}

impl RemoteArtifact {
    pub(crate) fn asset_name(self) -> &'static str {
        match self {
            Self::LinuxX64 => "tuic-remote-x86_64-unknown-linux-gnu",
            Self::LinuxArm64 => "tuic-remote-aarch64-unknown-linux-gnu",
            Self::MacosArm64 => "tuic-remote-aarch64-apple-darwin",
            Self::WindowsX64 => "tuic-remote-x86_64-pc-windows-msvc.exe",
        }
    }

    fn download_url(self) -> String {
        format!(
            "https://github.com/sstraus/tuicommander/releases/latest/download/{}",
            self.asset_name()
        )
    }
}

/// Map `uname -s`/`uname -m` output to one of the known release artifacts.
/// A native Windows target (no `uname` at all — plain `cmd`/PowerShell over
/// SSH) can't be detected this way and surfaces as an explicit error rather
/// than a silent guess; only the MSYS/Cygwin `uname` shape is recognized.
pub(crate) fn map_uname_to_artifact(os: &str, arch: &str) -> Result<RemoteArtifact, String> {
    let os = os.trim();
    let arch = arch.trim();
    match (os, arch) {
        ("Linux", "x86_64") => Ok(RemoteArtifact::LinuxX64),
        ("Linux", "aarch64" | "arm64") => Ok(RemoteArtifact::LinuxArm64),
        ("Darwin", "arm64" | "aarch64") => Ok(RemoteArtifact::MacosArm64),
        (os, "x86_64") if os.starts_with("MINGW") || os.starts_with("CYGWIN") => {
            Ok(RemoteArtifact::WindowsX64)
        }
        _ => Err(format!(
            "unsupported or undetectable remote platform: uname -s={os:?} -m={arch:?}"
        )),
    }
}

/// Download a release artifact's bytes, verifying against a `.sha256`
/// checksum sidecar if one happens to exist for this asset. A missing sidecar
/// (404, or any other fetch failure) is not itself an error — it just means
/// this download proceeds unverified, matching the trust level of the
/// already-documented manual install.
pub(crate) async fn download_artifact(artifact: RemoteArtifact) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let url = artifact.download_url();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "download failed for {}: HTTP {}",
            artifact.asset_name(),
            resp.status()
        ));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();

    if let Ok(checksum_resp) = client.get(format!("{url}.sha256")).send().await
        && checksum_resp.status().is_success()
        && let Ok(text) = checksum_resp.text().await
    {
        let expected = text.split_whitespace().next().unwrap_or("").to_lowercase();
        if !expected.is_empty() {
            let actual = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
            if expected != actual {
                return Err(format!(
                    "checksum mismatch for {}: release published {expected}, downloaded bytes hash to {actual} — refusing to install",
                    artifact.asset_name()
                ));
            }
        }
    }

    Ok(bytes)
}

// ---------------------------------------------------------------------------
// SSH command execution
// ---------------------------------------------------------------------------

/// Run `remote_command` over SSH and collect its output, bounded by
/// `SSH_COMMAND_TIMEOUT`. Shares the exact SSH posture (identity file,
/// keep-alive, host-key policy) a real tunnel or Test Connection would use,
/// via `build_ssh_remote_command_args`.
async fn run_ssh_command(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    remote_command: &str,
) -> Result<std::process::Output, String> {
    let args = build_ssh_remote_command_args(ssh, remote_command);
    let env = build_ssh_env(agent_socket);
    let mut cmd = tokio::process::Command::new(&args[0]);
    cmd.args(&args[1..]);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::null());
    tokio::time::timeout(SSH_COMMAND_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "ssh command timed out".to_string())?
        .map_err(|e| format!("failed to run ssh: {e}"))
}

/// Run `remote_command` over SSH, writing `stdin_payload` to its stdin then
/// closing it (so `read_line`-style remote readers, like `--set-password`,
/// see EOF and don't block). Uses `wait_with_output` (not a manual
/// `try_wait()` poll loop) so stdout/stderr are drained concurrently with
/// waiting for exit — a hand-rolled poll loop that doesn't do this can
/// deadlock the instant the child writes past the OS pipe buffer (see
/// src-tauri/AGENTS.md's "Script timeouts must drain stdout/stderr..." note).
async fn run_ssh_command_with_stdin(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    remote_command: &str,
    stdin_payload: &[u8],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let args = build_ssh_remote_command_args(ssh, remote_command);
    let env = build_ssh_env(agent_socket);
    let mut cmd = tokio::process::Command::new(&args[0]);
    cmd.args(&args[1..]);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn ssh: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("ssh child has no stdin handle")?;
    let payload = stdin_payload.to_vec();

    let write_fut = async move {
        stdin.write_all(&payload).await.map_err(|e| e.to_string())?;
        stdin.shutdown().await.map_err(|e| e.to_string())
    };
    let wait_fut = async {
        tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| "ssh command timed out".to_string())
    };
    let (write_result, wait_result) = tokio::join!(write_fut, wait_fut);
    write_result?;
    wait_result?.map_err(|e| format!("failed waiting for ssh: {e}"))
}

/// Instance ids are already constrained to a lowercase DNS label by
/// `AppInstance::named` — reject anything else here too, before it's ever
/// interpolated into a remote shell command string, regardless of what a
/// caller passes. Belt-and-suspenders: today's only caller (the merged
/// connection editor) already validates this on save, but a shell-command
/// builder should never trust that its caller did.
fn validate_instance_id(instance_id: &str) -> Result<(), String> {
    crate::app_instance::AppInstance::named(instance_id)
        .map(|_| ())
        .map_err(|e| format!("invalid instance id: {e}"))
}

fn instance_flag(instance_id: Option<&str>) -> Result<String, String> {
    match instance_id {
        Some(id) => {
            validate_instance_id(id)?;
            Ok(format!(" --instance {id}"))
        }
        None => Ok(String::new()),
    }
}

// ---------------------------------------------------------------------------
// Daemon state probing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub(crate) enum SshDaemonState {
    /// A `tuic-remote` process is already listening on the target port.
    Running,
    /// Nothing is listening, but the binary is present and can be started.
    NotRunningBinaryPresent,
    /// Nothing is listening and no binary was found — installation is needed
    /// before it can be started.
    NotRunningBinaryMissing,
}

/// Build the probe's remote shell command. Pure and directly testable,
/// separated from `probe_ssh_daemon_state`'s execution so the exact command
/// this module sends over SSH can be asserted on without spawning a process.
fn build_probe_command(port: u16) -> String {
    format!(
        "if lsof -ti tcp:{port} -sTCP:LISTEN 2>/dev/null | head -1 | xargs -I{{}} ps -p {{}} -o comm= 2>/dev/null | grep -q tuic-remote; \
         then echo RUNNING; \
         elif command -v tuic-remote >/dev/null 2>&1 || [ -x {REMOTE_BIN_PATH} ]; \
         then echo BINARY_PRESENT; \
         else echo BINARY_MISSING; fi"
    )
}

/// Parse `build_probe_command`'s stdout. Pure and directly testable.
fn parse_probe_output(stdout: &str) -> Result<SshDaemonState, String> {
    match stdout.trim() {
        "RUNNING" => Ok(SshDaemonState::Running),
        "BINARY_PRESENT" => Ok(SshDaemonState::NotRunningBinaryPresent),
        "BINARY_MISSING" => Ok(SshDaemonState::NotRunningBinaryMissing),
        other => Err(format!("unexpected daemon-probe output: {other:?}")),
    }
}

/// Probe the remote host's daemon state with a single SSH round-trip: is
/// something listening on `port` (verified by process name, same discipline
/// as `tunnels::port::kill_ssh_on_port` — never trust the port check alone),
/// and if not, is the binary at least present?
pub(crate) async fn probe_ssh_daemon_state(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    port: u16,
) -> Result<SshDaemonState, String> {
    let remote_command = build_probe_command(port);
    let output = run_ssh_command(ssh, agent_socket, &remote_command).await?;
    if !output.status.success() {
        return Err(format!(
            "failed to probe remote daemon state: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_probe_output(&String::from_utf8_lossy(&output.stdout))
}

/// Probe the remote's OS/arch via `uname -s`/`uname -m`, for picking a
/// release artifact to install.
pub(crate) async fn probe_remote_artifact(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
) -> Result<RemoteArtifact, String> {
    let output = run_ssh_command(ssh, agent_socket, "uname -s; uname -m").await?;
    if !output.status.success() {
        return Err(format!(
            "uname failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines();
    let os = lines
        .next()
        .ok_or_else(|| "uname produced no output for -s".to_string())?;
    let arch = lines
        .next()
        .ok_or_else(|| "uname produced no output for -m".to_string())?;
    map_uname_to_artifact(os, arch)
}

// ---------------------------------------------------------------------------
// Install / start / stop / configure
// ---------------------------------------------------------------------------

/// Detect the remote's platform, download the matching release artifact, and
/// stream it directly into place over the SAME SSH connection's stdin — no
/// separate `scp` dependency. Overwrites any existing binary at
/// `REMOTE_BIN_PATH` (used for both fresh installs and "Update").
pub(crate) async fn install_tuic_remote_binary(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
) -> Result<(), String> {
    let artifact = probe_remote_artifact(ssh, agent_socket).await?;
    let bytes = download_artifact(artifact).await?;
    let remote_command = format!(
        "mkdir -p {REMOTE_BIN_DIR} && cat > {REMOTE_BIN_PATH} && chmod +x {REMOTE_BIN_PATH}"
    );
    let output =
        run_ssh_command_with_stdin(ssh, agent_socket, &remote_command, &bytes, TRANSFER_TIMEOUT)
            .await?;
    if !output.status.success() {
        return Err(format!(
            "failed to transfer tuic-remote to the remote host: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Start `tuic-remote` on the remote host, backgrounded and detached from the
/// SSH session (so it outlives this command's own process exiting).
/// Fire-and-forget: the caller is expected to poll reachability afterward
/// (the same tunnel-then-health-check flow `connect()` already does) rather
/// than trust this call's mere success as proof the daemon is actually up.
fn build_start_command(instance_flag: &str, port: u16) -> String {
    format!(
        "TUIC_PORT={port} nohup {REMOTE_BIN_PATH}{instance_flag} > {REMOTE_LOG_PATH} 2>&1 < /dev/null & disown; sleep 0.3; echo started"
    )
}

pub(crate) async fn start_ssh_daemon(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    instance_id: Option<&str>,
    port: u16,
) -> Result<(), String> {
    let flag = instance_flag(instance_id)?;
    let remote_command = build_start_command(&flag, port);
    let output = run_ssh_command(ssh, agent_socket, &remote_command).await?;
    if !output.status.success() || !String::from_utf8_lossy(&output.stdout).contains("started") {
        return Err(format!(
            "failed to start the remote daemon: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Stop `tuic-remote` on the remote host — PID-verified, mirroring
/// `tunnels::port::kill_ssh_on_port`'s discipline (confirm the listening
/// process is actually `tuic-remote` by name before signaling it), just
/// expressed as a single remote shell command since the process lives on the
/// far end of the SSH connection, not locally.
fn build_stop_command(port: u16) -> String {
    format!(
        "pid=$(lsof -ti tcp:{port} -sTCP:LISTEN 2>/dev/null | head -1); \
         if [ -n \"$pid\" ] && ps -p \"$pid\" -o comm= 2>/dev/null | grep -q tuic-remote; \
         then kill \"$pid\"; echo KILLED; \
         else echo NOT_FOUND; fi"
    )
}

fn parse_stop_output(stdout: &str) -> Result<(), String> {
    match stdout.trim() {
        "KILLED" => Ok(()),
        "NOT_FOUND" => Err("no tuic-remote process found listening on that port".to_string()),
        other => Err(format!("unexpected stop-daemon output: {other:?}")),
    }
}

pub(crate) async fn stop_ssh_daemon(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    port: u16,
) -> Result<(), String> {
    let remote_command = build_stop_command(port);
    let output = run_ssh_command(ssh, agent_socket, &remote_command).await?;
    parse_stop_output(&String::from_utf8_lossy(&output.stdout))
}

fn build_set_password_command(instance_flag: &str) -> String {
    format!("{REMOTE_BIN_PATH}{instance_flag} --set-password")
}

/// The exact stdin payload `set_password_interactive` (`lib.rs`) expects:
/// username, then password, each newline-terminated — it reads both via
/// plain `stdin.read_line()`.
fn build_set_password_stdin(username: &str, password: &str) -> String {
    format!("{username}\n{password}\n")
}

/// Pipe credentials into `tuic-remote --set-password` over SSH.
/// `set_password_interactive` (`lib.rs`) reads plain `stdin.read_line()` for
/// both username and password — not a TTY-only prompt — so this works the
/// same way a human typing at an interactive `ssh` session would, just fed
/// programmatically. The plaintext password lives only in this process's
/// memory and the SSH child's stdin pipe; it is never written to a file or
/// logged.
pub(crate) async fn set_ssh_daemon_password(
    ssh: &SshConnectionParams,
    agent_socket: Option<&Path>,
    instance_id: Option<&str>,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let flag = instance_flag(instance_id)?;
    let remote_command = build_set_password_command(&flag);
    let payload = build_set_password_stdin(username, password);
    let output = run_ssh_command_with_stdin(
        ssh,
        agent_socket,
        &remote_command,
        payload.as_bytes(),
        SSH_COMMAND_TIMEOUT,
    )
    .await?;
    if !output.status.success() {
        return Err(format!(
            "failed to set the remote daemon's password: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Version comparison
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub(crate) enum VersionCheckResult {
    Match,
    Outdated {
        remote_version: String,
        local_version: String,
    },
}

/// Pure string comparison, not a semver-aware comparison — this app's version
/// is `CARGO_PKG_VERSION`, always bumped together across the desktop app and
/// `tuic-remote` (built from the same repo, same release), so "different at
/// all" is exactly the signal worth surfacing; a semver library would add a
/// dependency for a distinction (older vs. newer) this codebase's own release
/// process doesn't need.
pub(crate) fn compare_versions(local_version: &str, remote_version: &str) -> VersionCheckResult {
    if local_version == remote_version {
        VersionCheckResult::Match
    } else {
        VersionCheckResult::Outdated {
            remote_version: remote_version.to_string(),
            local_version: local_version.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands / HTTP parity
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn probe_ssh_daemon(
    ssh: SshConnectionParams,
    port: u16,
) -> Result<SshDaemonState, String> {
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    probe_ssh_daemon_state(&ssh, agent_socket.as_deref(), port).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn install_ssh_daemon(ssh: SshConnectionParams) -> Result<(), String> {
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    install_tuic_remote_binary(&ssh, agent_socket.as_deref()).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn start_ssh_remote_daemon(
    ssh: SshConnectionParams,
    instance_id: Option<String>,
    port: u16,
) -> Result<(), String> {
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    start_ssh_daemon(&ssh, agent_socket.as_deref(), instance_id.as_deref(), port).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn stop_ssh_remote_daemon(ssh: SshConnectionParams, port: u16) -> Result<(), String> {
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    stop_ssh_daemon(&ssh, agent_socket.as_deref(), port).await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn set_ssh_remote_password(
    ssh: SshConnectionParams,
    instance_id: Option<String>,
    username: String,
    password: String,
) -> Result<(), String> {
    let agent_socket = crate::tunnels::agent::discover_agent_socket();
    set_ssh_daemon_password(
        &ssh,
        agent_socket.as_deref(),
        instance_id.as_deref(),
        &username,
        &password,
    )
    .await
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn check_remote_version(local_version: String, remote_version: String) -> VersionCheckResult {
    compare_versions(&local_version, &remote_version)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- map_uname_to_artifact ---

    #[test]
    fn maps_linux_x64() {
        assert_eq!(
            map_uname_to_artifact("Linux", "x86_64").unwrap(),
            RemoteArtifact::LinuxX64
        );
    }

    #[test]
    fn maps_linux_arm64_both_spellings() {
        assert_eq!(
            map_uname_to_artifact("Linux", "aarch64").unwrap(),
            RemoteArtifact::LinuxArm64
        );
        assert_eq!(
            map_uname_to_artifact("Linux", "arm64").unwrap(),
            RemoteArtifact::LinuxArm64
        );
    }

    #[test]
    fn maps_macos_arm64() {
        assert_eq!(
            map_uname_to_artifact("Darwin", "arm64").unwrap(),
            RemoteArtifact::MacosArm64
        );
    }

    #[test]
    fn maps_mingw_and_cygwin_as_windows_x64() {
        assert_eq!(
            map_uname_to_artifact("MINGW64_NT-10.0", "x86_64").unwrap(),
            RemoteArtifact::WindowsX64
        );
        assert_eq!(
            map_uname_to_artifact("CYGWIN_NT-10.0", "x86_64").unwrap(),
            RemoteArtifact::WindowsX64
        );
    }

    #[test]
    fn rejects_unknown_combination() {
        assert!(map_uname_to_artifact("SunOS", "sparc64").is_err());
    }

    #[test]
    fn rejects_macos_intel_not_a_shipped_artifact() {
        // docs/user-guide/remote-access.md's table has no macOS x86_64 entry.
        assert!(map_uname_to_artifact("Darwin", "x86_64").is_err());
    }

    #[test]
    fn trims_whitespace_from_uname_output() {
        assert_eq!(
            map_uname_to_artifact("Linux\n", "x86_64\r\n").unwrap(),
            RemoteArtifact::LinuxX64
        );
    }

    #[test]
    fn asset_names_match_the_documented_table() {
        assert_eq!(
            RemoteArtifact::LinuxX64.asset_name(),
            "tuic-remote-x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            RemoteArtifact::LinuxArm64.asset_name(),
            "tuic-remote-aarch64-unknown-linux-gnu"
        );
        assert_eq!(
            RemoteArtifact::MacosArm64.asset_name(),
            "tuic-remote-aarch64-apple-darwin"
        );
        assert_eq!(
            RemoteArtifact::WindowsX64.asset_name(),
            "tuic-remote-x86_64-pc-windows-msvc.exe"
        );
    }

    #[test]
    fn download_url_points_at_the_latest_release() {
        assert_eq!(
            RemoteArtifact::LinuxX64.download_url(),
            "https://github.com/sstraus/tuicommander/releases/latest/download/tuic-remote-x86_64-unknown-linux-gnu"
        );
    }

    // --- validate_instance_id / instance_flag ---

    #[test]
    fn instance_flag_empty_for_none() {
        assert_eq!(instance_flag(None).unwrap(), "");
    }

    #[test]
    fn instance_flag_formats_a_valid_id() {
        assert_eq!(
            instance_flag(Some("dev-box")).unwrap(),
            " --instance dev-box"
        );
    }

    #[test]
    fn instance_flag_rejects_shell_metacharacters() {
        // AppInstance::named only accepts a lowercase DNS label — this must
        // reject anything that could act as shell syntax if ever
        // interpolated unescaped into a remote command string.
        assert!(instance_flag(Some("dev; rm -rf /")).is_err());
        assert!(instance_flag(Some("dev$(whoami)")).is_err());
        assert!(instance_flag(Some("dev box")).is_err());
        assert!(instance_flag(Some("UPPERCASE")).is_err());
    }

    #[test]
    fn instance_flag_rejects_reserved_default() {
        assert!(instance_flag(Some("default")).is_err());
    }

    // --- compare_versions ---

    #[test]
    fn compare_versions_match() {
        assert_eq!(
            compare_versions("1.7.7", "1.7.7"),
            VersionCheckResult::Match
        );
    }

    #[test]
    fn compare_versions_outdated_carries_both_versions() {
        assert_eq!(
            compare_versions("1.7.7", "1.7.6"),
            VersionCheckResult::Outdated {
                remote_version: "1.7.6".to_string(),
                local_version: "1.7.7".to_string(),
            }
        );
    }

    // --- Command builders / output parsers ---
    //
    // `run_ssh_command`/`run_ssh_command_with_stdin` always invoke the
    // literal `ssh` argv[0] from `build_ssh_remote_command_args`, with no
    // dependency-injection point to swap in a fake binary the way
    // `tunnels::supervisor`'s tests do for the tunnel path. Rather than add
    // one (which would mean either threading a binary override through every
    // public function here, or a shared mutable env var racing across
    // parallel test threads), the actual substantive logic — the exact shell
    // commands this module sends, and how it classifies what comes back —
    // is split into pure `build_*`/`parse_*` functions and tested directly.
    // This exercises everything that can go wrong here (a wrong command
    // string, a misclassified result) without spawning a process at all.

    #[test]
    fn probe_command_checks_port_then_binary_presence_then_falls_back_to_missing() {
        let cmd = build_probe_command(9877);
        assert!(cmd.contains("tcp:9877"));
        assert!(cmd.contains("grep -q tuic-remote"));
        assert!(cmd.contains("echo RUNNING"));
        assert!(cmd.contains("command -v tuic-remote"));
        assert!(cmd.contains(REMOTE_BIN_PATH));
        assert!(cmd.contains("echo BINARY_PRESENT"));
        assert!(cmd.contains("echo BINARY_MISSING"));
    }

    #[test]
    fn parse_probe_output_recognizes_all_three_states() {
        assert_eq!(
            parse_probe_output("RUNNING\n").unwrap(),
            SshDaemonState::Running
        );
        assert_eq!(
            parse_probe_output("BINARY_PRESENT\n").unwrap(),
            SshDaemonState::NotRunningBinaryPresent
        );
        assert_eq!(
            parse_probe_output("BINARY_MISSING\n").unwrap(),
            SshDaemonState::NotRunningBinaryMissing
        );
    }

    #[test]
    fn parse_probe_output_rejects_unexpected_text() {
        assert!(parse_probe_output("garbage").is_err());
        assert!(parse_probe_output("").is_err());
    }

    #[test]
    fn start_command_sets_the_port_env_var_and_backgrounds_detached() {
        let cmd = build_start_command("", 9877);
        assert!(cmd.starts_with("TUIC_PORT=9877 "));
        assert!(cmd.contains(REMOTE_BIN_PATH));
        assert!(cmd.contains("nohup"));
        assert!(cmd.contains("disown"));
        assert!(cmd.contains(REMOTE_LOG_PATH));
        assert!(cmd.contains("echo started"));
    }

    #[test]
    fn start_command_includes_the_instance_flag_when_given() {
        let flag = instance_flag(Some("dev-box")).unwrap();
        let cmd = build_start_command(&flag, 9877);
        assert!(cmd.contains(&format!("{REMOTE_BIN_PATH} --instance dev-box")));
    }

    #[test]
    fn stop_command_checks_the_pid_belongs_to_tuic_remote_before_killing() {
        let cmd = build_stop_command(9877);
        assert!(cmd.contains("tcp:9877"));
        // The kill only happens inside the branch that already verified the
        // process name — this is the PID-verification discipline itself,
        // not just a string containing "kill" somewhere.
        let kill_pos = cmd
            .find("kill \"$pid\"")
            .expect("must kill by the verified pid");
        let grep_pos = cmd
            .find("grep -q tuic-remote")
            .expect("must verify process name");
        assert!(
            grep_pos < kill_pos,
            "process-name check must precede the kill"
        );
    }

    #[test]
    fn parse_stop_output_distinguishes_killed_from_not_found() {
        assert!(parse_stop_output("KILLED\n").is_ok());
        assert!(parse_stop_output("NOT_FOUND\n").is_err());
        assert!(parse_stop_output("garbage").is_err());
    }

    #[test]
    fn set_password_command_targets_the_right_instance() {
        assert_eq!(
            build_set_password_command(""),
            format!("{REMOTE_BIN_PATH} --set-password")
        );
        assert_eq!(
            build_set_password_command(" --instance dev-box"),
            format!("{REMOTE_BIN_PATH} --instance dev-box --set-password")
        );
    }

    #[test]
    fn set_password_stdin_is_username_then_password_newline_terminated() {
        assert_eq!(
            build_set_password_stdin("alice", "hunter2"),
            "alice\nhunter2\n"
        );
    }

    #[test]
    fn set_password_stdin_never_logs_or_echoes_differently_for_odd_characters() {
        // Not a security boundary in itself (the credential vault is), but a
        // regression here (e.g. accidentally shell-interpreting the payload
        // instead of piping it as literal stdin bytes) would silently mangle
        // a real password — assert the exact bytes for a password containing
        // characters that WOULD matter if this were ever shell-interpolated
        // instead of piped as stdin.
        assert_eq!(
            build_set_password_stdin("alice", "p$w'\"; rm -rf /"),
            "alice\np$w'\"; rm -rf /\n"
        );
    }
}
