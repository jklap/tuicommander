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

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpAttachKind, AcpAttachmentState, AcpClientErrorCode, AcpDetachKind, AcpOperation,
    AcpSessionAuthority,
};

mod acp_support;

use acp_support::{Fixture, authority};

/// The session ids the scenarios answer with, spelled once.
const FIRST: &str = "01932d5e-0000-7000-8000-0000000000aa";
const FORKED: &str = "01932d5e-0000-7000-8000-0000000000bb";

fn session(id: &str) -> v1::SessionId {
    v1::SessionId::new(id)
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
    assert!(!capabilities.availability(AcpOperation::List).available);

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

/// Loading attaches a session ego already owns; closing lets go of it.
///
/// The id is not the agent's to choose here — it is the one the caller named,
/// and a client that took the durable id from anywhere but the caller's request
/// would attach to a session nobody asked for.
#[tokio::test]
async fn a_loaded_session_attaches_under_the_id_that_was_asked_for() {
    let fixture = Fixture::with("session-attach");
    let connection = fixture.connect().await;

    let attachment = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Load,
            session(FIRST),
            authority(fixture.root()),
        )
        .await
        .expect("session/load");
    assert_eq!(attachment.session_id, session(FIRST));
    assert_eq!(attachment.state, AcpAttachmentState::Idle);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![attachment]);

    fixture
        .manager
        .detach(
            connection.connection_id,
            AcpDetachKind::Close,
            session(FIRST),
        )
        .await
        .expect("session/close");

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a closed session stayed attached: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A fork is a second session, and the original keeps its own attachment.
///
/// Ego owns the lineage; what the client must not do is treat the fork's new
/// durable id as a rename of the one it forked from. Losing the original here
/// would silently detach a session that is still perfectly alive.
#[tokio::test]
async fn a_fork_attaches_the_new_id_and_keeps_the_original() {
    let fixture = Fixture::with("session-fork");
    let connection = fixture.connect().await;
    let root = fixture.root();

    let original = fixture
        .manager
        .new_session(connection.connection_id, authority(root.clone()))
        .await
        .expect("session/new");
    assert_eq!(original.session_id, session(FIRST));

    let fork = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Fork,
            session(FIRST),
            authority(root),
        )
        .await
        .expect("session/fork");
    assert_eq!(fork.session_id, session(FORKED));

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(snapshot.attachments, vec![original, fork]);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_resumed_session_attaches_and_a_deleted_one_detaches() {
    let fixture = Fixture::with("session-resume-delete");
    let connection = fixture.connect().await;

    let attachment = fixture
        .manager
        .attach(
            connection.connection_id,
            AcpAttachKind::Resume,
            session(FIRST),
            authority(fixture.root()),
        )
        .await
        .expect("session/resume");
    assert_eq!(attachment.session_id, session(FIRST));

    fixture
        .manager
        .detach(
            connection.connection_id,
            AcpDetachKind::Delete,
            session(FIRST),
        )
        .await
        .expect("session/delete");

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(
        snapshot.attachments.is_empty(),
        "a deleted session stayed attached: {snapshot:?}"
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Every lifecycle operation is gated on the capability that names it.
///
/// One test per operation would prove the same thing five times; what matters
/// is that no operation was left ungated, which is a statement about the set.
#[tokio::test]
async fn no_lifecycle_operation_reaches_an_agent_that_advertised_none() {
    let fixture = Fixture::with("no-lifecycle");
    let connection = fixture.connect().await;
    let root = fixture.root();

    for (kind, operation) in [
        (AcpAttachKind::Load, AcpOperation::Load),
        (AcpAttachKind::Fork, AcpOperation::Fork),
        (AcpAttachKind::Resume, AcpOperation::Resume),
    ] {
        let error = fixture
            .manager
            .attach(
                connection.connection_id,
                kind,
                session(FIRST),
                authority(root.clone()),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        assert_eq!(error.operation, Some(operation));
    }

    for (kind, operation) in [
        (AcpDetachKind::Close, AcpOperation::Close),
        (AcpDetachKind::Delete, AcpOperation::Delete),
    ] {
        let error = fixture
            .manager
            .detach(connection.connection_id, kind, session(FIRST))
            .await
            .unwrap_err();
        assert_eq!(error.code, AcpClientErrorCode::CapabilityUnavailable);
        assert_eq!(error.operation, Some(operation));
    }

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
