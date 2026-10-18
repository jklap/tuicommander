/// Catches: enforced Codex command approvals losing options behind command metadata.
#[tokio::test]
async fn captured_codex_enforced_approval_reaches_answer_controls() {
    let mut refused = 0;
    let mut final_awaiting = false;
    replay_native_question(
        "codex-enforced-approval-2026.tcap",
        "codex",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && record.data == b"3"
            {
                assert!(session.awaiting_input && session.question_confident);
                let prompt = session
                    .choice_prompt
                    .as_ref()
                    .expect("enforced approval must export options");
                assert_eq!(prompt.title, "Would you like to run the following command?");
                assert_eq!(prompt.options.len(), 3);
                assert_eq!(prompt.options[0].label, "Yes, proceed");
                assert_eq!(
                    prompt.options[2].label,
                    "No, and tell Codex what to do differently"
                );
                assert_eq!(
                    prompt.selection_mode, None,
                    "captured 3 cancels without Enter"
                );
                refused += 1;
            }
            final_awaiting = session.awaiting_input;
        },
    );
    assert_eq!(refused, 1);
    assert!(!final_awaiting, "refused enforced command must retract");
}
