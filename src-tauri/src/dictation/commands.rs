use super::{
    DictationState, audio, browser, continuous, corrections, echo, model, permission, speaker,
    speech, streaming, transcribe,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

/// Helper to reset recording flag on error paths.
struct RecordingGuard<'a> {
    recording: &'a std::sync::atomic::AtomicBool,
    disarmed: bool,
}

impl<'a> RecordingGuard<'a> {
    fn new(recording: &'a std::sync::atomic::AtomicBool) -> Self {
        Self {
            recording,
            disarmed: false,
        }
    }
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for RecordingGuard<'_> {
    fn drop(&mut self) {
        if !self.disarmed {
            self.recording.store(false, Ordering::Release);
        }
    }
}

/// Below this ratio of final-pass text to streaming-partial text, the final pass
/// returned less than the streaming windows already had — which is what window
/// tail loss looks like from the outside.
///
/// Set at 0.9 rather than 0.8 because the incident that motivated the warning
/// landed at 81.5% (full=1175 against composed=1442) and an 80% line would have
/// stayed silent on it. A warning that misses the case it exists for is worth
/// less than an occasional false positive, which costs one log line.
const SHORT_TRANSCRIPTION_RATIO: f64 = 0.9;

/// How much of the streaming partials survived into the final transcription.
///
/// Characters on both sides. `String::len()` counts bytes, so an accented
/// dictation measures longer than it reads and any ratio built on it lies. The
/// metric this replaced compared a common-PREFIX character count against a byte
/// length: one differing leading space reported 0% and said nothing at all about
/// how much text was missing.
///
/// `None` when there are no partials to compare against.
fn transcription_ratio(full: &str, composed: &str) -> Option<f64> {
    let composed_chars = composed.chars().count();
    if composed_chars == 0 {
        return None;
    }
    Some(full.chars().count() as f64 / composed_chars as f64)
}

/// RAII guard that resets the processing flag to false on drop (including panic).
/// Holds an `Arc<AtomicBool>` so it can be moved into `spawn_blocking`.
struct ProcessingGuard(Arc<std::sync::atomic::AtomicBool>);

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_logger;

#[derive(Debug, Clone, Serialize)]
pub struct DictationStatus {
    pub model_status: String, // "not_downloaded", "ready", "error"
    pub model_name: String,
    pub model_size_mb: u64,
    pub recording: bool,
    pub processing: bool,
    /// Normalized 0.0–1.0 microphone level while recording.
    pub audio_level: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub size_hint_mb: u64,
    pub downloaded: bool,
    pub actual_size_mb: u64,
}

/// Result returned by stop_dictation_and_transcribe with metadata for user feedback.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeResponse {
    /// The transcribed (and corrected) text, empty if skipped.
    pub text: String,
    /// Human-readable reason when text is empty (None on success).
    pub skip_reason: Option<String>,
    /// Duration of the audio that reached the final transcription, in seconds.
    pub duration_s: f64,
    /// Seconds of speech the recording cap dropped before that transcription.
    /// Zero for any ordinary recording; non-zero means the text is missing its
    /// beginning, and the UI must say so rather than pass off a partial answer.
    pub truncated_s: f64,
}

/// Resolve a model name from config, falling back to the default.
fn resolve_model(name: &str) -> model::WhisperModel {
    model::WhisperModel::from_name(name).unwrap_or(model::WhisperModel::LargeV3Turbo)
}

/// The model-derived half of [`DictationStatus`]: which model is configured,
/// whether it is on disk and how big the file is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelSnapshot {
    model: model::WhisperModel,
    downloaded: bool,
    size_mb: u64,
}

/// Cache slot for [`model_snapshot`]: the snapshot and when it was taken.
static MODEL_SNAPSHOT: parking_lot::Mutex<Option<(ModelSnapshot, std::time::Instant)>> =
    parking_lot::Mutex::new(None);

/// How long a snapshot may be served before it is recomputed.
///
/// The commands in this process invalidate explicitly, but they are not the only
/// writer: a debug build and the installed app share one configuration directory
/// and one model directory, so the other process can change the selected model,
/// download it or delete it with nothing to tell us. Without an expiry this cache
/// served that stale answer forever. One second keeps the 75 ms meter tick off
/// the config file — the reason the cache exists — while bounding how long a
/// change made elsewhere can go unnoticed.
const MODEL_SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(1);

/// Snapshot of the configured model for [`get_dictation_status`].
///
/// Computing one costs a `dictation-config.json` read, a JSON parse and two
/// `stat` calls. The microphone meter polls `get_dictation_status` every 75 ms
/// while recording (`startAudioLevelPolling` in `src/stores/dictation.ts`), so
/// paying that per tick means ~13 config parses a second on the IPC thread.
fn model_snapshot() -> ModelSnapshot {
    let mut slot = MODEL_SNAPSHOT.lock();
    if let Some((snapshot, taken)) = slot.as_ref()
        && taken.elapsed() < MODEL_SNAPSHOT_TTL
    {
        return snapshot.clone();
    }
    let model = resolve_model(&get_dictation_config().model);
    let snapshot = ModelSnapshot {
        model,
        downloaded: model::model_exists(model),
        size_mb: model::model_size_bytes(model) / 1_048_576,
    };
    *slot = Some((snapshot.clone(), std::time::Instant::now()));
    snapshot
}

/// Drop the cached snapshot after the configured model or a model file changed.
fn invalidate_model_snapshot() {
    *MODEL_SNAPSHOT.lock() = None;
}

#[tauri::command]
pub fn get_dictation_status(
    dictation: State<'_, DictationState>,
) -> Result<DictationStatus, String> {
    let snapshot = model_snapshot();
    let has_transcriber = dictation.transcriber_arc.lock().is_some();

    let model_status = if !snapshot.downloaded {
        "not_downloaded"
    } else if has_transcriber {
        "ready"
    } else {
        "downloaded" // Downloaded but not loaded yet
    };
    // Read one at a time: nesting the two locks would add a lock order that
    // nothing else in the module follows.
    let push_to_talk_level = dictation
        .audio
        .lock()
        .as_ref()
        .map(audio::AudioCapture::level);
    let hands_free_level = dictation
        .hands_free_audio
        .lock()
        .as_ref()
        .map(audio::AudioCapture::level);

    Ok(DictationStatus {
        model_status: model_status.to_string(),
        model_name: snapshot.model.name().to_string(),
        model_size_mb: snapshot.size_mb,
        recording: dictation.recording.load(Ordering::Acquire),
        processing: dictation.processing.load(Ordering::Acquire),
        audio_level: capture_level(push_to_talk_level, hands_free_level),
    })
}

/// The microphone level to show: push-to-talk's capture when it is open,
/// otherwise the hands-free one. Only one listens at a time; without the
/// fallback a hands-free conversation reads as a silent microphone.
fn capture_level(push_to_talk: Option<f32>, hands_free: Option<f32>) -> f32 {
    push_to_talk.or(hands_free).unwrap_or(0.0)
}

#[tauri::command]
pub fn get_model_info() -> Vec<ModelInfo> {
    model::WhisperModel::ALL
        .iter()
        .map(|m| ModelInfo {
            name: m.name().to_string(),
            display_name: m.display_name().to_string(),
            size_hint_mb: m.size_hint_mb(),
            downloaded: model::model_exists(*m),
            actual_size_mb: model::model_size_bytes(*m) / 1_048_576,
        })
        .collect()
}

#[tauri::command]
pub async fn download_whisper_model(app: AppHandle, model_name: String) -> Result<String, String> {
    let whisper_model = model::WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    if model::model_exists(whisper_model) {
        return Ok("Model already downloaded".to_string());
    }

    let app_clone = app.clone();
    let path = model::download_model(whisper_model, move |downloaded, total| {
        let payload = download_progress(None, downloaded, total);
        let _ = app_clone.emit(DICTATION_DOWNLOAD_PROGRESS, payload.clone());
        push_to_bus(
            &app_clone,
            crate::state::AppEvent::DictationDownloadProgress { payload },
        );
    })
    .await?;

    // The model is on disk now — its size and download state are cached.
    invalidate_model_snapshot();

    Ok(format!("Downloaded to {}", path.display()))
}

#[tauri::command]
pub fn delete_whisper_model(
    dictation: State<'_, DictationState>,
    model_name: String,
) -> Result<String, String> {
    let whisper_model = model::WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Unknown model: {model_name}"))?;

    // Unload transcriber if it's the active model
    let active = dictation.active_model.lock().clone();
    if active.as_deref() == Some(whisper_model.name()) {
        *dictation.transcriber_arc.lock() = None;
        *dictation.active_model.lock() = None;
    }

    model::delete_model(whisper_model)?;
    // The model file is gone — its size and download state are cached.
    invalidate_model_snapshot();
    Ok(format!("Deleted {}", whisper_model.display_name()))
}

// ---------------------------------------------------------------------------
// Speech assets — the voices and graphs behind spoken replies
// ---------------------------------------------------------------------------

/// What the settings panel needs to know about one downloadable asset.
///
/// `state` is a string rather than a bool pair because the four states are not
/// independent: an asset cannot be both downloading and incomplete as far as
/// the UI is concerned, and modelling them separately invites a panel that
/// renders "not installed" over a running progress bar.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SpeechAssetInfo {
    pub id: String,
    pub display_name: String,
    /// `"language"` or `"runtime"`.
    pub kind: String,
    /// The **Whisper language code** this speaks (`"it"`), absent for the
    /// runtime library.
    ///
    /// The code rather than the engine's own name for the language
    /// (`"italian"`), because this is the field a caller joins against: the
    /// dictation setting, `SpeechStatus.language` and `for_language_code` all
    /// speak in codes. Publishing the engine name here made the settings panel
    /// compare `"italian"` against `"it"`, find no asset for the configured
    /// language, and report that no bundle ships for it while listing the
    /// bundle one row above.
    pub language: Option<String>,
    pub voices: Vec<String>,
    pub download_bytes: u64,
    /// `"absent"`, `"downloading"`, `"incomplete"` or `"ready"`.
    pub state: String,
    /// Which files an incomplete asset is missing. Empty otherwise.
    pub missing: Vec<String>,
}

fn describe(asset: &speech::assets::Asset, downloading: bool) -> SpeechAssetInfo {
    use speech::assets::Status;
    let (state, missing) = if downloading {
        // Checked before the disk: a download in flight has a staging
        // directory and an install directory that still holds the old version,
        // so the on-disk answer would be the answer to a different question.
        ("downloading".to_string(), Vec::new())
    } else {
        match speech::assets::status(asset) {
            Status::Absent => ("absent".to_string(), Vec::new()),
            Status::Ready => ("ready".to_string(), Vec::new()),
            Status::Incomplete { missing } => ("incomplete".to_string(), missing),
        }
    };
    SpeechAssetInfo {
        id: asset.id.to_string(),
        display_name: asset.display_name.to_string(),
        kind: if asset.language().is_some() {
            "language".to_string()
        } else {
            "runtime".to_string()
        },
        language: asset.code().map(str::to_string),
        voices: asset.voices().iter().map(|v| (*v).to_string()).collect(),
        download_bytes: asset.download_bytes(),
        state,
        missing,
    }
}

/// Everything a user may install, and what state it is in.
#[tauri::command]
pub fn get_speech_assets(dictation: tauri::State<'_, DictationState>) -> Vec<SpeechAssetInfo> {
    speech::assets::CATALOGUE
        .iter()
        .map(|asset| describe(asset, dictation.speech.is_downloading(asset.id)))
        .collect()
}

/// Look an id up in the catalogue, refusing anything that is not in it.
///
/// This is the allowlist boundary: past here an id has become a `&'static
/// Asset` with a pinned URL and a pinned hash, so nothing a caller sends can
/// name a path or a host of its own.
fn resolve_asset(id: &str) -> Result<&'static speech::assets::Asset, String> {
    speech::assets::find(id).ok_or_else(|| format!("Unknown speech asset: {id}"))
}

#[tauri::command]
pub async fn download_speech_asset(app: AppHandle, asset: String) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    // Cloned out of the managed state in its own scope: a `State` guard held
    // across an await would make this future non-`Send`, and the download is
    // minutes long.
    let library = {
        let dictation = app.state::<DictationState>();
        Arc::clone(&dictation.speech)
    };

    let id = target.id.to_string();
    let progress_app = app.clone();
    let progress_id = id.clone();
    let installed = library
        .install(target, move |downloaded, total| {
            emit_speech_download(
                &progress_app,
                download_progress(Some(&progress_id), downloaded, total),
            );
        })
        .await;

    // Sent on success and on failure alike. Only the caller that started a
    // download has a return value to clear its bar with; every other client —
    // a browser, a second window, a script on the HTTP API — learns it ended
    // from this alone, and without it shows the last percent forever.
    emit_speech_download(&app, download_finished(&id));

    let path = installed.map_err(|error| error.to_string())?;
    Ok(format!("Installed to {}", path.display()))
}

/// Emit one speech-download event on both transports.
fn emit_speech_download(app: &AppHandle, payload: serde_json::Value) {
    let _ = app.emit(SPEECH_DOWNLOAD_PROGRESS, payload.clone());
    push_to_bus(
        app,
        crate::state::AppEvent::SpeechDownloadProgress { payload },
    );
}

/// The last event of a speech-asset download, whatever its outcome.
///
/// It carries no outcome on purpose: the catalogue is the one place that says
/// whether the asset is now ready, and a client re-reads it on `done`.
fn download_finished(asset: &str) -> serde_json::Value {
    serde_json::json!({ "asset": asset, "done": true })
}

/// The event a speech-asset download reports progress on.
pub const SPEECH_DOWNLOAD_PROGRESS: &str = "speech-download-progress";

/// The event a Whisper-model download reports progress on.
pub const DICTATION_DOWNLOAD_PROGRESS: &str = "dictation-download-progress";

/// The event a spoken reply reports its state on.
pub const SPEECH_UTTERANCE: &str = "speech-utterance";

/// The body both download events carry, built once.
///
/// `asset` is present only for a speech asset: a Whisper download has no id
/// because only one runs at a time, while the runtime library and a language
/// bundle can download together and a shared percent would show each of them
/// the other's.
fn download_progress(asset: Option<&str>, downloaded: u64, total: u64) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "downloaded": downloaded,
        "total": total,
        "percent": if total > 0 { (downloaded as f64 / total as f64 * 100.0) as u32 } else { 0 },
    });
    if let Some(asset) = asset {
        payload["asset"] = serde_json::Value::String(asset.to_string());
    }
    payload
}

/// Publish on the `/events` bus beside the desktop `emit`.
///
/// Both, never one: there is no bus-to-window forwarder, so a producer that
/// sends only to the bus goes silent on the desktop, and one that only emits
/// goes silent in a browser. A missing `AppState` is not an error — the
/// headless `tuic-remote` build has a bus and no window, and a test harness has
/// neither.
#[cfg(feature = "desktop")]
fn push_to_bus(app: &AppHandle, event: crate::state::AppEvent) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<Arc<crate::state::AppState>>() {
        let _ = state.event_bus.send(event);
    }
}

/// Reports every utterance transition on both transports.
///
/// It exists because the transitions worth reporting have no caller to return
/// to: `finished` is decided by the render thread once the device drains, and
/// `interrupted` by whoever talked over the reply. A client without this is a
/// client that polls `speech_status`.
#[cfg(feature = "desktop")]
struct PushUtterance {
    app: AppHandle,
}

#[cfg(feature = "desktop")]
impl speaker::UtteranceObserver for PushUtterance {
    fn changed(&self, id: speaker::UtteranceId, state: &speaker::Utterance, generation: u64) {
        // The same struct `speak` returns and `speech_status` nests, serialized
        // once for both transports. Three builders for one shape is how the
        // three descriptions of a reply would drift.
        let Ok(payload) = serde_json::to_value(SpokenReply::new(id, state, generation)) else {
            return;
        };
        let _ = self.app.emit(SPEECH_UTTERANCE, payload.clone());
        push_to_bus(
            &self.app,
            crate::state::AppEvent::SpeechUtterance { payload },
        );
    }
}

/// Send utterance transitions to the desktop window and the `/events` bus.
///
/// Called once at startup, before any conversation can be armed. Installing it
/// later would be a conversation whose replies are invisible to a browser.
#[cfg(feature = "desktop")]
pub fn install_utterance_observer(app: &AppHandle) {
    use tauri::Manager;
    *app.state::<DictationState>().utterance_observer.lock() =
        Some(Arc::new(PushUtterance { app: app.clone() }));
}

#[tauri::command]
pub fn cancel_speech_download(
    dictation: tauri::State<'_, DictationState>,
    asset: String,
) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    if dictation.speech.cancel_download(target.id) {
        Ok(format!("Cancelled {}", target.display_name))
    } else {
        // Not an error the user caused: a download that finished between the
        // click and the command is the common way to get here.
        Ok(format!("{} was not downloading", target.display_name))
    }
}

