/// Replay real PTY input as well as output; snapshots are the accumulator state
/// consumed by both desktop and mobile, not merely emitted parser events.
fn replay_native_question(
    fixture: &str,
    agent: &str,
    mut observe: impl FnMut(&tuic_terminal::pty_capture::CaptureRecord, &crate::state::SessionState),
) {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture)).unwrap();
    let (rows, cols) = capture.geometry.expect("real recorded geometry");
    let sid = "question-parity";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some(agent.into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    for record in capture.records {
        // Observe immediately before input too: the visible question is what
        // the human actually answered, even if its paint spans several chunks.
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            observe(
                &record,
                &state.session_maps.session_states.get(sid).unwrap(),
            );
            crate::mcp_http::session::apply_input_bookkeeping(
                &state,
                sid,
                std::str::from_utf8(&record.data).unwrap(),
            );
        } else {
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
        }
        while let Ok(event) = rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&state, &event);
        }
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            observe(
                &record,
                &state.session_maps.session_states.get(sid).unwrap(),
            );
        }
    }
}

/// Catches: Codex 0.162 queued/open questions missing the shared answer controls.
#[tokio::test]
async fn captured_codex_0162_question_reaches_answer_controls() {
    let mut opened = 0;
    let mut answered = Vec::new();
    let mut final_awaiting = false;
    replay_native_question(
        "codex-native-question-2026.tcap",
        "codex",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if record.data == b"\x1b[1;2D" {
                    assert!(
                        session.awaiting_input && session.question_confident,
                        "queued question must enable the mobile opening control"
                    );
                    assert!(session.choice_prompt.is_none());
                    opened += 1;
                } else if record.data == b"3" || record.data == b"2" {
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("open question options");
                    assert!(session.awaiting_input);
                    assert_eq!(prompt.selection_mode, None, "numbers answer immediately");
                    let labels: Vec<_> = prompt
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect();
                    if record.data == b"3" {
                        assert_eq!(prompt.title, "Pick a color.");
                        assert_eq!(labels, ["Red", "Green", "Blue", "Other"]);
                    } else {
                        assert_eq!(prompt.title, "May I run `ls`?");
                        assert_eq!(labels, ["Yes", "No", "Other"]);
                    }
                    answered.push(record.data.clone());
                }
            }
            final_awaiting = session.awaiting_input;
        },
    );
    assert_eq!(opened, 2, "both real queued questions were opened");
    assert_eq!(
        answered,
        [b"3".to_vec(), b"2".to_vec()],
        "one answer per question"
    );
    assert!(!final_awaiting, "answered question must retract");
}
