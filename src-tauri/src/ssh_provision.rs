//! SSH remote daemon provisioning, confirmed step by step.
//!
//! A "Remote Server — SSH" connection with `start_if_not_running` set, whose
//! daemon is not answering, gets an OFFER (`remote_runtime` publishes
//! `provision_offer` on its status) — never an action. The same goes for a
//! daemon that answers but has no password configured yet. Acting on an offer
//! is two calls:
//!
//! 1. [`plan_for`] builds a [`ProvisionPlan`]: the destination and every remote
//!    command that will run, in order, exactly as `remote_deploy` will send it
//!    (both read the same builders), plus a digest of all of it.
//! 2. The user reads that plan in `ProvisionConfirmDialog` and accepts it; the
//!    caller sends back the digest. [`start_daemon`] / [`configure_password`]
//!    rebuild the plan from the STORED connection and refuse when the digest
//!    differs — so what runs is what was shown, and an edit in between means
//!    reviewing again, not running something nobody saw.
//!
//! Every entry point takes a connection id only. Host, user, port, instance and
//! credentials are read from `connections.json` and the vault, never from the
//! caller, so the HTTP routes cannot be turned into "run this over SSH there".
//!
//! Starting a daemon is `remote_deploy::deploy_ephemeral_for`: the binary is the
//! release asset pinned to this app's version and checked against its published
//! SHA-256 (512 MiB cap, 300 s timeout, Windows hosts refused), launched on
//! loopback with a pairing token read from stdin. Setting a password runs
//! `tuic-remote --set-password-if-unset` over the same SSH channel with the
//! saved username and password on stdin; the daemon itself refuses when it
//! already has credentials, so an existing password is never overwritten.
//! Neither secret appears in a command line, a log line or a response.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::remote_connection::{RemoteConnection, RemoteTransport};
use crate::state::AppState;

/// Upper bound for the `--set-password-if-unset` round trip. bcrypt cost 12 on
/// a slow remote takes about a second; the rest is the SSH handshake.
const SET_PASSWORD_TIMEOUT: Duration = Duration::from_secs(30);

/// Which offer a plan answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProvisionAction {
    /// Install (when the pinned binary is missing or different) and start
    /// `tuic-remote` on the remote host.
    Start,
    /// Give a daemon that has no credentials the connection's saved ones.
    SetPassword,
}

/// One thing the plan does on the remote host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProvisionStep {
    pub(crate) description: String,
    /// The exact remote shell command, when the step is one. `None` for a step
    /// that is a local action (an `scp` upload of the pinned asset).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
}

/// Everything a confirmation shows, and the digest that binds the accept to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProvisionPlan {
    pub(crate) connection_id: String,
    pub(crate) connection_name: String,
    pub(crate) action: ProvisionAction,
    /// `user@host:port` the commands run on.
    pub(crate) destination: String,
    pub(crate) summary: String,
    pub(crate) steps: Vec<ProvisionStep>,
    /// SHA-256 over everything above. The execute calls must echo it back.
    pub(crate) digest: String,
}

/// The SSH-specific parts of a stored connection, or why it has none.
struct SshTarget<'a> {
    ssh: &'a crate::ssh_connection::SshConnectionParams,
    remote_daemon_port: u16,
    start_if_not_running: bool,
    leave_running_on_disconnect: bool,
    instance_id: Option<&'a str>,
}

fn ssh_target(connection: &RemoteConnection) -> Result<SshTarget<'_>, String> {
    match &connection.transport {
        RemoteTransport::Ssh {
            ssh,
            remote_daemon_port,
            start_if_not_running,
            leave_running_on_disconnect,
            instance_id,
        } => Ok(SshTarget {
            ssh,
            remote_daemon_port: *remote_daemon_port,
            start_if_not_running: *start_if_not_running,
            leave_running_on_disconnect: *leave_running_on_disconnect,
            instance_id: instance_id.as_deref(),
        }),
        _ => Err("remote daemon provisioning needs a Remote Server — SSH connection".to_string()),
    }
}