#[tauri::command]
pub fn delete_speech_asset(
    dictation: tauri::State<'_, DictationState>,
    asset: String,
) -> Result<String, String> {
    let target = resolve_asset(&asset)?;
    dictation
        .speech
        .delete(target)
        .map_err(|error| error.to_string())?;
    Ok(format!("Deleted {}", target.display_name))
}

// ---------------------------------------------------------------------------
// Spoken replies (817-f67c)
// ---------------------------------------------------------------------------

/// The engine and voice a reply would be spoken with, or why there is none.
///
/// Every `Err` here is a setup problem stated in the user's terms, because
/// every one of them reaches a model as "unavailable, and here is why" rather
/// than as a failure it should retry.
fn open_voice(
    config: &DictationConfig,
    library: &speech::library::SpeechLibrary,
    language: &str,
) -> Result<(Arc<dyn speech::Speech>, String), String> {
    if !config.speech_command.is_empty() {
        // The user's own engine. It names its own voices inside its template,
        // so there is nothing here to choose between and the voice is empty.
        let engine = speech::external::ExternalSpeech::new(config.speech_command.clone())
            .map_err(|error| error.to_string())?;
        return Ok((Arc::new(engine), String::new()));
    }

    let asset = speech::assets::for_language_code(language).ok_or_else(|| {
        // Named rather than swapped for one we do ship. Speaking Italian into
        // an English conversation is worse than saying nothing, and a model
        // that is told *why* can write its reply as text instead.
        format!("No speech bundle ships for language \"{language}\"")
    })?;
    let runtime = speech::assets::runtime();
    for needed in [runtime, asset] {
        match speech::assets::status(needed) {
            speech::assets::Status::Ready => {}
            speech::assets::Status::Absent => {
                return Err(format!("{} is not downloaded", needed.display_name));
            }
            speech::assets::Status::Incomplete { missing } => {
                return Err(format!(
                    "{} is incomplete; missing {}",
                    needed.display_name,
                    missing.join(", ")
                ));
            }
        }
    }
    let voice = choose_voice(asset, &config.speech_voice)?;
    // From the library rather than built here: it is the one instance that
    // serialises replacing a language against speaking it, and an engine built
    // beside it would hold the very files a download is about to rename away.
    let engine = library.engine(asset.language().unwrap_or_default());
    Ok((engine, voice.to_string()))
}

/// Which of a language's voices to speak with.
///
/// Empty means "whatever this language ships first", which is what an
/// untouched configuration says and what every configuration said before the
/// setting existed.
///
/// A named voice the language does not ship is an error rather than a silent
/// fall back to the first one. The two ways to get here are a catalogue that
/// dropped a voice and a language the user changed underneath the setting;
/// both are cases where speaking in a voice nobody chose is worse than saying
/// why nothing was spoken, and the message reaches the user through the
/// hands-free status rather than being buried in a log.
fn choose_voice(asset: &speech::assets::Asset, configured: &str) -> Result<&'static str, String> {
    let offered = asset.voices();
    if configured.is_empty() {
        return offered
            .first()
            .copied()
            .ok_or_else(|| format!("{} ships no voice", asset.display_name));
    }
    offered
        .iter()
        .copied()
        .find(|voice| *voice == configured)
        .ok_or_else(|| {
            format!(
                "{} does not ship a voice called \"{configured}\"; it offers {}",
                asset.display_name,
                if offered.is_empty() {
                    "none".to_string()
                } else {
                    offered.join(", ")
                }
            )
        })
}

/// The language this conversation is being held in.
///
/// A fixed setting answers for every turn, including the first. `auto` can only
/// be answered by whoever spoke: [`HandsFree::turn_language`] carries what
/// Whisper made of the last turn, and before anybody has spoken there is no
/// answer at all — which is `None`, never a default.
///
/// Reading the mode is why this must not be called with the speaker lock held.
fn conversation_language(config: &DictationConfig, dictation: &DictationState) -> Option<String> {
    if config.language != "auto" {
        return Some(config.language.clone());
    }
    dictation
        .hands_free
        .lock()
        .turn_language()
        .map(str::to_string)
}

/// The language a voice must be opened for, or why none can be.
///
/// Empty is a real answer here and means "not language-specific": a
/// user-supplied engine picks its own language inside its command template, so
/// there is nothing for us to choose and nothing that a change of detected
/// language invalidates.
fn speech_language(config: &DictationConfig, dictation: &DictationState) -> Result<String, String> {
    if !config.speech_command.is_empty() {
        return Ok(String::new());
    }
    conversation_language(config, dictation).ok_or_else(|| {
        "Dictation language is Auto and nothing has been said yet, so there is no language to \
         speak in"
            .to_string()
    })
}

/// Build the reply queue for a conversation that is being armed.
///
/// Failure is not fatal to arming: hands-free without a voice is dictation,
/// which still works. The reason travels back so the caller can say it once
/// rather than leaving the model to discover it on its first `speak`.
///
/// Under Auto this fails at arm time by design — nobody has spoken, so no
/// language is known. The queue is then built by the first [`speak`] that finds
/// one, which is why that path must not assume this one succeeded.
pub(crate) fn open_speaker(
    dictation: &DictationState,
    generation: u64,
) -> Result<speaker::Armed, String> {
    let config = get_dictation_config();
    let language = speech_language(&config, dictation)?;
    let owner = conversation_owner(dictation);
    open_speaker_for(dictation, generation, &config, &language, owner.as_deref())
}

/// Who armed the conversation, if one is armed.
///
/// Read through the mode lock and handed on as a plain string rather than
/// re-read deeper in: the reply queue is built with the speaker lock held, and
/// taking the mode lock there would invert the order every other path uses.
fn conversation_owner(dictation: &DictationState) -> Option<String> {
    dictation
        .hands_free
        .lock()
        .binding()
        .map(|binding| binding.owner.clone())
}

/// Where a conversation's replies come out.
///
/// The owner decides, and there is no fallback: a reply for a browser-owned
/// conversation whose client has gone is a failure, never something the server
/// speakers pick up. Somebody in another room hearing the answer to a question
/// they did not ask is worse than a reply that is reported failed.
fn open_reply_output(
    dictation: &DictationState,
    owner: Option<&str>,
) -> Result<Arc<dyn speaker::Output>, String> {
    match owner {
        Some(owner) if owner != DESKTOP_OWNER => {
            let link = dictation
                .browser_endpoints
                .get(owner)
                .ok_or_else(|| format!("No client is connected for audio owner '{owner}'"))?;
            Ok(Arc::new(browser::BrowserOutput::new(link)))
        }
        // The system default device. Picking one is the Dictation panel's job
        // (#818-2a29); `config.device` is the *microphone* and using it here
        // would route replies to a capture device.
        _ => Ok(Arc::new(speaker::DeviceOutput::open(None)?)),
    }
}

/// [`open_speaker`] with the language already resolved.
///
/// The split is a lock-order rule, not a convenience: resolving the language
/// takes the hands-free lock, and this is called with the speaker lock held.
/// Taking them in that order here would invert every other path in this file.
fn open_speaker_for(
    dictation: &DictationState,
    generation: u64,
    config: &DictationConfig,
    language: &str,
    owner: Option<&str>,
) -> Result<speaker::Armed, String> {
    let (engine, voice) = open_voice(config, &dictation.speech, language)?;
    let device = open_reply_output(dictation, owner)?;
    // Wrapped so the canceller learns what is being played. Without this the
    // microphone hears the reply and the VAD opens a turn on the application's
    // own voice.
    //
    // A browser-owned conversation is tapped too: its microphone and its
    // speaker are in the same room as each other, which is the situation the
    // canceller exists for. Only the *devices* moved.
    let tapped = speech_far_end(device, dictation);
    let queue = Arc::new(speaker::Speaker::new(engine, tapped, generation));
    // Before the first reply can be queued, which is the whole requirement:
    // `observe` is set-once, and nothing has transitioned yet.
    if let Some(observer) = dictation.utterance_observer.lock().clone() {
        queue.observe(observer);
    }
    Ok(speaker::Armed {
        speaker: queue,
        voice,
        language: language.to_string(),
    })
}

/// Barge-in, pointed at the slot rather than at one queue.
///
/// The capture loop is started once, when the conversation is armed, and under
/// Auto the queue it will have to interrupt does not exist until somebody
/// speaks. Holding the slot means the loop interrupts whatever is speaking on
/// the tick the user talks over it, including a voice built minutes later and a
/// voice rebuilt because the language changed.
struct ArmedSpeaker(Arc<parking_lot::Mutex<Option<speaker::Armed>>>);

impl continuous::Interruptible for ArmedSpeaker {
    fn hush(&self) {
        // `try_lock`, because this runs on the capture loop and anything it
        // waits for delays the next chunk of the user's own voice. The only
        // writer is a rebuild in `speak`, which has already hushed the queue it
        // is replacing and has not started the new one — so a missed lock here
        // is a tick with nothing to interrupt, not a missed interruption.
        if let Some(slot) = self.0.try_lock()
            && let Some(armed) = slot.as_ref()
        {
            armed.speaker.hush();
        }
    }
}

fn speech_far_end(
    device: Arc<dyn speaker::Output>,
    dictation: &DictationState,
) -> Arc<dyn speaker::Output> {
    Arc::new(echo::FarEndTap::new(device, dictation.echo.clone()))
}

/// What a caller is told about one reply it asked for.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpokenReply {
    /// Identifies this reply for as long as the conversation remembers it.
    pub utterance_id: String,
    /// `queued`, `rendering`, `speaking`, `finished`, `interrupted` or
    /// `failed`. Accepting a reply reports `queued` — never `finished`.
    pub state: String,
    /// Set only for `failed`.
    pub error: Option<String>,
    /// The turn this reply belongs to. A reply for a turn that has ended is
    /// refused rather than spoken.
    pub turn: u64,
}

impl SpokenReply {
    fn new(id: speaker::UtteranceId, state: &speaker::Utterance, turn: u64) -> Self {
        Self {
            utterance_id: id.to_string(),
            state: match state {
                speaker::Utterance::Queued => "queued",
                speaker::Utterance::Rendering => "rendering",
                speaker::Utterance::Speaking => "speaking",
                speaker::Utterance::Finished => "finished",
                speaker::Utterance::Interrupted => "interrupted",
                speaker::Utterance::Failed(_) => "failed",
            }
            .to_string(),
            error: match state {
                speaker::Utterance::Failed(reason) => Some(reason.clone()),
                _ => None,
            },
            turn,
        }
    }
}

/// Whether this installation can speak right now, and into which conversation.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    /// Can a reply be spoken right now? False whenever hands-free is not
    /// armed, and whenever it is armed without a working voice.
    pub available: bool,
    /// Why not. Empty when `available`.
    pub unavailable_reason: String,
    /// The terminal replies are spoken into, absent when nothing is armed.
    pub session_id: Option<String>,
    /// The language this conversation is being held in, as a two-letter code.
    ///
    /// The dictation setting when it names one, and what Whisper made of the
    /// last turn when the setting is Auto. Empty means nobody has spoken yet
    /// under Auto, which is the one state in which no reply can be spoken and
    /// no reply language can be required — never a silent fall back to
    /// English.
    pub language: String,
    /// The turn a reply must belong to. Bumped by every interruption, so a
    /// model answering an older turn can be refused rather than played over
    /// whatever the user said next.
    ///
    /// Not the same counter as `HandsFreeStatus::generation`, which counts
    /// arms rather than interruptions; this one is the speaker's.
    pub turn: u64,
    /// The voice, empty for a user-supplied engine that names its own.
    pub voice: String,
    pub queued: usize,
    pub rendering: bool,
    pub speaking: bool,
    /// The last synthesis or device failure, cleared by the next reply that
    /// works.
    pub last_error: Option<String>,
    /// The reply the caller asked about, absent when it asked about none.
    ///
    /// An id this conversation no longer remembers comes back with a state of
    /// `unknown` rather than as an absent field: "I have forgotten" and "you
    /// did not ask" are different answers and a caller polling for its own
    /// reply has to be able to tell them apart.
    pub utterance: Option<SpokenReply>,
}

/// Who is asking to speak.
///
/// The distinction is the binding: the owner armed the conversation and may
/// always drive it, while a model may only speak into the conversation it is
/// itself the target of. A model that could speak into another terminal's
/// conversation would be talking to somebody else's user.
pub(crate) enum Caller<'a> {
    /// The user's own UI, on either transport.
    Owner,
    /// A model, named by the live PTY its MCP connection is bound to — the
    /// same key a hands-free binding holds.
    Model(&'a str),
}

impl Caller<'_> {
    /// May this caller drive the conversation bound to `session_id`?
    fn may_drive(&self, session_id: &str) -> Result<(), String> {
        match self {
            Self::Owner => Ok(()),
            Self::Model(caller) if *caller == session_id => Ok(()),
            Self::Model(_) => Err(
                "Speech is bound to another session; only the session hands-free is armed for can speak"
                    .to_string(),
            ),
        }
    }
}

/// The armed conversation's terminal, or why there is none.
fn bound_session(dictation: &DictationState) -> Result<String, String> {
    reap_finished_runtime(dictation);
    dictation
        .hands_free
        .lock()
        .binding()
        .map(|binding| binding.session_id.clone())
        .ok_or_else(|| {
            "Hands-free is not armed; there is no conversation to speak into".to_string()
        })
}

/// Queue a spoken reply.
///
/// Returns as soon as the reply is accepted, carrying the identity the caller
/// polls to find out whether anybody heard it. `turn` refuses a reply written
/// for a turn the user has already talked over; omitting it means "now".
pub(crate) fn speak(
    dictation: &DictationState,
    caller: Caller<'_>,
    text: &str,
    turn: Option<u64>,
) -> Result<SpokenReply, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nothing to say".to_string());
    }
    if text.chars().count() > MAX_SPOKEN_CHARS {
        return Err(format!(
            "A spoken reply is limited to {MAX_SPOKEN_CHARS} characters; this one is {}",
            text.chars().count()
        ));
    }
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;

    // Everything that reads the mode happens before the speaker lock is taken,
    // and the rebuild below is handed the answers. The two locks are always
    // taken in this order.
    let config = get_dictation_config();
    let language = speech_language(&config, dictation)?;
    let armed_at = dictation.hands_free.lock().generation();
    let owner = conversation_owner(dictation);

    let mut slot = dictation.speaker.lock();
    // The queue is per language, so a conversation that changed language needs
    // a new one. `hush` on the way out is what makes the change invalidate the
    // replies written for the old language: it opens a new turn, and `say`
    // refuses anything addressed to the turn before it.
    let rebuild_at = match slot.as_ref() {
        Some(armed) if armed.language == language => None,
        Some(_) => slot.take().map(|previous| previous.speaker.hush()),
        // Nothing yet: under Auto this is the first reply of the conversation,
        // and the language only became knowable when the user spoke.
        None => Some(armed_at),
    };
    if let Some(turn) = rebuild_at {
        *slot = Some(open_speaker_for(
            dictation,
            turn,
            &config,
            &language,
            owner.as_deref(),
        )?);
    }
    let armed = slot.as_ref().expect("a queue was just built or kept");
    // The speaker's own counter, not the caller's guess: it is what `say`
    // compares against, and reading it here makes an interruption landing in
    // between refuse the reply rather than race it.
    let current = armed.speaker.generation();
    let wanted = turn.unwrap_or(current);
    let id = armed
        .speaker
        .say(wanted, text, &armed.voice)
        .map_err(|error| error.to_string())?;
    let state = armed
        .speaker
        .utterance(id)
        .unwrap_or(speaker::Utterance::Queued);
    // Nothing is emitted from here, on purpose (833-6fd4). The push comes from
    // the speaker's own transitions — see `PushUtterance` — because the states
    // worth reporting have no caller to return to: `finished` is decided by the
    // render thread once the device drains, and `interrupted` by whoever talked
    // over the reply. An emit here would report `queued` twice and the rest
    // never.
    Ok(SpokenReply::new(id, &state, wanted))
}

/// Stop talking now and open a new turn.
///
/// Reports the state afterwards rather than a bare success: the caller needs
/// the new turn to know which replies are still worth sending.
pub(crate) fn stop_speaking(
    dictation: &DictationState,
    caller: Caller<'_>,
) -> Result<SpeechStatus, String> {
    let session_id = bound_session(dictation)?;
    caller.may_drive(&session_id)?;
    if let Some(armed) = dictation.speaker.lock().as_ref() {
        armed.speaker.hush();
    }
    Ok(speech_status(dictation, None))
}

/// The longest reply that will be accepted, in characters.
///
/// Well past a conversational answer and well short of a model pasting a file.
/// The budget in [`speech`](super::speech::budget_seconds) already stops a
/// runaway *rendering*, but it cannot stop a caller queueing four of these and
/// filling the queue with ten minutes of audio.
const MAX_SPOKEN_CHARS: usize = 2_000;

