use agent_client_protocol::schema::v1;

mod acp_support;
use acp_support::{Fixture, authority, text, until};

// Catches: a delayed steer rejection appends earlier text after a later attachment.
#[tokio::test]
async fn rejected_steer_preserves_submission_order_before_later_attachment() {
    let fixture = Fixture::with("critic1548-rejected-before-image");
    for name in ["ego-steer-initialize.json", "ego-steer-rejected.json"] {
        std::fs::copy(
            std::path::Path::new("tests/fixtures/acp").join(name),
            fixture.root().join(name),
        )
        .unwrap();
    }
    let connection = fixture.connect().await;
    let session = fixture
        .manager
        .new_session(connection.connection_id, authority(fixture.root()))
        .await
        .unwrap();
    let mut events = fixture
        .manager
        .subscribe(connection.connection_id, 0)
        .unwrap();
    fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![text("active")],
        )
        .await
        .unwrap();

    let earlier = fixture.manager.prompt(
        connection.connection_id,
        session.session_id.clone(),
        vec![text("earlier correction")],
    );
    tokio::pin!(earlier);
    tokio::select! {
        result = &mut earlier => panic!("steer answered before scenario barrier: {result:?}"),
        _ = until(&mut events, |event| matches!(event, tuicommander_lib::acp::AcpClientEvent::SessionUpdate { .. })) => {}
    }
    // The scenario notification proves the request reached ego, with its answer still pending.
    let later = fixture
        .manager
        .prompt(
            connection.connection_id,
            session.session_id.clone(),
            vec![
                text("later attachment"),
                v1::ContentBlock::Image(v1::ImageContent::new("aGVsbG8=", "image/png")),
            ],
        )
        .await
        .unwrap();
    fixture
        .manager
        .list_sessions(connection.connection_id, Default::default())
        .await
        .unwrap();
    let earlier = earlier.await.unwrap();
    let snapshot = fixture.manager.snapshot(connection.connection_id).unwrap();
    let queue = &snapshot.attachments[0].queued_prompts;
    let observed = queue
        .iter()
        .map(|entry| entry.summary.as_str())
        .collect::<Vec<_>>();
    // Disconnect before asserting, so a failure cannot leave the fixture process alive.
    fixture
        .manager
        .disconnect(connection.connection_id)
        .await
        .unwrap();
    assert_ne!(earlier, later);
    assert_eq!(
        observed,
        ["earlier correction", "later attachment Image"],
        "rejected steering must retain FIFO order with later attachment submissions"
    );
}
