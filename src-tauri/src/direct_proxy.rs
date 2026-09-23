//! Certificate pinning (trust on first use) for Direct connections to an
//! `https://` daemon whose certificate no system root vouches for — typically
//! the daemon's own self-signed certificate (`selfsigned.rs` on the far side).
//!
//! Two pieces:
//!
//! * [`probe_direct_tls`] answers "what does this URL present?": nothing (plain
//!   `http://`), a certificate the OS trusts, a certificate nobody vouches for
//!   (with its SHA-256 fingerprint, so a person can compare it with the one the
//!   remote machine shows in Settings → Remote Access before pinning it), or —
//!   for an already pinned connection — whether the pin still matches.
//! * [`DirectProxies`] runs one loopback relay per pinned connection. It
//!   accepts plain TCP on `127.0.0.1:<port>`, opens TLS to the daemon with a
//!   verifier that accepts ONLY the pinned fingerprint, and copies bytes both
//!   ways. Neither `reqwest` nor the WebView can be told "trust exactly this
//!   leaf for this one host", but both can talk to a loopback URL — exactly the
//!   shape the SSH transport's tunnel already produces, so
//!   `remote_runtime::resolve_base_url` hands the relay's URL out like a
//!   tunnel's and nothing downstream changes.
//!
//! What this module deliberately does NOT do: carry credentials. The relay is a
//! byte pipe; the daemon still demands the session token that
//! `remote_runtime::authenticate` trades the vault password for. (The wip
//! design injected Basic Auth into proxied requests, which made the daemon
//! open to every local process that could reach the loopback port — dropped.)
//! A relay is started only by the Rust connect flow, from the SAVED
//! connection; there is no command that pairs a caller-supplied URL with
//! anything.
//!
//! Both verifiers still verify the handshake signature with rustls's own
//! helpers: only the chain-of-trust policy is replaced by a fingerprint
//! comparison, never the proof that the peer holds the certificate's key.
//! The capture verifier (accept any certificate) is used for one throwaway
//! handshake whose only output is the fingerprint — no application byte is
//! ever written on it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Budget for one TCP connect + TLS handshake (probe or relay leg). A daemon
/// that accepts the socket and never answers must not hang Connect forever.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Concurrent relayed connections per proxy. A WebView opens a handful (HTTP/1.1
/// pool, terminal WebSockets, `/events`), the backend a few more; anything past
/// this is refused rather than allowed to grow without bound.
const MAX_RELAYED_CONNECTIONS: usize = 128;

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

/// SHA-256 of a DER certificate, lowercase hex — the same computation
/// `selfsigned.rs` shows for this machine's own certificate.
pub(crate) fn cert_fingerprint_sha256(der: &[u8]) -> String {
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(der))
}

/// Canonical form of a stored or typed fingerprint: 64 lowercase hex digits.
/// Colons and whitespace (how fingerprints are usually displayed) are dropped;
/// anything else is not a SHA-256 fingerprint and yields `None`.
pub(crate) fn normalize_fingerprint(raw: &str) -> Option<String> {
    let hex: String = raw
        .chars()
        .filter(|c| *c != ':' && !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

// ---------------------------------------------------------------------------
// Verifiers
// ---------------------------------------------------------------------------

fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Accepts any certificate and records the leaf — ONLY for the throwaway
/// handshake that reads a fingerprint to show the user.
struct CaptureVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    captured: std::sync::Mutex<Option<Vec<u8>>>,
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
        *self.captured.lock().unwrap_or_else(|e| e.into_inner()) = Some(end_entity.to_vec());
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

/// Accepts a certificate only when its fingerprint equals the pin. A mismatch
/// is a hard failure — never a silent re-pin: rotation and interception look
/// identical from here, and only the user can tell them apart.
struct PinnedVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    /// Already normalized ([`normalize_fingerprint`]).
    expected: String,
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
        if cert_fingerprint_sha256(end_entity) == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            // `InvalidCertificate`, so `is_certificate_error` classifies it.
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

fn connector_with_verifier(verifier: Arc<dyn ServerCertVerifier>) -> Result<TlsConnector, String> {
    let config = rustls::ClientConfig::builder_with_provider(crypto_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

fn pinned_connector(fingerprint: &str) -> Result<TlsConnector, String> {
    connector_with_verifier(Arc::new(PinnedVerifier {
        provider: crypto_provider(),
        expected: fingerprint.to_string(),
    }))
}

fn native_roots_connector() -> Result<TlsConnector, String> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let config = rustls::ClientConfig::builder_with_provider(crypto_provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

// ---------------------------------------------------------------------------
// Targets and handshakes
// ---------------------------------------------------------------------------

/// Where an `https://` Direct URL answers. `host` is bare (no IPv6 brackets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpsTarget {
    pub(crate) host: String,
    pub(crate) port: u16,
}

/// `Ok(None)` for `http://` (no TLS involved), `Ok(Some(..))` for `https://`,
/// `Err` for anything else.
pub(crate) fn https_target(url: &str) -> Result<Option<HttpsTarget>, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|e| format!("invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" => return Ok(None),
        "https" => {}
        other => {
            return Err(format!(
                "unsupported scheme {other:?}: expected http or https"
            ));
        }
    }
    let host = match parsed.host() {
        Some(url::Host::Domain(d)) => d.to_string(),
        Some(url::Host::Ipv4(ip)) => ip.to_string(),
        Some(url::Host::Ipv6(ip)) => ip.to_string(),
        None => return Err("URL has no host".to_string()),
    };
    let port = parsed.port_or_known_default().unwrap_or(443);
    Ok(Some(HttpsTarget { host, port }))
}

fn is_certificate_error(err: &std::io::Error) -> bool {
    err.get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|e| matches!(e, rustls::Error::InvalidCertificate(_)))
}

/// TCP connect + TLS handshake, both under [`HANDSHAKE_TIMEOUT`].
async fn tls_connect(
    target: &HttpsTarget,
    connector: &TlsConnector,
) -> std::io::Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let server_name = ServerName::try_from(target.host.clone())
        .map_err(|e| std::io::Error::other(format!("invalid host name {:?}: {e}", target.host)))?;
    let handshake = async {
        let tcp = TcpStream::connect((target.host.as_str(), target.port)).await?;
        connector.connect(server_name, tcp).await
    };
    tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake)
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "TLS handshake timed out"))?
}

