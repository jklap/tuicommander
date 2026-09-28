//! ego reaches TUICommander over the stdio bridge, not a TCP port.
//!
//! The entry a session gets used to be an HTTP URL built from the port this
//! process bound. That port exists only when the TCP listener binds, and the
//! listener binds only when Remote Access is on — which is off by default. So
//! the default install handed ego no server at all, and the fix was never a
//! second listener: `<config dir>/mcp.sock` already binds unconditionally and
//! `tuic-bridge` is already shipped as the MCP-stdio adapter in front of it.

use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde_json::Value;
use std::path::PathBuf;
use tuicommander_lib::acp::{TUICOMMANDER_MCP_SERVER_NAME, tuicommander_mcp_server};

/// The entry names our own sidecar, and nothing else.
///
/// A stdio MCP server is a command line the agent executes, which is exactly
/// why this one is built here instead of being accepted from a caller: the
/// path is the binary we ship, resolved beside our own executable. A request
/// body reaches neither the name nor the command.
#[test]
fn the_synthesised_entry_runs_our_own_bridge() {
    let server = tuicommander_mcp_server(Some(PathBuf::from("/opt/tuic/tuic-bridge")), None, None)
        .expect("a located bridge must yield an entry");

    let value = serde_json::to_value(&server).expect("the entry must serialize");
    assert_eq!(
        value.get("name").and_then(Value::as_str),
        Some(TUICOMMANDER_MCP_SERVER_NAME)
    );
    assert_eq!(
        value.get("command").and_then(Value::as_str),
        Some("/opt/tuic/tuic-bridge")
    );
    assert_eq!(
        value.get("args"),
        Some(&serde_json::json!([])),
        "the bridge needs no arguments to find the socket: {value}"
    );
    assert!(
        value.get("url").is_none(),
        "an HTTP url here would mean the TCP listener is back in the path: {value}"
    );
}

/// A named instance must reach its own socket, not Boss's.
///
/// `TUIC_APP_INSTANCE` used to be the variable here, and the bridge never read
/// it: `tuic-bridge` resolves `TUIC_SOCKET`, then `<config dir>/mcp.sock`, then
/// any `mcp-*.sock` beside it, while a named instance binds
/// `$TMPDIR/tuic-mcp-<sha>.sock`. With both instances running, ego drove the
/// DEFAULT one — a test build steering Boss's repositories. The bound path is
/// also the only answer that survives a primary socket already held, where this
/// process binds a `-<pid>` alternative no id could have predicted.
#[test]
fn the_bound_socket_is_carried_to_the_bridge_rather_than_left_to_its_own_search() {
    let server = tuicommander_mcp_server(
        Some(PathBuf::from("/opt/tuic/tuic-bridge")),
        Some(std::path::Path::new("/tmp/tuic-mcp-0badc0de.sock")),
        None,
    )
    .expect("a located bridge must yield an entry");

    let value = serde_json::to_value(&server).expect("the entry must serialize");
    assert_eq!(
        value.get("env"),
        Some(&serde_json::json!([
            { "name": "TUIC_SOCKET", "value": "/tmp/tuic-mcp-0badc0de.sock" }
        ])),
        "the bound socket must travel with the entry: {value}"
    );
    let env = value["env"].to_string();
    assert!(
        !env.contains("TUIC_APP_INSTANCE"),
        "the instance id is not what the bridge reads, and sending it says otherwise: {env}"
    );
}

/// No socket bound yet means no variable, rather than an empty one.
///
/// An empty `TUIC_SOCKET` is not "search as usual" to the bridge — it is a path
/// it will try and fail to connect to. Absent is the honest answer.
#[test]
fn an_unbound_socket_sets_no_environment_at_all() {
    let server = tuicommander_mcp_server(Some(PathBuf::from("/opt/tuic/tuic-bridge")), None, None)
        .expect("a located bridge must yield an entry");

    let value = serde_json::to_value(&server).expect("the entry must serialize");
    assert_eq!(value.get("env"), Some(&serde_json::json!([])));
}

/// No bridge means no entry, and no entry means no tools — not a guess.
///
/// The previous shape refused to build `http://127.0.0.1:0/mcp` for the same
/// reason: an entry that cannot answer makes ego report a server it could not
/// admit, when the truth is that TUICommander was not ready.
#[test]
fn a_missing_bridge_yields_no_entry_rather_than_a_command_that_cannot_run() {
    assert!(tuicommander_mcp_server(None, None, None).is_none());
    assert!(
        tuicommander_mcp_server(
            None,
            Some(std::path::Path::new("/tmp/tuic-mcp-0badc0de.sock")),
            None,
        )
        .is_none(),
        "a bound socket is not a substitute for a binary"
    );
}

/// Stdio is a transport this client carries, so the snapshot must say so.
///
/// v1 has no `mcpCapabilities.stdio` field at all — only `http` and `sse` — so
/// stdio is the protocol baseline and there is nothing for an agent to
/// advertise. `mcp_stdio` was set to `false` back when this client genuinely
/// carried no stdio server; leaving it there now would make the snapshot and
/// the behaviour disagree in the other direction.
#[test]
fn the_snapshot_reports_the_stdio_transport_this_client_carries() {
    let snapshot = tuicommander_lib::acp::capability_snapshot(&initialize_response(false, false))
        .expect("a v1 response must produce a snapshot");

    assert!(
        snapshot.mcp_stdio,
        "the client hands ego a stdio server, so the snapshot cannot deny the transport"
    );
}

/// Advertising stdio must not become a free pass for the other transports.
///
/// `http` and `sse` are real capabilities an agent either published or did
/// not, and the gate exists because an agent that answers `http == false`
/// still returns a session — so an ungated HTTP entry buys a session and no
/// tools, with nothing saying why.
#[test]
fn an_unadvertised_transport_is_still_refused() {
    let snapshot = tuicommander_lib::acp::capability_snapshot(&initialize_response(false, false))
        .expect("a v1 response must produce a snapshot");
    assert!(!snapshot.mcp_http);
    assert!(!snapshot.mcp_sse);

    let advertised = tuicommander_lib::acp::capability_snapshot(&initialize_response(true, true))
        .expect("a v1 response must produce a snapshot");
    assert!(advertised.mcp_http);
    assert!(advertised.mcp_sse);
}

fn initialize_response(http: bool, sse: bool) -> v1::InitializeResponse {
    let mut response = v1::InitializeResponse::new(ProtocolVersion::V1);
    response.agent_capabilities.mcp_capabilities.http = http;
    response.agent_capabilities.mcp_capabilities.sse = sse;
    response
}
