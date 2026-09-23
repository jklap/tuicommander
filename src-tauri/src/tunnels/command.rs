use std::path::Path;

use super::profile::{ForwardSpec, TunnelProfile};
use crate::ssh_connection::{SshConnectionParams, StrictHostKeyChecking};

/// Build the option flags shared by every SSH invocation this codebase makes
/// — a long-lived port-forwarding tunnel (`build_ssh_args`) AND a one-shot
/// Test Connection reachability check (`build_ssh_test_args`): keep-alive
/// tuning, host-key policy, a blanket refusal to forward the agent, the SSH
/// port, and an optional identity file. Deliberately does NOT include
/// argv[0] or anything mode-specific (`BatchMode`, whether a remote command
/// runs, port forwards) — callers own those, since the two modes need
/// different ones. Extracted (story: SSH Tunnels + Remote Servers
/// consolidation, Phase 2) so Test Connection's SSH check can never drift
/// from what a real tunnel/remote-server connection actually does.
fn build_ssh_option_args(ssh: &SshConnectionParams) -> Vec<String> {
    let mut args = Vec::new();

    // Keep-alive
    args.push("-o".to_string());
    args.push(format!("ServerAliveInterval={}", ssh.server_alive_interval));
    args.push("-o".to_string());
    args.push(format!(
        "ServerAliveCountMax={}",
        ssh.server_alive_count_max
    ));

    // Host key policy
    let shk_value = match ssh.strict_host_key_checking {
        StrictHostKeyChecking::Yes => "yes",
        StrictHostKeyChecking::AcceptNew => "accept-new",
    };
    args.push("-o".to_string());
    args.push(format!("StrictHostKeyChecking={shk_value}"));

    // Security policy: never forward the agent
    args.push("-o".to_string());
    args.push("ForwardAgent=no".to_string());

    // SSH port
    args.push("-p".to_string());
    args.push(ssh.port.to_string());

    // Identity file (optional)
    if let Some(identity) = &ssh.identity_file {
        args.push("-i".to_string());
        args.push(identity.to_string_lossy().into_owned());
    }

    args
}

/// Build the ssh argument vector for a tunnel profile.
pub fn build_ssh_args(profile: &TunnelProfile) -> Vec<String> {
    // argv[0]; no shell/stdin/TTY; no interactive prompts; fail if any
    // forward can't bind.
    let mut args = vec![
        "ssh".to_string(),
        "-N".to_string(),
        "-n".to_string(),
        "-T".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
    ];

    args.extend(build_ssh_option_args(&profile.ssh));

    // Port forwards
    for forward in &profile.forwards {
        match forward {
            ForwardSpec::Local {
                bind_port,
                remote_host,
                remote_port,
            } => {
                args.push("-L".to_string());
                args.push(format!("{bind_port}:{remote_host}:{remote_port}"));
            }
            ForwardSpec::Remote {
                bind_port,
                local_host,
                local_port,
            } => {
                args.push("-R".to_string());
                args.push(format!("{bind_port}:{local_host}:{local_port}"));
            }
        }
    }

    // Destination — must be last
    args.push(format!("{}@{}", profile.ssh.user, profile.ssh.host));

    args
}

/// Build a one-shot, no-forwards SSH connectivity-check argv: connect,
/// authenticate, run a trivial remote command (`true`), and exit — unlike
/// `build_ssh_args`'s `-N` (never exits on its own; meant for a long-lived
/// port-forward), this is meant to be run to completion under a bounded
/// wait. Shares every identity/keepalive/host-key-policy flag with
/// `build_ssh_args` via `build_ssh_option_args`, so Test Connection can
/// never observe a different SSH posture than a real tunnel/remote-server
/// connection would. `ConnectTimeout=5` bounds the network phase
/// specifically (tighter than a tunnel needs, since this is a quick
/// user-facing check, not a long-lived connection).
pub fn build_ssh_test_args(ssh: &SshConnectionParams) -> Vec<String> {
    let mut args = vec![
        "ssh".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=5".to_string(),
    ];
    args.extend(build_ssh_option_args(ssh));
    args.push(format!("{}@{}", ssh.user, ssh.host));
    args.push("true".to_string());
    args
}