/// Handshake with the capture verifier and return the presented leaf's
/// fingerprint. The stream is dropped unused.
async fn capture_presented_fingerprint(target: &HttpsTarget) -> Result<String, String> {
    let verifier = Arc::new(CaptureVerifier {
        provider: crypto_provider(),
        captured: std::sync::Mutex::new(None),
    });
    let connector = connector_with_verifier(verifier.clone())?;
    drop(
        tls_connect(target, &connector)
            .await
            .map_err(|e| format!("TLS handshake with {} failed: {e}", target.host))?,
    );
    verifier
        .captured
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_deref()
        .map(cert_fingerprint_sha256)
        .ok_or_else(|| "TLS handshake completed but no certificate was presented".to_string())
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type")]
pub(crate) enum ProbeResult {
    /// `http://`: no TLS involved; nothing contacted.
    NoTlsNeeded,
    /// `https://` whose certificate the OS root store trusts: no pin needed.
    Trusted,
    /// `https://`, nobody vouches for the certificate and nothing is pinned.
    /// The user must confirm this fingerprint before anything connects.
    NeedsConfirmation { fingerprint: String },
    /// `https://`, pinned, and the server still presents that certificate.
    PinnedMatch,
    /// `https://`, pinned, and the server now presents a different one.
    PinnedMismatch { presented_fingerprint: String },
}

/// Classify what `url` presents. Read-only: persists nothing, sends no
/// application data. A pin that is not a valid fingerprint is an error, not
/// "unpinned" — a garbled pin must never quietly downgrade to TOFU.
pub(crate) async fn probe_direct_tls(
    url: &str,
    pinned_fingerprint: Option<&str>,
) -> Result<ProbeResult, String> {
    let Some(target) = https_target(url)? else {
        return Ok(ProbeResult::NoTlsNeeded);
    };
    if let Some(raw) = pinned_fingerprint {
        let expected = normalize_fingerprint(raw)
            .ok_or_else(|| "stored certificate pin is not a SHA-256 fingerprint".to_string())?;
        return match tls_connect(&target, &pinned_connector(&expected)?).await {
            Ok(_) => Ok(ProbeResult::PinnedMatch),
            Err(e) if is_certificate_error(&e) => Ok(ProbeResult::PinnedMismatch {
                presented_fingerprint: capture_presented_fingerprint(&target).await?,
            }),
            Err(e) => Err(format!("Unreachable: {e}")),
        };
    }
    match tls_connect(&target, &native_roots_connector()?).await {
        Ok(_) => Ok(ProbeResult::Trusted),
        Err(e) if is_certificate_error(&e) => Ok(ProbeResult::NeedsConfirmation {
            fingerprint: capture_presented_fingerprint(&target).await?,
        }),
        Err(e) => Err(format!("Unreachable: {e}")),
    }
}

/// IPC twin of `POST /config/remote-connections/probe-direct-tls`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub async fn probe_direct_tls_connection(
    url: String,
    tls_fingerprint: Option<String>,
) -> Result<ProbeResult, String> {
    probe_direct_tls(&url, tls_fingerprint.as_deref()).await
}

// ---------------------------------------------------------------------------
// Pinned loopback relay
// ---------------------------------------------------------------------------

