//! HTTP mirrors for the `/system/*` desktop-integration commands.
//!
//! Each handler answers exactly what its Tauri twin resolves to, so the same
//! frontend code works over IPC and over HTTP.
//!
//! Desktop-only, like the three commands it mirrors: the updater, the audio
//! output and the relay client all live behind the `desktop` feature.

use crate::AppState;
use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::json_result;

/// `GET /system/relay-status` — mirror of the `get_relay_status` command.
pub(super) async fn relay_status_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(crate::relay_status_json(&state))
}

/// Body of `POST /system/notification-sound`.
///
/// `device` is part of the shape on purpose: the desktop command takes it and
/// `src/notifications.ts` always sends it, so dropping it here would silently
/// play every browser-mode notification on the default output.
#[derive(serde::Deserialize)]
pub(super) struct NotificationSoundRequest {
    pub sound: crate::notification_sound::NotificationSound,
    pub volume: f32,
    #[serde(default)]
    pub device: Option<String>,
    /// Same field as the desktop command's `choice` (default tone, a borrowed
    /// preset, or a custom file). Optional so an older client that never sends
    /// it keeps playing the default tone.
    #[serde(default)]
    pub choice: Option<crate::notification_sound::SoundChoice>,
}

/// The sound source an HTTP caller may select. A borrowed preset is honoured;
/// a `custom` file path is not — it falls back to the default tone. The custom
/// path is a native filesystem path, and the desktop feature's security review
/// relied on no remote client being able to trigger playback of an arbitrary
/// path (the desktop UI only ever sets one through the native file picker).
fn http_sound_choice(
    choice: Option<crate::notification_sound::SoundChoice>,
) -> crate::notification_sound::SoundChoice {
    choice.filter(|c| c.preset != "custom").unwrap_or_default()
}

/// `POST /system/notification-sound` — mirror of `play_notification_sound`.
/// The command returns nothing, so this answers the `null` IPC resolves to.
pub(super) async fn play_notification_sound_http(
    Json(body): Json<NotificationSoundRequest>,
) -> impl IntoResponse {
    crate::notification_sound::play_notification_sound(
        body.sound,
        body.volume,
        body.device,
        http_sound_choice(body.choice),
    );
    Json(serde_json::Value::Null)
}

/// Query of `GET /system/check-update`.
#[derive(serde::Deserialize)]
pub(super) struct CheckUpdateQuery {
    pub channel: String,
}

/// `GET /system/check-update` — mirror of `check_update_channel`.
pub(super) async fn check_update_channel_http(Query(q): Query<CheckUpdateQuery>) -> Response {
    json_result(crate::updater::check_update_channel(q.channel).await)
}

#[cfg(test)]
mod notification_sound_choice_tests {
    use super::http_sound_choice;
    use crate::notification_sound::SoundChoice;

    #[test]
    fn http_keeps_a_borrowed_preset_but_never_plays_a_custom_file() {
        let borrowed = http_sound_choice(Some(SoundChoice {
            preset: "attention".into(),
            custom_path: None,
        }));
        assert_eq!(borrowed.preset, "attention");

        let custom = http_sound_choice(Some(SoundChoice {
            preset: "custom".into(),
            custom_path: Some("/etc/passwd".into()),
        }));
        assert_eq!(custom.preset, "default");
        assert!(custom.custom_path.is_none());

        assert_eq!(http_sound_choice(None).preset, "default");
    }
}