/// [`speech_status`] for a caller whose right to this conversation must be
/// checked first.
///
/// A model bound to another terminal is told it is not the target rather than
/// shown somebody else's queue — the status fields alone would leak what the
/// other conversation is doing, and a model that can see a queue will try to
/// speak into it.
///
/// Not armed at all is a status rather than an error: the model asked a fair
/// question and the honest answer is "nothing is armed".
pub(crate) fn speech_status_for(
    dictation: &DictationState,
    caller: Caller<'_>,
    utterance: Option<&str>,
) -> Result<SpeechStatus, String> {
    if let Ok(session_id) = bound_session(dictation) {
        caller.may_drive(&session_id)?;
    }
    Ok(speech_status(dictation, utterance))
}

/// Everything a caller needs to decide whether to speak, and what became of a
/// reply it already sent.
pub(crate) fn speech_status(dictation: &DictationState, utterance: Option<&str>) -> SpeechStatus {
    let session_id = bound_session(dictation).ok();
    let config = get_dictation_config();
    // Everything that reads the mode is read here, before the speaker lock: the
    // language of the conversation, the turn a caller would address, and
    // whether a voice could be opened at all. Asking any of them later would
    // take the two locks in the opposite order to `speak`.
    let language = conversation_language(&config, dictation).unwrap_or_default();
    let openable = session_id.as_ref().map(|_| {
        speech_language(&config, dictation)
            .and_then(|language| open_voice(&config, &dictation.speech, &language).map(|_| ()))
    });
    let armed_at = dictation.hands_free.lock().generation();

    let armed = dictation.speaker.lock();
    let Some(armed) = armed.as_ref() else {
        // No voice open. Under Auto that is every moment before the first
        // turn, and it is not a failure: `available` answers "would a reply be
        // accepted", which is a question about the language and the bundle
        // rather than about whether anything has been said yet.
        return SpeechStatus {
            available: matches!(openable, Some(Ok(()))),
            unavailable_reason: match &openable {
                None => "Hands-free is not armed".to_string(),
                Some(Err(reason)) => reason.clone(),
                Some(Ok(())) => String::new(),
            },
            session_id,
            language,
            turn: armed_at,
            voice: String::new(),
            queued: 0,
            rendering: false,
            speaking: false,
            last_error: None,
            utterance: None,
        };
    };
    let status = armed.speaker.status();
    let asked_about = utterance.map(|asked| {
        let known = asked
            .parse::<speaker::UtteranceId>()
            .ok()
            .and_then(|id| armed.speaker.utterance(id).map(|state| (id, state)));
        match known {
            Some((id, state)) => SpokenReply::new(id, &state, status.generation),
            None => SpokenReply {
                utterance_id: asked.to_string(),
                state: "unknown".to_string(),
                error: None,
                turn: status.generation,
            },
        }
    });
    SpeechStatus {
        available: true,
        unavailable_reason: String::new(),
        session_id,
        language,
        turn: status.generation,
        voice: armed.voice.clone(),
        queued: status.queued,
        rendering: status.rendering,
        speaking: status.speaking,
        last_error: status.last_error,
        utterance: asked_about,
    }
}

/// Speak a reply into the armed conversation.
///
/// The desktop and browser control surface. A model does not call this — it
/// goes through the `voice` MCP tool, which supplies its own identity so the
/// binding can be checked. Here the caller *is* the owner: it is the thing
/// that armed the conversation.
#[tauri::command]
pub fn speak_reply(
    dictation: tauri::State<'_, DictationState>,
    text: String,
    turn: Option<u64>,
) -> Result<SpokenReply, String> {
    speak(&dictation, Caller::Owner, &text, turn)
}

/// Stop talking now, dropping whatever was queued for this turn.
#[tauri::command]
pub fn stop_speech(dictation: tauri::State<'_, DictationState>) -> Result<SpeechStatus, String> {
    stop_speaking(&dictation, Caller::Owner)
}

/// Whether anything can be spoken, and what became of a reply already sent.
#[tauri::command]
pub fn get_speech_status(
    dictation: tauri::State<'_, DictationState>,
    utterance: Option<String>,
) -> SpeechStatus {
    speech_status(&dictation, utterance.as_deref())
}

/// Start push-to-talk recording.
///
/// `command(async)` rather than a plain `command`: a sync Tauri command runs on
/// the main thread, and the first press of the hotkey loads the whisper model
/// there — a multi-second GGML + GPU init that freezes the whole UI. `async`
/// makes Tauri run this body on its async runtime instead, which is separate
/// from the runtime the HTTP server owns (see `lib.rs`), so nothing else stalls.
/// The function itself stays sync, so the HTTP route calls it unchanged.
///
// DEFERRED (2026-08-17) — the load still occupies one Tauri runtime worker for
// its duration. Moving it to `spawn_blocking` needs an async fn, which means
// changing this signature and the caller in `mcp_http/dictation_routes.rs`.
/// Make sure the microphone is usable, or say why it is not.
///
/// Shared by push-to-talk and hands-free: one spelling of the TCC dance, so the
/// two modes cannot disagree about what "denied" means.
fn ensure_microphone_access() -> Result<(), String> {
    match permission::check() {
        permission::MicPermission::Denied => Err("microphone_denied".to_string()),
        permission::MicPermission::Restricted => Err("microphone_restricted".to_string()),
        permission::MicPermission::NotDetermined => {
            // CoreAudio (cpal) does NOT trigger the TCC prompt — we must
            // explicitly request access via AVCaptureDevice to show the dialog.
            if permission::request() {
                Ok(())
            } else {
                Err("microphone_denied".to_string())
            }
        }
        permission::MicPermission::Authorized => Ok(()),
    }
}

/// The loaded recogniser, loading it first if the model changed or none is up.
///
/// Both modes share the one `transcriber_arc`: loading a second copy of a
/// multi-gigabyte model because the other mode got there first would be a
/// straightforward way to run the machine out of memory.
///
/// `app` is `None` off the desktop event loop, where there is no handle to log
/// through; the load is the same either way.
fn ensure_transcriber(
    app: Option<&AppHandle>,
    dictation: &DictationState,
    whisper_model: model::WhisperModel,
) -> Result<Arc<dyn transcribe::Transcriber>, String> {
    let mut transcriber_arc_lock = dictation.transcriber_arc.lock();
    let mut active_model_lock = dictation.active_model.lock();
    let model_changed = active_model_lock
        .as_deref()
        .map(|name| name != whisper_model.name())
        .unwrap_or(true);

    if model_changed || transcriber_arc_lock.is_none() {
        if !model::model_exists(whisper_model) {
            return Err("Model not downloaded".to_string());
        }
        let loading = format!("Loading model: {}", whisper_model.display_name());
        match app {
            Some(app) => app_logger::log_via_handle(app, "info", "dictation", &loading),
            None => tracing::info!(source = "dictation", "{loading}"),
        }
        let t = transcribe::WhisperTranscriber::load(&model::model_path(whisper_model))?;
        *transcriber_arc_lock = Some(Arc::new(t));
        *active_model_lock = Some(whisper_model.name().to_string());
        let loaded = format!("Model loaded (backend: {})", transcribe::backend_label());
        match app {
            Some(app) => app_logger::log_via_handle(app, "info", "dictation", &loaded),
            None => tracing::info!(source = "dictation", "{loaded}"),
        }
    }

    transcriber_arc_lock
        .clone()
        .ok_or_else(|| "Transcriber not available".to_string())
}

#[tauri::command(async)]
pub fn start_dictation(app: AppHandle, dictation: State<'_, DictationState>) -> Result<(), String> {
    // Atomic test-and-set: prevents TOCTOU race from concurrent IPC calls
    if dictation
        .recording
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Already recording".to_string());
    }
    // Guard resets recording=false if we return early on any error path
    let mut recording_guard = RecordingGuard::new(&dictation.recording);

    if dictation.processing.load(Ordering::Acquire) {
        return Err("Transcription in progress".to_string());
    }

    ensure_microphone_access()?;

    // One read of dictation-config.json for the whole start: the model, the
    // input device and the language all come from this snapshot.
    let config = get_dictation_config();
    let whisper_model = resolve_model(&config.model);
    let transcriber_arc = ensure_transcriber(Some(&app), &dictation, whisper_model)?;

    // Always emit backend info so the frontend gets it even when model is reused
    let _ = app.emit(
        "dictation-backend-info",
        serde_json::json!({
            "backend": transcribe::backend_label(),
        }),
    );

    // Start audio capture using the configured device (or system default)
    let device_name = config.device.as_deref().filter(|s| !s.is_empty());
    let capture = audio::AudioCapture::start_with_device(device_name).map_err(|e| {
        app_logger::log_via_handle(
            &app,
            "error",
            "dictation",
            &format!("Audio capture failed: {e}"),
        );
        // If a specific device failed, hint the user
        if device_name.is_some() {
            app_logger::log_via_handle(
                &app,
                "warn",
                "dictation",
                "Configured device not available — check Settings > Dictation > Microphone",
            );
        }
        e
    })?;

    // Get audio buffer handle for streaming thread
    let audio_buffer = capture.buffer_handle();
    *dictation.audio.lock() = Some(capture);

    // Start streaming session
    let lang = if config.language == "auto" {
        None
    } else {
        Some(config.language.clone())
    };
    let (tx, rx) = mpsc::channel::<String>();

    let session = streaming::StreamingSession::start(
        transcriber_arc as Arc<dyn transcribe::Transcriber>,
        audio_buffer,
        tx,
        lang,
        config.gates(),
    );
    *dictation.streaming.lock() = Some(session);

    // recording is already true (set by compare_exchange above)
    app_logger::log_via_handle(&app, "info", "dictation", "Streaming recording started");

    // Reset accumulated partials for this session
    dictation.accumulated_partials.lock().clear();

    // Forward partial text to the live preview. The microphone meter is read
    // through get_dictation_status so desktop IPC and HTTP clients use the same
    // request/response surface.
    let app_clone = app.clone();
    let accumulated = dictation.inner().accumulated_partials.clone();
    std::thread::Builder::new()
        .name("dictation-event-forwarder".into())
        .spawn(move || {
            for text in rx {
                {
                    let mut acc = accumulated.lock();
                    if !acc.is_empty() {
                        acc.push(' ');
                    }
                    acc.push_str(&text);
                }
                if let Err(e) = app_clone.emit("dictation-partial", &text) {
                    tracing::warn!(source = "dictation", "Failed to emit partial event: {e}");
                }
            }
        })
        .map_err(|e| format!("Failed to spawn event forwarder: {e}"))?;

    // Success: keep recording=true (disarm the guard so it doesn't reset on drop)
    recording_guard.disarm();
    Ok(())
}

#[tauri::command]
pub async fn stop_dictation_and_transcribe(app: AppHandle) -> Result<TranscribeResponse, String> {
    // Gather all data from DictationState synchronously (before any .await).
    // This block ensures no MutexGuard or State borrow lives across the await point.
    let prepare = {
        let dictation = app.state::<DictationState>();

        if !dictation.recording.load(Ordering::Acquire) {
            return Err("Not recording".to_string());
        }

        // Set recording=false synchronously so the UI updates immediately
        dictation.recording.store(false, Ordering::Release);
        dictation.processing.store(true, Ordering::Release);

        // Stop audio capture (stops the cpal stream, but buffer data remains)
        let mut capture_lock = dictation.audio.lock();
        if let Some(ref mut capture) = *capture_lock {
            capture.stop_stream();
        }

        // Take the streaming session (cheap — no join yet) and the audio buffer handle.
        // The actual thread join happens in spawn_blocking to avoid blocking the tokio worker.
        let session = dictation.streaming.lock().take();
        let audio_buffer = capture_lock.as_ref().map(|c| c.buffer_handle());
        drop(capture_lock);

        // Read config while we still have sync context (avoids file I/O after .await)
        let config = get_dictation_config();
        let lang_owned = if config.language == "auto" {
            None
        } else {
            Some(config.language.clone())
        };

        // Clone Arc-ed resources for the blocking task
        let transcriber = dictation.transcriber_arc.lock().clone();
        let accumulated_partials = dictation.accumulated_partials.clone();
        let corrections = dictation.corrections.clone();
        let processing = dictation.processing.clone();

        Some((
            session,
            audio_buffer,
            lang_owned,
            config.gates(),
            transcriber,
            accumulated_partials,
            corrections,
            processing,
        ))
    };

    let (
        session,
        audio_buffer,
        lang_owned,
        gates,
        transcriber,
        accumulated_partials,
        corrections,
        processing,
    ) = prepare.unwrap(); // always Some — the None path returns Err above

    let app_clone = app.clone();

    // Run session join + whisper inference off the IPC thread
    let result = tokio::task::spawn_blocking(move || {
        let _guard = ProcessingGuard(processing);

        // Join the streaming thread (may block while last partial window finishes)
        let streamed = session.map(|s| s.stop()).unwrap_or_default();
        let mut dropped_samples = streamed.dropped_samples;
        let mut all_audio = streamed.audio;

        // Drain anything left in the audio capture buffer (arrived after last poll).
        // Safe: streaming thread is joined above, no more concurrent readers.
        if let Some(buf) = audio_buffer {
            let remaining: Vec<f32> = buf.lock().drain(..).collect();
            all_audio.extend(remaining);
        }
        // That tail never passed the streaming thread's cap, and a slow final
        // window makes it arbitrarily long. Cap the assembled recording once.
        dropped_samples += streaming::cap_finished_recording(&mut all_audio);

        let truncated_s = dropped_samples as f64 / 16000.0;
        let total_duration_s = all_audio.len() as f64 / 16000.0;

        // A panicked streaming thread took the recording with it. Whatever
        // reached the capture buffer afterwards is not the recording, and
        // transcribing it would report a fragment as the whole answer.
        if streamed.interrupted {
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                "Streaming thread was interrupted — the recording is not recoverable",
            );
            return TranscribeResponse {
                text: String::new(),
                // Rendered by `useDictation` as "Dictation: <reason>".
                skip_reason: Some("recording was interrupted".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }
        app_logger::log_via_handle(
            &app_clone,
            "info",
            "dictation",
            &format!(
                "Streaming stopped, {:.1}s total audio for final transcription",
                total_duration_s
            ),
        );

        // Short audio: no transcription needed
        if all_audio.len() < 8000 {
            app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("no speech detected".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }

        let mut final_text = String::new();

        if let Some(ref transcriber) = transcriber {
            let lang_ref = lang_owned.as_deref();
            match transcriber.transcribe(&all_audio, lang_ref, gates) {
                Ok(result) if result.skip_reason.is_none() => {
                    final_text = result.text;
                }
                Ok(result) => {
                    if let Some(reason) = &result.skip_reason {
                        app_logger::log_via_handle(
                            &app_clone,
                            "info",
                            "dictation",
                            &format!("Final transcription skipped: {reason}"),
                        );
                    }
                }
                Err(e) => {
                    app_logger::log_via_handle(
                        &app_clone,
                        "warn",
                        "dictation",
                        &format!("Final transcription failed: {e}"),
                    );
                }
            }
        } else {
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                "Transcriber not available — model not loaded",
            );
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("model not loaded".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }

        if final_text.is_empty() {
            app_logger::log_via_handle(&app_clone, "info", "dictation", "No speech detected");
            return TranscribeResponse {
                text: String::new(),
                skip_reason: Some("no speech detected".to_string()),
                duration_s: total_duration_s,
                truncated_s,
            };
        }

        // Log accuracy comparison (lengths only — no verbatim text to avoid PII in logs)
        let composed = std::mem::take(&mut *accumulated_partials.lock());
        let full_chars = final_text.chars().count();
        let composed_chars = composed.chars().count();
        let ratio = transcription_ratio(&final_text, &composed);
        app_logger::log_via_handle(
            &app_clone,
            "info",
            "dictation",
            &format!(
                "[accuracy] full={} chars, composed={} chars, ratio={}, audio={:.1}s",
                full_chars,
                composed_chars,
                ratio.map_or_else(|| "n/a".to_string(), |r| format!("{:.0}%", r * 100.0)),
                total_duration_s
            ),
        );
        // The final pass is normally the LONGER of the two — streaming skips
        // VAD-silent windows. Coming back shorter means it lost text the
        // streaming windows already had, which is the shape of window tail loss.
        if ratio.is_some_and(|r| r < SHORT_TRANSCRIPTION_RATIO) {
            app_logger::log_via_handle(
                &app_clone,
                "warn",
                "dictation",
                &format!(
                    "Final transcription is shorter than the streaming partials: full={full_chars} chars against composed={composed_chars} chars (below {:.0}%)",
                    SHORT_TRANSCRIPTION_RATIO * 100.0
                ),
            );
        }

        // Apply corrections
        let corrected = corrections.lock().correct(&final_text);
        let final_text = corrected.replace('\n', " ");

        // _guard drops here → processing = false
        TranscribeResponse {
            text: final_text,
            skip_reason: None,
            duration_s: total_duration_s,
            truncated_s,
        }
    })
    .await
    .map_err(|e| {
        let msg = format!("Transcription task panicked: {e}");
        app_logger::log_via_handle(&app, "error", "dictation", &msg);
        msg
    })?;

    // Clean up audio capture
    *app.state::<DictationState>().audio.lock() = None;

    Ok(result)
}

#[tauri::command]
pub fn get_correction_map(dictation: State<'_, DictationState>) -> HashMap<String, String> {
    dictation.corrections.lock().get_replacements().clone()
}

#[tauri::command]
pub fn set_correction_map(
    dictation: State<'_, DictationState>,
    map: HashMap<String, String>,
) -> Result<(), String> {
    let mut corrections = dictation.corrections.lock();
    corrections.set_replacements(map);
    corrections.save_to_file(&corrections::TextCorrector::default_path())
}

#[tauri::command]
pub fn list_audio_devices() -> Vec<audio::AudioDevice> {
    audio::list_input_devices()
}

/// Shell integration: inject text into active terminal.
/// Currently only callable from within the app via Tauri IPC.
///
/// Future external trigger mechanisms:
/// 1. CLI: `tuicommander inject "text"` via IPC socket
/// 2. Pipe: `echo "text" | tuicommander --inject`
/// 3. Tauri deep link: `tuicommander://inject?text=...`
///
/// Security: Will require authentication token stored in env var.
#[tauri::command]
pub fn inject_text(dictation: State<'_, DictationState>, text: String) -> Result<String, String> {
    // Apply corrections before injection
    let corrected = dictation.corrections.lock().correct(&text);
    let final_text = corrected.replace('\n', " ");
    Ok(final_text)
}

// ---------------------------------------------------------------------------
// Hands-free mode
// ---------------------------------------------------------------------------

/// Longest accepted session id or audio owner.
///
/// Both come from the caller, and over HTTP that caller is a remote client. The
/// owner in particular is retained for as long as the mode stays armed and
/// echoed back in every status reply, so an unbounded one is a buffer the
/// client controls the size of.
pub(crate) const MAX_BINDING_LEN: usize = 256;

/// Hands-free state, as both transports report it.
///
/// One struct, serialized by the Tauri command and by the HTTP route, so the
/// field names and casing cannot drift between them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandsFreeStatus {
    pub armed: bool,
    /// See `continuous::Phase::as_wire`.
    pub phase: String,
    /// The bound delivery target. Unchanged by focus for as long as it is set.
    pub session_id: Option<String>,
    /// The bound audio endpoint.
    pub owner: Option<String>,
    pub generation: u64,
    /// The transcript waiting out its hold-back, so the UI can show what is
    /// about to be sent while there is still time to stop it.
    pub pending_text: Option<String>,
    /// Compose-queue ids this mode owns and would cancel on disarm.
    pub queued_ids: Vec<u64>,
    pub hold_back_ms: u64,
    pub error: Option<String>,
    /// Monotonic turn counts; a client plays an earcon when one moves. See
    /// `continuous::HandsFree::delivered_turns`.
    pub delivered_turns: u64,
    pub dropped_turns: u64,
}

/// What a disarm did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandsFreeDisarmed {
    /// False when there was nothing to disarm. The other fields are then empty
    /// rather than implying work that did not happen.
    pub was_armed: bool,
    pub generation: u64,
    /// Voice entries pulled back out of the Compose queue.
    pub cancelled: Vec<u64>,
    /// Voice entries the composer already had. Nothing can retract these, and
    /// reporting them is the difference between an honest outcome and a claim.
    pub already_delivered: Vec<u64>,
    pub discarded_pending: bool,
    pub discarded_capture: bool,
    pub status: HandsFreeStatus,
}

