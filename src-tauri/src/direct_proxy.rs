//! Local loopback reverse proxy for "Direct" remote connections that need
//! TLS certificate pinning (self-signed/untrusted cert) and/or HTTP Basic
//! Auth injection.
//!
//! Story: SSH Tunnels + Remote Servers consolidation, Phase 4
//! ("Self-signed HTTPS for Direct").
//!
//! ## Why a raw byte-copying proxy, not an HTTP/WebSocket client library
//!
//! HTTP/1.1 requests/responses and a WebSocket connection (after its Upgrade
//! handshake) are both just bytes on one TCP stream. A local listener that
//! accepts a plain TCP connection, opens an outbound connection to the real
//! remote (TLS-wrapped via `tokio-rustls` for `https://`, plain otherwise),
//! and relays bytes both directions transparently handles ordinary HTTP
//! calls AND the terminal WebSocket's upgrade-then-frames traffic with the
//! same code path. This sidesteps `reqwest`'s TLS backend being `native-tls`
//! by default in this codebase (no `rustls-tls` feature is enabled — adding
//! one would silently change every other `reqwest::Client` in this crate,
//! e.g. `github.rs`/`updater.rs`, which is out of scope here) and
//! `tokio-tungstenite`'s bundled TLS (which has no hook for a custom
//! per-connection verifier).
//!
//! ## Why this proxy exists at all, not just a client-side `Authorization` header
//!
//! A plaintext connection password must never cross into the frontend's JS
//! realm: this codebase's own accepted security stance (root `AGENTS.md`,
//! "Plugin capabilities do not isolate plugins from each other") means any
//! frontend-callable "return a secret in plaintext" command is a real
//! exposure to a malicious plugin sharing the same JS realm. Only this
//! Rust-side proxy — sourced straight from the OS keyring — ever holds the
//! password. `rpcImpl`/the terminal WebSocket client need zero changes: they
//! already only know how to talk to a local loopback `baseUrl`, exactly the
//! shape the SSH transport's tunnel already produces.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_rustls::TlsConnector;

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

/// SHA-256 fingerprint of a DER certificate, lowercase hex. Identical
/// algorithm to `selfsigned.rs`'s `fingerprint_sha256`, applied here to a
/// REMOTE server's presented certificate instead of our own generated one.
pub(crate) fn cert_fingerprint_sha256(der: &[u8]) -> String {
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(der))
}

// ---------------------------------------------------------------------------
// Custom certificate verifiers
//
// Both verifiers still cryptographically verify the handshake signature
// (`verify_tls12_signature`/`verify_tls13_signature`, delegated to rustls's
// own helpers using the installed crypto provider's algorithms) — only the
// certificate *trust chain* policy is overridden, never proof that the peer
// holds the presented certificate's private key. This is the standard
// pattern rustls itself documents for a custom verifier.
// ---------------------------------------------------------------------------

/// Accepts any certificate, recording the leaf's DER bytes — used only to
/// perform a throwaway handshake so a self-signed/untrusted server's
/// fingerprint can be read and shown to the user for confirmation, before
/// anything is pinned or proxied for real.
struct CaptureVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    captured: std::sync::Mutex<Option<Vec<u8>>>,
}

impl CaptureVerifier {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self {
            provider,
            captured: std::sync::Mutex::new(None),
        })
    }

    fn captured_fingerprint(&self) -> Option<String> {
        self.captured
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_deref()
            .map(cert_fingerprint_sha256)
    }
}

impl std::fmt::Debug for CaptureVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureVerifier").finish()
    }
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.captured.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(end_entity.as_ref().to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Accepts a certificate ONLY if its SHA-256 fingerprint matches the pinned
/// value. A mismatch is a hard rejection — this verifier never silently
/// re-pins, since a changed fingerprint could mean legitimate cert rotation
/// OR a MITM, and only the user can tell the difference.
struct PinnedVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    expected_fingerprint: String,
}

impl std::fmt::Debug for PinnedVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedVerifier").finish()
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = cert_fingerprint_sha256(end_entity.as_ref());
        if presented == self.expected_fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            tracing::debug!(
                source = "direct_proxy",
                expected = %self.expected_fingerprint,
                presented = %presented,
                "pinned certificate mismatch"
            );
            // `InvalidCertificate`, not `General` — this is what
            // `is_certificate_error` (and every rustls-internal caller that
            // distinguishes "cert policy failure" from other TLS errors)
            // actually matches on. The detailed expected-vs-presented values
            // are reconstructed by `probe_direct_tls`'s `PinnedMismatch`
            // branch for the user-facing message; this error only needs to
            // be classifiable, not descriptive.
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn server_name_for(host: &str) -> Result<ServerName<'static>, String> {
    ServerName::try_from(host.to_string()).map_err(|e| format!("invalid host name {host:?}: {e}"))
}

