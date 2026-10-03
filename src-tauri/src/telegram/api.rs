use super::{Error, Paths};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use zeroize::Zeroizing;

/// Bound response allocation before any update reaches the mail port.
pub(super) const RESPONSE_LIMIT: usize = 1024 * 1024;

/// One update has no documented byte bound. This finite recovery budget allows
/// unusually large updates without turning batch recovery into unbounded allocation;
/// an update above 8 MiB requires explicit operator recovery, never a silent skip.
pub(super) const SINGLE_UPDATE_LIMIT: usize = 8 * 1024 * 1024;
/// Telegram permits at most 100 updates per request.
pub(super) const BATCH_LIMIT: u8 = 100;

/// Shared production configuration seam; tests seed a proxy before applying it.
pub(super) fn secure_client_builder(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        // Strictly larger than our longest 25-second long poll.
        .timeout(Duration::from_secs(40))
}

pub(crate) struct BotApi {
    paths: Paths,
    client: reqwest::Client,
    base: String,
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    result: Option<Vec<Value>>,
    error_code: Option<u16>,
    parameters: Option<Parameters>,
}
#[derive(Deserialize)]
struct Parameters {
    retry_after: Option<u64>,
}

impl BotApi {
    pub(crate) fn new(paths: Paths) -> Result<Self, Error> {
        let client = secure_client_builder(reqwest::Client::builder())
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Self {
            paths,
            client,
            base: "https://api.telegram.org".into(),
        })
    }

    #[cfg(test)]
    pub(super) fn client(&self) -> &reqwest::Client {
        &self.client
    }

    #[cfg(test)]
    pub(super) fn loopback(paths: Paths, address: std::net::SocketAddr) -> Self {
        assert!(address.ip().is_loopback());
        Self {
            paths,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .timeout(Duration::from_secs(40))
                .build()
                .unwrap(),
            base: format!("http://{address}"),
        }
    }

    pub(crate) async fn get_updates(&self, offset: i64, timeout: u8) -> Result<Vec<Value>, Error> {
        self.get_updates_with_limit(offset, timeout, if offset == -1 { 1 } else { BATCH_LIMIT })
            .await
    }

    pub(super) async fn get_updates_with_limit(
        &self,
        offset: i64,
        timeout: u8,
        limit: u8,
    ) -> Result<Vec<Value>, Error> {
        if timeout > 25
            || offset < -1
            || !(1..=BATCH_LIMIT).contains(&limit)
            || (offset == -1 && limit != 1)
        {
            return Err(Error::Config);
        }
        // Reading the allowlist here also revokes pending network work when its
        // source is missing/invalid. The caller checks each incoming chat again.
        self.paths.allowlist()?;
        let token = self.paths.token()?;
        let url = Zeroizing::new(format!("{}/bot{}/getUpdates", self.base, token.as_str()));
        // Never trace the URL or retain reqwest errors (their Display includes it).
        let mut response = self
            .client
            .post(url.as_str())
            .json(&serde_json::json!({"offset":offset,"timeout":timeout,"limit":limit}))
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Error::Unauthorized);
        }
        if status == 409 {
            return Err(Error::Conflict);
        }
        if status >= 500 {
            return Err(Error::Transport);
        }
        if status != 200 && status != 429 {
            return Err(Error::Rejected(status));
        }
        let response_limit = if limit == 1 {
            SINGLE_UPDATE_LIMIT
        } else {
            RESPONSE_LIMIT
        };
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if body.len().saturating_add(chunk.len()) > response_limit {
                return Err(Error::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let envelope: Envelope = serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
        if !envelope.ok || status == 429 {
            return Err(match envelope.error_code.unwrap_or(status) {
                401 => Error::Unauthorized,
                409 => Error::Conflict,
                429 => Error::RateLimited(
                    envelope
                        .parameters
                        .and_then(|p| p.retry_after)
                        .unwrap_or(60)
                        .max(1),
                ),
                500..=599 => Error::Transport,
                code => Error::Rejected(code),
            });
        }
        let updates = envelope.result.ok_or(Error::Protocol)?;
        if updates.len() > usize::from(limit) {
            return Err(Error::Protocol);
        }
        Ok(updates)
    }
}
