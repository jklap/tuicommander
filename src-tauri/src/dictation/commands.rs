use super::{DictationState, audio, corrections, model, permission, streaming, transcribe};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
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

    // Check microphone permission before attempting audio capture
    let mic_status = permission::check();
    match mic_status {
        permission::MicPermission::Denied => {
            return Err("microphone_denied".to_string());
        }
        permission::MicPermission::Restricted => {
            return Err("microphone_restricted".to_string());
        }
        permission::MicPermission::NotDetermined => {
            // CoreAudio (cpal) does NOT trigger the TCC prompt — we must
            // explicitly request access via AVCaptureDevice to show the dialog.
            if !permission::request() {
                return Err("microphone_denied".to_string());
            }
        }
        permission::MicPermission::Authorized => {}
    }

    // One read of dictation-config.json for the whole start: the model, the
    // input device and the language all come from this snapshot.
    let config = get_dictation_config();
    let whisper_model = resolve_model(&config.model);

    // Reload transcriber if model changed or not loaded
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
        app_logger::log_via_handle(
            &app,
            "info",
            "dictation",
            &format!("Loading model: {}", whisper_model.display_name()),
        );
        let t = transcribe::WhisperTranscriber::load(&model::model_path(whisper_model))?;
        *transcriber_arc_lock = Some(Arc::new(t));
        *active_model_lock = Some(whisper_model.name().to_string());
        app_logger::log_via_handle(
            &app,
            "info",
            "dictation",
            &format!("Model loaded (backend: {})", transcribe::backend_label()),
        );
    }

    // Always emit backend info so the frontend gets it even when model is reused
    let _ = app.emit(
        "dictation-backend-info",
        serde_json::json!({
            "backend": transcribe::backend_label(),
        }),
    );

    let transcriber_arc = transcriber_arc_lock
        .clone()
        .ok_or("Transcriber not available")?;
    drop(active_model_lock);
    drop(transcriber_arc_lock);

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
            match transcriber.transcribe(&all_audio, lang_ref) {
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
        let match_pct = if !composed.is_empty() && !final_text.is_empty() {
            let common = final_text
                .chars()
                .zip(composed.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let max_len = final_text.len().max(composed.len());
            (common as f64 / max_len as f64 * 100.0).round() as u32
        } else {
            0
        };
        app_logger::log_via_handle(
            &app_clone,
            "info",
            "dictation",
            &format!(
                "[accuracy] full={} chars, composed={} chars, match={}%, audio={:.1}s",
                final_text.len(),
                composed.len(),
                match_pct,
                total_duration_s
            ),
        );

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
}

fn default_model() -> String {
    "large-v3-turbo".to_string()
}

fn default_long_press_ms() -> u32 {
    400
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

    /// Persist a config that names `model`, then clear the snapshot cache so the
    /// next read observes it.
    fn write_model_config(model: &str) {
        set_dictation_config(DictationConfig {
            model: model.to_string(),
            ..Default::default()
        })
        .expect("config save");
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