struct ProxyHandle {
    #[cfg(test)]
    port: u16,
    /// The accept loop. It owns the listener and a `JoinSet` of live relays, so
    /// aborting it closes the port AND every relayed connection.
    task: tokio::task::JoinHandle<()>,
}

/// One pinned relay per connection id.
#[derive(Default)]
pub(crate) struct DirectProxies {
    handles: DashMap<String, ProxyHandle>,
}

impl DirectProxies {
    /// Start (or restart) the relay for `connection_id` and return its loopback
    /// port. `fingerprint` must already be normalized.
    pub(crate) async fn start(
        &self,
        connection_id: &str,
        target: HttpsTarget,
        fingerprint: &str,
    ) -> Result<u16, String> {
        self.stop(connection_id);
        let connector = Arc::new(pinned_connector(fingerprint)?);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| format!("No loopback port for the pinned relay: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("No loopback port for the pinned relay: {e}"))?
            .port();
        let target = Arc::new(target);
        let task = tokio::spawn(async move {
            let mut relays = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok((inbound, peer)) => {
                            if !admits(peer, relays.len()) {
                                continue;
                            }
                            let connector = Arc::clone(&connector);
                            let target = Arc::clone(&target);
                            relays.spawn(async move {
                                if let Err(e) = relay(inbound, &target, &connector).await {
                                    tracing::debug!(source = "direct_proxy", error = %e, "relay ended");
                                }
                            });
                        }
                        // EMFILE and friends: back off instead of spinning.
                        Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                    },
                    Some(_) = relays.join_next(), if !relays.is_empty() => {}
                }
            }
        });
        self.handles.insert(
            connection_id.to_string(),
            ProxyHandle {
                #[cfg(test)]
                port,
                task,
            },
        );
        Ok(port)
    }

    /// Stop the relay for `connection_id`, if any. Idempotent.
    pub(crate) fn stop(&self, connection_id: &str) {
        if let Some((_, handle)) = self.handles.remove(connection_id) {
            handle.task.abort();
        }
    }

    #[cfg(test)]
    pub(crate) fn port_for(&self, connection_id: &str) -> Option<u16> {
        self.handles.get(connection_id).map(|h| h.port)
    }
}

/// Only loopback peers (the listener is bound to 127.0.0.1, so this is
/// belt-and-braces), and only up to [`MAX_RELAYED_CONNECTIONS`] at once.
fn admits(peer: SocketAddr, live: usize) -> bool {
    peer.ip().is_loopback() && live < MAX_RELAYED_CONNECTIONS
}