/// The remote command `configure_password` runs: the binary `remote_deploy`
/// installs when present, otherwise `tuic-remote` from the remote `PATH`, always
/// in the refusing `--set-password-if-unset` mode. The instance id is the only
/// variable part and is validated as a DNS label.
pub(crate) fn set_password_command(instance: Option<&str>) -> Result<String, String> {
    let instance = crate::remote_deploy::instance_arg(instance)?;
    Ok(format!(
        "B=\"$HOME/.cache/tuic/tuic-remote\"; [ -x \"$B\" ] || B=tuic-remote; \"$B\"{instance} --set-password-if-unset"
    ))
}

/// The stdin `--set-password-if-unset` reads: username, then password, one
/// line each. `tuic-remote` trims each line, so a value it would alter (a line
/// break, or whitespace at either end) is refused here rather than silently
/// changed — the daemon must end up with exactly the saved secret.
pub(crate) fn set_password_stdin(username: &str, password: &str) -> Result<String, String> {
    if username.is_empty() {
        return Err("this connection has no Auth username to set".to_string());
    }
    if password.is_empty() {
        return Err("this connection has no saved password to set".to_string());
    }
    for (label, value) in [("username", username), ("password", password)] {
        if value.contains(['\r', '\n']) {
            return Err(format!("the saved {label} contains a line break"));
        }
        if value.trim() != value {
            return Err(format!(
                "the saved {label} starts or ends with whitespace, which tuic-remote would strip — set it on the remote host by hand"
            ));
        }
    }
    Ok(format!("{username}\n{password}\n"))
}

/// Replace every occurrence of `secret` in `text`. Applied to every error built
/// from remote output before it can reach a log or a response.
fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[redacted]")
    }
}

fn digest_of(
    connection_id: &str,
    action: ProvisionAction,
    destination: &str,
    summary: &str,
    steps: &[ProvisionStep],
) -> String {
    let body = serde_json::to_vec(&(connection_id, action, destination, summary, steps))
        .expect("plan parts always serialize");
    hex::encode(Sha256::digest(&body))
}

/// Build the plan for one stored connection. Pure: nothing is contacted and no
/// secret is read.
pub(crate) fn build_plan(
    connection: &RemoteConnection,
    action: ProvisionAction,
    password_saved: bool,
) -> Result<ProvisionPlan, String> {
    connection.validate()?;
    let target = ssh_target(connection)?;
    let destination = format!(
        "{}@{}:{}",
        target.ssh.user, target.ssh.host, target.ssh.port
    );
    let instance_note = target
        .instance_id
        .map(|id| format!(", instance {id}"))
        .unwrap_or_default();
    let (summary, steps) = match action {
        ProvisionAction::Start => {
            if !target.start_if_not_running {
                return Err(
                    "\"Start remote daemon if not running\" is off for this connection".to_string(),
                );
            }
            let version = env!("CARGO_PKG_VERSION");
            let launch = crate::remote_deploy::launch_command(
                target.remote_daemon_port,
                connection.survive_secs,
                target.instance_id,
            )?;
            (
                format!(
                    "Start tuic-remote v{version} on {destination} (127.0.0.1:{}{instance_note}).",
                    target.remote_daemon_port
                ),
                vec![
                    ProvisionStep {
                        description:
                            "Detect the remote platform (Linux or macOS; Windows hosts are refused)"
                                .to_string(),
                        command: Some(crate::remote_deploy::UNAME_COMMAND.to_string()),
                    },
                    ProvisionStep {
                        description: format!(
                            "Read the SHA-256 of the installed {}",
                            crate::remote_deploy::REMOTE_BINARY
                        ),
                        command: Some(crate::remote_deploy::hash_command()),
                    },
                    ProvisionStep {
                        description: format!(
                            "Only if it differs from tuic-remote v{version} for that platform — the release asset pinned to this app's version, checked against its published SHA-256 before use: upload it with scp to a temporary file next to {} and move it into place",
                            crate::remote_deploy::REMOTE_BINARY
                        ),
                        command: None,
                    },
                    ProvisionStep {
                        description: format!(
                            "Start it on 127.0.0.1:{} (stopping a previous daemon of the same instance only after checking its PID file names a tuic-remote); it exits {}s after its last client leaves. The pairing token is sent on stdin, never in the command",
                            target.remote_daemon_port, connection.survive_secs
                        ),
                        command: Some(launch),
                    },
                ],
            )
        }
        ProvisionAction::SetPassword => {
            let username = connection
                .auth_username
                .as_deref()
                .filter(|u| !u.is_empty())
                .ok_or("this connection has no Auth username to set")?;
            if !password_saved {
                return Err("this connection has no saved password to set".to_string());
            }
            (
                format!(
                    "Set the password of the tuic-remote daemon on {destination}{instance_note} — only if it has none yet."
                ),
                vec![
                    ProvisionStep {
                        description: format!(
                            "Give the daemon the username \"{username}\" and the password saved for this connection, sent on stdin. The daemon refuses if it already has credentials, so nothing is overwritten"
                        ),
                        command: Some(set_password_command(target.instance_id)?),
                    },
                    ProvisionStep {
                        description: "A running daemon reads its password when it starts: restart it afterwards for the password to take effect".to_string(),
                        command: None,
                    },
                ],
            )
        }
    };
    let digest = digest_of(&connection.id, action, &destination, &summary, &steps);
    Ok(ProvisionPlan {
        connection_id: connection.id.clone(),
        connection_name: connection.name.clone(),
        action,
        destination,
        summary,
        steps,
        digest,
    })
}

