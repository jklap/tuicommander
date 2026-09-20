//! One ego turn, start to finish, with nobody watching.
//!
//! The AI Chat panel drives a conversation a person is sitting in front of.
//! This drives a single turn nobody is looking at: a Smart Prompt in `api`
//! mode asks ego to transform some text and wants the answer back, once. There
//! is no transcript, no stream to render and no second turn.
//!
//! Three rules follow from "nobody is watching", and they are the whole design:
//!
//! - **No host tools.** [`AcpClientManager::new_session`] gives every session
//!   TUICommander's own MCP server, which is how ego reaches terminals and
//!   repositories. Handing that to a turn with no one at the keyboard is an
//!   authority nobody granted, so an unattended session is opened with no MCP
//!   server at all.
//! - **Every question is refused, immediately.** Ego still has tools of its
//!   own, and it may ask permission to use one. A seat nobody takes is a turn
//!   that never ends, so each request is settled the instant it arrives with
//!   the answer that grants nothing — the same `Cancelled`/`Cancel` pair the
//!   connection already uses when there is no one left to tell. How many were
//!   refused is reported, because "the model wanted a tool this mode cannot
//!   give it" is the difference between a diagnosis and an empty box.
//! - **The connection is this call's.** It is launched here and shut down here,
//!   rather than borrowed from whatever the panel happens to have open. A Smart
//!   Prompt must run with the panel closed, and sharing would couple a
//!   one-shot to a conversation's lifetime for the sake of one process launch.
//!
//! The fold from events to an answer is [`TurnCollector`], which is pure and
//! carries the tests. Everything around it is I/O.

use std::path::PathBuf;
use std::time::Duration;

use agent_client_protocol::schema::v1;
use serde::Serialize;

use super::{
    AcpClientError, AcpClientEvent, AcpConnectRequest, AcpConnectionId, AcpConnectionState,
    AcpHostRequestId, AcpSessionAuthority,
};
use crate::acp_commands::ego_config;
use crate::state::AppState;

/// How long one unattended turn may take before it is abandoned.
///
/// Server-side and not a parameter: no caller needs a different value, and a
/// timeout an HTTP body could choose is a way to pin an ego process for as long
/// as the sender likes. It matches the headless-CLI budget in
/// `useSmartPrompts`, which is the same kind of work.
const TURN_TIMEOUT: Duration = Duration::from_secs(300);

/// What one unattended turn produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EgoTurn {
    /// Everything ego said, in order, with the surrounding blank space removed.
    pub text: String,
    /// How the turn ended, in ego's own vocabulary.
    pub stop_reason: v1::StopReason,
    /// How many questions were refused because nobody was there to answer.
    ///
    /// Non-zero with empty `text` is the diagnosis for the one failure this
    /// mode has that a person would not expect: the model reached for a tool
    /// and an unattended turn cannot grant one.
    pub declined: usize,
}

/// A request that must be settled without granting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Permission(AcpHostRequestId),
    Elicitation(AcpHostRequestId),
}

/// What the driver must do after folding one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Keep reading.
    Continue,
    /// Settle this request, then keep reading.
    Refuse(Refusal),
    /// The turn is over; the collector holds the answer.
    Done,
}

/// The fold from a connection's events to one turn's answer.
///
/// Pure on purpose: everything that decides what the answer *is* lives here and
/// is tested against event sequences, while the driver around it only moves
/// bytes. The alternative — deciding inside the read loop — is testable only
/// against a live ego.
#[derive(Debug, Default)]
pub(crate) struct TurnCollector {
    text: String,
    declined: usize,
    stop_reason: Option<v1::StopReason>,
}

