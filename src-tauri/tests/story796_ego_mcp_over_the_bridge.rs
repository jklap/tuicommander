//! ego reaches TUICommander over the stdio bridge, not a TCP port.
//!
//! The entry a session gets used to be an HTTP URL built from the port this
//! process bound. That port exists only when the TCP listener binds, and the
//! listener binds only when Remote Access is on — which is off by default. So
//! the default install handed ego no server at all, and the fix was never a
//! second listener: `<config dir>/mcp.sock` already binds unconditionally and
//! `tuic-bridge` is already shipped as the MCP-stdio adapter in front of it.

use agent_client_protocol::schema::{ProtocolVersion, v1};

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
