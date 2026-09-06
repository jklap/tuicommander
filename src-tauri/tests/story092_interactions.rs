//! Story092 batch 5a: the questions the agent asks back.
//!
//! Everything else on this connection is the client asking and the agent
//! answering. These two methods run the other way, and they are the only place
//! where a *person* is on the critical path: the agent stops, and stays
//! stopped, until someone decides.
//!
//! That is what makes the seat the unit here. The SDK hands the responder to a
//! callback that holds the dispatch loop, so answering there would freeze the
//! connection for as long as the person takes — including the updates that say
//! what they are being asked about. The responder is therefore carried out of
//! the callback and parked, and what is proved below is that it is always
//! answered exactly once: by the person, by the cancel that took the question
//! away from them, or immediately when there was no seat to offer.

use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{
    AcpClientErrorCode, AcpClientEvent, AcpEventEnvelope, AcpHostRequestId, AcpPendingInteraction,
};

mod acp_support;

use acp_support::{Fixture, authority, chunk, text, until};

const SESSION: &str = "01932d5e-0000-7000-8000-0000000000aa";
const OTHER_SESSION: &str = "01932d5e-0000-7000-8000-0000000000bb";

fn selected(option: &str) -> v1::RequestPermissionOutcome {
    v1::RequestPermissionOutcome::Selected(v1::SelectedPermissionOutcome::new(
        v1::PermissionOptionId::new(option),
    ))
}

