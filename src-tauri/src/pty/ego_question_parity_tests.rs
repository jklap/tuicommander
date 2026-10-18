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