impl TurnCollector {
    /// Fold one event and say what the driver owes the agent.
    pub(crate) fn observe(&mut self, event: &AcpClientEvent) -> Step {
        match event {
            AcpClientEvent::SessionUpdate { update } => {
                // Only what ego *said*. A thought chunk is reasoning on the way
                // to the answer, and a tool call is work, not an answer; either
                // one pasted into a terminal by `outputTarget` would be the
                // model thinking out loud in a person's shell.
                if let v1::SessionUpdate::AgentMessageChunk(chunk) = update.as_ref()
                    && let v1::ContentBlock::Text(text) = &chunk.content
                {
                    self.text.push_str(&text.text);
                }
                Step::Continue
            }
            AcpClientEvent::PermissionRequested { request_id, .. } => {
                self.declined += 1;
                Step::Refuse(Refusal::Permission(*request_id))
            }
            AcpClientEvent::ElicitationRequested { request_id, .. } => {
                self.declined += 1;
                Step::Refuse(Refusal::Elicitation(*request_id))
            }
            AcpClientEvent::TurnSettled { stop_reason, .. } => {
                self.stop_reason = Some(*stop_reason);
                Step::Done
            }
            // The process is gone. Reading on would wait for a turn that can no
            // longer end, so stop and let the driver report what it has.
            AcpClientEvent::ConnectionState {
                state:
                    AcpConnectionState::Closed | AcpConnectionState::Failed | AcpConnectionState::Killed,
            } => Step::Done,
            _ => Step::Continue,
        }
    }

    /// The finished turn, if one finished.
    ///
    /// `None` means the stream ended without ego ever settling the turn — a
    /// different thing from a turn that ended with nothing to say, and the
    /// driver reports it as such rather than as an empty answer.
    pub(crate) fn finish(self) -> Option<EgoTurn> {
        Some(EgoTurn {
            text: self.text.trim().to_string(),
            stop_reason: self.stop_reason?,
            declined: self.declined,
        })
    }
}

/// The JSON object inside one ego answer.
///
/// A model fences JSON, or introduces it, or thanks you for asking, and saying
/// "only JSON" in the prompt does not make that stop. First `{` to last `}` is
/// the whole rule. It is deliberately not a parser: an answer with no object in
/// it comes back unchanged, so the failure surfaces as a parse error carrying
/// what ego actually said instead of as an empty result.
pub(crate) fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => &text[start..=end],
        _ => text.trim(),
    }
}

/// What an unattended turn said, or why it said nothing usable.
///
/// Three failures a person has to be able to tell apart, and none of them may
/// reach a panel as an empty list: ego reached for a tool this mode cannot
/// grant, ego finished with nothing to say, and — upstream of this — ego never
/// ran at all. The first is the one that looks like a bug and is not.
pub(crate) fn answer_text(turn: EgoTurn) -> Result<String, String> {
    if !turn.text.is_empty() {
        return Ok(turn.text);
    }
    if turn.declined > 0 {
        return Err(format!(
            "ego asked to use {} tool{} that an unattended turn cannot grant, and produced no answer",
            turn.declined,
            if turn.declined == 1 { "" } else { "s" }
        ));
    }
    Err("ego finished the turn without saying anything".to_string())
}

/// How much of an unusable answer is quoted back in the error.
const MAX_QUOTED: usize = 300;

