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
            "terminal": false,
            "session": {"configOptions": {"boolean": {}}},
            "elicitation": {"form": {}}
        })
    );
}

#[test]
fn initialize_response_becomes_an_immutable_full_capability_snapshot() {
    let response: v1::InitializeResponse = serde_json::from_value(json!({
        "protocolVersion": 1,
        "agentInfo": {"name": "ego", "title": "Ego", "version": "1"},
        "agentCapabilities": {
            "loadSession": true,
            "sessionCapabilities": {"list": {}, "delete": {}, "additionalDirectories": {}, "fork": {}, "resume": {}, "close": {}},
            "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
            "mcpCapabilities": {"http": true, "sse": true}
        },
        "_meta": {"ego": {
            "pause": {"version": 1, "method": "_ego/pause"},
            "resume": {"version": 1, "method": "_ego/resume"},
            "compact": {"version": 1, "method": "_ego/compact"}
        }}
    })).unwrap();
    let snapshot = capability_snapshot(&response).unwrap();
    assert!(snapshot.load && snapshot.list && snapshot.resume && snapshot.fork);
    assert!(snapshot.delete && snapshot.close && snapshot.prompt_image);
    assert!(snapshot.prompt_embedded_context && snapshot.mcp_http && snapshot.mcp_sse);
    assert_eq!(snapshot.ego_pause_version, Some(1));
    assert_eq!(snapshot.ego_resume_version, Some(1));
    assert_eq!(snapshot.ego_compact_version, Some(1));
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
        "agentCapabilities": {}, "_meta": {"ego": {
            "pause": {"version": 2, "method": "_ego/pause"},
            "resume": {"version": 1, "method": "wrong"},
            "compact": {"version": 1, "method": "_ego/compact"}
        }}
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
}
