//! Story092 batch 1: final ACP launch, initialize, and capability contract.
//!
//! These tests intentionally name the crate-visible ACP boundary that the
//! implementation must provide.  They do not exercise later session or turn
//! lifecycle behavior.

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::{ProtocolVersion, v1};
use serde_json::json;
use tuicommander_lib::acp::{
    AcpOperation, AcpUnavailableReason, EgoAcpConfig, build_initialize_request,
    capability_snapshot, launch_spec,
};

mod acp_support;

#[test]
fn configured_ego_launch_is_direct_and_has_exact_argv() {
    let executable = PathBuf::from("/opt/ego/bin/ego");
    let root = PathBuf::from("/private/tmp/worktree");
    let spec = launch_spec(&EgoAcpConfig { executable }, &root).unwrap();

    assert_eq!(spec.program, Path::new("/opt/ego/bin/ego"));
    assert_eq!(spec.args, ["acp", "-C", "/private/tmp/worktree"]);
}

#[test]
fn initialize_is_v1_and_advertises_exact_client_capabilities() {
    let request = build_initialize_request();
    assert_eq!(request.protocol_version, ProtocolVersion::V1);
    assert_eq!(
        serde_json::to_value(&request.client_capabilities).unwrap(),
        json!({
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false
        })
    );
}

/// The bytes ego actually sends, not the shape this plan predicted it would.
///
/// Two things were guessed wrong and both made the extension undiscoverable.
/// The metadata hangs off `agentCapabilities`, not off the response, because it
/// *is* a capability; and pause and resume arrive as one `hold` entry carrying
/// both method names under one version, not as two independently versioned
/// entries. The second is the better contract and the reason to follow it
/// rather than ask ego to change: they are two halves of one extension, and two
/// versions can disagree — a `pause` at 1 with a `resume` at 2 is a state no
/// agent can be in and every client would then have to decide what to do about.
///
/// Verified against `ego`'s `crates/ego-acp/src/serve.rs` capability builder,
/// its `host_hold_091_red.rs` assertions and `docs/06-acp.md` §4.
fn ego_initialize_response() -> serde_json::Value {
    json!({
        "protocolVersion": 1,
        "agentInfo": {"name": "ego", "title": "Ego", "version": "1"},
        "agentCapabilities": {
            "loadSession": true,
            "sessionCapabilities": {"list": {}, "delete": {}, "additionalDirectories": {}, "fork": {}, "resume": {}, "close": {}},
            "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
            "mcpCapabilities": {"http": true, "sse": true},
            "_meta": {"ego": {
                "hold": {"version": 1, "pause": "_ego/pause", "resume": "_ego/resume"},
                "compact": {"version": 1, "method": "_ego/compact"}
            }}
        }
    })
}

#[test]
fn initialize_response_becomes_an_immutable_full_capability_snapshot() {
    let response: v1::InitializeResponse =
        serde_json::from_value(ego_initialize_response()).unwrap();
    let snapshot = capability_snapshot(&response).unwrap();
    assert!(snapshot.load && snapshot.list && snapshot.resume && snapshot.fork);
    assert!(snapshot.delete && snapshot.close && snapshot.prompt_image);
    assert!(snapshot.prompt_embedded_context && snapshot.mcp_http && snapshot.mcp_sse);
    assert!(!snapshot.mcp_stdio);
    assert!(!snapshot.client_form_elicitation);
    assert!(!snapshot.client_boolean_config);
    assert_eq!(
        snapshot.availability(AcpOperation::McpStdio).reason,
        Some(AcpUnavailableReason::ExcludedByContract)
    );
    assert_eq!(
        snapshot
            .availability(AcpOperation::ClientFormElicitation)
            .reason,
        Some(AcpUnavailableReason::ExcludedByContract)
    );
    assert_eq!(
        snapshot
            .availability(AcpOperation::ClientBooleanConfig)
            .reason,
        Some(AcpUnavailableReason::ExcludedByContract)
    );
    assert_eq!(snapshot.ego_hold_version, Some(1));
    assert_eq!(snapshot.ego_compact_version, Some(1));
    assert!(snapshot.availability(AcpOperation::Pause).available);
    assert!(snapshot.availability(AcpOperation::ResumeTurn).available);
    assert_eq!(snapshot.protocol, ProtocolVersion::V1);
}

#[test]
fn absent_or_mismatched_capabilities_are_rejected_before_write() {
    let response: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1,
        "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&response).unwrap();

    assert_eq!(
        snapshot.availability(AcpOperation::Load).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::Compact).reason,
        Some(AcpUnavailableReason::NotAdvertised)
    );

    let mismatched: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1, "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {"_meta": {"ego": {
            "hold": {"version": 2, "pause": "_ego/pause", "resume": "_ego/resume"},
            "compact": {"version": 1, "method": "_ego/compact"}
        }}}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&mismatched).unwrap();
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert_eq!(
        snapshot.availability(AcpOperation::ResumeTurn).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert!(snapshot.availability(AcpOperation::Compact).available);

    // A version this client knows, spelled with a method name it does not.
    // Guessing that `_ego/hold` is `_ego/pause` under another name is exactly
    // the guess the snapshot exists to refuse.
    let renamed: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1, "agentInfo": {"name": "ego", "version": "1"},
        "agentCapabilities": {"_meta": {"ego": {
            "hold": {"version": 1, "pause": "_ego/hold", "resume": "_ego/resume"}
        }}}
    }))
    .unwrap();
    let snapshot = capability_snapshot(&renamed).unwrap();
    assert_eq!(
        snapshot.availability(AcpOperation::Pause).reason,
        Some(AcpUnavailableReason::WrongExtensionVersion)
    );
    assert_eq!(snapshot.ego_hold_version, None);
}

/// The pinned client and the pinned agent agree about the extension.
///
/// The unit test above uses a hand-written response; this one takes the bytes
/// off the wire from the fixture agent, whose `ready` scenario is the recorded
/// ego shape. Without it, both ends of the check would be this repository's own
/// opinion of what ego sends — which is precisely how the shape went wrong.
#[tokio::test]
async fn the_agent_this_client_launches_advertises_the_hold_extension() {
    let fixture = acp_support::Fixture::with("ready");
    let snapshot = fixture.connect().await;
    let capabilities = snapshot
        .capabilities
        .expect("a ready connection is negotiated");

    assert_eq!(capabilities.ego_hold_version, Some(1));
    assert_eq!(capabilities.ego_compact_version, Some(1));
    assert!(capabilities.availability(AcpOperation::Pause).available);
    assert!(
        capabilities
            .availability(AcpOperation::ResumeTurn)
            .available
    );
    assert!(capabilities.availability(AcpOperation::Compact).available);
    assert!(capabilities.load && capabilities.list && capabilities.fork);

    fixture
        .manager
        .disconnect(snapshot.connection_id)
        .await
        .unwrap();
}