/// Quote an answer back without pasting a whole transcript into a toast.
fn quoted(text: &str) -> String {
    match text.char_indices().nth(MAX_QUOTED) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// Ask ego one question and get back text worth parsing, or a reason.
///
/// The seam every unattended feature sits on, and the reason it is one function
/// rather than three copies: what each of them does with the answer differs, but
/// how they must *fail* does not. A turn can fail because ego is not configured,
/// because ego would not start, because ego reached for a tool this mode cannot
/// grant, or because ego said nothing — and each of those has to arrive at a
/// panel as its own sentence. Written out three times, two of them would
/// eventually become an empty list.
///
/// `system` rides the prompt rather than a system slot: `session/prompt` has no
/// separate one, and the alternative — writing ego's own config — is forbidden
/// by the plan (§6.2, decision 2).
pub(crate) async fn ask(
    state: &AppState,
    root: PathBuf,
    system: &str,
    content: String,
) -> Result<String, String> {
    let turn = run_prompt(state, root, format!("{system}\n\n{content}"))
        .await
        .map_err(|err| format!("ego could not run this turn: {}", err.message))?;
    answer_text(turn)
}

/// Report an answer that would not parse, with enough of it to act on.
///
/// Shared so that "ego answered, but not with what this needs" reads the same
/// wherever it happens, and so that quoting a runaway answer cannot paste a
/// transcript into a toast.
pub(crate) fn unparseable(what: &str, err: impl std::fmt::Display, answer: &str) -> String {
    format!(
        "ego did not answer with the {what} this needs ({err}). It said: {}",
        quoted(answer)
    )
}

/// Run one prompt through a fresh ego and give back what it said.
///
/// Every exit shuts the connection down, including the failing ones: an ego
/// left running for a turn nobody is waiting on is a process that will only be
/// noticed when there are a dozen of them.
pub(crate) async fn run_prompt(
    state: &AppState,
    root: PathBuf,
    prompt: String,
) -> Result<EgoTurn, AcpClientError> {
    let config = ego_config(state)?;
    let connection = state
        .acp
        .connect(&config, AcpConnectRequest { root: root.clone() })
        .await?;

    let outcome = drive(
        state,
        &connection.connection_id,
        connection.latest_sequence,
        root,
        prompt,
    )
    .await;

    // The settlement is not reported: the caller asked for an answer, and a
    // connection that would not shut down cleanly says nothing about whether
    // the answer is good.
    let _ = state.acp.disconnect(connection.connection_id).await;
    outcome
}

/// Everything between a live connection and an answer.
async fn drive(
    state: &AppState,
    connection_id: &AcpConnectionId,
    from_sequence: u64,
    root: PathBuf,
    prompt: String,
) -> Result<EgoTurn, AcpClientError> {
    let connection_id = *connection_id;
    // Subscribed before the session exists, from where the connect snapshot
    // ended: a subscription taken after the prompt would race the first chunks,
    // and one taken from zero would replay the connection's own startup.
    let mut events = state.acp.subscribe(connection_id, from_sequence + 1)?;

    let attachment = state
        .acp
        .new_unattended_session(
            connection_id,
            AcpSessionAuthority {
                cwd: root,
                additional_directories: Vec::new(),
                mcp_servers: Vec::new(),
            },
        )
        .await?;
    let session_id = attachment.session_id.clone();

    state
        .acp
        .prompt(
            connection_id,
            session_id.clone(),
            vec![v1::ContentBlock::Text(v1::TextContent::new(prompt))],
        )
        .await?;

    let collected = tokio::time::timeout(
        TURN_TIMEOUT,
        collect(state, connection_id, &mut events, &session_id),
    )
    .await
    .map_err(|_| {
        // `None` for the operation: a turn that ran out of time is not a
        // capability ego failed to offer, and naming one would send a reader
        // looking for a missing feature.
        AcpClientError::agent_error(
            connection_id,
            None,
            format!(
                "ego did not finish the turn within {}s",
                TURN_TIMEOUT.as_secs()
            ),
        )
        .with_session_id(session_id.clone())
    })??;

    collected
        .finish()
        .ok_or_else(|| AcpClientError::transport_closed(connection_id).with_session_id(session_id))
}

/// Read until the turn settles, refusing every question on the way.
///
/// The refusal is sent from inside this loop, not queued for afterwards: a
/// question is what *stops* the turn, so anything deferred until the turn ends
/// waits for something that will never happen. That is the hang this whole
/// module is shaped around.
async fn collect(
    state: &AppState,
    connection_id: AcpConnectionId,
    events: &mut super::events::AcpEventStream,
    session_id: &v1::SessionId,
) -> Result<TurnCollector, AcpClientError> {
    let mut collector = TurnCollector::default();

    while let Some(event) = events.recv().await {
        // A gap is terminal here for the same reason it is on the panel's
        // stream: the missing events are gone, and an answer assembled from
        // what survived would be a different answer with no sign of it.
        let envelope = event?;
        // Connection-wide events carry no session; everything else must be this
        // one's. A second session on this connection is not possible today, and
        // filtering rather than assuming keeps it that way if it becomes so.
        if envelope
            .session_id
            .as_ref()
            .is_some_and(|id| id != session_id)
        {
            continue;
        }

        match collector.observe(&envelope.event) {
            Step::Continue => {}
            Step::Refuse(refusal) => refuse(state, connection_id, refusal).await,
            Step::Done => break,
        }
    }

    Ok(collector)
}

/// Settle one request with the answer that grants nothing.
///
/// A failure is not propagated. Every way this can fail means the seat is
/// already gone — the turn was cancelled, ego withdrew the question, the
/// connection settled — and none of them is a reason to throw away an answer
/// the caller is still waiting for.
async fn refuse(state: &AppState, connection_id: AcpConnectionId, refusal: Refusal) {
    let _ = match refusal {
        Refusal::Permission(request_id) => state
            .acp
            .respond_permission(
                connection_id,
                request_id,
                v1::RequestPermissionOutcome::Cancelled,
            )
            .await
            .map(|_| ()),
        Refusal::Elicitation(request_id) => state
            .acp
            .respond_elicitation(connection_id, request_id, v1::ElicitationAction::Cancel)
            .await
            .map(|_| ()),
    };
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(text: &str) -> AcpClientEvent {
        AcpClientEvent::SessionUpdate {
            update: Box::new(v1::SessionUpdate::AgentMessageChunk(v1::ContentChunk::new(
                v1::ContentBlock::Text(v1::TextContent::new(text.to_string())),
            ))),
        }
    }

    fn thought(text: &str) -> AcpClientEvent {
        AcpClientEvent::SessionUpdate {
            update: Box::new(v1::SessionUpdate::AgentThoughtChunk(v1::ContentChunk::new(
                v1::ContentBlock::Text(v1::TextContent::new(text.to_string())),
            ))),
        }
    }

    fn settled(stop_reason: v1::StopReason) -> AcpClientEvent {
        AcpClientEvent::TurnSettled {
            stop_reason,
            usage: None,
        }
    }

    fn permission_asked(request_id: AcpHostRequestId) -> AcpClientEvent {
        AcpClientEvent::PermissionRequested {
            request_id,
            request: Box::new(v1::RequestPermissionRequest::new(
                v1::SessionId::from("s1"),
                v1::ToolCallUpdate::new("t1", v1::ToolCallUpdateFields::new()),
                Vec::new(),
            )),
        }
    }

    fn elicitation_asked(request_id: AcpHostRequestId) -> AcpClientEvent {
        AcpClientEvent::ElicitationRequested {
            request_id,
            request: Box::new(v1::CreateElicitationRequest::new(
                v1::ElicitationMode::Url(v1::ElicitationUrlMode::new(
                    v1::ElicitationSessionScope::new(v1::SessionId::from("s1")),
                    v1::ElicitationId::from("e1"),
                    "https://example.invalid",
                )),
                "which one?",
            )),
        }
    }

    #[test]
    fn the_answer_is_every_chunk_ego_said_in_order() {
        let mut collector = TurnCollector::default();
        for event in [
            chunk("Hello, "),
            chunk("world"),
            settled(v1::StopReason::EndTurn),
        ] {
            collector.observe(&event);
        }
        let turn = collector.finish().expect("the turn settled");
        assert_eq!(turn.text, "Hello, world");
        assert_eq!(turn.stop_reason, v1::StopReason::EndTurn);
        assert_eq!(turn.declined, 0);
    }

    #[test]
    fn surrounding_blank_space_is_not_part_of_the_answer() {
        let mut collector = TurnCollector::default();
        collector.observe(&chunk("\n\n  the answer  \n"));
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").text, "the answer");
    }

    #[test]
    fn reasoning_is_not_the_answer() {
        let mut collector = TurnCollector::default();
        // A thought pasted into a person's shell by `outputTarget` is the model
        // thinking out loud where the result was supposed to go.
        for event in [
            thought("let me think"),
            chunk("42"),
            settled(v1::StopReason::EndTurn),
        ] {
            collector.observe(&event);
        }
        assert_eq!(collector.finish().expect("settled").text, "42");
    }

    #[test]
    fn a_permission_request_is_refused_the_moment_it_arrives() {
        let request_id = AcpHostRequestId::new();
        let mut collector = TurnCollector::default();
        // Not `Continue`: a question is what stops the turn, so anything the
        // driver defers until the turn ends waits for something that cannot
        // happen.
        assert_eq!(
            collector.observe(&permission_asked(request_id)),
            Step::Refuse(Refusal::Permission(request_id))
        );
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").declined, 1);
    }

    #[test]
    fn an_elicitation_is_refused_on_its_own_seat() {
        let request_id = AcpHostRequestId::new();
        let mut collector = TurnCollector::default();
        // The two seats take different answers, and answering one on the other
        // leaves the real question waiting.
        assert_eq!(
            collector.observe(&elicitation_asked(request_id)),
            Step::Refuse(Refusal::Elicitation(request_id))
        );
        collector.observe(&settled(v1::StopReason::EndTurn));
        assert_eq!(collector.finish().expect("settled").declined, 1);
    }

    #[test]
    fn a_turn_with_nothing_to_say_still_reports_what_it_asked_for() {
        let mut collector = TurnCollector::default();
        collector.observe(&permission_asked(AcpHostRequestId::new()));
        collector.observe(&settled(v1::StopReason::Cancelled));
        let turn = collector.finish().expect("settled");
        // An empty answer and "the model wanted a tool this mode cannot grant"
        // look identical without this count. The caller says which one happened.
        assert_eq!(turn.text, "");
        assert_eq!(turn.declined, 1);
        assert_eq!(turn.stop_reason, v1::StopReason::Cancelled);
    }

    #[test]
    fn a_connection_that_died_ends_the_read() {
        let mut collector = TurnCollector::default();
        // Reading on would wait for a turn that can no longer be settled, which
        // is the timeout rather than the answer.
        assert_eq!(
            collector.observe(&AcpClientEvent::ConnectionState {
                state: AcpConnectionState::Failed
            }),
            Step::Done
        );
        assert!(collector.finish().is_none());
    }

    #[test]
    fn a_turn_that_never_settled_is_not_an_empty_answer() {
        let mut collector = TurnCollector::default();
        collector.observe(&chunk("half of a sen"));
        // The stream ended mid-turn. Reporting "" here would tell the caller
        // ego had nothing to say, which is the one thing it did not do.
        assert!(collector.finish().is_none());
    }

    fn turn(text: &str, declined: usize) -> EgoTurn {
        EgoTurn {
            text: text.to_string(),
            stop_reason: v1::StopReason::EndTurn,
            declined,
        }
    }

    #[test]
    fn an_object_is_found_however_it_was_introduced() {
        for answer in [
            r#"{"a":1}"#,
            "```json\n{\"a\":1}\n```",
            "Sure! Here it is:\n{\"a\":1}\nHope that helps.",
        ] {
            assert_eq!(extract_json(answer), r#"{"a":1}"#, "for {answer:?}");
        }
    }

    #[test]
    fn an_answer_with_no_object_is_handed_back_to_fail_loudly() {
        // Returning "" or "{}" here would turn "ego argued with the prompt"
        // into an empty list with nothing to show a person.
        assert_eq!(
            extract_json("  I would rather not.  "),
            "I would rather not."
        );
    }

    #[test]
    fn the_outermost_braces_win() {
        // Nested objects, and a trailing sentence after the close.
        assert_eq!(
            extract_json(r#"here: {"a":{"b":2}} done"#),
            r#"{"a":{"b":2}}"#
        );
    }

    #[test]
    fn a_refused_tool_is_named_rather_than_reported_as_silence() {
        // The one failure that looks like a bug and is not: the answer is empty
        // because the model wanted something this mode does not grant.
        let err = answer_text(turn("", 3)).unwrap_err();
        assert!(err.contains("3 tools"), "{err}");

        let one = answer_text(turn("", 1)).unwrap_err();
        assert!(one.contains("1 tool that"), "{one}");
    }

    #[test]
    fn an_empty_turn_is_an_error_and_never_an_empty_answer() {
        let err = answer_text(turn("", 0)).unwrap_err();
        assert!(err.contains("without saying anything"), "{err}");
    }

    #[test]
    fn a_turn_that_said_something_is_simply_that() {
        assert_eq!(answer_text(turn("hello", 0)).unwrap(), "hello");
        // Declined questions do not spoil an answer that still arrived.
        assert_eq!(answer_text(turn("hello", 2)).unwrap(), "hello");
    }

    #[test]
    fn a_quoted_answer_cannot_paste_a_transcript_into_a_toast() {
        let long = "x".repeat(MAX_QUOTED * 3);
        let shown = quoted(&long);

        assert!(shown.ends_with('…'));
        assert_eq!(shown.chars().count(), MAX_QUOTED + 1);
        // A short answer is quoted whole, with nothing added.
        assert_eq!(quoted("short"), "short");
    }

    #[test]
    fn quoting_cuts_on_a_character_and_not_a_byte() {
        // `&text[..cut]` panics on a byte index inside a character, so the cut
        // has to come from `char_indices`. Every char here is 4 bytes.
        let long = "🙂".repeat(MAX_QUOTED * 2);
        assert_eq!(quoted(&long).chars().count(), MAX_QUOTED + 1);
    }
}
