use super::*;
// Catches: code-like messages, expired credentials or replay authorize a stranger.
#[test]
fn pairing_code_expired_or_wrong_authorizes_nothing() {
    let (_dir, paths) = crate::telegram::tests::setup();
    let now = 1_000_000;
    write(
        &paths,
        "pairing.json",
        br#"{"code":"ABC123","expires_at":1600000}"#,
    )
    .unwrap();
    assert!(!pair(&paths, "/start", 222, now).unwrap());
    assert!(!pair(&paths, "ABC124", 222, now).unwrap());
    assert!(!pair(&paths, "ABC123", 222, now + 600_000).unwrap());
    assert!(!chats(&paths).unwrap().contains(&222));
    assert!(pair(&paths, "ABC123", 222, now + 599_000).unwrap());
    assert!(chats(&paths).unwrap().contains(&222));
    assert!(!pair(&paths, "ABC123", 333, now).unwrap());
    assert!(!chats(&paths).unwrap().contains(&333));
}
// Catches: reading Settings echoes the secret or private file content.
#[test]
fn token_never_returned_by_settings_api() {
    let (dir, paths) = crate::telegram::tests::setup();
    write(&paths, "bot.token", b"123:secret-settings-token").unwrap();
    let state = AppState::new(
        dir.path().into(),
        dir.path().join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(10),
        )),
    );
    let snapshot = serde_json::to_value(snapshot(&paths, &state).unwrap()).unwrap();
    assert_eq!(snapshot["token_set"], true);
    assert!(snapshot.get("token").is_none());
    assert!(!snapshot.to_string().contains("secret-settings-token"));
}
// Catches: setup follows a secret-file symlink or grants malformed IDs.
#[test]
fn setup_rejects_unsafe_private_files_and_non_private_ids() {
    let (_dir, paths) = crate::telegram::tests::setup();
    for id in ["0", "-123", "+123", "00123", "abc", "9223372036854775808"] {
        assert!(chat_id(id).is_err(), "{id}");
    }
    assert_eq!(chat_id("123").unwrap(), 123);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        write(&paths, "new.token", b"private").unwrap();
        assert_eq!(
            std::fs::metadata(paths.file("new.token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        symlink(paths.file("new.token"), paths.file("linked.token")).unwrap();
        assert_eq!(
            write(&paths, "linked.token", b"replacement"),
            Err(Error::PrivateFile)
        );
        assert_eq!(std::fs::read(paths.file("new.token")).unwrap(), b"private");
    }
    save_chats(&paths, &Default::default()).unwrap();
    assert!(paths.allowlist_entries().unwrap().is_empty());
    assert_eq!(paths.allowlist(), Err(Error::Config));
}

// Catches: removing the final chat leaves it authorized, or disabling requires a vanished agent.
#[tokio::test]
async fn manual_chat_revocation_and_disable_use_persisted_state() {
    let (dir, paths) = crate::telegram::tests::setup();
    let state = Arc::new(AppState::new(
        dir.path().into(),
        dir.path().join("worktrees"),
        crate::config::AppConfig::default(),
        Arc::new(parking_lot::Mutex::new(
            crate::app_logger::LogRingBuffer::new(10),
        )),
    ));
    change_at(
        &state,
        paths.clone(),
        Change::AddChat {
            chat_id: "222".into(),
        },
    )
    .await
    .unwrap();
    assert!(paths.allowlist().unwrap().contains(&222));
    change_at(
        &state,
        paths.clone(),
        Change::RemoveChat {
            chat_id: "222".into(),
        },
    )
    .await
    .unwrap();
    assert!(!paths.allowlist().unwrap().contains(&222));
    let saved = config(&paths).unwrap();
    change_at(
        &state,
        paths.clone(),
        Change::Configure {
            enabled: false,
            target_tuic_session: saved.target_tuic_session,
        },
    )
    .await
    .unwrap();
    assert!(!config(&paths).unwrap().enabled);
    assert!(Config::load(&paths).unwrap().is_none());
    assert_eq!(
        change_at(
            &state,
            paths.clone(),
            Change::Configure {
                enabled: true,
                target_tuic_session: "missing-peer".into()
            }
        )
        .await
        .unwrap_err(),
        Error::State
    );
}
