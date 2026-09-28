use agent_client_protocol::schema::v1;
use tuicommander_lib::acp::{AcpClientEvent, AcpTurnState};

mod acp_support;
use acp_support::{Fixture, authority, text, until_settled};

#[tokio::test]
async fn phone_prompt_waits_for_desktop_turn_to_end_before_reaching_ego() {
    let fixture = Fixture::with("prompt-queued-after-turn");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .expect("session/new");
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let mut phone = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();

    let first = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .expect("desktop prompt");
    let second = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("phone")],
        )
        .await
        .expect("phone prompt accepted while desktop runs");
    assert_ne!(first, second);

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    assert_eq!(
        snapshot.attachments[0].active_turn.as_ref().unwrap().state,
        AcpTurnState::Running
    );
    assert_eq!(snapshot.attachments[0].queued_prompts.len(), 1);
    assert_eq!(snapshot.attachments[0].queued_prompts[0].summary, "phone");
    for stream in [&mut desktop, &mut phone] {
        let seen = acp_support::until(stream, |event| {
            matches!(event, AcpClientEvent::PromptQueueChanged { .. })
        })
        .await;
        assert_eq!(seen.last().unwrap().turn_id, Some(second));
    }

    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .expect("fixture barrier before end_turn");
    let first_end = until_settled(&mut desktop).await;
    assert_eq!(first_end.last().unwrap().turn_id, Some(first));
    let second_end = until_settled(&mut desktop).await;
    assert_eq!(second_end.last().unwrap().turn_id, Some(second));
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn either_view_can_cancel_a_queued_prompt_before_ego_receives_it() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();
    let queued = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("phone")],
        )
        .await
        .unwrap();

    fixture
        .manager
        .cancel_queued(connection.connection_id, session.session_id.clone(), queued)
        .await
        .expect("desktop cancelled phone's queued prompt");
    assert!(
        fixture
            .manager
            .snapshot(connection.connection_id)
            .unwrap()
            .attachments[0]
            .queued_prompts
            .is_empty()
    );
    let seen = acp_support::until(&mut desktop, |event| matches!(event, AcpClientEvent::PromptQueueChanged { queued_prompts } if queued_prompts.is_empty())).await;
    assert!(matches!(
        seen.last().unwrap().event,
        AcpClientEvent::PromptQueueChanged { .. }
    ));

    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .unwrap();
    until_settled(&mut desktop).await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancelling_the_running_turn_is_visible_to_both_views() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut desktop = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let mut phone = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    let turn = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();

    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .expect("phone cancels the shared running turn");
    for stream in [&mut desktop, &mut phone] {
        let cancelling = acp_support::until(stream, |event| {
            matches!(
                event,
                AcpClientEvent::AttachmentState {
                    state: tuicommander_lib::acp::AcpAttachmentState::Cancelling
                }
            )
        })
        .await;
        assert_eq!(cancelling.last().unwrap().turn_id, Some(turn));
        let settled = until_settled(stream).await;
        assert_eq!(settled.last().unwrap().turn_id, Some(turn));
    }
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn queued_image_bytes_stay_out_of_snapshots_and_shared_events() {
    let fixture = Fixture::with("prompt-queued-cancelled");
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut stream = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("desktop")],
        )
        .await
        .unwrap();
    let secret = "aGVsbG8tZnJvbS10aGUtcGhvbmU=";
    let queued = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![v1::ContentBlock::Image(v1::ImageContent::new(
                secret,
                "image/png",
            ))],
        )
        .await
        .unwrap();

    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let wire = serde_json::to_string(&snapshot).unwrap();
    assert!(
        wire.contains("Image"),
        "a second view can identify the queued image"
    );
    assert!(
        !wire.contains(secret),
        "image bytes were copied into the shared snapshot"
    );
    let seen = acp_support::until(&mut stream, |event| {
        matches!(event, AcpClientEvent::PromptQueueChanged { .. })
    })
    .await;
    let queued_event = serde_json::to_string(seen.last().unwrap()).unwrap();
    assert!(
        !queued_event.contains(secret),
        "image bytes were retained by the journal"
    );

    fixture
        .manager
        .cancel_queued(connection.connection_id, session.session_id.clone(), queued)
        .await
        .unwrap();
    fixture
        .manager
        .cancel(connection.connection_id, session.session_id.clone())
        .await
        .unwrap();
    until_settled(&mut stream).await;
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
}