/// Build an ssh argv that connects and runs `remote_command` via the login
/// shell, then exits — the same SSH posture as `build_ssh_test_args` (shares
/// every identity/keepalive/host-key-policy flag via `build_ssh_option_args`,
/// so remote-daemon provisioning can never drift from what a real tunnel or
/// Test Connection does), but for an arbitrary caller-supplied command instead
/// of the fixed `true` reachability probe. Story: SSH Tunnels + Remote
/// Servers consolidation, Phase 5 ("Remote daemon provisioning").
///
/// `ConnectTimeout=5` bounds only the network/handshake phase — the remote
/// command itself can run as long as it needs; callers that need an overall
/// deadline (e.g. a slow `curl`/binary transfer) must apply their own
/// `tokio::time::timeout` around the spawned process.
pub fn build_ssh_remote_command_args(
    ssh: &SshConnectionParams,
    remote_command: &str,
) -> Vec<String> {
    let mut args = vec![
        "ssh".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=5".to_string(),
    ];
    args.extend(build_ssh_option_args(ssh));
    args.push(format!("{}@{}", ssh.user, ssh.host));
    args.push(remote_command.to_string());
    args
}

/// Build environment variables for the ssh process.
/// Sets SSH_AUTH_SOCK if agent_socket is provided.
pub fn build_ssh_env(agent_socket: Option<&Path>) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(socket) = agent_socket {
        env.push((
            "SSH_AUTH_SOCK".to_string(),
            socket.to_string_lossy().into_owned(),
        ));
    }
    env
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::ssh_connection::SshConnectionParams;
    use crate::tunnels::profile::TunnelProfile;

    fn base_profile() -> TunnelProfile {
        TunnelProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test".to_string(),
            ssh: SshConnectionParams::new("example.com", "alice"),
            forwards: Vec::new(),
            auto_connect: false,
        }
    }

    // Helper: assert that a flag (possibly with a value) is absent in the arg vector.
    fn assert_flag_absent(args: &[String], flag: &str) {
        assert!(
            !args.iter().any(|a| a == flag),
            "flag {flag:?} must not appear in args: {args:?}"
        );
    }

    // Helper: find the value following a `-o` option prefix.
    fn find_option<'a>(args: &'a [String], prefix: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|w| w[0] == "-o" && w[1].starts_with(prefix))
            .map(|w| w[1].as_str())
    }

    #[test]
    fn local_forward_only() {
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "internal.example.com".to_string(),
            remote_port: 80,
        }];

        let args = build_ssh_args(&profile);

        // Verify the -L flag and its value appear consecutively
        let l_pos = args
            .iter()
            .position(|a| a == "-L")
            .expect("-L must be present");
        assert_eq!(args[l_pos + 1], "8080:internal.example.com:80");
        // No -R flag
        assert_flag_absent(&args, "-R");
        // Last arg is user@host
        assert_eq!(args.last().unwrap(), "alice@example.com");
    }

    #[test]
    fn remote_forward_only() {
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Remote {
            bind_port: 9090,
            local_host: "127.0.0.1".to_string(),
            local_port: 3000,
        }];

        let args = build_ssh_args(&profile);

        let r_pos = args
            .iter()
            .position(|a| a == "-R")
            .expect("-R must be present");
        assert_eq!(args[r_pos + 1], "9090:127.0.0.1:3000");
        assert_flag_absent(&args, "-L");
        assert_eq!(args.last().unwrap(), "alice@example.com");
    }

    #[test]
    fn mixed_forwards() {
        let mut profile = base_profile();
        profile.forwards = vec![
            ForwardSpec::Local {
                bind_port: 8080,
                remote_host: "internal.example.com".to_string(),
                remote_port: 80,
            },
            ForwardSpec::Remote {
                bind_port: 9090,
                local_host: "127.0.0.1".to_string(),
                local_port: 3000,
            },
        ];

        let args = build_ssh_args(&profile);

        assert!(args.iter().any(|a| a == "-L"), "-L must be present");
        assert!(args.iter().any(|a| a == "-R"), "-R must be present");

        let l_pos = args.iter().position(|a| a == "-L").unwrap();
        assert_eq!(args[l_pos + 1], "8080:internal.example.com:80");

        let r_pos = args.iter().position(|a| a == "-R").unwrap();
        assert_eq!(args[r_pos + 1], "9090:127.0.0.1:3000");
    }

    #[test]
    fn with_identity_file() {
        let mut profile = base_profile();
        profile.ssh.identity_file = Some(PathBuf::from("/home/alice/.ssh/id_ed25519"));

        let args = build_ssh_args(&profile);

        let i_pos = args
            .iter()
            .position(|a| a == "-i")
            .expect("-i must be present");
        assert_eq!(args[i_pos + 1], "/home/alice/.ssh/id_ed25519");
    }

    #[test]
    fn without_identity_file() {
        let profile = base_profile();
        let args = build_ssh_args(&profile);
        assert_flag_absent(&args, "-i");
    }

    #[test]
    fn ssh_auth_sock_override_in_env() {
        let socket = Path::new("/run/user/1000/ssh-agent.sock");
        let env = build_ssh_env(Some(socket));

        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "SSH_AUTH_SOCK");
        assert_eq!(env[0].1, "/run/user/1000/ssh-agent.sock");
    }

    #[test]
    fn no_agent_socket_yields_empty_env() {
        let env = build_ssh_env(None);
        assert!(env.is_empty());
    }

    #[test]
    fn forward_agent_no_always_present() {
        let profile = base_profile();
        let args = build_ssh_args(&profile);

        let found = find_option(&args, "ForwardAgent=");
        assert_eq!(found, Some("ForwardAgent=no"), "ForwardAgent must be 'no'");
    }

    #[test]
    fn strict_host_key_checking_accept_new() {
        let mut profile = base_profile();
        profile.ssh.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;

        let args = build_ssh_args(&profile);

        let found = find_option(&args, "StrictHostKeyChecking=");
        assert_eq!(
            found,
            Some("StrictHostKeyChecking=accept-new"),
            "AcceptNew must map to 'accept-new'"
        );
    }

    // --- build_ssh_test_args (Test Connection's one-shot SSH check) ---

    #[test]
    fn test_args_run_a_trivial_remote_command_instead_of_dash_n() {
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);

        assert_flag_absent(&args, "-N");
        assert_eq!(
            args.last().unwrap(),
            "true",
            "must run a real remote command so the process exits on its own"
        );
        assert_eq!(args[args.len() - 2], "alice@example.com");
    }

    #[test]
    fn test_args_include_connect_timeout() {
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);
        let found = find_option(&args, "ConnectTimeout=");
        assert_eq!(found, Some("ConnectTimeout=5"));
    }

    #[test]
    fn test_args_include_batch_mode() {
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);
        let found = find_option(&args, "BatchMode=");
        assert_eq!(found, Some("BatchMode=yes"));
    }

    #[test]
    fn test_args_share_keepalive_and_host_key_policy_with_tunnel_args() {
        let mut ssh = SshConnectionParams::new("example.com", "alice");
        ssh.server_alive_interval = 42;
        ssh.server_alive_count_max = 7;
        ssh.strict_host_key_checking = StrictHostKeyChecking::AcceptNew;

        let test_args = build_ssh_test_args(&ssh);
        assert_eq!(
            find_option(&test_args, "ServerAliveInterval="),
            Some("ServerAliveInterval=42")
        );
        assert_eq!(
            find_option(&test_args, "ServerAliveCountMax="),
            Some("ServerAliveCountMax=7")
        );
        assert_eq!(
            find_option(&test_args, "StrictHostKeyChecking="),
            Some("StrictHostKeyChecking=accept-new")
        );
        assert_eq!(
            find_option(&test_args, "ForwardAgent="),
            Some("ForwardAgent=no")
        );
    }

    #[test]
    fn test_args_include_identity_file_when_set() {
        let mut ssh = SshConnectionParams::new("example.com", "alice");
        ssh.identity_file = Some(PathBuf::from("/home/alice/.ssh/id_ed25519"));
        let args = build_ssh_test_args(&ssh);
        let i_pos = args
            .iter()
            .position(|a| a == "-i")
            .expect("-i must be present");
        assert_eq!(args[i_pos + 1], "/home/alice/.ssh/id_ed25519");
    }

    #[test]
    fn test_args_omit_identity_file_when_unset() {
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);
        assert_flag_absent(&args, "-i");
    }

    #[test]
    fn test_args_never_include_ssh_agent_forwarding() {
        let ssh = SshConnectionParams::new("example.com", "alice");
        let args = build_ssh_test_args(&ssh);
        assert_flag_absent(&args, "-A");
    }

    #[test]
    fn test_args_use_the_configured_port() {
        let mut ssh = SshConnectionParams::new("example.com", "alice");
        ssh.port = 2222;
        let args = build_ssh_test_args(&ssh);
        let p_pos = args
            .iter()
            .position(|a| a == "-p")
            .expect("-p must be present");
        assert_eq!(args[p_pos + 1], "2222");
    }

    #[test]
    fn no_dash_a_flag_ever() {
        // -A enables agent forwarding — must never appear
        let mut profile = base_profile();
        profile.forwards = vec![ForwardSpec::Local {
            bind_port: 8080,
            remote_host: "host".to_string(),
            remote_port: 80,
        }];
        let args = build_ssh_args(&profile);
        assert_flag_absent(&args, "-A");
    }
}