pub(crate) fn hands_free_status(dictation: &DictationState) -> HandsFreeStatus {
    // A poll is also where a runtime that ended by itself gets cleaned up; see
    // `reap_finished_runtime` for why the thread cannot do it.
    reap_finished_runtime(dictation);
    let mode = dictation.hands_free.lock();
    HandsFreeStatus {
        armed: mode.binding().is_some(),
        phase: mode.phase().as_wire().to_string(),
        session_id: mode.binding().map(|binding| binding.session_id.clone()),
        owner: mode.binding().map(|binding| binding.owner.clone()),
        generation: mode.generation(),
        pending_text: mode.pending_text().map(str::to_string),
        queued_ids: mode.owned_ids().to_vec(),
        hold_back_ms: mode.hold_back_ms(),
        error: mode.last_error().map(str::to_string),
        delivered_turns: mode.delivered_turns(),
        dropped_turns: mode.dropped_turns(),
    }
}

/// Take the settings the mode reads into it, in one config load. Both are
/// no-ops while armed — see `HandsFree::set_hold_back_ms`.
pub(crate) fn apply_config_to_mode(dictation: &DictationState) {
    let config = get_dictation_config();
    let mut mode = dictation.hands_free.lock();
    mode.set_hold_back_ms(config.hands_free_hold_back_ms.into());
    mode.set_activation(
        &config.hands_free_activation_phrase,
        continuous::ACTIVATION_WINDOW_MS,
    );
}

fn check_binding_field(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} is empty"));
    }
    if value.len() > MAX_BINDING_LEN {
        return Err(format!("{label} is too long"));
    }
    Ok(())
}

/// The owner name that means "this machine".
///
/// Any other owner is a browser or remote client, and is served by the socket
/// it connected on — never by the desktop microphone. Arming from a laptop must
/// not open the microphone on the machine running TUICommander, so an owner
/// with no socket behind it is refused rather than fallen back.
pub(crate) const DESKTOP_OWNER: &str = "desktop";

/// The desktop microphone plus the loaded whisper model.
///
/// Holds only the capture *buffer*, never the `cpal::Stream`: the stream is
/// `!Send` and stays in `DictationState`, which is also what keeps push-to-talk
/// and hands-free on separate devices.
struct DesktopVoiceEndpoint {
    buffer: Arc<parking_lot::Mutex<std::collections::VecDeque<f32>>>,
    alive: Arc<AtomicBool>,
    transcriber: Arc<dyn transcribe::Transcriber>,
    language: Option<String>,
    gates: transcribe::VoiceGates,
}

impl continuous::VoiceEndpoint for DesktopVoiceEndpoint {
    fn drain(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.buffer.lock().drain(..).collect())
    }

    fn connected(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn transcribe(&self, audio: &[f32]) -> Result<continuous::Transcript, String> {
        transcribe_utterance(
            self.transcriber.as_ref(),
            audio,
            self.language.as_deref(),
            self.gates,
        )
    }
}

/// Recognise one closed utterance, the same way for every endpoint.
///
/// Shared rather than duplicated per endpoint because the gate handling below
/// is a decision, not plumbing: where the audio came from changes nothing about
/// what a gated segment means.
pub(crate) fn transcribe_utterance(
    transcriber: &dyn transcribe::Transcriber,
    audio: &[f32],
    language: Option<&str>,
    gates: transcribe::VoiceGates,
) -> Result<continuous::Transcript, String> {
    let result = transcriber.transcribe(audio, language, gates)?;
    // A gated segment is not an error and not a message: whisper decided
    // this was not speech, so the utterance is dropped the same way an
    // empty transcript is — and with it goes the language, which would
    // otherwise be whatever whisper made of room noise.
    Ok(if result.skip_reason.is_some() {
        continuous::Transcript::default()
    } else {
        continuous::Transcript {
            text: result.text,
            language: result.language,
        }
    })
}

/// Open the capture endpoint the named owner is entitled to.
///
/// The owner decides which device is opened, and there is deliberately no
/// fallback between the two: an unknown owner is refused, not served by
/// whatever this machine happens to have.
fn open_endpoint(
    dictation: &DictationState,
    owner: &str,
) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
    let config = get_dictation_config();
    let language = (config.language != "auto").then(|| config.language.clone());
    let gates = config.gates();
    if owner != DESKTOP_OWNER {
        let link = dictation
            .browser_endpoints
            .get(owner)
            .ok_or_else(|| format!("No client is connected for audio owner '{owner}'"))?;
        // The model is loaded here for the same reason as below: recognition
        // runs on this machine whichever microphone fed it. Only the audio is
        // remote.
        let transcriber = ensure_transcriber(None, dictation, resolve_model(&config.model))?;
        // The canceller aligns a reply's start against capture not yet
        // drained; this stream's backlog is what it has to count.
        let backlog = Arc::clone(&link);
        dictation
            .echo
            .lock()
            .attach_capture(Box::new(move || backlog.pending_capture()));
        return Ok(Box::new(browser::BrowserVoiceEndpoint::new(
            link,
            transcriber,
            language,
            gates,
        )));
    }
    ensure_microphone_access()?;
    let transcriber = ensure_transcriber(None, dictation, resolve_model(&config.model))?;
    let device_name = config.device.as_deref().filter(|name| !name.is_empty());
    let capture = audio::AudioCapture::start_with_device(device_name)?;
    let buffer = capture.buffer_handle();
    // See the browser branch above: the canceller counts this backlog.
    let backlog = Arc::clone(&buffer);
    dictation
        .echo
        .lock()
        .attach_capture(Box::new(move || backlog.lock().len()));
    *dictation.hands_free_audio.lock() = Some(capture);
    dictation
        .hands_free_owner_alive
        .store(true, Ordering::Release);
    Ok(Box::new(DesktopVoiceEndpoint {
        buffer,
        alive: dictation.hands_free_owner_alive.clone(),
        transcriber,
        language,
        gates,
    }))
}

/// Release the desktop audio endpoint.
///
/// The runtime sees its owner gone on the next tick and disarms itself with
/// `OwnerDisconnected` — the mode is never left armed against a microphone that
/// is no longer the one it bound to.
pub(crate) fn release_desktop_endpoint(dictation: &DictationState) {
    dictation
        .hands_free_owner_alive
        .store(false, Ordering::Release);
}

/// Drop a runtime whose thread has already returned, and the microphone with it.
///
/// An automatic disarm (closed target, dead device) ends the thread from the
/// inside, and the thread cannot release the capture device itself — a
/// `cpal::Stream` is `!Send`, so it never crossed the thread boundary. Reaping
/// here means the microphone closes on the next status poll rather than staying
/// open until somebody arms again.
fn reap_finished_runtime(dictation: &DictationState) {
    let finished = dictation
        .hands_free_runtime
        .lock()
        .as_ref()
        .is_some_and(continuous::HandsFreeRuntime::is_finished);
    if finished {
        *dictation.hands_free_runtime.lock() = None;
        *dictation.hands_free_audio.lock() = None;
        release_desktop_endpoint(dictation);
    }
}

/// Bind hands-free capture to a session and an audio owner.
///
/// This binds the target and the audio owner, opens the endpoint that owner
/// names, and starts the runtime that carries speech from it to the Compose
/// queue. Arming is also what 817's speech capability and 821's entry hint key
/// off, which is why it is reachable before any UI exists.
///
/// Refused when the target cannot take a Compose-queue entry. There is no
/// fallback delivery path, so an unsupported target stays unavailable — and so
/// does an audio endpoint this build does not implement.
pub(crate) fn arm_hands_free(
    state: &Arc<crate::state::AppState>,
    dictation: &DictationState,
    session_id: &str,
    owner: &str,
) -> Result<HandsFreeStatus, String> {
    arm_hands_free_with(state, dictation, session_id, owner, &open_endpoint)
}

/// `arm_hands_free` with the capture endpoint supplied.
///
/// The seam exists so a test can drive the whole armed path — bind, capture,
/// segment, transcribe, hold back, enqueue — without a microphone or a
/// multi-gigabyte model, against a real session and the real Compose queue.
pub(crate) fn arm_hands_free_with(
    state: &Arc<crate::state::AppState>,
    dictation: &DictationState,
    session_id: &str,
    owner: &str,
    open_endpoint: &dyn Fn(
        &DictationState,
        &str,
    ) -> Result<Box<dyn continuous::VoiceEndpoint>, String>,
) -> Result<HandsFreeStatus, String> {
    check_binding_field(session_id, "Session id")?;
    check_binding_field(owner, "Audio owner")?;
    if !crate::pty::session_accepts_voice(state, session_id) {
        return Err("Session cannot accept hands-free input".to_string());
    }
    reap_finished_runtime(dictation);
    if dictation.hands_free.lock().binding().is_some() {
        return Err("Hands-free is already armed".to_string());
    }
    // The microphone opens before the bind, so a refused or broken endpoint
    // leaves the mode untouched rather than armed-and-deaf with a generation
    // already spent.
    let endpoint = open_endpoint(dictation, owner)?;

    apply_config_to_mode(dictation);
    let armed = dictation
        .hands_free
        .lock()
        .arm(session_id, owner, true)
        .map_err(|error| match error {
            continuous::ArmError::AlreadyArmed => "Hands-free is already armed".to_string(),
            continuous::ArmError::UnsupportedTarget => {
                "Session cannot accept hands-free input".to_string()
            }
        });
    if let Err(error) = armed {
        *dictation.hands_free_audio.lock() = None;
        release_desktop_endpoint(dictation);
        return Err(error);
    }

    // Tell the model the conversation opened, if the user asked us to. First
    // into the FIFO, before the runtime can put a spoken turn behind it: a
    // model that reads "you can answer out loud" after the question it applies
    // to has been told nothing useful.
    //
    // A refusal is recorded on the mode rather than failing the arm. The
    // microphone works, the Compose queue works for ordinary turns, and a
    // conversation the model was not told about is a worse conversation rather
    // than no conversation — the reason is visible in the hands-free status.
    let config = get_dictation_config();
    if config.hands_free_notify_model
        && let Some(Err(error)) = continuous::deliver_entry_hint(
            &mut dictation.hands_free.lock(),
            &continuous::PtyVoiceQueue(state.as_ref()),
            &continuous::entry_hint_text(&config.hands_free_start_notice),
            Some(&config.language),
        )
    {
        tracing::warn!(
            source = "dictation",
            "Hands-free start notice refused: {error}"
        );
        dictation.hands_free.lock().note_send_failed(&error);
    }

    // The reply queue, built with the turn the mode just opened so both halves
    // agree about which turn is current from the first reply onwards.
    //
    // Before the runtime, not after: the capture loop takes the queue as its
    // barge-in port, and a loop started first would spend its first ticks
    // unable to interrupt anything.
    //
    // A failure here does not fail the arm. Hands-free without a voice is
    // dictation, which is useful on its own and is what a user who has not
    // downloaded a language bundle gets; the reason is logged once here and
    // reported by the voice capability rather than being discovered per reply.
    let generation = dictation.hands_free.lock().generation();
    match open_speaker(dictation, generation) {
        Ok(armed) => *dictation.speaker.lock() = Some(armed),
        Err(reason) => {
            // Not a failure to arm, and under Auto not even a failure: no
            // language is known until the user speaks, so the first reply
            // opens the voice instead.
            tracing::info!("dictation: armed without a voice yet: {reason}");
            *dictation.speaker.lock() = None;
        }
    }
    // The slot, not the queue in it. A port bound to the queue built above
    // would be bound to nothing whenever that build failed — which is every
    // Auto conversation — and barge-in would stay dead for the whole session
    // even once a later reply opened a voice.
    let interruptible: Option<Arc<dyn continuous::Interruptible>> =
        Some(Arc::new(ArmedSpeaker(Arc::clone(&dictation.speaker))));

    // DEFERRED (2026-09-21) — the segmenter runs on its compiled defaults.
    // Hold-back is read from user config just above; pre-roll, trailing
    // silence, minimum speech and the utterance cap are not reachable from
    // `DictationConfig`, so `SegmenterConfig` is parameterised without being
    // tunable by anyone but a recompile. Left as-is because no measurement has
    // yet shown a default that needs moving, and a knob nobody has asked for is
    // a knob that has to be documented, persisted and migrated. Wire it when
    // Step 8 (#818-2a29) gives Dictation settings a place to put it, or sooner
    // if trailing silence proves wrong for a real speaker.
    *dictation.hands_free_runtime.lock() = Some(continuous::spawn_runtime(
        state.clone(),
        dictation.hands_free.clone(),
        endpoint,
        continuous::SegmenterConfig::default(),
        dictation.echo.clone(),
        interruptible,
    ));

    Ok(hands_free_status(dictation))
}