const PLAN_CHANGED: &str =
    "The connection changed since this plan was shown. Review the new plan; nothing was run.";

fn check_digest(plan: &ProvisionPlan, digest: &str) -> Result<(), String> {
    if plan.digest == digest {
        Ok(())
    } else {
        Err(PLAN_CHANGED.to_string())
    }
}

/// The plan for a stored connection.
pub(crate) fn plan_for(
    state: &Arc<AppState>,
    id: &str,
    action: ProvisionAction,
) -> Result<ProvisionPlan, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let password_saved = match action {
        ProvisionAction::SetPassword => crate::remote_connection::connection_password_exists(id)?,
        ProvisionAction::Start => false,
    };
    build_plan(&connection, action, password_saved)
}

/// Run an accepted Start plan, then connect.
///
/// The digest is checked against a plan rebuilt from the stored connection
/// before anything is contacted.
pub(crate) async fn start_daemon(
    state: &Arc<AppState>,
    id: &str,
    plan_digest: &str,
) -> Result<(), String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let plan = build_plan(&connection, ProvisionAction::Start, false)?;
    check_digest(&plan, plan_digest)?;
    let target = ssh_target(&connection)?;
    let profile = crate::remote_runtime::ssh_profile(&connection)
        .ok_or("remote daemon provisioning needs an SSH connection")?;
    let token = crate::remote_runtime::ensure_pairing_token(&connection.id)?;
    tracing::info!(
        source = "remote",
        connection = id,
        "Starting remote daemon (confirmed)"
    );
    crate::remote_deploy::deploy_ephemeral_for(
        &profile,
        target.remote_daemon_port,
        &token,
        connection.survive_secs,
        target.instance_id,
    )
    .await
    .map_err(|e| redact(&format!("Starting the remote daemon failed: {e}"), &token))?;
    state.remote.mark_provisioned(id);
    crate::remote_runtime::connect(state, id)
        .await
        .map_err(|e| format!("The remote daemon started, but connecting failed: {e}"))
}

/// Run an accepted SetPassword plan. Returns what the user should do next.
pub(crate) async fn configure_password(
    state: &Arc<AppState>,
    id: &str,
    plan_digest: &str,
) -> Result<String, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let password_saved = crate::remote_connection::connection_password_exists(id)?;
    let plan = build_plan(&connection, ProvisionAction::SetPassword, password_saved)?;
    check_digest(&plan, plan_digest)?;
    let target = ssh_target(&connection)?;
    let username = connection.auth_username.clone().unwrap_or_default();
    let password = crate::credentials::get(crate::credentials::Credential::RemoteConnection(id))?
        .ok_or("this connection has no saved password to set")?;
    let stdin = set_password_stdin(&username, &password)?;
    let command = set_password_command(target.instance_id)?;
    let profile = crate::remote_runtime::ssh_profile(&connection)
        .ok_or("remote daemon provisioning needs an SSH connection")?;
    tracing::info!(
        source = "remote",
        connection = id,
        "Setting remote daemon password (confirmed)"
    );
    match crate::tunnels::exec::ssh_exec(
        &profile,
        &command,
        Some(stdin.as_bytes()),
        SET_PASSWORD_TIMEOUT,
    )
    .await
    {
        Ok(_) => Ok(
            "Password set on the remote daemon. Restart the daemon so it takes effect, then Connect."
                .to_string(),
        ),
        Err(reason) => Err(classify_set_password_failure(
            &format!("{reason:?}"),
            &password,
        )),
    }
}

