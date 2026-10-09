//! Real-file follow regressions. Schema comes from the recorded Claude corpus.

use super::*;
use crate::state::AppEvent;

const ROW: &str = include_str!("../fixtures/chat_view/recorded/shape-012.jsonl");

fn fixture_row(id: &str, text: &str) -> String {
    let mut row: Value = serde_json::from_str(ROW).expect("recorded row");
    row["uuid"] = Value::String(id.into());
    row["message"]["id"] = Value::String(id.into());
    row["message"]["content"][0]["text"] = Value::String(text.into());
    format!("{row}\n")
}

fn append(path: &Path, row: &str) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open")
        .write_all(row.as_bytes())
        .expect("append");
}

async fn changed(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>, sid: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::ChatViewChanged { session_id, .. } =
                rx.recv().await.expect("event bus")
            {
                assert_eq!(session_id, sid, "wake belongs to the watched terminal");
                return;
            }
        }
    })
    .await
    .expect("ticker did not wake the watched chat after the file changed");
}

// Catches: a same-path replacement of equal size stays at EOF, freezing the chat
// at the old conversation even though the file and later rows belong to a new one.
#[tokio::test]
async fn ticker_follows_appends_and_equal_size_transcript_replacement() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
    let path = tmp.path().join("transcript.jsonl");
    std::fs::write(&path, fixture_row("a", "before")).expect("write");
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "follow-test";
    state.chat_views.views.lock().insert(
        sid.into(),
        Arc::new(Mutex::new(View::new(
            path.clone(),
            MAX_LOG_ENTRIES,
            MAX_LOG_BYTES,
        ))),
    );
    let mut rx = state.event_bus.subscribe();
    let initial = chat_view_snapshot_blocking(state.clone(), sid.into(), None, 0)
        .await
        .expect("initial");
    assert_eq!(initial.updates[0]["content"]["text"], "before");
    append(&path, &fixture_row("b", "append"));
    changed(&mut rx, sid).await;
    let appended = chat_view_snapshot_blocking(
        state.clone(),
        sid.into(),
        Some(initial.epoch),
        initial.next_seq,
    )
    .await
    .expect("appended");
    assert!(!appended.reset);
    assert_eq!(appended.updates[0]["content"]["text"], "append");
    let replacement = tmp.path().join("replacement.jsonl");
    std::fs::write(
        &replacement,
        format!(
            "{}{}",
            fixture_row("c", "newone"),
            fixture_row("d", "newtwo")
        ),
    )
    .expect("replacement");
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        std::fs::metadata(&replacement).unwrap().len()
    );
    std::fs::rename(&replacement, &path).expect("replace");
    changed(&mut rx, sid).await;
    let replaced =
        chat_view_snapshot_blocking(state, sid.into(), Some(appended.epoch), appended.next_seq)
            .await
            .expect("replaced");
    assert!(
        replaced.reset,
        "replacement must invalidate the old conversation cursor"
    );
    assert_eq!(replaced.updates[0]["content"]["text"], "newone");
    assert_eq!(replaced.updates[1]["content"]["text"], "newtwo");
}

// Catches: a new JSONL binding retaining the old file identity or conversation.
#[test]
fn replacement_binding_then_appends_do_not_replay_the_old_file() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
    let first = tmp.path().join("first.jsonl");
    let second = tmp.path().join("second.jsonl");
    std::fs::write(&first, fixture_row("a", "old")).expect("first");
    std::fs::write(&second, fixture_row("b", "new")).expect("second");
    let mut view = View::new(first, MAX_LOG_ENTRIES, MAX_LOG_BYTES);
    view.advance(TAIL_WINDOW_BYTES).expect("initial");
    let epoch = view.log.epoch;
    let seq = view.log.next_seq;
    view.rebind(second.clone());
    view.advance(TAIL_WINDOW_BYTES).expect("rebound");
    append(&second, &fixture_row("c", "later"));
    view.advance(TAIL_WINDOW_BYTES).expect("append");
    let (reset, updates) = view.log.since(Some(epoch), seq);
    assert!(reset);
    assert_eq!(
        updates
            .iter()
            .map(|u| u["content"]["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["new", "later"]
    );
}

// Catches: ticker cleanup losing the reason a watched chat stopped following.
#[test]
fn unreadable_transcript_stops_with_a_reason_and_releases_registration() {
    let tmp = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
    let path = tmp.path().join("missing.jsonl");
    let state = crate::state::tests_support::make_test_app_state();
    let view = Arc::new(Mutex::new(View::new(path, MAX_LOG_ENTRIES, MAX_LOG_BYTES)));
    view.lock().ticking = true;
    state
        .chat_views
        .views
        .lock()
        .insert("one".into(), view.clone());
    assert!(
        matches!(tick(&state, "one", &view), Tick::Stop(reason) if reason.contains("transcript unreadable"))
    );
    assert!(!view.lock().ticking);
    assert!(!state.chat_views.views.lock().contains_key("one"));
}