async fn relay(
    mut inbound: TcpStream,
    target: &HttpsTarget,
    connector: &TlsConnector,
) -> std::io::Result<()> {
    let mut outbound = tls_connect(target, connector).await?;
    tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn fingerprint_is_lowercase_hex_sha256_of_the_der() {
        let der = b"not a certificate, just bytes";
        assert_eq!(
            cert_fingerprint_sha256(der),
            hex::encode(<sha2::Sha256 as sha2::Digest>::digest(der))
        );
    }

    #[test]
    fn normalize_accepts_colons_and_case_and_rejects_everything_else() {
        let fp = "AB".repeat(32);
        let colons = fp
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(
            normalize_fingerprint(&fp).as_deref(),
            Some(&*"ab".repeat(32))
        );
        assert_eq!(
            normalize_fingerprint(&colons).as_deref(),
            Some(&*"ab".repeat(32))
        );
        assert!(normalize_fingerprint("").is_none());
        assert!(normalize_fingerprint(&"a".repeat(63)).is_none());
        assert!(normalize_fingerprint(&"g".repeat(64)).is_none());
    }

    #[test]
    fn https_target_reads_host_and_port_and_refuses_other_schemes() {
        assert_eq!(
            https_target("https://example.com:8443/x").unwrap(),
            Some(HttpsTarget {
                host: "example.com".into(),
                port: 8443
            })
        );
        assert_eq!(
            https_target("https://example.com").unwrap().unwrap().port,
            443
        );
        assert_eq!(
            https_target("https://[::1]:9877").unwrap().unwrap().host,
            "::1"
        );
        assert_eq!(https_target("http://example.com:9877").unwrap(), None);
        assert!(https_target("ftp://example.com").is_err());
        assert!(https_target("not a url").is_err());
    }

    #[test]
    fn the_relay_admits_only_loopback_peers_up_to_the_cap() {
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let lan: SocketAddr = "192.168.1.5:5000".parse().unwrap();
        assert!(admits(local, 0));
        assert!(!admits(local, MAX_RELAYED_CONNECTIONS));
        assert!(!admits(lan, 0));
    }

    struct TestTlsServer {
        addr: SocketAddr,
        fingerprint: String,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestTlsServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    impl TestTlsServer {
        fn url(&self) -> String {
            format!("https://{}", self.addr)
        }
        fn target(&self) -> HttpsTarget {
            https_target(&self.url()).unwrap().unwrap()
        }
    }

    /// A TLS echo server on 127.0.0.1 presenting a throwaway self-signed
    /// certificate — what a remote daemon's `selfsigned.rs` serves.
    async fn start_tls_echo_server() -> TestTlsServer {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
        let fingerprint = cert_fingerprint_sha256(cert.der());
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(signing_key.serialize_der().into());
        let config = rustls::ServerConfig::builder_with_provider(crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(stream).await {
                        let (mut r, mut w) = tokio::io::split(tls);
                        let _ = tokio::io::copy(&mut r, &mut w).await;
                        let _ = w.shutdown().await;
                    }
                });
            }
        });
        TestTlsServer {
            addr,
            fingerprint,
            task,
        }
    }

    #[tokio::test]
    async fn an_http_url_needs_no_tls_and_contacts_nothing() {
        // Port 1 on loopback refuses: had the probe connected, it would error.
        assert_eq!(
            probe_direct_tls("http://127.0.0.1:1", None).await.unwrap(),
            ProbeResult::NoTlsNeeded
        );
    }

    #[tokio::test]
    async fn a_fresh_self_signed_target_needs_confirmation_with_its_real_fingerprint() {
        let server = start_tls_echo_server().await;
        assert_eq!(
            probe_direct_tls(&server.url(), None).await.unwrap(),
            ProbeResult::NeedsConfirmation {
                fingerprint: server.fingerprint.clone()
            }
        );
    }

    #[tokio::test]
    async fn a_matching_pin_is_accepted_in_any_display_form() {
        let server = start_tls_echo_server().await;
        let upper = server.fingerprint.to_ascii_uppercase();
        assert_eq!(
            probe_direct_tls(&server.url(), Some(&upper)).await.unwrap(),
            ProbeResult::PinnedMatch
        );
    }

    #[tokio::test]
    async fn a_changed_certificate_is_a_mismatch_never_a_silent_repin() {
        let server = start_tls_echo_server().await;
        let wrong = "0".repeat(64);
        assert_eq!(
            probe_direct_tls(&server.url(), Some(&wrong)).await.unwrap(),
            ProbeResult::PinnedMismatch {
                presented_fingerprint: server.fingerprint.clone()
            }
        );
    }

    #[tokio::test]
    async fn a_garbled_pin_is_an_error_not_a_downgrade_to_first_use() {
        let server = start_tls_echo_server().await;
        let err = probe_direct_tls(&server.url(), Some("not-a-fingerprint"))
            .await
            .unwrap_err();
        assert!(err.contains("not a SHA-256 fingerprint"), "{err}");
    }

    #[tokio::test]
    async fn the_native_root_store_refuses_a_self_signed_certificate() {
        let server = start_tls_echo_server().await;
        let err = tls_connect(&server.target(), &native_roots_connector().unwrap())
            .await
            .expect_err("a throwaway certificate is in no OS trust store");
        assert!(is_certificate_error(&err), "{err:?}");
    }

    #[tokio::test]
    async fn the_pinned_relay_carries_bytes_both_ways_and_closes_on_stop() {
        let server = start_tls_echo_server().await;
        let proxies = DirectProxies::default();
        let port = proxies
            .start("conn", server.target(), &server.fingerprint)
            .await
            .unwrap();
        assert_eq!(proxies.port_for("conn"), Some(port));

        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(b"through the pinned relay").await.unwrap();
        let mut echoed = [0u8; 24];
        client.read_exact(&mut echoed).await.unwrap();
        assert_eq!(&echoed, b"through the pinned relay");

        proxies.stop("conn");
        assert!(proxies.port_for("conn").is_none());
        // Stop aborts the accept loop, which owns the live relays: the open
        // connection ends and the port stops accepting.
        let mut rest = Vec::new();
        let read =
            tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut rest)).await;
        assert!(read.is_ok(), "a stopped relay must close its connections");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            assert!(
                std::time::Instant::now() < deadline,
                "port {port} still accepting"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn the_relay_refuses_to_talk_to_a_server_presenting_another_certificate() {
        let server = start_tls_echo_server().await;
        let proxies = DirectProxies::default();
        let port = proxies
            .start("conn", server.target(), &"0".repeat(64))
            .await
            .unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let _ = client.write_all(b"secret").await;
        let mut buf = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut buf))
            .await
            .expect("the relay closes the connection");
        assert!(
            read.is_err() || buf.is_empty(),
            "nothing may come back: {buf:?}"
        );
        proxies.stop("conn");
    }

    #[test]
    fn stopping_an_unknown_relay_is_a_no_op() {
        DirectProxies::default().stop("never-started");
    }
}