/// Turn a failed `--set-password-if-unset` run into a message: the daemon's own
/// refusal reads as "nothing was changed"; anything else is reported with the
/// password redacted.
fn classify_set_password_failure(detail: &str, password: &str) -> String {
    if detail.contains(crate::PASSWORD_ALREADY_CONFIGURED) {
        return "The remote daemon already has credentials; nothing was changed.".to_string();
    }
    if detail.contains("Unknown argument") {
        return "The remote tuic-remote is too old for --set-password-if-unset; nothing was changed. Start or update it from this app first.".to_string();
    }
    redact(
        &format!("Setting the remote password failed: {detail}"),
        password,
    )
}

/// Stop a daemon this app started for a connection, PID-verified. `Ok(true)`
/// when a `tuic-remote` was signalled.
pub(crate) async fn stop_daemon(state: &Arc<AppState>, id: &str) -> Result<bool, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let target = ssh_target(&connection)?;
    let profile = crate::remote_runtime::ssh_profile(&connection)
        .ok_or("remote daemon provisioning needs an SSH connection")?;
    let stopped = crate::remote_deploy::stop_ephemeral_for(&profile, target.instance_id)
        .await
        .map_err(|reason| format!("Stopping the remote daemon failed: {reason:?}"))?;
    state.remote.forget_provisioned(id);
    Ok(stopped)
}

/// Whether Disconnect stops the daemon: only one this app started through a
/// confirmed Start plan in this run, and only when the connection does not ask
/// to leave it running. A daemon somebody else started is never touched.
pub(crate) fn should_stop_on_disconnect(started_here: bool, leave_running: bool) -> bool {
    started_here && !leave_running
}