// ---------------------------------------------------------------------------
// Probing: does this Direct URL need a proxy at all, and if it needs
// pinning, what fingerprint does the server actually present?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub(crate) enum ProbeResult {
    /// `http://` — no TLS involved at all.
    NoTlsNeeded,
    /// `https://` with a certificate trusted by the OS's native root store.
    Trusted,
    /// `https://` with an untrusted/self-signed certificate, not yet pinned.
    /// The frontend must confirm this fingerprint before a proxy is started.
    NeedsConfirmation { fingerprint: String },
    /// `https://`, already pinned, and the server still presents the same
    /// certificate.
    PinnedMatch,
    /// `https://`, already pinned, but the server now presents a DIFFERENT
    /// certificate. Never auto-accepted — surfaced as a hard mismatch.
    PinnedMismatch { presented_fingerprint: String },
}

fn parse_https_target(url: &str) -> Result<Option<(String, u16)>, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Ok(None);
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);
    Ok(Some((host, port)))
}

/// General form of [`parse_https_target`] for either scheme, used by the
/// proxy start path (which needs a target regardless of TLS). Returns
/// `(host, port, is_https)`.
fn parse_direct_target(url: &str) -> Result<(String, u16, bool), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    let is_https = match parsed.scheme() {
        "https" => true,
        "http" => false,
        other => {
            return Err(format!(
                "unsupported scheme {other:?} — expected http or https"
            ));
        }
    };
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "URL has no resolvable port".to_string())?;
    Ok((host, port, is_https))
}

/// What TLS treatment (if any) the proxy's outbound leg should use — decided
/// by the caller (which already ran `probe_direct_tls` and knows the
/// classification), not re-derived here, so there's exactly one place that
/// decides "is this cert trustworthy."
#[derive(Debug, Clone)]
pub(crate) enum OutboundTls {
    /// Plain `http://` — no TLS at all.
    None,
    /// `https://` with a certificate the OS's native root store already
    /// trusts — still needs a real TLS-wrapped outbound leg, just no pinning.
    NativeRoots,
    /// `https://` with a self-signed/untrusted certificate, pinned to this
    /// fingerprint (already confirmed by the user via `ProbeResult::NeedsConfirmation`).
    Pinned(String),
}

fn build_tls_connector(tls: &OutboundTls) -> Result<Option<Arc<TlsConnector>>, String> {
    match tls {
        OutboundTls::None => Ok(None),
        OutboundTls::NativeRoots => {
            let mut root_store = rustls::RootCertStore::empty();
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = root_store.add(cert);
            }
            let config = rustls::ClientConfig::builder_with_provider(crypto_provider())
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .with_root_certificates(root_store)
                .with_no_client_auth();
            Ok(Some(Arc::new(TlsConnector::from(Arc::new(config)))))
        }
        OutboundTls::Pinned(fingerprint) => {
            let verifier = Arc::new(PinnedVerifier {
                provider: crypto_provider(),
                expected_fingerprint: fingerprint.clone(),
            });
            let config = rustls::ClientConfig::builder_with_provider(crypto_provider())
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .dangerous()
                .with_custom_certificate_verifier(verifier)
                .with_no_client_auth();
            Ok(Some(Arc::new(TlsConnector::from(Arc::new(config)))))
        }
    }
}

/// Does a rustls/io handshake error look like "the certificate isn't
/// trusted" specifically, as opposed to a network-level failure (DNS,
/// connection refused, timeout)? Only the former should fall through to the
/// pinning flow — the latter is a real connectivity error the caller should
/// surface as-is.
fn is_certificate_error(err: &std::io::Error) -> bool {
    err.get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|rustls_err| matches!(rustls_err, rustls::Error::InvalidCertificate(_)))
}

async fn handshake_with_verifier(
    host: &str,
    port: u16,
    verifier: Arc<dyn ServerCertVerifier>,
) -> std::io::Result<()> {
    let provider = crypto_provider();
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect((host, port)).await?;
    let server_name = server_name_for(host).map_err(std::io::Error::other)?;
    let _stream = connector.connect(server_name, tcp).await?;
    Ok(())
}

async fn handshake_with_native_roots(host: &str, port: u16) -> std::io::Result<()> {
    let mut root_store = rustls::RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for cert in native.certs {
        let _ = root_store.add(cert);
    }
    let provider = crypto_provider();
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect((host, port)).await?;
    let server_name = server_name_for(host).map_err(std::io::Error::other)?;
    let _stream = connector.connect(server_name, tcp).await?;
    Ok(())
}

async fn capture_presented_fingerprint(host: &str, port: u16) -> Result<String, String> {
    let provider = crypto_provider();
    let verifier = CaptureVerifier::new(provider);
    handshake_with_verifier(host, port, verifier.clone())
        .await
        .map_err(|e| e.to_string())?;
    verifier
        .captured_fingerprint()
        .ok_or_else(|| "TLS handshake completed but no certificate was captured".to_string())
}

