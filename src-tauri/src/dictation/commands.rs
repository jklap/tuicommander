use super::{
    DictationState, audio, continuous, corrections, model, permission, streaming, transcribe,
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

    Ok(DictationStatus {
        model_status: model_status.to_string(),
        model_name: snapshot.model.name().to_string(),
        model_size_mb: snapshot.size_mb,
        recording: dictation.recording.load(Ordering::Acquire),
        processing: dictation.processing.load(Ordering::Acquire),
        audio_level: dictation
            .audio
            .lock()
            .as_ref()
            .map_or(0.0, audio::AudioCapture::level),
    })
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
        let _ = app_clone.emit(
            "dictation-download-progress",
            serde_json::json!({
                "downloaded": downloaded,
                "total": total,
                "percent": if total > 0 { (downloaded as f64 / total as f64 * 100.0) as u32 } else { 0 },
            }),
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

/// The one audio endpoint this story implements.
///
/// Any other owner is a browser or remote client, whose endpoint adapter is
/// Step 8 (story 818). It is refused here rather than served by the desktop
/// microphone: arming from a laptop must not open the microphone on the machine
/// running TUICommander.
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

    fn transcribe(&self, audio: &[f32]) -> Result<String, String> {
        let result = self
            .transcriber
            .transcribe(audio, self.language.as_deref(), self.gates)?;
        // A gated segment is not an error and not a message: whisper decided
        // this was not speech, so the utterance is dropped the same way an
        // empty transcript is.
        Ok(if result.skip_reason.is_some() {
            String::new()
        } else {
            result.text
        })
    }
}

/// Open the desktop capture endpoint for an armed session.
fn open_desktop_endpoint(
    dictation: &DictationState,
    owner: &str,
) -> Result<Box<dyn continuous::VoiceEndpoint>, String> {
    if owner != DESKTOP_OWNER {
        return Err(format!(
            "Audio endpoint '{owner}' is not available on this build"
        ));
    }
    ensure_microphone_access()?;
    let config = get_dictation_config();
    let transcriber = ensure_transcriber(None, dictation, resolve_model(&config.model))?;
    let device_name = config.device.as_deref().filter(|name| !name.is_empty());
    let capture = audio::AudioCapture::start_with_device(device_name)?;
    let gates = config.gates();
    let buffer = capture.buffer_handle();
    *dictation.hands_free_audio.lock() = Some(capture);
    dictation
        .hands_free_owner_alive
        .store(true, Ordering::Release);
    Ok(Box::new(DesktopVoiceEndpoint {
        buffer,
        alive: dictation.hands_free_owner_alive.clone(),
        transcriber,
        language: (config.language != "auto").then_some(config.language),
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
    arm_hands_free_with(state, dictation, session_id, owner, &open_desktop_endpoint)
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
    /// A speech engine the user supplies, as argv rather than a shell line.
    /// Empty means the bundled engine. See
    /// [`speech::external`](crate::dictation::speech::external) for the
    /// markers and for what it means that this runs as the user.
    #[serde(default)]
    pub speech_command: Vec<String>,
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
            speech_command: Vec::new(),
        }
    }
}

const DICTATION_CONFIG_FILE: &str = "dictation-config.json";

#[tauri::command]
pub fn get_dictation_config() -> DictationConfig {
    crate::config::load_json_config(DICTATION_CONFIG_FILE)
}

#[tauri::command]
pub fn set_dictation_config(config: DictationConfig) -> Result<(), String> {
    // DEFERRED (2026-09-21) — switching the input device while hands-free is
    // armed should release the endpoint the mode bound to (that is an owner
    // disconnect, see `release_desktop_endpoint`). It needs `DictationState`
    // here, and this function's signature is shared with the HTTP route in
    // `mcp_http/dictation_routes.rs`, which story 814-6d13 pass 4 may not edit.
    // Until then the mode keeps capturing from the device it armed with.
    crate::config::ConfigFile::<DictationConfig>::new(DICTATION_CONFIG_FILE).save(&config)?;
    // The configured model is part of the cached status snapshot.
    invalidate_model_snapshot();
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

        fn transcribe(&self, _audio: &[f32]) -> Result<String, String> {
            Ok(self.transcript.clone())
        }
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
            let present = state.pending_injections.get(session_id).is_some_and(|queue| {
                queue
                    .iter()
                    .any(|entry| entry.voice_generation().is_some())
            });
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
    }

    /// The hold-back is a setting, not a constant, and arming is what reads it.
    #[test]
    fn arming_takes_the_hold_back_from_the_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        set_dictation_config(DictationConfig {
            hands_free_hold_back_ms: 4_000,
            ..Default::default()
        })
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
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        set_dictation_config(DictationConfig {
            hands_free_activation_phrase: "ciao tuic".to_string(),
            hands_free_hold_back_ms: 100,
            ..Default::default()
        })
        .expect("config save");
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
            text, "run the tests",
            "the phrase addresses the tool and may not reach the model"
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
    /// running TUICommander. The remote endpoint adapter is story 818; until it
    /// exists that owner is unavailable, not quietly served by the desktop mic.
    #[cfg(unix)]
    #[test]
    fn an_owner_without_an_endpoint_is_refused_rather_than_given_the_desktop_microphone() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        crate::test_support::agent_session(&state, "voice-remote", crate::pty::SHELL_BUSY);
        crate::test_support::insert_recording_session(&state, "voice-remote");
        let dictation = DictationState::new();

        let refused = arm_hands_free(&state, &dictation, "voice-remote", "browser-42")
            .expect_err("a remote owner has no endpoint on this build");

        assert_eq!(
            refused,
            "Audio endpoint 'browser-42' is not available on this build"
        );
        assert!(!hands_free_status(&dictation).armed);
        assert!(
            dictation.hands_free_audio.lock().is_none(),
            "a refused owner must not have opened a capture device"
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
        set_dictation_config(DictationConfig {
            model: model.to_string(),
            ..Default::default()
        })
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
}