/// Called by Disconnect and delete before the runtime is torn down: forget the
/// "started here" mark and, when [`should_stop_on_disconnect`] says so, stop
/// the daemon in the background (bounded by `remote_deploy::STOP_TIMEOUT`).
pub(crate) fn stop_after_disconnect(state: &Arc<AppState>, id: &str) {
    let started_here = state.remote.take_provisioned(id);
    if !started_here {
        return;
    }
    let Ok(connection) = crate::remote_runtime::load_connection(state, id) else {
        return;
    };
    let Ok(target) = ssh_target(&connection) else {
        return;
    };
    if !should_stop_on_disconnect(started_here, target.leave_running_on_disconnect) {
        return;
    }
    let instance = target.instance_id.map(str::to_string);
    let Some(profile) = crate::remote_runtime::ssh_profile(&connection) else {
        return;
    };
    let id = id.to_string();
    tokio::spawn(async move {
        match crate::remote_deploy::stop_ephemeral_for(&profile, instance.as_deref()).await {
            Ok(stopped) => {
                tracing::info!(source = "remote", connection = %id, stopped, "Stopped remote daemon started by this app")
            }
            Err(reason) => {
                tracing::warn!(source = "remote", connection = %id, ?reason, "Could not stop remote daemon")
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tauri commands (HTTP parity: mcp_http/config_routes.rs)
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plan_ssh_daemon_provision(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    action: ProvisionAction,
) -> Result<ProvisionPlan, String> {
    plan_for(state.inner(), &id, action)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn start_ssh_daemon(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    plan_digest: String,
) -> Result<(), String> {
    start_daemon(state.inner(), &id, &plan_digest).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn stop_ssh_daemon(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<bool, String> {
    stop_daemon(state.inner(), &id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn configure_ssh_daemon_password(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    plan_digest: String,
) -> Result<String, String> {
    configure_password(state.inner(), &id, &plan_digest).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn provisioned_connection() -> RemoteConnection {
        let mut connection = RemoteConnection::new_ssh("box", "dev.example.test", "boss");
        if let RemoteTransport::Ssh {
            start_if_not_running,
            ..
        } = &mut connection.transport
        {
            *start_if_not_running = true;
        }
        connection
    }

    fn set_instance(connection: &mut RemoteConnection, id: &str) {
        if let RemoteTransport::Ssh { instance_id, .. } = &mut connection.transport {
            *instance_id = Some(id.to_string());
        }
    }

    #[test]
    fn the_start_plan_shows_exactly_the_commands_remote_deploy_sends() {
        let mut connection = provisioned_connection();
        set_instance(&mut connection, "dev-box");
        let plan = build_plan(&connection, ProvisionAction::Start, false).unwrap();
        let commands: Vec<&str> = plan
            .steps
            .iter()
            .filter_map(|s| s.command.as_deref())
            .collect();
        assert_eq!(
            commands,
            vec![
                crate::remote_deploy::UNAME_COMMAND.to_string(),
                crate::remote_deploy::hash_command(),
                crate::remote_deploy::launch_command(9877, 1_800, Some("dev-box")).unwrap(),
            ]
        );
        assert_eq!(plan.destination, "boss@dev.example.test:22");
        assert!(plan.summary.contains(env!("CARGO_PKG_VERSION")));
        assert!(plan.summary.contains("instance dev-box"));
        assert_eq!(plan.digest.len(), 64);
    }

    #[test]
    fn start_needs_the_connection_to_opt_in() {
        let connection = RemoteConnection::new_ssh("box", "h", "u");
        let err = build_plan(&connection, ProvisionAction::Start, false).unwrap_err();
        assert!(err.contains("Start remote daemon if not running"), "{err}");
    }

    #[test]
    fn non_ssh_connections_have_no_plan() {
        let connection = RemoteConnection::new_direct("d", "http://h:9877", "u");
        assert!(build_plan(&connection, ProvisionAction::Start, false).is_err());
        assert!(build_plan(&connection, ProvisionAction::SetPassword, true).is_err());
    }

    /// The digest binds an accept to one plan: any stored field that changes
    /// what runs changes it, so a stale accept is refused.
    #[test]
    fn the_digest_follows_every_field_that_changes_what_runs() {
        let base = provisioned_connection();
        let digest = build_plan(&base, ProvisionAction::Start, false)
            .unwrap()
            .digest;
        assert_eq!(
            digest,
            build_plan(&base, ProvisionAction::Start, false)
                .unwrap()
                .digest,
            "stable for an unchanged connection"
        );
        let mut changed = Vec::new();
        let mut c = base.clone();
        if let RemoteTransport::Ssh { ssh, .. } = &mut c.transport {
            ssh.host = "other.example.test".into();
        }
        changed.push(c);
        let mut c = base.clone();
        if let RemoteTransport::Ssh {
            remote_daemon_port, ..
        } = &mut c.transport
        {
            *remote_daemon_port = 9999;
        }
        changed.push(c);
        let mut c = base.clone();
        set_instance(&mut c, "other");
        changed.push(c);
        let mut c = base.clone();
        c.survive_secs = 60;
        changed.push(c);
        for c in changed {
            let plan = build_plan(&c, ProvisionAction::Start, false).unwrap();
            assert_ne!(plan.digest, digest);
            assert_eq!(check_digest(&plan, &digest).unwrap_err(), PLAN_CHANGED);
        }
    }

    #[test]
    fn hostile_instance_ids_produce_no_plan() {
        for hostile in ["a; reboot", "$(id)", "a`id`", "A", "default", "a b"] {
            let mut connection = provisioned_connection();
            set_instance(&mut connection, hostile);
            assert!(build_plan(&connection, ProvisionAction::Start, false).is_err());
            assert!(set_password_command(Some(hostile)).is_err());
        }
    }

    #[test]
    fn set_password_always_uses_the_refusing_mode() {
        let command = set_password_command(None).unwrap();
        assert!(command.ends_with("\"$B\" --set-password-if-unset"));
        assert!(!command.contains("--set-password "));
        let named = set_password_command(Some("dev-box")).unwrap();
        assert!(named.ends_with("\"$B\" --instance dev-box --set-password-if-unset"));
    }

    #[test]
    fn the_set_password_plan_carries_no_secret() {
        let mut connection = provisioned_connection();
        connection.auth_username = Some("boss".into());
        let plan = build_plan(&connection, ProvisionAction::SetPassword, true).unwrap();
        let text = serde_json::to_string(&plan).unwrap();
        assert!(text.contains("--set-password-if-unset"));
        assert!(text.contains("\\\"boss\\\""));
        assert!(!text.contains("password=") && !text.contains("TUIC_PAIRING_TOKEN=s"));
    }

    #[test]
    fn set_password_needs_a_username_and_a_saved_password() {
        let mut connection = provisioned_connection();
        connection.auth_username = None;
        assert!(build_plan(&connection, ProvisionAction::SetPassword, true).is_err());
        connection.auth_username = Some(String::new());
        assert!(build_plan(&connection, ProvisionAction::SetPassword, true).is_err());
        connection.auth_username = Some("boss".into());
        assert!(build_plan(&connection, ProvisionAction::SetPassword, false).is_err());
        assert!(build_plan(&connection, ProvisionAction::SetPassword, true).is_ok());
    }

    #[test]
    fn stdin_is_username_then_password_byte_for_byte() {
        assert_eq!(
            set_password_stdin("alice", "hunter2").unwrap(),
            "alice\nhunter2\n"
        );
        // Characters that would matter to a shell are fine: this is stdin.
        assert_eq!(
            set_password_stdin("alice", "p$w'\"; rm -rf /").unwrap(),
            "alice\np$w'\"; rm -rf /\n"
        );
    }

    #[test]
    fn stdin_refuses_values_tuic_remote_would_alter() {
        for (user, pass) in [
            ("alice", "pw\ninjected"),
            ("al\nice", "pw"),
            ("alice", " pw"),
            ("alice", "pw "),
            ("alice", ""),
            ("", "pw"),
        ] {
            assert!(set_password_stdin(user, pass).is_err(), "{user:?}/{pass:?}");
        }
    }

    #[test]
    fn failures_never_echo_the_password() {
        let message = classify_set_password_failure("Unknown(\"oops hunter2 oops\")", "hunter2");
        assert!(!message.contains("hunter2"), "{message}");
        assert!(message.contains("[redacted]"));
    }

    /// The daemon's own refusal is what guarantees "never overwrite": it is
    /// reported as nothing changed, and the check itself refuses either half.
    #[test]
    fn an_existing_password_is_refused_and_reported_as_unchanged() {
        let message = classify_set_password_failure(
            &format!("Unknown(\"Error: {}\")", crate::PASSWORD_ALREADY_CONFIGURED),
            "pw",
        );
        assert!(message.contains("nothing was changed"), "{message}");

        let mut auth = crate::config::AuthConfig::default();
        assert!(!crate::auth_credentials_configured(&auth));
        auth.password_hash = "$2b$12$x".into();
        assert!(crate::auth_credentials_configured(&auth));
        let auth = crate::config::AuthConfig {
            username: "someone".into(),
            ..Default::default()
        };
        assert!(crate::auth_credentials_configured(&auth));
    }

    #[test]
    fn disconnect_stops_only_a_daemon_started_here_unless_asked_to_leave_it() {
        assert!(should_stop_on_disconnect(true, false));
        assert!(!should_stop_on_disconnect(true, true));
        assert!(!should_stop_on_disconnect(false, false));
        assert!(!should_stop_on_disconnect(false, true));
    }

    fn state_with(connection: &RemoteConnection) -> (Arc<AppState>, ()) {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::remote_connection::RemoteConnectionStore::save(
            &state.data_dir,
            std::slice::from_ref(connection),
        )
        .unwrap();
        (state, ())
    }

    /// A stale digest is refused before any SSH: the stored host is a closed
    /// port, so reaching it would have produced an SSH error instead.
    #[tokio::test]
    async fn a_stale_digest_runs_nothing() {
        let mut connection = provisioned_connection();
        if let RemoteTransport::Ssh { ssh, .. } = &mut connection.transport {
            ssh.host = "127.0.0.1".into();
            ssh.port = 1;
        }
        connection.auth_username = Some("boss".into());
        let (state, _dir) = state_with(&connection);
        let err = start_daemon(&state, &connection.id, &"0".repeat(64))
            .await
            .unwrap_err();
        assert_eq!(err, PLAN_CHANGED);
        assert!(!state.remote.take_provisioned(&connection.id));
    }

    #[tokio::test]
    async fn unknown_ids_are_refused() {
        let connection = provisioned_connection();
        let (state, _dir) = state_with(&connection);
        assert!(plan_for(&state, "not-a-connection", ProvisionAction::Start).is_err());
        assert!(start_daemon(&state, "not-a-connection", "x").await.is_err());
        assert!(stop_daemon(&state, "not-a-connection").await.is_err());
    }
}
