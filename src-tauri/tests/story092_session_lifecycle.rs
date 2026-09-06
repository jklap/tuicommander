//! Story092 batch 3: durable session operations across the connection.
//!
//! The connection layer could be proved without ever sending a second request,
//! because everything before this happened inside `initialize`. From here the
//! manager has to reach a live connection *after* `connect` returned, which is
//! the thing the earlier shape could not do at all: the SDK connection lived
//! only inside the supervisor closure and nothing outside could speak on it.
//!
//! Ego owns durable session identity, history, lineage and state. What these
//! tests are about is the client half — that a returned durable id becomes an
//! attachment carrying the authority it was created with, that an operation the
//! agent never advertised is refused before a byte is written, and that a
//! refusal leaves nothing attached.

use tuicommander_lib::acp::{
    AcpAttachmentState, AcpClientErrorCode, AcpOperation, AcpSessionAuthority,
};

mod acp_support;

use acp_support::Fixture;

fn authority(cwd: std::path::PathBuf) -> AcpSessionAuthority {
    AcpSessionAuthority {
        cwd,
        additional_directories: Vec::new(),
        mcp_servers: Vec::new(),
    }
}

#[tokio::test]
async fn a_new_session_attaches_the_durable_id_the_agent_returned() {
    let fixture = Fixture::with("session-new");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let attachment = fixture
        .manager
        .new_session(connection.connection_id, authority(root.clone()))
        .await
        .expect("session/new");

    assert_eq!(attachment.state, AcpAttachmentState::Idle);
    assert_eq!(attachment.cwd, root);
    assert!(attachment.active_turn.is_none());
    assert!(attachment.pending_permission_ids.is_empty());

    // The attachment is connection state, so the connection snapshot carries it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![attachment.clone()]);

    let listed = fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect("session/list");
    assert_eq!(listed.sessions.len(), 1);
    assert_eq!(listed.sessions[0].session_id, attachment.session_id);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An unadvertised operation is refused before the wire, not after it.
///
/// The scenario asserts the silence: its last step reads stdin and fails if
/// anything arrived. A client that optimistically sent `session/list` and let
/// the agent say `method_not_found` would pass a test that only looked at the
/// error, and would have told the agent about a session it has no business
/// knowing.
#[tokio::test]
async fn an_unadvertised_operation_is_refused_without_writing_a_byte() {
    let fixture = Fixture::with("no-lifecycle");
    let connection = fixture.connect().await;
    let capabilities = connection.capabilities.as_ref().expect("negotiated");
    assert_eq!(
        capabilities.availability(AcpOperation::List).available,
        false
    );

    let error = fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect_err("an unadvertised list is refused");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert_eq!(error.operation, Some(AcpOperation::List));
    assert!(!error.retryable);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Extra roots are refused when the agent never said it honours them.
///
/// `session/new` itself is baseline, so nothing stops the request from going
/// out — which is the danger. An agent that does not know the field ignores it
/// and answers with a session, and the caller is handed a session it believes
/// spans three directories while the agent will only ever touch one. Ego is
/// exactly such an agent: its advertised `sessionCapabilities` carry no
/// `additionalDirectories`.
#[tokio::test]
async fn extra_roots_are_refused_when_the_agent_never_advertised_them() {
    let fixture = Fixture::with("no-extra-roots");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let error = fixture
        .manager
        .new_session(
            connection.connection_id,
            AcpSessionAuthority {
                cwd: root.clone(),
                additional_directories: vec![root.join("elsewhere")],
                mcp_servers: Vec::new(),
            },
        )
        .await
        .expect_err("extra roots this agent never advertised");
    assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
    assert_eq!(error.operation, Some(AcpOperation::AdditionalDirectories));
    assert!(!error.retryable);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_refused_new_session_leaves_the_connection_with_no_attachment() {
    let fixture = Fixture::with("refuses-new");
    let connection = fixture.connect().await;

    let error = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect_err("the agent refused");
    assert_eq!(error.code, AcpClientErrorCode::AgentError);
    assert_eq!(error.connection_id, Some(connection.connection_id));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a refused session was attached anyway: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A settled connection admits no further session work.
///
/// Reaching a dead process would either hang on a reply that never comes or
/// look like a fresh failure of the operation. It is neither: the connection is
/// gone, and the caller has to make a new one.
#[tokio::test]
async fn a_settled_connection_refuses_session_work_as_transport_closed() {
    let fixture = Fixture::with("session-new");
    let connection = fixture.connect().await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();

    let error = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect_err("a settled connection carries no session");
    assert_eq!(error.code, AcpClientErrorCode::TransportClosed);
}