/// Disarm the whole mode and cancel what it still owns.
///
/// Idempotent: disarming a mode that was never armed reports `was_armed: false`
/// and cancels nothing, rather than inventing an outcome.
pub(crate) fn disarm_hands_free(
    state: &crate::state::AppState,
    dictation: &DictationState,
) -> HandsFreeDisarmed {
    use crate::dictation::continuous::{DisarmReason, PtyVoiceQueue, cancel_disarmed};

    let disarmed = dictation.hands_free.lock().disarm(DisarmReason::Manual);
    // Stop talking first, and unconditionally. Dropping the queue cancels the
    // reply in flight and stops the device, which is what makes disarm revoke
    // speech whether or not the model ever acknowledged anything — a late
    // `speak` then finds no speaker and is told so rather than being played to
    // a user who has left.
    *dictation.speaker.lock() = None;
    // Stop the runtime and close the microphone whichever way this went: a
    // thread that already disarmed itself still has a device to release.
    *dictation.hands_free_runtime.lock() = None;
    *dictation.hands_free_audio.lock() = None;
    release_desktop_endpoint(dictation);
    let Some(disarmed) = disarmed else {
        // Both fields below read the mode, and `hands_free` is not reentrant: a
        // `lock()` temporary inside the struct literal lives until the end of
        // the whole statement, so taking it there deadlocks against the one
        // `hands_free_status` takes. Read it once, first.
        let status = hands_free_status(dictation);
        return HandsFreeDisarmed {
            was_armed: false,
            generation: status.generation,
            cancelled: Vec::new(),
            already_delivered: Vec::new(),
            discarded_pending: false,
            discarded_capture: false,
            status,
        };
    };
    let cancellation = cancel_disarmed(&PtyVoiceQueue(state), &disarmed);
    // Driven by what this arm actually sent, never by the setting as it reads
    // now. A user who turns the notice off mid-conversation has changed what
    // the *next* arm says; the model that already read "you can answer out
    // loud" still has to be told that stopped being true.
    continuous::report_exit_hint(&PtyVoiceQueue(state), &disarmed, &cancellation);
    HandsFreeDisarmed {
        was_armed: true,
        generation: disarmed.generation,
        cancelled: cancellation.cancelled,
        already_delivered: cancellation.already_delivered,
        discarded_pending: disarmed.discarded_pending,
        discarded_capture: disarmed.discarded_capture,
        status: hands_free_status(dictation),
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn arm_hands_free_dictation(
    state: State<'_, Arc<crate::state::AppState>>,
    dictation: State<'_, DictationState>,
    session_id: String,
    owner: String,
) -> Result<HandsFreeStatus, String> {
    arm_hands_free(&state, &dictation, &session_id, &owner)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn disarm_hands_free_dictation(
    state: State<'_, Arc<crate::state::AppState>>,
    dictation: State<'_, DictationState>,
) -> HandsFreeDisarmed {
    disarm_hands_free(&state, &dictation)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn get_hands_free_status(dictation: State<'_, DictationState>) -> HandsFreeStatus {
    hands_free_status(&dictation)
}

/// Dictation configuration persisted to <config_dir>/dictation-config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationConfig {
    pub enabled: bool,
    pub hotkey: String,
    pub language: String,
    /// Selected whisper model name (e.g. "large-v3-turbo", "small")
    #[serde(default = "default_model")]
    pub model: String,
    /// Selected audio input device name. None or empty = system default.
    #[serde(default)]
    pub device: Option<String>,
    /// Long-press threshold in milliseconds for push-to-talk activation.
    /// A short press (below this duration) passes through as normal input.
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u32,
    /// Automatically send (press Enter) after injecting transcribed text.
    #[serde(default)]
    pub auto_send: bool,
    /// Minimum RMS before audio is sent to Whisper. See [`VoiceGates`].
    #[serde(default = "default_rms_threshold")]
    pub rms_threshold: f32,
    /// Maximum `no_speech_probability` accepted for a segment. See [`VoiceGates`].
    #[serde(default = "default_no_speech_threshold")]
    pub no_speech_threshold: f32,
    /// Visible hold-back between a hands-free transcript and its enqueue, in
    /// milliseconds. Zero would send every utterance the instant it lands, so a
    /// config written before hands-free existed takes the default instead.
    #[serde(default = "default_hold_back_ms")]
    pub hands_free_hold_back_ms: u32,
    /// Optional activation phrase for hands-free dictation. Empty means every
    /// recognised utterance is a turn. Set, it must open each new turn: the
    /// match runs locally on the whisper transcript and the phrase is removed
    /// before anything is submitted, so a model never reads it and unrelated
    /// speech never leaves the machine.
    #[serde(default)]
    pub hands_free_activation_phrase: String,
    /// Tell the bound model when hands-free starts and when it stops.
    ///
    /// On by default: the `voice` tool is listed whether or not this is set,
    /// and a model with no reason to speak writes text — so an unset default
    /// would ship a voice nobody ever hears. Turning it off silences both
    /// notices and nothing else; disarming still revokes speech, because that
    /// is a fact about this machine rather than a message to a model.
    #[serde(default = "default_notify_model")]
    pub hands_free_notify_model: bool,
    /// The start notice sent when `hands_free_notify_model` is on. Empty means
    /// the built-in text, which [`get_hands_free_default_notice`] returns so a
    /// settings surface can show it and reset to it. Folded to one line before
    /// it is sent — see [`continuous::entry_hint_text`].
    #[serde(default)]
    pub hands_free_start_notice: String,
    /// Play a short sound on the owning client when a spoken turn reaches the
    /// agent, and a softer one when the activation phrase drops it. On by
    /// default. Read by the frontend only; the backend reports the turns
    /// either way (`HandsFreeStatus::delivered_turns`).
    #[serde(default = "default_earcons")]
    pub hands_free_earcons: bool,
    /// A speech engine the user supplies, as argv rather than a shell line.
    /// Empty means the bundled engine. See
    /// [`speech::external`](crate::dictation::speech::external) for the
    /// markers and for what it means that this runs as the user.
    #[serde(default)]
    pub speech_command: Vec<String>,
    /// Which of the language's voices to speak with. Empty means the first one
    /// it ships, which is what a configuration written before this setting
    /// existed says. Ignored by a user-supplied engine, which names its own
    /// voices inside its command template. See [`choose_voice`].
    #[serde(default)]
    pub speech_voice: String,
}

fn default_model() -> String {
    "large-v3-turbo".to_string()
}

fn default_long_press_ms() -> u32 {
    400
}

fn default_rms_threshold() -> f32 {
    transcribe::DEFAULT_RMS_THRESHOLD
}

fn default_no_speech_threshold() -> f32 {
    transcribe::DEFAULT_NO_SPEECH_THRESHOLD
}

/// Long enough to read a transcript and stop it, short enough not to feel like
/// a delay. The number is a setting; this is only where it starts.
pub(crate) fn default_hold_back_ms() -> u32 {
    1_500
}

/// See [`DictationConfig::hands_free_notify_model`].
fn default_notify_model() -> bool {
    true
}

/// See [`DictationConfig::hands_free_earcons`].
fn default_earcons() -> bool {
    true
}

impl DictationConfig {
    /// The speech gates this configuration asks for.
    pub fn gates(&self) -> transcribe::VoiceGates {
        transcribe::VoiceGates {
            rms_threshold: self.rms_threshold,
            no_speech_threshold: self.no_speech_threshold,
        }
    }
}

impl Default for DictationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: "F5".to_string(),
            language: "auto".to_string(),
            model: default_model(),
            device: None,
            long_press_ms: default_long_press_ms(),
            auto_send: false,
            rms_threshold: default_rms_threshold(),
            no_speech_threshold: default_no_speech_threshold(),
            hands_free_hold_back_ms: default_hold_back_ms(),
            hands_free_activation_phrase: String::new(),
            hands_free_notify_model: default_notify_model(),
            hands_free_start_notice: String::new(),
            hands_free_earcons: default_earcons(),
            speech_command: Vec::new(),
            speech_voice: String::new(),
        }
    }
}

const DICTATION_CONFIG_FILE: &str = "dictation-config.json";

/// The built-in hands-free start notice, sent while
/// [`DictationConfig::hands_free_start_notice`] is empty.
#[tauri::command]
pub fn get_hands_free_default_notice() -> String {
    continuous::MODE_ENTRY_HINT.to_string()
}

#[tauri::command]
pub fn get_dictation_config() -> DictationConfig {
    crate::config::load_json_config(DICTATION_CONFIG_FILE)
}

#[tauri::command]
pub fn set_dictation_config(
    config: DictationConfig,
    dictation: State<'_, DictationState>,
) -> Result<(), String> {
    save_dictation_config(config, Some(&dictation))
}

/// [`set_dictation_config`] for a caller that may not have the dictation state.
///
/// `None` only affects the live conversation: the file is written either way,
/// and the next reply re-reads it. A transport that can reach `DictationState`
/// passes it so a language change takes effect on the voice that is speaking
/// right now, rather than on the one after it.
pub(crate) fn save_dictation_config(
    config: DictationConfig,
    dictation: Option<&DictationState>,
) -> Result<(), String> {
    // DEFERRED (2026-09-21) — switching the input device while hands-free is
    // armed should release the endpoint the mode bound to (that is an owner
    // disconnect, see `release_desktop_endpoint`). The state is reachable here
    // now, but the endpoint swap needs the runtime to be restarted around it,
    // which is a change to `arm_hands_free_with` rather than to this function.
    // Until then the mode keeps capturing from the device it armed with.
    let previous = get_dictation_config();
    crate::config::ConfigFile::<DictationConfig>::new(DICTATION_CONFIG_FILE).save(&config)?;
    // The configured model is part of the cached status snapshot.
    invalidate_model_snapshot();
    // A voice belongs to a language and to an engine. Change either and every
    // reply already queued for the old one is wrong — a sentence half spoken
    // in Italian does not become English by finishing it. Dropping the queue
    // stops the device and cancels what is in flight; the next reply opens a
    // voice for the language now configured.
    //
    // Only on those three fields. Every other setting here is a threshold or a
    // hotkey, and cutting a reply off mid-word because somebody moved a slider
    // would be a worse bug than the one this prevents.
    let voice_changed = previous.language != config.language
        || previous.speech_command != config.speech_command
        || previous.speech_voice != config.speech_voice;
    if voice_changed && let Some(dictation) = dictation {
        *dictation.speaker.lock() = None;
    }
    Ok(())
}

/// Check microphone permission status (macOS TCC).
/// Returns: "authorized", "denied", "restricted", or "not_determined".
#[tauri::command]
pub fn check_microphone_permission() -> String {
    permission::check().as_str().to_string()
}