/// Probe a Direct connection's URL to determine whether a proxy is needed
/// for TLS reasons, and if so, what the server's certificate fingerprint
/// actually is. Does not consider whether auth is configured — that's a
/// separate, additive reason to run a proxy (see `DirectProxyManager::start`).
pub(crate) async fn probe_direct_tls(
    url: &str,
    pinned_fingerprint: Option<&str>,
) -> Result<ProbeResult, String> {
    let Some((host, port)) = parse_https_target(url)? else {
        return Ok(ProbeResult::NoTlsNeeded);
    };

    if let Some(expected) = pinned_fingerprint {
        let verifier = Arc::new(PinnedVerifier {
            provider: crypto_provider(),
            expected_fingerprint: expected.to_string(),
        });
        return match handshake_with_verifier(&host, port, verifier).await {
            Ok(()) => Ok(ProbeResult::PinnedMatch),
            Err(e) if is_certificate_error(&e) => {
                let presented = capture_presented_fingerprint(&host, port).await?;
                Ok(ProbeResult::PinnedMismatch {
                    presented_fingerprint: presented,
                })
            }
            Err(e) => Err(e.to_string()),
        };
    }

    match handshake_with_native_roots(&host, port).await {
        Ok(()) => Ok(ProbeResult::Trusted),
        Err(e) if is_certificate_error(&e) => {
            let fingerprint = capture_presented_fingerprint(&host, port).await?;
            Ok(ProbeResult::NeedsConfirmation { fingerprint })
        }
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// The proxy itself
// ---------------------------------------------------------------------------

/// Any stream `AsyncRead + AsyncWrite`, so plain TCP and TLS-wrapped TCP can
/// be handled behind one type in the relay loop.
trait AsyncStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> AsyncStream for T {}

struct DirectProxyHandle {
    // Only read by `#[cfg(test)]`'s `port_for` today — genuinely a live field,
    // not dead code, just not needed outside tests yet.
    #[cfg_attr(not(test), allow(dead_code))]
    port: u16,
    /// Checked explicitly on every loop iteration, not just relied on via
    /// `notify.notified()`'s wakeup — see `stop()`'s doc comment for why
    /// `notify_waiters()` alone is not a reliable shutdown signal here.
    should_stop: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

pub(crate) struct DirectProxyManager {
    handles: DashMap<String, DirectProxyHandle>,
}

impl DirectProxyManager {
    pub(crate) fn new() -> Self {
        Self {
            handles: DashMap::new(),
        }
    }

    /// Start (or restart) a proxy for `connection_id`, relaying to
    /// `remote_host:remote_port` with the given outbound TLS treatment (see
    /// [`OutboundTls`]) and, optionally, Basic Auth credentials to inject.
    pub(crate) async fn start(
        &self,
        connection_id: String,
        remote_host: String,
        remote_port: u16,
        tls: OutboundTls,
        basic_auth: Option<(String, String)>,
    ) -> Result<u16, String> {
        self.stop(&connection_id);

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| e.to_string())?;
        let local_port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_for_task = should_stop.clone();
        let notify = Arc::new(Notify::new());
        let notify_for_task = notify.clone();
        let cookie_seen = Arc::new(AtomicBool::new(false));

        let tls_connector = build_tls_connector(&tls)?;

        tokio::spawn(async move {
            loop {
                // Checked FIRST on every iteration, not only reacted to via
                // `notify`'s wakeup: `tokio::select!` only registers a branch
                // as "waiting" if it actually gets polled without another
                // branch already being Ready, so under a steady stream of
                // incoming connections `accept()` can keep winning the race
                // every single iteration — `notify_waiters()` then wakes
                // nobody (there is no queued permit the way `notify_one`
                // provides), and the shutdown signal is silently lost
                // forever. This flag makes shutdown a checked condition
                // instead of a one-shot broadcast that can race a busy loop.
                if should_stop_for_task.load(Ordering::Relaxed) {
                    break;
                }
                tokio::select! {
                    () = notify_for_task.notified() => break,
                    accepted = listener.accept() => {
                        if should_stop_for_task.load(Ordering::Relaxed) {
                            break;
                        }
                        let Ok((inbound, _addr)) = accepted else { continue };
                        let remote_host = remote_host.clone();
                        let tls_connector = tls_connector.clone();
                        let basic_auth = basic_auth.clone();
                        let cookie_seen = cookie_seen.clone();
                        tokio::spawn(async move {
                            if let Err(e) = relay_one_connection(
                                inbound,
                                &remote_host,
                                remote_port,
                                tls_connector,
                                basic_auth,
                                cookie_seen,
                            )
                            .await
                            {
                                tracing::debug!(
                                    source = "direct_proxy",
                                    error = %e,
                                    "relay connection ended"
                                );
                            }
                        });
                    }
                }
            }
        });

        self.handles.insert(
            connection_id,
            DirectProxyHandle {
                port: local_port,
                should_stop,
                notify,
            },
        );
        Ok(local_port)
    }

    /// Stop the proxy for `connection_id`, if one is running. Idempotent.
    ///
    /// Sets the stop flag BEFORE notifying: `notify_waiters()` only wakes a
    /// task that is actively polling `.notified()` at that exact instant —
    /// under a busy accept loop, `accept()` can keep winning
    /// `tokio::select!`'s race every iteration, so `notified()` may never
    /// actually be the one polled and the notification is silently lost with
    /// no queued permit to redeliver it later (unlike `notify_one`). The flag
    /// makes the loop check an explicit, persistent condition each time
    /// through instead of depending on catching a one-shot broadcast at the
    /// right instant — `notify_waiters()` remains purely a wake-up nudge for
    /// the idle case (nothing else would otherwise wake a task blocked only
    /// on `accept()`).
    pub(crate) fn stop(&self, connection_id: &str) {
        if let Some((_, handle)) = self.handles.remove(connection_id) {
            handle.should_stop.store(true, Ordering::Relaxed);
            handle.notify.notify_waiters();
        }
    }

    #[cfg(test)]
    pub(crate) fn is_running(&self, connection_id: &str) -> bool {
        self.handles.contains_key(connection_id)
    }

    #[cfg(test)]
    pub(crate) fn port_for(&self, connection_id: &str) -> Option<u16> {
        self.handles.get(connection_id).map(|h| h.port)
    }
}

impl Default for DirectProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn relay_one_connection(
    inbound: TcpStream,
    remote_host: &str,
    remote_port: u16,
    tls_connector: Option<Arc<TlsConnector>>,
    basic_auth: Option<(String, String)>,
    cookie_seen: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let tcp = TcpStream::connect((remote_host, remote_port)).await?;
    let mut outbound: Box<dyn AsyncStream> = match &tls_connector {
        Some(connector) => {
            let server_name = server_name_for(remote_host).map_err(std::io::Error::other)?;
            Box::new(connector.connect(server_name, tcp).await?)
        }
        None => Box::new(tcp),
    };

    if let Some(creds) = basic_auth.filter(|_| !cookie_seen.load(Ordering::Relaxed)) {
        relay_with_auth_injection(inbound, &mut outbound, &creds, &cookie_seen).await
    } else {
        let mut inbound = inbound;
        tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await?;
        Ok(())
    }
}

/// Read exactly one HTTP request's headers off `inbound`, inject an
/// `Authorization: Basic ...` header before its trailing blank line, forward
/// it to `outbound`, then read that request's response headers back,
/// watching for the daemon's own `tui-session` cookie (see this module's doc
/// comment — once seen, `pollHealth`/`rpcImpl`'s browser-side `fetch()` calls
/// carry it automatically on every later same-origin request, so injection
/// is only ever needed once per proxy's life). After the first
/// request/response cycle, degrades to a plain bidirectional byte copy for
/// the rest of this TCP connection's life — any bytes already buffered by
/// reading line-by-line are preserved by `BufReader`'s own internal buffer,
/// so a request/response body sent alongside its headers in the same read is
/// never dropped.
async fn relay_with_auth_injection(
    inbound: TcpStream,
    outbound: &mut Box<dyn AsyncStream>,
    (username, password): &(String, String),
    cookie_seen: &Arc<AtomicBool>,
) -> std::io::Result<()> {
    let (in_read, mut in_write) = tokio::io::split(inbound);
    let mut in_read = BufReader::new(in_read);

    let mut request_lines = read_header_block(&mut in_read).await?;
    if !request_lines.is_empty() {
        let credential = format!("{username}:{password}");
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, credential);
        let blank_line = request_lines.pop();
        request_lines.push(format!("Authorization: Basic {encoded}\r\n"));
        if let Some(blank) = blank_line {
            request_lines.push(blank);
        }
        for line in &request_lines {
            outbound.write_all(line.as_bytes()).await?;
        }
    }

    let (out_read, mut out_write) = tokio::io::split(outbound);
    let mut out_read = BufReader::new(out_read);
    let response_lines = read_header_block(&mut out_read).await?;
    let saw_session_cookie = response_lines.iter().any(|line| {
        line.to_ascii_lowercase().starts_with("set-cookie:") && line.contains("tui-session")
    });
    for line in &response_lines {
        in_write.write_all(line.as_bytes()).await?;
    }
    if saw_session_cookie {
        cookie_seen.store(true, Ordering::Relaxed);
    }

    let client_to_server = tokio::io::copy(&mut in_read, &mut out_write);
    let server_to_client = tokio::io::copy(&mut out_read, &mut in_write);
    let (a, b) = tokio::join!(client_to_server, server_to_client);
    a?;
    b?;
    Ok(())
}

/// Read lines up to and including the first blank line (`\r\n` or `\n`),
/// i.e. one HTTP header block. Returns whatever was read even if the stream
/// closed before a blank line was seen (an empty vec if nothing was read at
/// all) — callers treat an empty result as "nothing to forward."
async fn read_header_block<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Vec<String>> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let is_blank = line == "\r\n" || line == "\n";
        lines.push(line);
        if is_blank {
            break;
        }
    }
    Ok(lines)
}

