//! The Edge voice list, fetched once and kept.
//!
//! The list is the service's own and changes rarely; fetching it on every
//! settings render would make opening the panel a network round trip. A failed
//! fetch is never cached, so going online fixes it on the next open.

use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::speech::edge::{self, EdgeVoice};

const TTL: Duration = Duration::from_secs(6 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

static CACHE: Mutex<Option<(Instant, Vec<EdgeVoice>)>> = Mutex::new(None);

/// Every voice the service offers, or why it could not be asked.
async fn all_voices() -> Result<Vec<EdgeVoice>, String> {
    if let Some((fetched, voices)) = CACHE.lock().as_ref()
        && fetched.elapsed() < TTL
    {
        return Ok(voices.clone());
    }
    let body = reqwest::Client::new()
        .get(edge::VOICE_LIST_URL)
        .header("User-Agent", edge::VOICE_LIST_USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| {
            format!("Cannot load the Microsoft Edge voice list ({error}); it needs an internet connection")
        })?
        .text()
        .await
        .map_err(|error| format!("Cannot read the Microsoft Edge voice list: {error}"))?;
    let voices = edge::parse_voices(&body)?;
    *CACHE.lock() = Some((Instant::now(), voices.clone()));
    Ok(voices)
}

/// The voices that speak a dictation language, as the service lists them.
pub async fn voices_for_language(language: &str) -> Result<Vec<EdgeVoice>, String> {
    Ok(edge::voices_for_language(&all_voices().await?, language))
}
