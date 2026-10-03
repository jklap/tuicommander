// Submit-confirmation gap found by the 1423 path audit (MCP submit receipt).
/// Catches: `acknowledged` meaning "any byte after Enter". Codex re-renders its
/// composer while it ingests a long paste; when the Enter was swallowed that
/// repaint still carries the text, and reporting `acknowledged: true,
/// composer_state: cleared` tells the caller a turn started that never did.
#[cfg(unix)]
#[tokio::test]
async fn session_submit_repaint_that_keeps_text_in_the_composer_is_not_acknowledged() {
    let state = test_state();
    let session_id = "submit-swallowed-enter";
    let bytes = install_atomic_submit_test_session(&state, session_id);
    let vt = crate::state::VtLogBuffer::new(24, 80, 1000);
    state
        .grid
        .vt_log_buffers
        .insert(session_id.to_string(), parking_lot::Mutex::new(vt));
    let call_state = Arc::clone(&state);
    let args = serde_json::json!({
        "action": "submit",
        "session_id": session_id,
        "input": "inspect the repository",
        "timeout_ms": 1_000,
    });
    let call = tokio::spawn(async move {
        handle_mcp_tool_call(&call_state, loopback_addr(), "session", &args, None).await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    while bytes.lock().unwrap().last() != Some(&b'\r') {
        assert!(tokio::time::Instant::now() < deadline, "framed write missing");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let repaint = "\x1b[22;1H\u{203a} inspect the repository";
    state
        .grid
        .vt_log_buffers
        .get(session_id)
        .unwrap()
        .lock()
        .process(repaint.as_bytes());
    state
        .session_maps
        .output_buffers
        .get(session_id)
        .unwrap()
        .lock()
        .write(repaint.as_bytes());

    let response = call.await.unwrap();

    assert_eq!(response["acknowledged"], false, "{response}");
}

