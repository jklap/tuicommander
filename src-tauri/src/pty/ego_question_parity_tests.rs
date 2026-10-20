/// Catches: a successful tool receipt leaking into the shared native selector title.
#[tokio::test]
async fn captured_ego_selector_title_excludes_tool_receipts() {
    let mut saw_selector = false;
    replay_native_question("ego-mcp-questions-2088-2089.tcap", "ego", |_, session| {
        let Some(prompt) = session.choice_prompt.as_ref() else {
            return;
        };
        if prompt
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>()
            != ["Red", "Green", "Blue"]
        {
            return;
        }
        saw_selector = true;
        assert_eq!(prompt.title, "Pick a colour?");
        assert_eq!(session.question_text.as_deref(), Some("Pick a colour?"));
        assert!(session.awaiting_input && session.question_confident);
        assert_eq!(
            prompt.selection_mode,
            Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
        );
    });
    assert!(
        saw_selector,
        "recorded selector must reach shared client state"
    );
}

/// Catches: real ego diagnostics/submitted history contaminating selector titles.
#[tokio::test]
async fn captured_ego_selector_title_excludes_diagnostics() {
    let mut saw_color = false;
    let mut saw_permission = false;
    let mut final_awaiting = false;
    replay_native_question(
        "ego-native-questions-2028.tcap",
        "ego",
        |record, session| {
            if let Some(prompt) = session.choice_prompt.as_ref() {
                let labels: Vec<_> = prompt
                    .options
                    .iter()
                    .map(|option| option.label.as_str())
                    .collect();
                if labels == ["Red", "Green", "Blue"] {
                    assert_eq!(prompt.title, "Pick a color: Red, Green, or Blue.");
                    saw_color = true;
                } else if labels == ["Yes", "No"] {
                    assert_eq!(prompt.title, "May I run ls?");
                    saw_permission = true;
                }
                assert!(!prompt.title.contains("εgo:"));
                assert_eq!(
                    session.question_text.as_deref(),
                    Some(prompt.title.as_str())
                );
                assert_eq!(
                    prompt.selection_mode,
                    Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
                );
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                final_awaiting = session.awaiting_input;
            }
        },
    );
    assert!(
        saw_color && saw_permission,
        "both recorded selectors must reach clients"
    );
    assert!(!final_awaiting, "answer retracts the native selector");
}

/// Catches: a scroll-only newline resurrecting answered ego choices after busy.
#[tokio::test]
async fn captured_ego_answered_selector_stays_cleared_after_busy_hook() {
    let mut resumed = false;
    let mut saw_answer = false;
    replay_native_question(
        "ego-selector-retirement-2028.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                saw_answer |= record.data == b"1\r";
                return;
            }
            resumed |= saw_answer
                && record
                    .data
                    .windows(b"7770;state=busy".len())
                    .any(|bytes| bytes == b"7770;state=busy");
            if resumed {
                assert!(
                    !session.awaiting_input && session.choice_prompt.is_none(),
                    "answered selector re-armed after protocol busy: {:?}",
                    record.data,
                );
            }
        },
    );
    assert!(resumed, "capture must include the answer and busy hook");
}

/// Catches: fixing retirement by permanently swallowing an identical next question.
#[test]
fn ego_question_same_selector_recovers_after_fresh_native_wait() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "ego-selector-retirement-2028.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    let sid = "ego-next-identical-question";
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
        .agent_type = Some("ego".into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    // Replay through busy, before the old selector disappears. Then replay the
    // captured native awaiting + selector paint as a controlled next-question
    // transition. Every external byte comes from the real recording.
    for index in (0..=30).chain([23, 26]) {
        let record = &capture.records[index];
        if record.direction == crate::pty_capture::CaptureDirection::Input {
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
        if index == 30 {
            let session = state.session_maps.session_states.get(sid).unwrap();
            assert!(!session.awaiting_input && session.choice_prompt.is_none());
        }
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(session.awaiting_input);
    assert_eq!(
        session.choice_prompt.as_ref().unwrap().title,
        "Which release checks should we run?"
    );
}

/// Catches: enforced ego approvals missing their native keys or losing the refusal wait.
#[tokio::test]
async fn captured_ego_enforced_approval_reaches_answer_controls() {
    let mut refused = false;
    let mut settled = false;
    let mut final_awaiting = false;
    replay_native_question(
        "ego-enforced-approval-2028.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if record.data == b"n" {
                    assert!(session.awaiting_input && session.question_confident);
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("enforced ego approval keys");
                    assert_eq!(prompt.title, "Allow run python3 -c 'print(987654321)'?");
                    assert_eq!(
                        prompt
                            .options
                            .iter()
                            .map(|option| option.key.as_str())
                            .collect::<Vec<_>>(),
                        ["y", "a", "n", "e", "d"]
                    );
                    assert_eq!(prompt.selection_mode, None);
                    refused = true;
                } else if refused && record.data == b"\r" {
                    assert!(
                        session.awaiting_input && session.question_confident,
                        "refusal explanation is still waiting"
                    );
                    assert!(session.choice_prompt.is_none());
                    assert_eq!(
                        session.question_text.as_deref(),
                        Some("What should it do instead? (Enter alone just says no)")
                    );
                    settled = true;
                }
            }
            final_awaiting = session.awaiting_input;
        },
    );
    assert!(refused && settled);
    assert!(!final_awaiting);
}

/// Catches: empty native awaiting dedup swallowing the second MCP approval.
#[tokio::test]
async fn captured_ego_second_mcp_approval_rearms_awaiting() {
    let mut approvals = 0;
    let mut first_controls = false;
    replay_native_question(
        "ego-mcp-questions-2088-2089.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && matches!(record.data.as_slice(), b"y" | b"y\r")
            {
                approvals += 1;
                assert!(
                    session.awaiting_input,
                    "approval {approvals} must await an answer"
                );
                // The first approval's OSC already worked before this regression.
                // Check the second first so RED identifies the lost wait, not the
                // separate hard-wrapped approval-title parsing failure.
                if approvals == 1 {
                    first_controls = session.choice_prompt.is_some();
                }
                if approvals >= 2 {
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("native approval controls");
                    let target = if approvals == 2 {
                        "call_tool; call tuic/session; run"
                    } else {
                        "progress; run"
                    };
                    assert!(
                        prompt
                            .title
                            .starts_with(&format!("Allow call tuic/{target} /"))
                    );
                    assert!(
                        prompt
                            .title
                            .ends_with("/src-tauri/binaries/tuic-bridge-aarch64-app?")
                    );
                    assert!(session.question_confident);
                    assert_eq!(
                        session.question_text.as_deref(),
                        Some(prompt.title.as_str())
                    );
                    assert_eq!(
                        prompt
                            .options
                            .iter()
                            .map(|o| o.key.as_str())
                            .collect::<Vec<_>>(),
                        ["y", "a", "n", "e", "d"]
                    );
                }
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"7770;state=awaiting".len())
                    .any(|s| s == b"7770;state=awaiting")
            {
                assert!(
                    session.awaiting_input,
                    "fresh native awaiting must not depend on a parsed title"
                );
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"7770;state=busy".len())
                    .any(|s| s == b"7770;state=busy")
            {
                assert!(
                    !session.awaiting_input && session.choice_prompt.is_none(),
                    "busy must retire answered approval"
                );
            }
        },
    );
    assert_eq!(approvals, 3, "all recorded approvals must be exercised");
    assert!(
        first_controls,
        "the first hard-wrapped approval also needs native controls"
    );
}