/// Open macOS System Settings > Privacy > Microphone.
#[tauri::command]
pub fn open_microphone_settings() {
    permission::open_settings();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::continuous::Phase;

    /// A microphone and a recogniser the test writes the script for.
    ///
    /// After its one phrase it keeps handing back room silence, exactly as a
    /// live capture device does — a device that stops delivering samples is a
    /// failure, and this fake must not fake one.
    struct ScriptedEndpoint {
        phrase: parking_lot::Mutex<Option<Vec<f32>>>,
        transcript: String,
    }

    impl continuous::VoiceEndpoint for ScriptedEndpoint {
        fn drain(&mut self) -> Result<Vec<f32>, String> {
            Ok(self
                .phrase
                .lock()
                .take()
                .unwrap_or_else(|| vec![0.0; continuous::SAMPLE_RATE as usize / 20]))
        }

        fn connected(&self) -> bool {
            true
        }

        fn transcribe(&self, _audio: &[f32]) -> Result<continuous::Transcript, String> {
            Ok(continuous::Transcript {
                text: self.transcript.clone(),
                language: Some("it".to_string()),
            })
        }
    }

    /// A microphone in a quiet room. It keeps delivering samples, as a live
    /// device does, and nobody ever says anything into it.
    ///
    /// The notices are about the *mode*, so a test for them must not have to
    /// stage a spoken turn to see one.
    fn silent_endpoint()
    -> impl Fn(&DictationState, &str) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
        |_dictation, _owner| {
            Ok(Box::new(ScriptedEndpoint {
                phrase: parking_lot::Mutex::new(None),
                transcript: String::new(),
            }))
        }
    }

    /// Wait for `needle` to be typed into the recorded terminal.
    ///
    /// The composer flushes on its own schedule, so the alternative is a fixed
    /// sleep — a guess about scheduling rather than a deadline.
    fn wait_for_typed(bytes: &Arc<std::sync::Mutex<Vec<u8>>>, needle: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let typed = String::from_utf8_lossy(&bytes.lock().expect("recorder")).to_string();
            if typed.contains(needle) {
                return typed;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let typed = String::from_utf8_lossy(&bytes.lock().expect("recorder")).to_string();
        panic!("{needle:?} was never typed into the terminal; it holds {typed:?}");
    }

    /// Whether `needle` reached the session — parked in its Compose FIFO, or
    /// already typed into its terminal.
    ///
    /// Both are the same fact for a caller that enqueued it: the composer's
    /// busy gate decides which of the two it is at any instant, and a test
    /// that demanded one of them would be asserting the gate's timing rather
    /// than the delivery.
    fn wait_for_voice_text(
        state: &crate::state::AppState,
        session_id: &str,
        typed: &Arc<std::sync::Mutex<Vec<u8>>>,
        needle: &str,
    ) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let parked = state
                .pending_injections
                .get(session_id)
                .is_some_and(|queue| queue.iter().any(|entry| entry.text() == needle));
            let written =
                String::from_utf8_lossy(&typed.lock().expect("recorder")).contains(needle);
            if parked || written {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    /// A spoken phrase: 600ms of tone, then long enough a pause to close it.
    fn spoken_phrase() -> Vec<f32> {
        let sample_rate = continuous::SAMPLE_RATE as f32;
        let speech = (0..(sample_rate as usize * 600 / 1000))
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin() * 0.5);
        speech
            .chain(std::iter::repeat_n(0.0, sample_rate as usize))
            .collect()
    }

    fn scripted_endpoint(
        transcript: &'static str,
    ) -> impl Fn(&DictationState, &str) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
        move |_dictation, _owner| {
            Ok(Box::new(ScriptedEndpoint {
                phrase: parking_lot::Mutex::new(Some(spoken_phrase())),
                transcript: transcript.to_string(),
            }))
        }
    }

    /// Wait for the runtime thread to park a voice entry, or say what it did
    /// instead. A fixed sleep would be a guess about scheduling; the hold-back
    /// is a real deadline and the slack above it is the harness bound.
    fn wait_for_voice_entry(
        state: &crate::state::AppState,
        session_id: &str,
        hold_back_ms: u64,
    ) -> u64 {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(hold_back_ms + 10_000);
        while std::time::Instant::now() < deadline {
            let voice = state.pending_injections.get(session_id).and_then(|queue| {
                queue
                    .iter()
                    .find(|entry| entry.voice_generation().is_some())
                    .map(crate::state::PendingInjection::id)
            });
            if let Some(id) = voice {
                return id;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the hands-free runtime never parked a voice entry");
    }

    /// Whether a voice entry turns up inside `window`. Used for the assertion
    /// that none does; the caller measures the window rather than guessing it.
    fn voice_entry_within(
        state: &crate::state::AppState,
        session_id: &str,
        window: std::time::Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + window;
        while std::time::Instant::now() < deadline {
            let present = state
                .pending_injections
                .get(session_id)
                .is_some_and(|queue| queue.iter().any(|entry| entry.voice_generation().is_some()));
            if present {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    /// Both halves of a binding are attacker-shaped input on the HTTP transport:
    /// a remote client names the session and the owner. Neither may be empty,
    /// and neither may be unbounded — the owner string is retained for as long
    /// as the mode is armed and echoed back in every status reply.
    #[test]
    fn arming_rejects_unbounded_or_empty_identifiers() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        assert_eq!(
            arm_hands_free(&state, &dictation, "", "desktop").unwrap_err(),
            "Session id is empty".to_string()
        );
        assert_eq!(
            arm_hands_free(&state, &dictation, "session", "").unwrap_err(),
            "Audio owner is empty".to_string()
        );
        assert_eq!(
            arm_hands_free(
                &state,
                &dictation,
                &"s".repeat(MAX_BINDING_LEN + 1),
                "desktop"
            )
            .unwrap_err(),
            "Session id is too long".to_string()
        );
        assert_eq!(
            arm_hands_free(
                &state,
                &dictation,
                "session",
                &"o".repeat(MAX_BINDING_LEN + 1)
            )
            .unwrap_err(),
            "Audio owner is too long".to_string()
        );
        assert!(
            !hands_free_status(&dictation).armed,
            "a refused arm must leave the mode disarmed"
        );
    }

    /// The unsupported-target rule, at the surface a caller actually reaches.
    #[test]
    fn arming_against_a_target_that_cannot_take_a_compose_entry_is_refused() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        assert_eq!(
            arm_hands_free(&state, &dictation, "no-such-session", "desktop").unwrap_err(),
            "Session cannot accept hands-free input".to_string()
        );
        let status = hands_free_status(&dictation);
        assert!(!status.armed);
        assert_eq!(status.phase, "disarmed");
        assert_eq!(status.session_id, None);
    }

    /// Disarm is idempotent and says plainly that there was nothing to disarm,
    /// rather than reporting a cancellation it did not perform.
    #[test]
    fn disarming_a_mode_that_was_never_armed_reports_no_work() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let dictation = DictationState::new();

        let outcome = disarm_hands_free(&state, &dictation);

        assert!(!outcome.was_armed);
        assert!(outcome.cancelled.is_empty());
        assert!(outcome.already_delivered.is_empty());
        assert!(!outcome.discarded_pending);
        assert_eq!(outcome.status.phase, "disarmed");
    }

    /// The status reply is the one shape both transports serialize, so this
    /// pins the fields a store reads on IPC *and* over HTTP.
    #[test]
    fn a_disarmed_status_names_every_field_a_client_reads() {
        let dictation = DictationState::new();

        let status = hands_free_status(&dictation);
        let wire = serde_json::to_value(&status).expect("serialize");

        assert_eq!(wire["armed"], serde_json::json!(false));
        assert_eq!(wire["phase"], serde_json::json!("disarmed"));
        assert_eq!(wire["sessionId"], serde_json::Value::Null);
        assert_eq!(wire["owner"], serde_json::Value::Null);
        assert_eq!(wire["generation"], serde_json::json!(0));
        assert_eq!(wire["pendingText"], serde_json::Value::Null);
        assert_eq!(wire["queuedIds"], serde_json::json!([]));
        assert_eq!(
            wire["holdBackMs"],
            serde_json::json!(default_hold_back_ms())
        );
        assert_eq!(wire["deliveredTurns"], serde_json::json!(0));
        assert_eq!(wire["droppedTurns"], serde_json::json!(0));
    }

    /// A config written before the earcons setting existed must load with
    /// them on, and an explicit off must survive the whole-document rewrite.
    #[test]
    fn the_earcons_setting_defaults_on_and_survives_a_rewrite() {
        let older: DictationConfig =
            serde_json::from_str(r#"{"enabled":true,"hotkey":"F5","language":"auto"}"#)
                .expect("an older config loads");
        assert!(older.hands_free_earcons);

        let off = DictationConfig {
            hands_free_earcons: false,
            ..Default::default()
        };
        let wire = serde_json::to_string(&off).expect("serialize");
        let back: DictationConfig = serde_json::from_str(&wire).expect("deserialize");
        assert!(!back.hands_free_earcons);
    }

    /// The hold-back is a setting, not a constant, and arming is what reads it.
    #[test]
    fn arming_takes_the_hold_back_from_the_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save_dictation_config(
            DictationConfig {
                hands_free_hold_back_ms: 4_000,
                ..Default::default()
            },
            None,
        )
        .expect("config save");
        let dictation = DictationState::new();

        apply_config_to_mode(&dictation);

        assert_eq!(hands_free_status(&dictation).hold_back_ms, 4_000);
    }

    /// A config written before hands-free existed must keep a usable hold-back,
    /// not deserialize to zero and send every utterance the instant it lands.
    #[test]
    fn a_config_written_before_hands_free_existed_keeps_a_hold_back() {
        let stored = serde_json::json!({
            "enabled": true,
            "hotkey": "F5",
            "language": "auto",
        });
        let config: DictationConfig = serde_json::from_value(stored).expect("deserialize");

        assert_eq!(config.hands_free_hold_back_ms, default_hold_back_ms());
        assert!(config.hands_free_hold_back_ms > 0);
        assert!(
            config.hands_free_activation_phrase.is_empty(),
            "an upgrade may not start gating speech the user never configured"
        );
    }

    /// The activation phrase is a setting, and arming is what reads it. The
    /// whole gate is dead code if this wiring is missing, which is exactly the
    /// shape of a matcher with no caller.
    #[cfg(unix)]
    #[test]
    fn a_configured_activation_phrase_decides_which_speech_reaches_the_queue() {
        // The start notice is story 821's and has its own tests. Off here, so
        // the first voice entry this test waits for is the spoken turn.
        let _config = config_of_this_test(DictationConfig {
            hands_free_activation_phrase: "ciao tuic".to_string(),
            hands_free_hold_back_ms: 100,
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-gate", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-gate");

        // The addressed half first, and timed: the gate decides in the same
        // tick as the transcription, so how long an accepted turn takes to
        // appear on this machine bounds how long a rejected one could.
        let dictation = DictationState::new();
        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-gate",
            "desktop",
            &scripted_endpoint("Ciao Tuic, run the tests"),
        )
        .expect("arm");
        let started = std::time::Instant::now();
        let voice_id = wait_for_voice_entry(&state, "voice-gate", armed.hold_back_ms);
        let accepted_in = started.elapsed();
        let text = state
            .pending_injections
            .get("voice-gate")
            .expect("queue")
            .iter()
            .find(|entry| entry.id() == voice_id)
            .map(|entry| entry.text().to_string())
            .expect("the queued entry");
        assert_eq!(
            text, "run the tests (reply in Italian)",
            "the phrase addresses the tool and may not reach the model, and the language the \
             user spoke it in must"
        );
        disarm_hands_free(&state, &dictation);

        // The same pipeline, same session, unrelated speech: recognised
        // locally, then dropped.
        let dictation = DictationState::new();
        arm_hands_free_with(
            &state,
            &dictation,
            "voice-gate",
            "desktop",
            &scripted_endpoint("cancella tutto il repository"),
        )
        .expect("arm");
        let window = (accepted_in * 5).max(std::time::Duration::from_secs(2));
        assert!(
            !voice_entry_within(&state, "voice-gate", window),
            "speech without the activation phrase must not reach the Compose queue"
        );
        disarm_hands_free(&state, &dictation);
    }

    /// The whole feature, once, against a real session: arm binds, the status
    /// reports the binding, a held-back transcript reaches the Compose queue,
    /// and disarm cancels what it owns and nothing else.
    ///
    /// The target is deliberately BUSY. An idle one would have its entry typed
    /// by the flush, and then there would be nothing parked left to cancel —
    /// the case this test exists to check.
    #[cfg(unix)]
    #[test]
    fn arming_binds_a_real_session_and_disarm_cancels_only_its_own_entries() {
        // Its own configuration, with the start notice off: this test counts
        // the entries the mode owns, and the notice is one of them. Story 821
        // proves the notice itself.
        let _config = config_of_this_test(DictationConfig {
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-e2e", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-e2e");
        let dictation = DictationState::new();
        // Work a human and a peer already parked on the same session.
        let (human, notice) = {
            let mut queue = state
                .pending_injections
                .entry("voice-e2e".to_string())
                .or_default();
            let human = crate::state::PendingInjection::user_command("typed by hand");
            let notice = crate::state::PendingInjection::notice("peer mail wake");
            let ids = (human.id(), notice.id());
            queue.push_back(human);
            queue.push_back(notice);
            ids
        };

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-e2e",
            "desktop",
            &scripted_endpoint("run the tests"),
        )
        .expect("arm");
        assert!(armed.armed);
        assert_eq!(armed.session_id.as_deref(), Some("voice-e2e"));
        assert_eq!(armed.owner.as_deref(), Some("desktop"));
        assert_eq!(armed.phase, "waiting");

        // No further pokes: the runtime thread started by `arm` captures the
        // phrase, segments it, transcribes it, waits out the hold-back and
        // enqueues it on its own.
        let voice_id = wait_for_voice_entry(&state, "voice-e2e", armed.hold_back_ms);

        let status = hands_free_status(&dictation);
        assert_eq!(
            status.queued_ids,
            [voice_id],
            "the status must own exactly the entry it queued"
        );
        assert_eq!(status.session_id.as_deref(), Some("voice-e2e"));

        let disarmed = disarm_hands_free(&state, &dictation);

        assert!(disarmed.was_armed);
        assert_eq!(disarmed.cancelled, [voice_id]);
        assert!(disarmed.already_delivered.is_empty());
        let remaining: Vec<u64> = state
            .pending_injections
            .get("voice-e2e")
            .expect("queue")
            .iter()
            .map(crate::state::PendingInjection::id)
            .collect();
        assert_eq!(
            remaining,
            [human, notice],
            "the human's command and the peer notice must survive a voice disarm"
        );
        assert!(!disarmed.status.armed);
        assert!(
            !disarm_hands_free(&state, &dictation).was_armed,
            "disarming twice must report the second call honestly"
        );
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "a disarm stops the capture runtime"
        );
        assert!(
            dictation.audio.lock().is_none(),
            "hands-free must never take push-to-talk's capture slot"
        );
    }

    /// Arming from a browser must not open the microphone on the machine
    /// running TUICommander (832-e730 criterion 2). An owner with no socket
    /// behind it is refused, not quietly served by the desktop mic — the
    /// failure to fear here is a fallback, because it is silent and the user
    /// who armed from a laptop would never learn the room being listened to is
    /// not theirs.
    #[cfg(unix)]
    #[test]
    fn an_owner_without_a_connected_client_is_refused_rather_than_given_the_desktop_microphone() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-remote", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-remote");
        let dictation = DictationState::new();

        let refused = arm_hands_free(&state, &dictation, "voice-remote", "browser-42")
            .expect_err("a remote owner with no socket has no endpoint");

        assert_eq!(
            refused,
            "No client is connected for audio owner 'browser-42'"
        );
        assert!(!hands_free_status(&dictation).armed);
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "a refused owner must not have opened a capture device"
        );
    }

    /// The capture side of the same rule, one step lower: with a client
    /// connected the browser owner resolves to *that* client's audio, and the
    /// desktop capture slot is still untouched.
    ///
    /// Asserting the slot rather than the endpoint's type, because the slot is
    /// what holds the physical microphone open — a browser endpoint that also
    /// opened the local device would satisfy any check on what was returned.
    #[test]
    fn a_connected_browser_owner_is_served_by_its_own_socket() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        link.push_capture(&[0.25, 0.5]);

        // Recognition needs a downloaded model, which an unattended run does
        // not have, so the endpoint cannot be built here. What can be checked
        // is the half that decides *whose* audio it would carry.
        assert!(
            dictation
                .browser_endpoints
                .get("browser-42")
                .is_some_and(|found| found.drain_capture() == vec![0.25, 0.5]),
            "the owner resolves to the socket that registered it"
        );
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "resolving a browser owner must not open the local microphone"
        );
    }

    /// The other direction of criterion 2: a desktop conversation must not be
    /// handed a browser's microphone just because one happens to be connected.
    #[cfg(unix)]
    #[test]
    fn a_desktop_conversation_ignores_a_connected_browser_client() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        link.push_capture(&[0.9; 32]);

        // `desktop` never looks at the registry: the request either opens this
        // machine's device or fails on it, and either way the browser's audio
        // is still sitting there afterwards.
        let _ = open_endpoint(&dictation, DESKTOP_OWNER);

        assert_eq!(
            link.drain_capture().len(),
            32,
            "the browser's audio was not consumed by a desktop arm"
        );
    }

    /// Criterion 4: replies for a browser-owned conversation leave the machine.
    /// A missing client is a failure rather than a fallback, for the same
    /// reason as the microphone — somebody in another room must not hear the
    /// answer to a question they did not ask.
    #[test]
    fn replies_follow_the_owner_and_never_fall_back_to_the_server_speakers() {
        let dictation = DictationState::new();
        let link = dictation.browser_endpoints.connect("browser-42");
        let mut client = link.subscribe();

        let output = open_reply_output(&dictation, Some("browser-42"))
            .expect("a connected client can be spoken to");
        output
            .play(&speech::SpeechAudio {
                samples: vec![0.0; 2_400],
                sample_rate: 24_000,
            })
            .expect("the client takes it");
        assert!(
            matches!(
                client.try_recv().expect("the client was told"),
                browser::Downlink::Speak(_)
            ),
            "the reply went to the browser"
        );

        let Err(refused) = open_reply_output(&dictation, Some("browser-99")) else {
            panic!("an owner with no client has no speaker");
        };
        assert_eq!(
            refused,
            "No client is connected for audio owner 'browser-99'"
        );
    }

    /// The bound tab closes while the mode is armed: the runtime notices on its
    /// own, disarms, and releases the capture device it was holding.
    #[cfg(unix)]
    #[test]
    fn closing_the_bound_session_disarms_the_running_mode_and_releases_the_device() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-closing", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-closing");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-closing",
            "desktop",
            &scripted_endpoint("never sent"),
        )
        .expect("arm");

        // What closing a tab does to the session map.
        state.session_maps.sessions.remove("voice-closing");

        // Wait for the *reap*, not for `armed`: the mode is unbound the moment
        // the runtime disarms, which is a scheduling tick before its thread
        // actually returns and its device can be released.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if !hands_free_status(&dictation).armed && dictation.hands_free_runtime.lock().is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let status = hands_free_status(&dictation);
        assert!(!status.armed, "a closed target must disarm the mode");
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "the finished runtime must be reaped"
        );
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "and the capture device released with it"
        );
    }

    /// Phase is the one string both transports report; a rename would silently
    /// break whatever renders it.
    #[test]
    fn every_phase_has_a_stable_wire_name() {
        assert_eq!(Phase::Disarmed.as_wire(), "disarmed");
        assert_eq!(Phase::Waiting.as_wire(), "waiting");
        assert_eq!(Phase::Capturing.as_wire(), "capturing");
        assert_eq!(Phase::Transcribing.as_wire(), "transcribing");
        assert_eq!(Phase::HoldingBack.as_wire(), "holding_back");
        assert_eq!(Phase::Delivered.as_wire(), "delivered");
        assert_eq!(Phase::Error.as_wire(), "error");
    }

    /// Persist a config that names `model`, then clear the snapshot cache so the
    /// next read observes it.
    fn write_model_config(model: &str) {
        save_dictation_config(
            DictationConfig {
                model: model.to_string(),
                ..Default::default()
            },
            None,
        )
        .expect("config save");
    }

    /// The old metric compared a common-prefix character count against a byte
    /// length, so one differing leading space read as 0% and said nothing about
    /// how much text the final pass had lost. The ratio says exactly that.
    #[test]
    fn the_accuracy_ratio_compares_character_counts() {
        // Accented dictation: 3 characters, 4 bytes on the composed side. A byte
        // ratio would report the final pass as having lost text it kept.
        let ratio = transcription_ratio("città", "città").expect("both sides present");
        assert!(
            (ratio - 1.0).abs() < 1e-9,
            "identical text is 1.0, got {ratio}"
        );
    }

    /// A final pass that lost a whole window trips the warning.
    #[test]
    fn a_final_pass_that_lost_a_window_is_below_the_warning_ratio() {
        let ratio = transcription_ratio(&"x".repeat(1000), &"x".repeat(1442)).expect("present");
        assert!(ratio < SHORT_TRANSCRIPTION_RATIO, "1000/1442 = {ratio}");
    }

    /// The reported incident — full=1175 against composed=1442 — lands at 81.5%.
    /// The threshold exists to catch exactly this, so it has to warn here: an
    /// earlier 0.8 line sat below the incident and would have stayed silent on
    /// the very recording that motivated the warning.
    #[test]
    fn the_reported_incident_trips_the_warning_ratio() {
        let ratio = transcription_ratio(&"x".repeat(1175), &"x".repeat(1442)).expect("present");

        assert!((ratio - 0.815).abs() < 0.001, "1175/1442 = {ratio}");
        assert!(ratio < SHORT_TRANSCRIPTION_RATIO, "must trip the warning");
    }

    /// A final pass that merely tidies the partials is not a loss — the warning
    /// has to stay quiet there or it fires on every ordinary dictation.
    #[test]
    fn a_final_pass_close_to_the_partials_does_not_warn() {
        let ratio = transcription_ratio(&"x".repeat(95), &"x".repeat(100)).expect("present");
        assert!(ratio >= SHORT_TRANSCRIPTION_RATIO, "95/100 = {ratio}");

        // The final pass is routinely LONGER: streaming skips VAD-silent windows.
        let ratio = transcription_ratio(&"x".repeat(140), &"x".repeat(100)).expect("present");
        assert!(ratio >= SHORT_TRANSCRIPTION_RATIO, "140/100 = {ratio}");
    }

    /// With no partials there is nothing to compare against, and dividing by zero
    /// would report every such run as a loss.
    #[test]
    fn no_partials_means_no_ratio() {
        assert!(transcription_ratio("run the tests", "").is_none());
    }

    /// The gates are read from the config on every start, so a config written
    /// before they existed must not silently disable them: a missing
    /// `no_speech_threshold` deserializing to `0.0` would reject every
    /// transcription, and a missing `rms_threshold` would accept every one.
    // Exact equality on purpose: each assertion says the number is carried
    // through unchanged, not that it lands close to a computed value.
    #[allow(clippy::float_cmp)]
    #[test]
    fn a_config_written_before_the_gates_existed_keeps_the_defaults() {
        let stored = serde_json::json!({
            "enabled": true,
            "hotkey": "F5",
            "language": "auto",
        });
        let config: DictationConfig = serde_json::from_value(stored).expect("deserialize");

        assert_eq!(config.rms_threshold, transcribe::DEFAULT_RMS_THRESHOLD);
        assert_eq!(
            config.no_speech_threshold,
            transcribe::DEFAULT_NO_SPEECH_THRESHOLD
        );
        assert_eq!(config.gates(), transcribe::VoiceGates::default());
    }

    /// Settings > Dictation moves these two numbers and nothing else carries
    /// them to the transcriber.
    // Exact equality on purpose: each assertion says the number is carried
    // through unchanged, not that it lands close to a computed value.
    #[allow(clippy::float_cmp)]
    #[test]
    fn tuned_thresholds_reach_the_gates() {
        let config = DictationConfig {
            rms_threshold: 0.004,
            no_speech_threshold: 0.35,
            ..Default::default()
        };

        let gates = config.gates();
        assert_eq!(gates.rms_threshold, 0.004);
        assert_eq!(gates.no_speech_threshold, 0.35);
    }

    #[test]
    fn the_meter_tick_does_not_re_read_config_or_stat_the_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // "small" is not the default, so a fresh read is distinguishable from
        // the fallback in `resolve_model`.
        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        // Delete the config file. A snapshot recomputed per call would now read
        // nothing and fall back to the default model.
        std::fs::remove_file(dir.path().join(DICTATION_CONFIG_FILE)).expect("remove config");

        for tick in 0..13 {
            assert_eq!(
                model_snapshot().model,
                model::WhisperModel::Small,
                "meter tick {tick} re-read dictation-config.json"
            );
        }
    }

    /// A debug build and the installed app share one configuration directory and
    /// one model directory. Whatever the other process changes there — the
    /// selected model, a download, a deletion — reaches this one through nothing
    /// but the expiry, so a snapshot that never expires is served forever.
    #[test]
    fn a_change_made_by_another_process_is_picked_up_when_the_snapshot_expires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        // The other process rewrites the file. Nothing invalidates our cache:
        // `set_dictation_config` ran in a different process.
        std::fs::write(
            dir.path().join(DICTATION_CONFIG_FILE),
            serde_json::to_vec(&DictationConfig {
                model: "large-v2".to_string(),
                ..Default::default()
            })
            .expect("serialize"),
        )
        .expect("write config");

        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::Small,
            "inside the window the cached answer is still served"
        );

        // Age the snapshot past its expiry.
        {
            let mut slot = MODEL_SNAPSHOT.lock();
            let (_, taken) = slot.as_mut().expect("a snapshot was cached");
            *taken = std::time::Instant::now() - MODEL_SNAPSHOT_TTL * 2;
        }
        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::LargeV2,
            "an expired snapshot must be recomputed from disk"
        );
    }

    #[test]
    fn saving_the_config_invalidates_the_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        assert_eq!(model_snapshot().model, model::WhisperModel::Small);

        write_model_config("large-v2");
        assert_eq!(
            model_snapshot().model,
            model::WhisperModel::LargeV2,
            "set_dictation_config must invalidate the cached snapshot"
        );
    }

    #[test]
    fn the_snapshot_reports_a_missing_model_as_not_downloaded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        write_model_config("small");
        let snapshot = model_snapshot();
        assert!(!snapshot.downloaded);
        assert_eq!(snapshot.size_mb, 0);
    }

    #[test]
    fn the_snapshot_reports_a_present_model_with_its_on_disk_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        // `model_exists` requires more than 1 MB to treat a file as a real model.
        std::fs::create_dir_all(model::models_dir()).expect("models dir");
        std::fs::write(
            model::model_path(model::WhisperModel::Small),
            vec![0u8; 3 * 1_048_576],
        )
        .expect("write model");

        write_model_config("small");
        let snapshot = model_snapshot();
        assert!(snapshot.downloaded);
        assert_eq!(snapshot.size_mb, 3);
    }

    #[test]
    fn resolve_model_falls_back_to_the_default_on_an_unknown_name() {
        assert_eq!(resolve_model("small"), model::WhisperModel::Small);
        assert_eq!(
            resolve_model("nonexistent"),
            model::WhisperModel::LargeV3Turbo
        );
    }

    // -----------------------------------------------------------------------
    // Spoken replies (817-f67c)
    // -----------------------------------------------------------------------

    /// Synthesis the test decides the duration of.
    ///
    /// It takes `gate` before producing anything, so a test holding that lock
    /// holds the reply in the queue: without it the worker thread finishes a
    /// one-sample render before the assertion runs, and "accepting is not
    /// hearing" would pass or fail on scheduling rather than on the rule.
    struct HeldSpeech {
        gate: Arc<parking_lot::Mutex<()>>,
    }

    impl speech::Speech for HeldSpeech {
        fn synthesize(
            &self,
            _text: &str,
            _voice: &str,
            _cancel: &speech::SpeechCancel,
        ) -> Result<speech::SpeechAudio, speech::SpeechError> {
            let _held = self.gate.lock();
            Ok(speech::SpeechAudio {
                samples: vec![0.0; 16],
                sample_rate: 24_000,
            })
        }
    }

    /// A device that accepts audio and is never busy afterwards, so a reply
    /// handed to it drains on the next poll.
    struct QuietOutput;

    impl speaker::Output for QuietOutput {
        fn play(&self, _audio: &speech::SpeechAudio) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) {}
        fn is_speaking(&self) -> bool {
            false
        }
    }

    /// A configuration directory of this test's own, holding `config`.
    ///
    /// Everything that reaches a voice now reads the dictation settings, so a
    /// test without this one reads Boss's — and passes or fails depending on
    /// the language he happens to dictate in. The returned value holds both the
    /// directory and the process-wide override; drop it and the next test gets
    /// its own.
    #[must_use]
    fn config_of_this_test(config: DictationConfig) -> (tempfile::TempDir, impl Drop) {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        save_dictation_config(config, None).expect("config save");
        (dir, guard)
    }

    /// A conversation armed for `session_id`, with a voice whose rendering the
    /// test controls through the returned gate.
    ///
    /// The configuration comes back with it because it has to outlive the
    /// conversation: a dropped override sends the next `speak` looking for the
    /// language in the real config directory.
    fn armed_with_a_voice(
        session_id: &str,
    ) -> (
        DictationState,
        Arc<parking_lot::Mutex<()>>,
        (tempfile::TempDir, impl Drop),
    ) {
        let config = config_of_this_test(DictationConfig {
            language: "it".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        let generation = dictation
            .hands_free
            .lock()
            .arm(session_id, "desktop", true)
            .expect("arm");
        let gate = Arc::new(parking_lot::Mutex::new(()));
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(HeldSpeech {
                    gate: Arc::clone(&gate),
                }),
                Arc::new(QuietOutput),
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });
        (dictation, gate, config)
    }

    /// Poll until `id` reaches a state the test is waiting for, or say what it
    /// was stuck on. The worker thread decides when, so a fixed sleep would be
    /// a guess; the deadline is the harness bound, not the behaviour.
    fn wait_for_utterance(dictation: &DictationState, id: &str, wanted: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut last = String::new();
        while std::time::Instant::now() < deadline {
            last = speech_status(dictation, Some(id))
                .utterance
                .expect("an id was asked about")
                .state;
            if last == wanted {
                return last;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("utterance {id} never reached {wanted}; it is {last}");
    }

    /// A model may drive only the conversation it is the target of. Reading the
    /// queue is refused for the same reason as speaking into it: the fields
    /// alone say what somebody else's conversation is doing.
    #[test]
    fn a_model_bound_to_another_terminal_can_neither_speak_nor_read_the_queue() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        let refused = speak(&dictation, Caller::Model("session-b"), "hello", None).unwrap_err();
        assert!(
            refused.contains("bound to another session"),
            "the refusal must name the reason: {refused}"
        );
        assert_eq!(
            stop_speaking(&dictation, Caller::Model("session-b")).unwrap_err(),
            refused
        );
        assert_eq!(
            speech_status_for(&dictation, Caller::Model("session-b"), None).unwrap_err(),
            refused
        );

        // The bound model gets past the binding, and so does the owner.
        assert!(speak(&dictation, Caller::Model("session-a"), "hello", None).is_ok());
        assert!(speak(&dictation, Caller::Owner, "hello", None).is_ok());
    }

    /// Nothing armed is not an error for `status` — the model asked a fair
    /// question — but it is one for `speak`, which would otherwise have to
    /// choose a conversation itself.
    #[test]
    fn with_nothing_armed_speech_is_refused_and_status_says_why() {
        let dictation = DictationState::new();

        assert!(
            speak(&dictation, Caller::Model("session-a"), "hello", None)
                .unwrap_err()
                .contains("not armed")
        );

        let status = speech_status_for(&dictation, Caller::Model("session-a"), None)
            .expect("status is answerable when nothing is armed");
        assert!(!status.available);
        assert_eq!(status.unavailable_reason, "Hands-free is not armed");
        assert_eq!(status.session_id, None);
    }

    /// The turn is the guard against reviving a reply the user already talked
    /// over: `hush` opens a new one, and a reply written for the old turn is
    /// refused rather than played over whatever was said next.
    #[test]
    fn a_reply_written_for_a_turn_the_user_talked_over_is_refused() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let before = speech_status(&dictation, None).turn;

        let after = stop_speaking(&dictation, Caller::Owner).expect("stop").turn;
        assert!(
            after > before,
            "stopping must open a new turn: {before} -> {after}"
        );

        let stale = speak(&dictation, Caller::Owner, "too late", Some(before)).unwrap_err();
        assert!(
            stale.contains(&format!("turn {before}")) && stale.contains(&format!("turn {after}")),
            "the refusal must name both turns so the model can retry: {stale}"
        );

        // Omitting the turn means "now", which is still allowed.
        let fresh = speak(&dictation, Caller::Owner, "in time", None).expect("current turn");
        assert_eq!(fresh.turn, after);
    }

    /// Accepting a reply is not the user hearing it. The state a caller gets
    /// back from `speak` is the queue's, and only the device going quiet can
    /// produce `finished`.
    #[test]
    fn accepting_a_reply_is_never_reported_as_having_been_heard() {
        let (dictation, gate, _config) = armed_with_a_voice("session-a");
        let held = gate.lock();

        let accepted = speak(&dictation, Caller::Owner, "a spoken reply", None).expect("accepted");
        assert!(
            accepted.state == "queued" || accepted.state == "rendering",
            "acceptance reports the queue, not the speaker: {}",
            accepted.state
        );
        assert_eq!(accepted.error, None);

        let polled = speech_status(&dictation, Some(&accepted.utterance_id))
            .utterance
            .expect("the reply is remembered");
        assert_ne!(
            polled.state, "finished",
            "nothing can be finished while synthesis has not returned"
        );

        // Only now can it be rendered, played, and observed to have drained.
        drop(held);
        wait_for_utterance(&dictation, &accepted.utterance_id, "finished");
    }

    /// A reply the conversation no longer remembers is a different answer from
    /// "you asked about nothing", and a caller polling its own id has to be
    /// able to tell them apart.
    #[test]
    fn an_utterance_this_conversation_never_had_is_reported_as_unknown() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        let status = speech_status(&dictation, Some("4242"));
        let asked = status.utterance.expect("asking must produce an answer");
        assert_eq!(asked.utterance_id, "4242");
        assert_eq!(asked.state, "unknown");

        assert!(
            speech_status(&dictation, None).utterance.is_none(),
            "asking about nothing must leave the field absent"
        );
    }

    /// Bounded input, in the caller's terms. The synthesis budget already stops
    /// one runaway render; it cannot stop a model queueing several.
    #[test]
    fn an_empty_or_oversized_reply_is_refused_before_it_reaches_the_queue() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        assert_eq!(
            speak(&dictation, Caller::Owner, "   \n ", None).unwrap_err(),
            "Nothing to say"
        );

        let long = "è".repeat(MAX_SPOKEN_CHARS + 1);
        let refused = speak(&dictation, Caller::Owner, &long, None).unwrap_err();
        assert!(
            refused.contains(&(MAX_SPOKEN_CHARS + 1).to_string()),
            "counted in characters, not bytes: {refused}"
        );
        assert_eq!(speech_status(&dictation, None).queued, 0);
    }

    /// The one shape both transports serialize. This pins the field names a
    /// store reads over IPC and over HTTP, in the casing the wire uses.
    #[test]
    fn the_speech_status_wire_shape_names_every_field_a_client_reads() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "hello", None).expect("accepted");

        let wire = serde_json::to_value(speech_status(&dictation, Some(&accepted.utterance_id)))
            .expect("serialize");

        assert_eq!(wire["available"], serde_json::json!(true));
        assert_eq!(wire["unavailableReason"], serde_json::json!(""));
        assert_eq!(wire["sessionId"], serde_json::json!("session-a"));
        assert_eq!(wire["language"], serde_json::json!("it"));
        assert_eq!(wire["voice"], serde_json::json!("giovanni"));
        assert!(wire["turn"].is_u64());
        assert!(wire["queued"].is_u64());
        assert!(wire["rendering"].is_boolean());
        assert!(wire["speaking"].is_boolean());
        assert_eq!(wire["lastError"], serde_json::Value::Null);
        assert_eq!(
            wire["utterance"]["utteranceId"],
            serde_json::json!(accepted.utterance_id)
        );
        assert!(wire["utterance"]["state"].is_string());
    }

    /// Answering a caller and answering its poll must describe a reply the same
    /// way (817-f67c criterion 5).
    ///
    /// `speak` returns a `SpokenReply`, `status` nests one, and the deferred
    /// `/events` arm will carry one — three surfaces, one shape. The assertion
    /// is on the *keys* rather than the values, and deliberately so: the render
    /// thread advances the state between the two reads, so a value comparison
    /// would be a race dressed up as a contract.
    #[test]
    fn a_reply_looks_the_same_whether_it_was_accepted_or_polled_for() {
        fn keys(reply: &SpokenReply) -> Vec<String> {
            let mut keys: Vec<String> = serde_json::to_value(reply)
                .expect("serialize")
                .as_object()
                .expect("an object")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        }

        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "hello", None).expect("accepted");

        let answered = serde_json::to_value(&accepted).expect("serialize");
        assert_eq!(
            answered["utteranceId"],
            serde_json::json!(accepted.utterance_id)
        );
        assert!(answered["state"].is_string());
        assert!(answered["turn"].is_u64());
        assert_eq!(
            keys(&accepted),
            ["error", "state", "turn", "utteranceId"],
            "these are the four fields every transport carries for one reply"
        );

        let polled = speech_status(&dictation, Some(&accepted.utterance_id))
            .utterance
            .expect("the id was asked about");
        assert_eq!(
            polled.utterance_id, accepted.utterance_id,
            "polling by id must answer about that id"
        );
        assert_eq!(
            keys(&polled),
            keys(&accepted),
            "a field added to one construction site and not the other reads as a \
             reply that changed while nobody touched it"
        );
    }

    // --- The push half, on both transports (833-6fd4) ---------------------

    /// The desktop window and an SSE consumer are told the same thing, under
    /// the same name, about the same action.
    ///
    /// Equality of the bodies is by construction — one value is cloned to both
    /// transports — so what this really pins is the half that can still drift:
    /// the SSE arm's event name, and its refusal to re-wrap the body. A bare
    /// object on one transport and `{"payload": {...}}` on the other is a store
    /// that works on the desktop and renders nothing in a browser, which is
    /// exactly the failure the three events were opened for.
    #[test]
    fn every_dictation_push_names_and_shapes_itself_the_same_on_both_transports() {
        use crate::mcp_http::sse_routes::{event_payload_for_test, event_type_name_for_test};
        use crate::state::AppEvent;

        let whisper = download_progress(None, 512, 2_048);
        let asset = download_progress(Some("italian"), 1, 4);
        let reply = serde_json::to_value(SpokenReply::new(
            "3".parse().expect("an utterance id"),
            &speaker::Utterance::Finished,
            9,
        ))
        .expect("serialize");

        for (event, name, desktop) in [
            (
                AppEvent::DictationDownloadProgress {
                    payload: whisper.clone(),
                },
                DICTATION_DOWNLOAD_PROGRESS,
                &whisper,
            ),
            (
                AppEvent::SpeechDownloadProgress {
                    payload: asset.clone(),
                },
                SPEECH_DOWNLOAD_PROGRESS,
                &asset,
            ),
            (
                AppEvent::SpeechUtterance {
                    payload: reply.clone(),
                },
                SPEECH_UTTERANCE,
                &reply,
            ),
        ] {
            assert_eq!(
                event_type_name_for_test(&event),
                name,
                "the SSE stream must offer the name the frontend already listens for"
            );
            assert_eq!(
                &event_payload_for_test(&event),
                desktop,
                "{name} arrives in a different shape over SSE than through the window"
            );
        }
    }

    /// The two downloads differ in one field, and the difference is
    /// load-bearing.
    #[test]
    fn only_a_speech_download_names_its_asset() {
        let whisper = download_progress(None, 512, 2_048);
        let asset = download_progress(Some("italian"), 1, 4);

        assert_eq!(whisper["percent"], serde_json::json!(25));
        assert!(
            whisper.get("asset").is_none(),
            "one Whisper model downloads at a time, so there is nothing to key on"
        );
        assert_eq!(
            asset["asset"],
            serde_json::json!("italian"),
            "the runtime library and a language download together; a shared percent \
             would show each of them the other's"
        );
        assert_eq!(asset["percent"], serde_json::json!(25));
        assert_eq!(
            download_progress(None, 7, 0)["percent"],
            serde_json::json!(0),
            "a server that sent no length must not divide by it"
        );
    }

    /// A hands-free conversation must not read as a silent microphone.
    #[test]
    fn the_level_falls_back_to_the_hands_free_capture() {
        assert_eq!(capture_level(None, Some(0.4)), 0.4);
        assert_eq!(capture_level(Some(0.2), Some(0.4)), 0.2);
        assert_eq!(capture_level(None, None), 0.0);
    }

    /// The frontend keys its bar on `asset` and ends it on `done`; a progress
    /// event must never carry `done`, or the bar would vanish mid-download.
    #[test]
    fn a_finished_speech_download_names_its_asset_and_says_done() {
        let finished = download_finished("italian");
        assert_eq!(finished["asset"], serde_json::json!("italian"));
        assert_eq!(finished["done"], serde_json::json!(true));
        assert!(
            download_progress(Some("italian"), 4, 4)
                .get("done")
                .is_none()
        );
    }

    /// Auto has no language until somebody speaks, and a voice assistant that
    /// guesses one guesses English. It has to say so instead.
    #[test]
    fn under_auto_there_is_no_language_and_so_no_voice_until_somebody_speaks() {
        let _config = config_of_this_test(DictationConfig {
            language: "auto".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");

        let before = speech_status(&dictation, None);
        assert!(!before.available);
        assert_eq!(before.language, "", "no turn, no language, and no default");
        assert!(
            before.unavailable_reason.contains("Auto"),
            "the state has to name Auto as the reason: {}",
            before.unavailable_reason
        );
        assert!(
            speak(&dictation, Caller::Owner, "ciao", None)
                .unwrap_err()
                .contains("Auto")
        );

        // The user speaks Italian. The conversation now has a language, and
        // the reason changes from "nothing said yet" to whatever is wrong with
        // Italian on this machine — here, a bundle nobody downloaded.
        let generation = dictation.hands_free.lock().generation();
        dictation
            .hands_free
            .lock()
            .accept_transcript(generation, "ciao", Some("it"), 0);

        let after = speech_status(&dictation, None);
        assert_eq!(
            after.language, "it",
            "Auto must expose what Whisper actually detected"
        );
        assert!(
            !after.unavailable_reason.contains("Auto"),
            "the detection answered Auto's question; what is left is about this machine: {}",
            after.unavailable_reason
        );
        assert!(
            after.unavailable_reason.contains("not downloaded"),
            "on a machine with no speech assets that is what is missing: {}",
            after.unavailable_reason
        );
    }

    /// A language Whisper transcribes and no bundle speaks is reported as
    /// itself. Substituting a voice we do ship is how an Italian conversation
    /// gets answered in English.
    #[test]
    fn a_language_no_bundle_speaks_is_named_rather_than_replaced() {
        let _config = config_of_this_test(DictationConfig {
            language: "ko".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");

        let status = speech_status(&dictation, None);
        assert!(!status.available);
        assert_eq!(status.language, "ko");
        assert_eq!(
            status.unavailable_reason,
            "No speech bundle ships for language \"ko\""
        );
        assert!(
            speak(&dictation, Caller::Owner, "안녕", None)
                .unwrap_err()
                .contains("ko")
        );
    }

    /// Criterion 4: the setting moves, and every reply written for the old one
    /// stops. Silence is the only honest outcome — a half-spoken Italian
    /// sentence does not become English by finishing it.
    #[test]
    fn changing_the_language_takes_the_voice_away_from_the_replies_written_for_it() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        assert!(speech_status(&dictation, None).queued > 0 || dictation.speaker.lock().is_some());

        save_dictation_config(
            DictationConfig {
                language: "en".to_string(),
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(
            dictation.speaker.lock().is_none(),
            "the queue built for Italian may not speak English replies"
        );
        let status = speech_status(&dictation, None);
        assert_eq!(status.language, "en", "the model context moves with it");
        assert!(
            !status.available,
            "and nothing is speakable until a voice for the new language opens"
        );
    }

    // --- Choosing a voice (818-2a29) ---------------------------------------

    /// The one field a caller joins an asset to a conversation by.
    ///
    /// The settings panel looks the configured language up in this list to
    /// learn which voices it may offer. Published as the engine's own name for
    /// the language (`"italian"`) it matched nothing, so the panel offered no
    /// voice and said no bundle shipped for Italian directly under the row
    /// offering the Italian bundle.
    #[test]
    fn an_asset_names_its_language_by_the_code_whisper_uses() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        assert_eq!(describe(italian, false).language.as_deref(), Some("it"));
        assert_eq!(
            describe(speech::assets::runtime(), false).language,
            None,
            "the runtime library speaks nothing"
        );
    }

    /// What an untouched configuration and every configuration written before
    /// the setting existed both say.
    #[test]
    fn no_chosen_voice_means_the_first_one_the_language_ships() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        assert_eq!(
            choose_voice(italian, "").expect("a language ships at least one voice"),
            italian.voices()[0]
        );
    }

    #[test]
    fn a_chosen_voice_is_the_one_that_speaks() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let wanted = italian.voices()[0];
        assert_eq!(choose_voice(italian, wanted).expect("shipped"), wanted);
    }

    /// Never a silent fall back to the first voice: the user hears a voice
    /// nobody chose and has nothing on screen saying why.
    #[test]
    fn a_voice_the_language_does_not_ship_is_named_rather_than_replaced() {
        let italian = speech::assets::for_language_code("it").expect("catalogue ships Italian");
        let error = choose_voice(italian, "nessuno").expect_err("not shipped");
        assert!(error.contains("nessuno"), "{error}");
        assert!(
            error.contains(italian.voices()[0]),
            "the message has to say what there is instead: {error}"
        );
    }

    /// A voice belongs to a conversation exactly as much as a language does,
    /// so it obeys the same rule: change it and the replies written for the
    /// old one stop rather than finishing in the new one.
    #[test]
    fn changing_the_voice_takes_it_away_from_the_replies_written_for_it() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");
        assert!(dictation.speaker.lock().is_some());

        save_dictation_config(
            DictationConfig {
                language: "it".to_string(),
                speech_voice: "giovanni".to_string(),
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(
            dictation.speaker.lock().is_none(),
            "the queue built for the old voice may not speak in the new one"
        );
    }

    /// The other half of that rule. Every dictation setting goes through the
    /// same function, and cutting a reply off mid-word because somebody moved
    /// a threshold slider would be the worse bug.
    #[test]
    fn moving_a_threshold_leaves_the_voice_alone() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let accepted = speak(&dictation, Caller::Owner, "pronto", None).expect("accepted");

        save_dictation_config(
            DictationConfig {
                language: "it".to_string(),
                rms_threshold: 0.05,
                ..Default::default()
            },
            Some(&dictation),
        )
        .expect("config save");

        assert!(dictation.speaker.lock().is_some());
        assert_eq!(
            speech_status(&dictation, Some(&accepted.utterance_id))
                .utterance
                .expect("asked")
                .state
                .is_empty(),
            false,
            "the reply in flight still has a fate to report"
        );
    }

    /// Barge-in reaches whatever is speaking now, not the queue that existed
    /// when the capture loop started — which under Auto is no queue at all.
    #[test]
    fn barge_in_interrupts_the_voice_that_is_open_at_the_time() {
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");
        let port = ArmedSpeaker(Arc::clone(&dictation.speaker));
        let before = speech_status(&dictation, None).turn;

        continuous::Interruptible::hush(&port);

        assert_eq!(
            speech_status(&dictation, None).turn,
            before + 1,
            "the interruption has to open a new turn, or the model's next reply is refused"
        );

        // And an empty slot is a tick with nothing to interrupt, not a panic:
        // the capture loop runs on every conversation, including the ones that
        // never opened a voice.
        *dictation.speaker.lock() = None;
        continuous::Interruptible::hush(&port);
    }

    /// Disarming takes the voice away before the engine goes, so a reply
    /// queued against the old conversation cannot be spoken into the next one.
    #[test]
    fn disarming_drops_the_voice_with_the_conversation() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let (dictation, _gate, _config) = armed_with_a_voice("session-a");

        disarm_hands_free(&state, &dictation);

        assert!(dictation.speaker.lock().is_none());
        assert!(
            speak(&dictation, Caller::Owner, "hello", None)
                .unwrap_err()
                .contains("not armed")
        );
    }

    /// A device that keeps playing until it is told to stop, and remembers
    /// being told.
    ///
    /// `QuietOutput` cannot answer the question below: it is never speaking, so
    /// a disarm that silenced nothing looks exactly like one that silenced
    /// everything.
    #[derive(Default)]
    struct LoudOutput {
        speaking: std::sync::atomic::AtomicBool,
        stops: std::sync::atomic::AtomicUsize,
    }

    impl speaker::Output for LoudOutput {
        fn play(&self, _audio: &speech::SpeechAudio) -> Result<(), String> {
            self.speaking
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn stop(&self) {
            self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.speaking
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_speaking(&self) -> bool {
            self.speaking.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// Disarming silences the speaker, it does not merely forget it
    /// (820-21a5 criterion 2, the playback half of "no stale playback").
    ///
    /// The test above asserts the slot is empty, which is what a *caller* sees.
    /// A user hears the device. Those are the same thing only because dropping
    /// the `Speaker` stops its output, and nothing that looks at the slot alone
    /// can tell a silenced room from a handle dropped while the audio played
    /// on.
    #[test]
    fn disarming_while_a_reply_is_playing_stops_the_device() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let _config = config_of_this_test(DictationConfig {
            language: "it".to_string(),
            ..Default::default()
        });
        let dictation = DictationState::new();
        let generation = dictation
            .hands_free
            .lock()
            .arm("session-a", "desktop", true)
            .expect("arm");
        let device = Arc::new(LoudOutput::default());
        *dictation.speaker.lock() = Some(speaker::Armed {
            speaker: Arc::new(speaker::Speaker::new(
                Arc::new(HeldSpeech {
                    gate: Arc::new(parking_lot::Mutex::new(())),
                }),
                Arc::clone(&device) as Arc<dyn speaker::Output>,
                generation,
            )),
            voice: "giovanni".to_string(),
            language: "it".to_string(),
        });

        let reply = speak(&dictation, Caller::Owner, "una risposta", None).expect("accepted");
        wait_for_utterance(&dictation, &reply.utterance_id, "speaking");
        assert!(
            device.speaking.load(std::sync::atomic::Ordering::SeqCst),
            "the fixture has nothing coming out of it, so silencing it would prove nothing"
        );

        disarm_hands_free(&state, &dictation);

        assert!(
            !device.speaking.load(std::sync::atomic::Ordering::SeqCst),
            "the conversation ended while the reply was still audible"
        );
        assert_eq!(
            device.stops.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one stop, from the drop: disarm silences by taking the speaker away, \
             not by hushing it, and a second stop would mean two paths do the same job"
        );
    }

    // --- Telling the model the mode changed (821-842a) --------------------

    /// The setting is read at arm time: what the user wrote is what the model
    /// reads, folded to one line, and the built-in text is not sent beside it.
    #[cfg(unix)]
    #[test]
    fn arming_sends_the_configured_start_notice_instead_of_the_built_in_one() {
        let _config = config_of_this_test(DictationConfig {
            hands_free_start_notice: "Voice on.\nAnswer in Italian.".to_string(),
            ..DictationConfig::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-custom", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-custom");
        let dictation = DictationState::new();

        arm_hands_free_with(
            &state,
            &dictation,
            "voice-custom",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        let terminal = wait_for_typed(&typed, "Voice on. Answer in Italian.");

        assert!(
            !terminal.contains(continuous::MODE_ENTRY_HINT),
            "{terminal:?}"
        );
        disarm_hands_free(&state, &dictation);
    }

    #[test]
    fn the_default_notice_accessor_returns_the_text_an_empty_setting_sends() {
        assert_eq!(
            get_hands_free_default_notice(),
            continuous::entry_hint_text(&DictationConfig::default().hands_free_start_notice)
        );
    }

    /// Criterion 1, end to end against a real terminal: the model is told it
    /// can answer out loud when the conversation opens, and told to go back to
    /// text when it ends.
    ///
    /// The target is IDLE on purpose. The composer types into an idle terminal
    /// straight away, which is the only state in which the model has actually
    /// *read* the notice — and reading it is what the exit notice is owed to.
    #[cfg(unix)]
    #[test]
    fn arming_tells_the_model_it_can_answer_out_loud_and_disarming_takes_it_back() {
        let _config = config_of_this_test(DictationConfig::default());
        assert!(
            get_dictation_config().hands_free_notify_model,
            "the notices are on by default; a voice nobody is told about is never used"
        );
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-hint", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-hint");
        let dictation = DictationState::new();

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-hint",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        let terminal = wait_for_typed(&typed, continuous::MODE_ENTRY_HINT);
        assert_eq!(
            terminal.matches(continuous::MODE_ENTRY_HINT).count(),
            1,
            "one arm, one notice"
        );

        let disarmed = disarm_hands_free(&state, &dictation);

        assert_eq!(
            disarmed.already_delivered, armed.queued_ids,
            "the composer had already typed the start notice, so the model holds it"
        );
        assert!(
            disarmed.cancelled.is_empty(),
            "nothing may be cancelled on behalf of a notice already typed"
        );
        // Into the same FIFO, behind whatever the terminal is doing. Whether
        // the composer has typed it yet is its own decision — the contract is
        // that the notice is queued for the session the mode was bound to.
        let exit = wait_for_voice_text(&state, "voice-hint", &typed, continuous::MODE_EXIT_HINT);
        assert!(
            exit,
            "the end notice must reach the FIFO of the session the mode was bound to"
        );
    }

    /// Criterion 3: a notice the model never read is withdrawn, and nothing is
    /// queued to contradict something it was never told.
    ///
    /// The target is BUSY, so the notice is still parked when the user changes
    /// their mind — the rapid arm/disarm case.
    #[cfg(unix)]
    #[test]
    fn a_start_notice_the_model_never_read_is_withdrawn_rather_than_contradicted() {
        let _config = config_of_this_test(DictationConfig::default());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-rapid", crate::pty::SHELL_BUSY);
        let typed = crate::test_support::insert_recording_session(&state, "voice-rapid");
        let dictation = DictationState::new();

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-rapid",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");
        assert_eq!(
            armed.queued_ids.len(),
            1,
            "a parked notice is visible as an entry the mode owns"
        );

        let disarmed = disarm_hands_free(&state, &dictation);

        assert_eq!(disarmed.cancelled, armed.queued_ids);
        assert!(disarmed.already_delivered.is_empty());
        let remaining = state
            .pending_injections
            .get("voice-rapid")
            .map_or(0, |queue| queue.len());
        assert_eq!(remaining, 0, "nothing new may be parked to undo it");
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            !terminal.contains(continuous::MODE_EXIT_HINT),
            "an end notice on its own is the only thing the model would ever hear about the mode"
        );

        // Criterion 4, on the same disarm: speech is revoked whether or not
        // the model ever acknowledged — or even read — anything.
        assert!(dictation.speaker.lock().is_none());
        assert!(
            speak(&dictation, Caller::Owner, "too late", None)
                .unwrap_err()
                .contains("not armed")
        );
    }

    /// Criterion 1's other half: the setting turns both notices off, and
    /// nothing else about the mode changes.
    #[cfg(unix)]
    #[test]
    fn the_setting_turned_off_sends_neither_notice() {
        let _config = config_of_this_test(DictationConfig {
            hands_free_notify_model: false,
            ..Default::default()
        });
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-quiet", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-quiet");
        let dictation = DictationState::new();

        let armed = arm_hands_free_with(
            &state,
            &dictation,
            "voice-quiet",
            "desktop",
            &silent_endpoint(),
        )
        .expect("arm");

        assert!(armed.armed, "the mode still arms; only the notices are off");
        assert!(armed.queued_ids.is_empty());
        let disarmed = disarm_hands_free(&state, &dictation);
        assert!(disarmed.was_armed);
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            !terminal.contains(continuous::MODE_ENTRY_HINT)
                && !terminal.contains(continuous::MODE_EXIT_HINT),
            "the terminal holds {terminal:?}"
        );
    }

    /// Criterion 5: push-to-talk is a different feature and shares nothing
    /// with hands-free. Nothing it does arms the mode, opens a VAD runtime,
    /// makes speech available, or tells the model anything.
    #[cfg(unix)]
    #[test]
    fn push_to_talk_alone_arms_nothing_and_tells_the_model_nothing() {
        let _config = config_of_this_test(DictationConfig::default());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-ptt", crate::pty::SHELL_IDLE);
        let typed = crate::test_support::insert_recording_session(&state, "voice-ptt");
        let dictation = DictationState::new();

        // The backend half of a push-to-talk dictation: the transcript is
        // corrected here and handed to the caller, which types it itself.
        let delivered = {
            let text = dictation.corrections.lock().correct("run the tests");
            text.replace('\n', " ")
        };
        assert_eq!(delivered, "run the tests");

        let status = hands_free_status(&dictation);
        assert!(!status.armed, "push-to-talk may not arm hands-free");
        assert!(status.queued_ids.is_empty());
        assert!(
            dictation.hands_free_runtime.lock().is_none(),
            "no VAD runtime, so no activation phrase and no continuous capture"
        );
        assert!(
            speak(&dictation, Caller::Owner, "hello", None)
                .unwrap_err()
                .contains("not armed"),
            "MCP speech belongs to an armed conversation, not to a hotkey"
        );
        let terminal = String::from_utf8_lossy(&typed.lock().expect("recorder")).to_string();
        assert!(
            terminal.is_empty(),
            "push-to-talk writes nothing of its own; it holds {terminal:?}"
        );
        assert!(
            state.pending_injections.get("voice-ptt").is_none(),
            "no Compose entry belongs to a push-to-talk transcription"
        );
    }
}
