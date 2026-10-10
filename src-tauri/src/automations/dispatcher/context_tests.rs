use super::*;

// Catches: opted-in stdout never reaching the real dispatch launch request,
// missing untrusted delimiters, unbounded payload, or stderr reaching the agent.
#[tokio::test]
async fn opted_in_precheck_launches_bounded_untrusted_stdout_before_exact_prompt() {
    let precheck = serde_json::from_value(serde_json::json!({
        "command": format!("{} {} {} 1>&2", print_file_script("input.txt"), if cfg!(windows) { "&" } else { ";" }, print_file_script("error.txt")),
        "timeout_secs": 30,
        "inject_output": true
    })).unwrap();
    let (fixture, id) = Fixture::new(Some(precheck), false);
    let workspace = std::path::Path::new(&fixture.dispatcher.effects.workspace);
    std::fs::write(workspace.join("input.txt"), "A".repeat(20 * 1024)).unwrap();
    std::fs::write(workspace.join("error.txt"), "STDERR-NEVER-INJECT").unwrap();
    fixture.dispatcher.dispatch(&id).await.unwrap();
    assert_eq!(fixture.saved(&id).status, RunStatus::Running);
    let calls = fixture.dispatcher.effects.calls.lock();
    let prompt = &calls[0].prompt;
    assert!(
        prompt.contains("<precheck-output untrusted=\"true\">\n"),
        "opted-in stdout must be delimited as untrusted before launch: {prompt:?}"
    );
    let body = prompt
        .split_once("<precheck-output untrusted=\"true\">\n")
        .unwrap()
        .1
        .split_once("\n</precheck-output>")
        .unwrap()
        .0;
    assert_eq!(
        body,
        format!("{}\n[output truncated]", "A".repeat(16 * 1024))
    );
    assert!(prompt.ends_with("\n\nLiteral {{files}}\nKeep this prompt."));
    assert!(!prompt.contains("STDERR-NEVER-INJECT"));
    let saved = serde_json::to_value(fixture.saved(&id)).unwrap();
    assert_eq!(
        saved["precheck_context"],
        serde_json::json!({"injected":true,"bytes":16384})
    );
}