// ---------------------------------------------------------------------------
// Tauri commands / HTTP parity
//
// Shared `_impl` functions take `&AppState` directly (matching this
// codebase's established pattern for a Tauri command that also needs an
// HTTP-reachable twin — see `pty.rs`'s `list_active_sessions_impl` and this
// story's own `tunnels::commands`/`remote_connection` `_impl` functions from
// earlier phases) so both transports call one code path.
// ---------------------------------------------------------------------------

/// Probe a Direct connection's URL for TLS trust, exactly as `ProbeResult`
/// describes. State-free — does not touch `AppState` or persist anything.
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn probe_direct_tls_connection(
    url: String,
    tls_fingerprint: Option<String>,
) -> Result<ProbeResult, String> {
    probe_direct_tls(&url, tls_fingerprint.as_deref()).await
}

/// Start a proxy for `connection_id` if one is actually needed (the target
/// is `https://`, or a password is configured for this connection), sourced
/// from the saved `RemoteConnection` and the keyring — the frontend never
/// sees the password. `tls_fingerprint`/`use_native_roots` describe the
/// outbound TLS treatment the caller already determined via
/// `probe_direct_tls_connection` (never re-derived here, so there is exactly
/// one place that decides "is this cert trustworthy").
///
/// Returns `None` when no proxy is needed at all (plain `http://` with no
/// credentials configured) — the frontend should then talk to the raw URL
/// directly, exactly as it does today.
pub(crate) async fn start_direct_proxy_impl(
    state: &crate::AppState,
    connection_id: &str,
    url: &str,
    tls_fingerprint: Option<&str>,
    use_native_roots: bool,
) -> Result<Option<u16>, String> {
    let (host, port, is_https) = parse_direct_target(url)?;

    let connections = crate::remote_connection::RemoteConnectionStore::load(&state.data_dir)
        .map_err(|e| e.to_string())?;
    let connection = connections
        .into_iter()
        .find(|c| c.id == connection_id)
        .ok_or_else(|| format!("connection '{connection_id}' not found"))?;
    let password = crate::credentials::get(crate::credentials::Credential::RemoteConnection(
        connection_id,
    ))?;
    let basic_auth = match (&connection.auth_username, password) {
        (Some(user), Some(pass)) if !user.trim().is_empty() && !pass.is_empty() => {
            Some((user.trim().to_string(), pass))
        }
        _ => None,
    };

    if !is_https && basic_auth.is_none() {
        return Ok(None);
    }

    let tls = if !is_https {
        OutboundTls::None
    } else if let Some(fingerprint) = tls_fingerprint {
        OutboundTls::Pinned(fingerprint.to_string())
    } else if use_native_roots {
        OutboundTls::NativeRoots
    } else {
        return Err("https:// target requires either a pinned fingerprint or native-roots trust — call probe_direct_tls_connection first".to_string());
    };

    state
        .direct_proxy_manager
        .start(connection_id.to_string(), host, port, tls, basic_auth)
        .await
        .map(Some)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn start_direct_proxy(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    connection_id: String,
    url: String,
    tls_fingerprint: Option<String>,
    use_native_roots: bool,
) -> Result<Option<u16>, String> {
    start_direct_proxy_impl(
        &state,
        &connection_id,
        &url,
        tls_fingerprint.as_deref(),
        use_native_roots,
    )
    .await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub fn stop_direct_proxy(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    connection_id: String,
) {
    state.direct_proxy_manager.stop(&connection_id);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Once;

    static CRYPTO_INIT: Once = Once::new();

    /// `rustls::crypto::ring::default_provider().install_default()` panics
    /// if called twice in the same process — the desktop/headless boot paths
    /// already do this once, but a pure `cargo test` run of this file alone
    /// (or alongside other test modules that don't boot the app) never has,
    /// so tests that build a `rustls::ClientConfig` need it installed
    /// exactly once, process-wide, guarded by `Once`.
    fn ensure_crypto_provider_installed() {
        CRYPTO_INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    #[test]
    fn fingerprint_matches_selfsigned_algorithm() {
        // Same computation as `selfsigned.rs`'s `fingerprint_sha256`: SHA-256
        // over the raw DER bytes, lowercase hex.
        let der = b"not a real certificate, just bytes for the algorithm test";
        let expected = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(der));
        assert_eq!(cert_fingerprint_sha256(der), expected);
    }

    #[test]
    fn parse_https_target_extracts_host_and_port() {
        assert_eq!(
            parse_https_target("https://example.com:8443/x").unwrap(),
            Some(("example.com".to_string(), 8443))
        );
    }

    #[test]
    fn parse_https_target_defaults_port_443() {
        assert_eq!(
            parse_https_target("https://example.com").unwrap(),
            Some(("example.com".to_string(), 443))
        );
    }

    #[test]
    fn parse_https_target_returns_none_for_http() {
        assert_eq!(parse_https_target("http://example.com:9877").unwrap(), None);
    }

    #[test]
    fn parse_https_target_rejects_invalid_url() {
        assert!(parse_https_target("not a url").is_err());
    }

    // --- read_header_block ---

    #[tokio::test]
    async fn read_header_block_stops_at_blank_line() {
        let input = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nBODY-STARTS-HERE".to_vec();
        let mut reader = BufReader::new(&input[..]);
        let lines = read_header_block(&mut reader).await.unwrap();
        assert_eq!(lines, vec!["GET / HTTP/1.1\r\n", "Host: x\r\n", "\r\n"]);

        // Bytes after the blank line remain in the reader for a later copy.
        let mut rest = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut rest)
            .await
            .unwrap();
        assert_eq!(rest, b"BODY-STARTS-HERE");
    }

    #[tokio::test]
    async fn read_header_block_handles_early_eof() {
        let input = b"GET / HTTP/1.1\r\n".to_vec();
        let mut reader = BufReader::new(&input[..]);
        let lines = read_header_block(&mut reader).await.unwrap();
        assert_eq!(lines, vec!["GET / HTTP/1.1\r\n"]);
    }

    #[tokio::test]
    async fn read_header_block_empty_input_returns_empty() {
        let input: Vec<u8> = Vec::new();
        let mut reader = BufReader::new(&input[..]);
        let lines = read_header_block(&mut reader).await.unwrap();
        assert!(lines.is_empty());
    }

    // --- TLS handshake / verifier behavior, against a real throwaway rcgen cert ---

    struct TestTlsServer {
        addr: SocketAddr,
        cert_der: Vec<u8>,
        shutdown: Arc<Notify>,
    }

    impl Drop for TestTlsServer {
        fn drop(&mut self) {
            self.shutdown.notify_waiters();
        }
    }

    /// Spin up a real TLS listener on 127.0.0.1 presenting a throwaway
    /// self-signed certificate (via `rcgen`, already a dependency), which
    /// just accepts one connection and closes — enough to drive a real
    /// handshake through `handshake_with_verifier`/`capture_presented_fingerprint`.
    async fn start_test_tls_server() -> TestTlsServer {
        ensure_crypto_provider_installed();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
        let cert_der = cert.der().to_vec();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(signing_key.serialize_der().into());

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], key_der)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = shutdown.clone();

        tokio::spawn(async move {
            // Loops accepting connections, not a one-shot: `probe_direct_tls`
            // makes two separate TCP connections per call (a native-roots
            // attempt, then a capture attempt once that's rejected), and a
            // test may call it more than once — a single-`accept` server
            // would refuse every connection after the first.
            loop {
                tokio::select! {
                    () = shutdown_for_task.notified() => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let acceptor = acceptor.clone();
                        tokio::spawn(async move {
                            // Complete the handshake, then just hold the
                            // connection open briefly — callers only need
                            // the handshake to complete.
                            if let Ok(mut tls) = acceptor.accept(stream).await {
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_millis(200),
                                    tokio::io::AsyncWriteExt::shutdown(&mut tls),
                                )
                                .await;
                            }
                        });
                    }
                }
            }
        });

        TestTlsServer {
            addr,
            cert_der,
            shutdown,
        }
    }

    #[tokio::test]
    async fn capture_verifier_records_the_presented_certificate() {
        let server = start_test_tls_server().await;
        let fingerprint =
            capture_presented_fingerprint(&server.addr.ip().to_string(), server.addr.port())
                .await
                .unwrap();
        assert_eq!(fingerprint, cert_fingerprint_sha256(&server.cert_der));
    }

    #[tokio::test]
    async fn pinned_verifier_accepts_a_matching_fingerprint() {
        let server = start_test_tls_server().await;
        let expected = cert_fingerprint_sha256(&server.cert_der);
        let verifier = Arc::new(PinnedVerifier {
            provider: crypto_provider(),
            expected_fingerprint: expected,
        });
        let result =
            handshake_with_verifier(&server.addr.ip().to_string(), server.addr.port(), verifier)
                .await;
        assert!(
            result.is_ok(),
            "expected handshake to succeed, got {result:?}"
        );
    }

    #[tokio::test]
    async fn pinned_verifier_rejects_a_mismatched_fingerprint() {
        let server = start_test_tls_server().await;
        let verifier = Arc::new(PinnedVerifier {
            provider: crypto_provider(),
            expected_fingerprint: "0".repeat(64), // never a real SHA-256 hex digest of this cert
        });
        let result =
            handshake_with_verifier(&server.addr.ip().to_string(), server.addr.port(), verifier)
                .await;
        let err = result
            .expect_err("a fingerprint mismatch must fail the handshake, never silently succeed");
        assert!(
            is_certificate_error(&err),
            "expected a certificate error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn native_roots_handshake_rejects_a_self_signed_cert() {
        // A throwaway rcgen cert is never in any OS trust store — this proves
        // `handshake_with_native_roots` correctly refuses it (the branch
        // `probe_direct_tls` relies on to fall through into the pinning flow).
        let server = start_test_tls_server().await;
        let result =
            handshake_with_native_roots(&server.addr.ip().to_string(), server.addr.port()).await;
        let err = result
            .expect_err("a self-signed cert must not be accepted by native-roots verification");
        assert!(
            is_certificate_error(&err),
            "expected a certificate error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn probe_direct_tls_no_tls_for_http_url() {
        let result = probe_direct_tls("http://127.0.0.1:9877", None)
            .await
            .unwrap();
        assert_eq!(result, ProbeResult::NoTlsNeeded);
    }

    #[tokio::test]
    async fn probe_direct_tls_needs_confirmation_for_a_fresh_self_signed_target() {
        let server = start_test_tls_server().await;
        let url = format!("https://{}:{}", server.addr.ip(), server.addr.port());
        let result = probe_direct_tls(&url, None).await.unwrap();
        match result {
            ProbeResult::NeedsConfirmation { fingerprint } => {
                assert_eq!(fingerprint, cert_fingerprint_sha256(&server.cert_der));
            }
            other => panic!("expected NeedsConfirmation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_direct_tls_pinned_match_for_the_same_target_and_fingerprint() {
        let server = start_test_tls_server().await;
        let url = format!("https://{}:{}", server.addr.ip(), server.addr.port());
        let pinned = cert_fingerprint_sha256(&server.cert_der);
        let result = probe_direct_tls(&url, Some(&pinned)).await.unwrap();
        assert_eq!(result, ProbeResult::PinnedMatch);
    }

    #[tokio::test]
    async fn probe_direct_tls_pinned_mismatch_never_silently_repins() {
        let server = start_test_tls_server().await;
        let url = format!("https://{}:{}", server.addr.ip(), server.addr.port());
        let wrong = "0".repeat(64);
        let result = probe_direct_tls(&url, Some(&wrong)).await.unwrap();
        match result {
            ProbeResult::PinnedMismatch {
                presented_fingerprint,
            } => {
                assert_eq!(
                    presented_fingerprint,
                    cert_fingerprint_sha256(&server.cert_der)
                );
            }
            other => panic!("expected PinnedMismatch, got {other:?}"),
        }
    }

    // --- DirectProxyManager: start/stop lifecycle and plain-TCP relay ---

    /// A minimal echo server standing in for a remote `tuic-remote` daemon —
    /// enough to prove the proxy actually relays bytes both directions.
    async fn start_echo_server() -> (SocketAddr, Arc<Notify>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = shutdown.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown_for_task.notified() => break,
                    accepted = listener.accept() => {
                        let Ok((mut stream, _)) = accepted else { continue };
                        tokio::spawn(async move {
                            let (mut r, mut w) = stream.split();
                            let _ = tokio::io::copy(&mut r, &mut w).await;
                        });
                    }
                }
            }
        });
        (addr, shutdown)
    }

    #[tokio::test]
    async fn proxy_relays_bytes_to_a_plain_tcp_target() {
        let (remote_addr, _remote_shutdown) = start_echo_server().await;
        let manager = DirectProxyManager::new();
        let port = manager
            .start(
                "conn-1".to_string(),
                remote_addr.ip().to_string(),
                remote_addr.port(),
                OutboundTls::None,
                None,
            )
            .await
            .unwrap();
        assert!(manager.is_running("conn-1"));
        assert_eq!(manager.port_for("conn-1"), Some(port));

        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(b"hello through the proxy").await.unwrap();
        client.shutdown().await.unwrap();

        let mut received = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut client, &mut received)
            .await
            .unwrap();
        assert_eq!(received, b"hello through the proxy");

        manager.stop("conn-1");
        assert!(!manager.is_running("conn-1"));
    }

    #[tokio::test]
    async fn proxy_start_is_idempotent_restart() {
        let (remote_addr, _remote_shutdown) = start_echo_server().await;
        let manager = DirectProxyManager::new();
        let first_port = manager
            .start(
                "conn-2".to_string(),
                remote_addr.ip().to_string(),
                remote_addr.port(),
                OutboundTls::None,
                None,
            )
            .await
            .unwrap();
        let second_port = manager
            .start(
                "conn-2".to_string(),
                remote_addr.ip().to_string(),
                remote_addr.port(),
                OutboundTls::None,
                None,
            )
            .await
            .unwrap();
        assert!(manager.is_running("conn-2"));
        assert_eq!(manager.port_for("conn-2"), Some(second_port));

        // The old listener must eventually be gone, not leaked — but `stop()`
        // only wakes the old task via `Notify`, it doesn't block until that
        // task has actually run and dropped its listener, so an IMMEDIATE
        // check here is a real race, not a behavioral guarantee (see
        // src-tauri/AGENTS.md's "Which timing assertions are load-bearing").
        // Poll with a generous bound instead: this asserts the eventual
        // invariant without being sensitive to scheduler timing.
        if first_port != second_port {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if TcpStream::connect(("127.0.0.1", first_port)).await.is_err() {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "old listener on port {first_port} was never released after restart"
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
    }

    #[tokio::test]
    async fn proxy_stop_of_unknown_id_is_a_safe_noop() {
        let manager = DirectProxyManager::new();
        manager.stop("never-started");
    }

    // --- Auth injection ---

    #[tokio::test]
    async fn relay_with_auth_injection_adds_authorization_header_to_the_first_request() {
        let (server_addr, server_shutdown) = start_capturing_http_server().await;
        let manager = DirectProxyManager::new();
        let port = manager
            .start(
                "conn-auth".to_string(),
                server_addr.ip().to_string(),
                server_addr.port(),
                OutboundTls::None,
                Some(("alice".to_string(), "hunter2".to_string())),
            )
            .await
            .unwrap();

        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client
            .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        // Signal "no more requests on this connection" — required for
        // `read_to_end` below to ever complete: the proxy's client<->server
        // relay is a `tokio::join!` of two copy directions, and the
        // client-to-server one only reaches EOF (letting the whole relay,
        // and therefore this socket, close) once the client itself stops
        // writing. A real non-keep-alive HTTP/1.1 client does the same.
        client.shutdown().await.unwrap();
        // A single `read()` is also not guaranteed to capture a multi-packet
        // response in one call (the proxy forwards response header lines as
        // they're read, which can arrive as separate TCP segments) — read to
        // EOF instead.
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut client, &mut buf)
            .await
            .unwrap();
        let response = String::from_utf8_lossy(&buf);
        assert!(
            response.contains("captured-authorization: "),
            "response: {response}"
        );

        let expected = format!(
            "Basic {}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "alice:hunter2")
        );
        assert!(response.contains(&expected), "response: {response}");

        manager.stop("conn-auth");
        server_shutdown.notify_waiters();
    }

    #[tokio::test]
    async fn relay_stops_injecting_once_a_session_cookie_is_observed() {
        let (server_addr, server_shutdown) = start_capturing_http_server().await;
        let manager = DirectProxyManager::new();
        let port = manager
            .start(
                "conn-cookie".to_string(),
                server_addr.ip().to_string(),
                server_addr.port(),
                OutboundTls::None,
                Some(("alice".to_string(), "hunter2".to_string())),
            )
            .await
            .unwrap();

        // First request: the fake server always echoes a Set-Cookie header
        // (see start_capturing_http_server) — this must flip cookie_seen.
        let mut first = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        first
            .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        first.shutdown().await.unwrap();
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut first, &mut buf)
            .await
            .unwrap();
        drop(first);

        // Second, separate connection: must NOT get an injected Authorization
        // header now that the cookie has been observed.
        let mut second = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        second
            .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        second.shutdown().await.unwrap();
        let mut buf2 = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut second, &mut buf2)
            .await
            .unwrap();
        let response2 = String::from_utf8_lossy(&buf2);
        assert!(
            !response2.contains("captured-authorization: Basic"),
            "response should show no injected auth on the second connection: {response2}"
        );

        manager.stop("conn-cookie");
        server_shutdown.notify_waiters();
    }

    /// A fake HTTP/1.1 server that echoes back whether it saw an
    /// `Authorization` header (as a `captured-authorization:` response
    /// header) and always sets the real `tui-session` cookie name, so tests
    /// can assert on injection and on the cookie-observed cutover without
    /// needing the real `mcp_http` auth stack.
    async fn start_capturing_http_server() -> (SocketAddr, Arc<Notify>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = shutdown.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown_for_task.notified() => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        tokio::spawn(async move {
                            let (read_half, mut write_half) = stream.into_split();
                            let mut reader = BufReader::new(read_half);
                            let lines = read_header_block(&mut reader).await.unwrap_or_default();
                            let auth_header = lines
                                .iter()
                                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                                .cloned()
                                .unwrap_or_default();
                            let auth_value = auth_header
                                .split_once(':')
                                .map(|(_, v)| v.trim())
                                .unwrap_or("");
                            let body = "ok";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\ncaptured-authorization: {auth_value}\r\nSet-Cookie: tui-session=abc; Path=/\r\nContent-Length: {}\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = write_half.write_all(response.as_bytes()).await;
                            let _ = write_half.shutdown().await;
                        });
                    }
                }
            }
        });
        (addr, shutdown)
    }
}