/// The id the agent's question was seated under.
///
/// Reading it off the stream rather than off a snapshot is deliberate: the
/// stream is the channel a frontend actually renders from, and an id it cannot
/// see there is one it cannot answer with.
fn asked(seen: &[AcpEventEnvelope]) -> AcpHostRequestId {
    seen.iter()
        .find_map(|event| match &event.event {
            AcpClientEvent::PermissionRequested { request_id, .. }
            | AcpClientEvent::ElicitationRequested { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no question was seated: {seen:?}"))
}

/// A person's answer reaches the agent, and the turn goes on around it.
///
/// The two chunks either side of the question are the point: the client is not
/// blocked while a person thinks, and what the agent said before it asked is
/// filed before the asking, not after.
#[tokio::test]
async fn a_permission_is_seated_answered_and_the_answer_reaches_the_agent() {
    let fixture = Fixture::with("permission-turn");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");

    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");

    let asking = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PermissionRequested { .. })
    })
    .await;
    let request_id = asked(&asking);
    assert_eq!(
        asking.iter().filter_map(chunk).collect::<Vec<_>>(),
        ["before"],
        "what the agent said before it asked is filed before the asking"
    );

    // The question is on the attachment too, so a frontend that was not
    // listening when it was asked still finds it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        vec![request_id]
    );
    let pending = fixture
        .manager
        .pending_interactions(connection.connection_id)
        .await
        .expect("pending interactions");
    let [
        AcpPendingInteraction::Permission {
            request_id: listed,
            session_id,
            request,
        },
    ] = pending.as_slice()
    else {
        panic!("exactly one permission is open: {pending:?}");
    };
    assert_eq!(*listed, request_id);
    assert_eq!(session_id.0.as_ref(), SESSION);
    assert_eq!(request.tool_call.tool_call_id.0.as_ref(), "call-1");

    let settlement = fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the answer is accepted");
    assert_eq!(settlement.request_id, request_id);

    // The scenario asserts the wire answer itself; a turn that ends proves the
    // agent read it, because the fixture will not go on until it has.
    let rest = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    assert!(matches!(
        rest.first().map(|event| &event.event),
        Some(AcpClientEvent::PermissionSettled { .. })
    ));
    assert_eq!(rest.iter().filter_map(chunk).collect::<Vec<_>>(), ["after"]);

    // A settled seat is gone from both places that listed it.
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(snapshot.attachments[0].pending_permission_ids.is_empty());
    assert!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions")
            .is_empty()
    );

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// An option the agent never offered is refused here, and the seat stays open.
///
/// Refusing locally rather than forwarding matters because the agent has to
/// treat the answer as authoritative. Leaving the seat open matters more: a
/// frontend that sent a stale option id gets to try again, instead of having
/// silently cancelled the question on the person's behalf.
#[tokio::test]
async fn an_option_the_agent_never_offered_is_refused_and_the_seat_stays_open() {
    let fixture = Fixture::with("permission-turn");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");
    let request_id = asked(
        &until(&mut stream, |event| {
            matches!(event, AcpClientEvent::PermissionRequested { .. })
        })
        .await,
    );

    let error = fixture
        .manager
        .respond_permission(
            connection.connection_id,
            request_id,
            selected("allow-always"),
        )
        .await
        .expect_err("an option that was never offered is not an answer");
    assert_eq!(error.code, AcpClientErrorCode::InvalidInput);
    assert!(
        error.message.contains("allow-always"),
        "the refusal names the option it refused: {error:?}"
    );

    // Still open, and still answerable.
    assert_eq!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions")
            .len(),
        1
    );
    fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect("the seat was still there to answer");

    until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;

    // And answering it twice is not a second answer.
    let error = fixture
        .manager
        .respond_permission(connection.connection_id, request_id, selected("allow"))
        .await
        .expect_err("a settled question cannot be answered again");
    assert_eq!(error.code, AcpClientErrorCode::NotFound);

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// Cancelling a turn answers every question that turn was waiting on.
///
/// ACP requires the `Cancelled` outcome for pending permissions on cancel, and
/// the same reasoning covers elicitations: a person is not going to answer a
/// question belonging to a turn that is being abandoned, and an unanswered
/// request would leave the agent waiting on it forever.
#[tokio::test]
async fn cancelling_a_turn_settles_every_question_it_was_waiting_on() {
    let fixture = Fixture::with("permission-cancel");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("hello")],
        )
        .await
        .expect("session/prompt");

    let asking = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::ElicitationRequested { .. })
    })
    .await;
    let mut open = asking.iter().filter_map(|event| match &event.event {
        AcpClientEvent::PermissionRequested { request_id, .. }
        | AcpClientEvent::ElicitationRequested { request_id, .. } => Some(*request_id),
        _ => None,
    });
    let permission = open.next().expect("the permission was asked first");
    let elicitation = open.next().expect("the elicitation was asked second");
    assert_ne!(permission, elicitation);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].pending_permission_ids,
        vec![permission]
    );
    assert_eq!(
        snapshot.attachments[0].pending_elicitation_ids,
        vec![elicitation]
    );

    // The scenario reads the two answers before `session/cancel`, so a client
    // that told the agent to stop while still holding a question would fail
    // there rather than here.
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .expect("session/cancel");

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::TurnSettled { .. })
    })
    .await;
    let settlements: Vec<_> = seen
        .iter()
        .filter_map(|event| match &event.event {
            AcpClientEvent::PermissionSettled {
                request_id,
                outcome,
            } => {
                assert_eq!(*outcome, v1::RequestPermissionOutcome::Cancelled);
                Some(*request_id)
            }
            AcpClientEvent::ElicitationSettled { request_id, action } => {
                assert!(matches!(action, v1::ElicitationAction::Cancel));
                Some(*request_id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        settlements,
        vec![permission, elicitation],
        "the sweep answers in the order the agent asked"
    );

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(snapshot.attachments[0].pending_permission_ids.is_empty());
    assert!(snapshot.attachments[0].pending_elicitation_ids.is_empty());

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

/// A question this client has no seat for is declined without a person.
///
/// Both cases name nothing this client could render against: an elicitation
/// scoped to a JSON-RPC request belongs to the phase before a session exists,
/// and a permission for a session this connection is not attached to has no
/// view to appear in. Holding either would leave the agent waiting on a human
/// who is never going to be shown the question.
#[tokio::test]
async fn a_question_with_no_seat_is_declined_rather_than_held() {
    let fixture = Fixture::with("interaction-unseatable");
    let connection = fixture.connect().await;
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .expect("subscribe");
    fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");

    let seen = until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PermissionSettled { .. })
    })
    .await;

    // Both halves are on the record even though nobody could act on them: a
    // host is owed the fact that the agent asked, and that the answer it got
    // was not a person's.
    let elicitation = seen
        .iter()
        .find(|event| matches!(event.event, AcpClientEvent::ElicitationRequested { .. }))
        .expect("the unscoped elicitation was recorded");
    assert_eq!(
        elicitation.session_id, None,
        "an elicitation outside a session is attributed to none"
    );
    let permission = seen
        .iter()
        .find(|event| matches!(event.event, AcpClientEvent::PermissionRequested { .. }))
        .expect("the permission for an unattached session was recorded");
    assert_eq!(
        permission.session_id.as_ref().map(|id| id.0.as_ref()),
        Some(OTHER_SESSION),
        "a permission keeps the session it named, attached or not"
    );

    // Nothing was ever seated, so nothing is waiting on a person.
    assert!(
        fixture
            .manager
            .pending_interactions(connection.connection_id)
            .await
            .expect("pending interactions")
            .is_empty()
    );
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert!(snapshot.attachments[0].pending_permission_ids.is_empty());
    assert!(snapshot.attachments[0].pending_elicitation_ids.is_empty());

    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
