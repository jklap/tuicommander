use crate::state::AppEvent;

/// Only the bound terminal contributes authored progress, never another peer.
pub(super) fn notice(event: &AppEvent, pty: &str) -> Option<String> {
    let AppEvent::ProgressRecorded { payload, .. } = event else {
        return None;
    };
    let entry = &payload["entry"];
    if entry["ptyId"].as_str()? != pty {
        return None;
    }
    if !matches!(entry["type"].as_str()?, "done" | "blocked") {
        return None;
    }
    let text = entry["text"].as_str()?;
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    // Catches: another peer's outcome leaks to the phone, intent becomes a
    // completed reply, or authored language/whitespace is rewritten.
    #[tokio::test]
    async fn notices_keep_bound_authored_language_and_ignore_other_progress() {
        use crate::telegram::tests::{FakeServer, PEER, setup};
        use axum::http::StatusCode;
        let (_dir, paths) = setup();
        let server = FakeServer::start(vec![
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":7}})),
            (StatusCode::OK, json!({"ok":true,"result":{"message_id":8}})),
        ])
        .await;
        let runtime = crate::telegram::tests::outbound_tests::runtime(paths, server.address).await;
        crate::state::tests_support::insert_dummy_session(&runtime.state, PEER);
        let state = runtime.state.clone();
        let (commands, receive) = tokio::sync::mpsc::channel(1);
        let worker = tokio::spawn(runtime.run(receive));
        // Let the worker subscribe; a native command reply is the readiness barrier.
        let (reply, done) = tokio::sync::oneshot::channel();
        commands
            .send(crate::telegram::runtime::Command::Update {
                value: json!({}),
                reply,
            })
            .await
            .unwrap();
        done.await.unwrap().unwrap();
        for (pty, kind) in [
            ("other", "done"),
            (PEER, "intent"),
            (PEER, "done"),
            (PEER, "blocked"),
        ] {
            state
                .event_bus
                .send(AppEvent::ProgressRecorded {
                    repo_path: "repo".into(),
                    payload: json!({"entry":{"id":7,"ptyId":pty,"type":kind,"text":" Finito.\n"}}),
                })
                .unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while server.requests().len() < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("runtime did not deliver bound progress");
        drop(commands);
        worker.await.unwrap();
        let (_, session) = state.session_maps.sessions.remove(PEER).unwrap();
        let mut session = session.lock();
        session._child.kill().unwrap();
        session._child.wait().unwrap();
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        for (method, body) in requests {
            assert!(method.ends_with("sendMessage"));
            assert_eq!(body["chat_id"], 1111111);
            assert_eq!(body["text"], " Finito.\n");
        }
    }
}
