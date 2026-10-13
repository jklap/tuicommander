//! Voice adapter regressions at the real `dictation-config.json` owner boundary, driven through
//! the registry. They need the `dictation` feature: the owner's runtime effects live behind it.
use super::{
    native::native_catalogue, registry::Registry, request::*, service::Service,
    voice::VoiceSettingsStore,
};
use crate::dictation::speaker::{self, Armed, Speaker};
use crate::dictation::speech::{Speech, SpeechAudio, SpeechCancel, SpeechError};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::sync::Arc;

const CANARY: &str = "canary-speech-command-4c1d";

struct Silent;

impl Speech for Silent {
    fn synthesize(&self, _: &str, _: &str, _: &SpeechCancel) -> Result<SpeechAudio, SpeechError> {
        Err(SpeechError::Failed("not used".into()))
    }
}

struct QuietOutput;

impl speaker::Output for QuietOutput {
    fn play(&self, _: &SpeechAudio) -> Result<(), String> {
        Ok(())
    }
    fn stop(&self) {}
    fn is_speaking(&self) -> bool {
        false
    }
}

fn armed() -> Armed {
    Armed {
        speaker: Arc::new(Speaker::new(Arc::new(Silent), Arc::new(QuietOutput), 1)),
        voice: "giovanni".into(),
        language: "it".into(),
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    file: std::path::PathBuf,
    slot: Arc<Mutex<Option<Armed>>>,
    registry: Registry,
}

fn fixture(initial: Option<Value>) -> Fixture {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let file = dir.path().join("dictation-config.json");
    if let Some(initial) = initial {
        std::fs::write(&file, initial.to_string()).unwrap();
    }
    let slot = Arc::new(Mutex::new(Some(armed())));
    let mut registry = Registry::default();
    registry
        .register(Service::new(
            Arc::new(native_catalogue().unwrap()),
            Arc::new(VoiceSettingsStore::at_path(
                file.clone(),
                Some(slot.clone()),
            )),
        ))
        .unwrap();
    Fixture {
        _dir: dir,
        file,
        slot,
        registry,
    }
}

async fn get(f: &Fixture, paths: &[&str]) -> Result<Snapshot, Failure> {
    match f
        .registry
        .execute(Request::Get {
            target: Target::Machine,
            store: "voice".into(),
            paths: paths.iter().map(|p| (*p).into()).collect(),
        })
        .await?
    {
        Reply::Snapshot(s) => Ok(s),
        Reply::Commit(_) => panic!("get returned a commit"),
    }
}

async fn set(
    f: &Fixture,
    revision: String,
    path: &str,
    value: Value,
) -> Result<CommitReply, Failure> {
    match f
        .registry
        .execute(Request::Set {
            target: Target::Machine,
            store: "voice".into(),
            path: path.into(),
            value,
            expected_revision: revision,
        })
        .await?
    {
        Reply::Commit(c) => Ok(c),
        Reply::Snapshot(_) => panic!("set returned a snapshot"),
    }
}

async fn revision(f: &Fixture) -> String {
    get(f, &["/language"]).await.unwrap().revision
}

fn on_disk(f: &Fixture) -> Value {
    serde_json::from_str(&std::fs::read_to_string(&f.file).unwrap()).unwrap()
}

/// Catches: the old voice still used after save. A queue opened for Italian must be dropped
/// when the language, engine or voice changes or spoken replies go off, and must survive a
/// slider-like edit.
#[tokio::test]
async fn voice_change_invalidates_cached_runtime() {
    let f = fixture(Some(json!({"language": "it", "speech_engine": "pocket"})));
    let reply = set(&f, revision(&f).await, "/long_press_ms", json!(450))
        .await
        .unwrap();
    assert!(reply.persisted && reply.applied);
    assert!(f.slot.lock().is_some(), "a threshold edit keeps the queue");

    for (path, value) in [
        ("/language", json!("en")),
        ("/speech_engine", json!("edge")),
        ("/speech_voice", json!("other")),
        ("/hands_free_spoken_replies", json!(false)),
    ] {
        *f.slot.lock() = Some(armed());
        let reply = set(&f, revision(&f).await, path, value).await.unwrap();
        assert!(reply.applied);
        assert!(f.slot.lock().is_none(), "{path} must drop the old voice");
    }
}

/// Catches: out-of-range values accepted because the range lived only in the UI.
#[tokio::test]
async fn out_of_range_level_rejected() {
    let f = fixture(Some(json!({"speech_volume_db": -20.0})));
    let before = revision(&f).await;
    let error = set(&f, before.clone(), "/speech_volume_db", json!(5.0))
        .await
        .unwrap_err();
    assert_eq!(error.code, "out_of_range");
    assert_eq!(on_disk(&f), json!({"speech_volume_db": -20.0}));
    assert_eq!(revision(&f).await, before);
    assert!(
        f.slot.lock().is_some(),
        "a rejected edit touches no runtime"
    );
}

/// Catches: an edit clobbering sibling fields, or touching another store's file.
#[tokio::test]
async fn adapter_preserves_other_fields() {
    let f = fixture(Some(json!({
        "language": "it",
        "hotkey": "F5",
        "speech_command": [CANARY],
        "auto_send": false,
    })));
    let neighbour = f.file.with_file_name("notifications.json");
    std::fs::write(&neighbour, "{\"sentinel\":true}").unwrap();
    set(&f, revision(&f).await, "/language", json!("en"))
        .await
        .unwrap();
    let after = on_disk(&f);
    assert_eq!(after["language"], json!("en"));
    assert_eq!(after["hotkey"], json!("F5"));
    assert_eq!(after["speech_command"], json!([CANARY]));
    assert_eq!(after["auto_send"], json!(false));
    assert_eq!(
        std::fs::read_to_string(&neighbour).unwrap(),
        "{\"sentinel\":true}"
    );
}

/// Catches: a rejected value leaking into an error or reply (the speech command template can
/// embed credentials), or the read-only corruption diagnostic reaching the file.
#[tokio::test]
async fn adapter_secret_fields_presence_only() {
    let f = fixture(Some(json!({"speech_command": [CANARY]})));
    let snapshot = get(&f, &["/language"]).await.unwrap();
    let mut texts = vec![serde_json::to_string(&snapshot).unwrap()];
    for result in [
        set(&f, "stale".into(), "/speech_command", json!([CANARY])).await,
        set(
            &f,
            snapshot.revision.clone(),
            "/no_such_field",
            json!(CANARY),
        )
        .await,
        set(
            &f,
            snapshot.revision.clone(),
            "/long_press_ms",
            json!(CANARY),
        )
        .await,
    ] {
        texts.push(serde_json::to_string(&result.unwrap_err()).unwrap());
    }
    let reply = set(&f, snapshot.revision, "/language", json!("en"))
        .await
        .unwrap();
    texts.push(serde_json::to_string(&reply).unwrap());
    for text in texts {
        assert!(!text.contains("canary-speech-command"), "{text}");
    }
    assert_eq!(on_disk(&f)["recovered_from_corruption"], json!(false));
}

/// Reality: a patch persists via the owner file and an immediate readback shows it.
#[tokio::test]
async fn patch_persists_and_reads_back() {
    let f = fixture(None);
    let before = get(&f, &["/long_press_ms"]).await.unwrap();
    let reply = set(&f, before.revision.clone(), "/long_press_ms", json!(321))
        .await
        .unwrap();
    assert!(reply.persisted);
    assert_eq!(on_disk(&f)["long_press_ms"], json!(321));
    let after = get(&f, &["/long_press_ms"]).await.unwrap();
    assert_eq!(after.values["/long_press_ms"].stored, json!(321));
    assert_eq!(after.revision, reply.revision);
    let stale = set(&f, before.revision, "/long_press_ms", json!(1))
        .await
        .unwrap_err();
    assert_eq!(stale.code, "conflict");
}

/// Catches: corrupt JSON read as defaults, then overwritten by the next edit.
#[tokio::test]
async fn corrupt_file_is_an_error_not_defaults() {
    let f = fixture(None);
    std::fs::write(&f.file, "{not json").unwrap();
    let error = get(&f, &["/language"]).await.unwrap_err();
    assert_eq!(error.code, "store_failed");
    assert!(!f.file.exists());
}
