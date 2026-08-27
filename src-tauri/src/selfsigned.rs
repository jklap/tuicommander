//! Self-signed TLS certificate generation for plain-LAN HTTPS access.
//!
//! Fallback for when Tailscale HTTPS isn't available: browsers only expose
//! secure-context-only APIs (e.g. the async Clipboard API) to `https://` or
//! `http://localhost` origins, so a plain `http://<lan-ip>` origin loses that
//! whole class of feature. See `plans/self-signed-https-cert.md`.

use std::net::IpAddr;
use std::path::Path;

const CERT_FILE: &str = "self-signed-cert.pem";
const KEY_FILE: &str = "self-signed-key.pem";
const META_FILE: &str = "self-signed-meta.json";

/// 10 years: this is a private, locally-trusted-by-exception cert accepted via
/// a one-time browser warning per device — short rotation only adds
/// re-click-through friction with no real security benefit here.
const VALIDITY_DAYS: i64 = 365 * 10;

/// Matches `tailscale::cert_renewal_loop`'s convention of renewing ahead of
/// expiry rather than exactly at it.
const RENEWAL_THRESHOLD_DAYS: i64 = 30;

/// Serializes cert generation/clearing so two concurrent callers (e.g. a
/// Tailscale-status-triggered restart racing the Settings "Regenerate"
/// button) can't interleave their cert.pem/key.pem/meta.json writes into a
/// mismatched set — each file is individually atomic via `persist_atomic`,
/// but the three together are not without this lock.
static GENERATION_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Cache metadata sidecar, so validity/coverage checks don't require parsing
/// the X.509 cert back out of the PEM.
#[derive(serde::Serialize, serde::Deserialize)]
struct CertMeta {
    not_after_unix: i64,
    sans: Vec<String>,
    /// SHA-256 fingerprint of the DER cert, lowercase hex. Shown in Settings
    /// so a user can verify the browser's warning dialog is showing this
    /// exact cert (not a MITM's) before clicking through.
    fingerprint_sha256: String,
}

/// Generate (or load a cached) self-signed cert covering localhost + all
/// current LAN IPs. Regenerates if missing, expiring within 30 days (matching
/// `tailscale::cert_renewal_loop`'s renewal threshold), or if the SAN list no
/// longer covers the machine's current IPs.
pub(crate) fn ensure_self_signed_cert(lan_ips: &[IpAddr]) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    ensure_self_signed_cert_in(&crate::config::config_dir(), lan_ips)
}

fn ensure_self_signed_cert_in(
    dir: &Path,
    lan_ips: &[IpAddr],
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let _guard = GENERATION_LOCK.lock();
    let cert_path = dir.join(CERT_FILE);
    let key_path = dir.join(KEY_FILE);
    let meta_path = dir.join(META_FILE);

    if let Some(cached) = load_cached(&cert_path, &key_path, &meta_path, lan_ips) {
        return Ok(cached);
    }

    generate_and_cache(&cert_path, &key_path, &meta_path, lan_ips)
}

/// Cert cache status for the Settings UI — never generates, only reports.
pub(crate) struct CertStatus {
    pub(crate) generated: bool,
    pub(crate) not_after_unix: Option<i64>,
    pub(crate) fingerprint_sha256: Option<String>,
}

pub(crate) fn cert_status() -> CertStatus {
    cert_status_in(&crate::config::config_dir())
}

fn cert_status_in(dir: &Path) -> CertStatus {
    match std::fs::read(dir.join(META_FILE)) {
        Ok(bytes) => match serde_json::from_slice::<CertMeta>(&bytes) {
            Ok(meta) => CertStatus {
                generated: true,
                not_after_unix: Some(meta.not_after_unix),
                fingerprint_sha256: Some(meta.fingerprint_sha256),
            },
            Err(_) => CertStatus {
                generated: false,
                not_after_unix: None,
                fingerprint_sha256: None,
            },
        },
        Err(_) => CertStatus {
            generated: false,
            not_after_unix: None,
            fingerprint_sha256: None,
        },
    }
}

/// Delete the cached cert so the next `ensure_self_signed_cert` regenerates
/// fresh — used by the Settings UI's "Regenerate" action.
pub(crate) fn clear_cached_cert() -> anyhow::Result<()> {
    clear_cached_cert_in(&crate::config::config_dir())
}

fn clear_cached_cert_in(dir: &Path) -> anyhow::Result<()> {
    let _guard = GENERATION_LOCK.lock();
    // META_FILE first and unconditionally attempted even if a later removal
    // fails: `load_cached` treats a missing/unreadable meta file as a
    // cache-miss, so invalidating it is what actually forces regeneration.
    // Best-effort on the rest (rather than stopping at the first error) so a
    // stuck cert.pem doesn't also leave a stale, still-cached key.pem/meta
    // behind — errors are collected and the first one is still returned so
    // the caller (and the Settings UI) sees an accurate failure.
    let mut first_err = None;
    for file in [META_FILE, CERT_FILE, KEY_FILE] {
        let path = dir.join(file);
        if path.exists()
            && let Err(e) = std::fs::remove_file(&path)
        {
            tracing::warn!(
                source = "selfsigned",
                path = %path.display(),
                "Failed to remove cached cert file: {e}"
            );
            first_err.get_or_insert(e);
        }
    }
    match first_err {
        None => Ok(()),
        Some(e) => Err(e.into()),
    }
}

fn load_cached(
    cert_path: &Path,
    key_path: &Path,
    meta_path: &Path,
    lan_ips: &[IpAddr],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let meta: CertMeta = serde_json::from_slice(&std::fs::read(meta_path).ok()?).ok()?;

    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let renewal_cutoff = meta.not_after_unix - RENEWAL_THRESHOLD_DAYS * 24 * 3600;
    if now >= renewal_cutoff {
        return None;
    }

    if !lan_ips
        .iter()
        .all(|ip| meta.sans.iter().any(|san| san == &ip.to_string()))
    {
        return None;
    }

    let cert_pem = std::fs::read(cert_path).ok()?;
    let key_pem = std::fs::read(key_path).ok()?;
    Some((cert_pem, key_pem))
}

fn generate_and_cache(
    cert_path: &Path,
    key_path: &Path,
    meta_path: &Path,
    lan_ips: &[IpAddr],
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut sans: Vec<String> = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    sans.extend(lan_ips.iter().map(IpAddr::to_string));
    // SAN entries must be unique per rcgen; lan_ips could in principle repeat
    // 127.0.0.1 on odd network configs.
    sans.sort_unstable();
    sans.dedup();

    let mut params = rcgen::CertificateParams::new(sans.clone())
        .map_err(|e| anyhow::anyhow!("Failed to build self-signed cert params: {e}"))?;
    let not_before = time::OffsetDateTime::now_utc();
    let not_after = not_before + time::Duration::days(VALIDITY_DAYS);
    params.not_before = not_before;
    params.not_after = not_after;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "TUICommander (self-signed)");

    let key_pair =
        rcgen::KeyPair::generate().map_err(|e| anyhow::anyhow!("Failed to generate key: {e}"))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| anyhow::anyhow!("Failed to self-sign cert: {e}"))?;

    let fingerprint_sha256 = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(cert.der()));
    let cert_pem = cert.pem().into_bytes();
    let key_pem = key_pair.serialize_pem().into_bytes();
    let meta = CertMeta {
        not_after_unix: not_after.unix_timestamp(),
        sans: sans.clone(),
        fingerprint_sha256: fingerprint_sha256.clone(),
    };
    let meta_json = serde_json::to_vec(&meta)
        .map_err(|e| anyhow::anyhow!("Failed to serialize cert cache metadata: {e}"))?;

    crate::config::persist_atomic(cert_path, &cert_pem).map_err(|e| anyhow::anyhow!(e))?;
    crate::config::persist_atomic(key_path, &key_pem).map_err(|e| anyhow::anyhow!(e))?;
    crate::config::persist_atomic(meta_path, &meta_json).map_err(|e| anyhow::anyhow!(e))?;

    tracing::info!(
        source = "selfsigned",
        sans = ?sans,
        not_after = %not_after,
        fingerprint_sha256,
        "Generated self-signed TLS cert"
    );

    Ok((cert_pem, key_pem))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn lan_ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn generates_a_parseable_cert_with_expected_sans() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];

        let (cert_pem, key_pem) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        let cert_str = String::from_utf8(cert_pem).unwrap();
        assert!(cert_str.starts_with("-----BEGIN CERTIFICATE-----"));
        let key_str = String::from_utf8(key_pem).unwrap();
        assert!(key_str.contains("PRIVATE KEY"));

        let meta: CertMeta =
            serde_json::from_slice(&std::fs::read(dir.path().join(META_FILE)).unwrap()).unwrap();
        assert!(meta.sans.contains(&"localhost".to_string()));
        assert!(meta.sans.contains(&"127.0.0.1".to_string()));
        assert!(meta.sans.contains(&"::1".to_string()));
        assert!(meta.sans.contains(&"192.168.1.50".to_string()));
        assert_eq!(
            meta.fingerprint_sha256.len(),
            64,
            "fingerprint must be a 32-byte SHA-256 hex digest"
        );
        assert!(
            meta.fingerprint_sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
    }

    #[test]
    fn reuses_cache_when_still_valid_and_ips_covered() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];

        let (cert_pem_1, key_pem_1) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();
        let (cert_pem_2, key_pem_2) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        assert_eq!(cert_pem_1, cert_pem_2, "cached cert must be reused as-is");
        assert_eq!(key_pem_1, key_pem_2, "cached key must be reused as-is");
    }

    #[test]
    fn ensure_self_signed_cert_is_serialized_across_concurrent_callers() {
        // Without `GENERATION_LOCK`, N threads racing on an empty cache would
        // each see `load_cached` return `None` and independently generate
        // their own random keypair, leaving whichever thread's writes landed
        // last — not necessarily a matching cert/key pair from any single
        // generation. With the lock, only the first caller actually
        // generates; every other caller serializes behind it and then hits
        // the now-valid cache, so all results must be identical.
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();
        let ips = [lan_ip(192, 168, 1, 50)];

        const N: usize = 8;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(N));
        let handles: Vec<_> = (0..N)
            .map(|_| {
                let dir_path = dir_path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    ensure_self_signed_cert_in(&dir_path, &ips).unwrap()
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let (first_cert, first_key) = &results[0];
        for (cert, key) in &results[1..] {
            assert_eq!(
                cert, first_cert,
                "concurrent callers must serialize onto one generation, not race into independent certs"
            );
            assert_eq!(
                key, first_key,
                "concurrent callers must serialize onto one generation, not race into independent keys"
            );
        }
    }

    #[test]
    fn regenerates_when_current_ips_are_no_longer_covered() {
        let dir = tempfile::tempdir().unwrap();
        let first_ips = [lan_ip(192, 168, 1, 50)];
        let (cert_pem_1, _) = ensure_self_signed_cert_in(dir.path(), &first_ips).unwrap();

        // Laptop moved to a new network — old SAN list no longer covers it.
        let second_ips = [lan_ip(10, 0, 0, 5)];
        let (cert_pem_2, _) = ensure_self_signed_cert_in(dir.path(), &second_ips).unwrap();

        assert_ne!(
            cert_pem_1, cert_pem_2,
            "SAN list changing must trigger regeneration"
        );
        let meta: CertMeta =
            serde_json::from_slice(&std::fs::read(dir.path().join(META_FILE)).unwrap()).unwrap();
        assert!(meta.sans.contains(&"10.0.0.5".to_string()));
        assert!(!meta.sans.contains(&"192.168.1.50".to_string()));
    }

    #[test]
    fn regenerates_when_cached_cert_is_expired() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];
        let (cert_pem_1, _) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        // Simulate an expired cache by rewriting the metadata sidecar with a
        // `not_after` in the past.
        let meta_path = dir.path().join(META_FILE);
        let mut meta: CertMeta =
            serde_json::from_slice(&std::fs::read(&meta_path).unwrap()).unwrap();
        meta.not_after_unix = time::OffsetDateTime::now_utc().unix_timestamp() - 1;
        std::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap()).unwrap();

        let (cert_pem_2, _) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        assert_ne!(
            cert_pem_1, cert_pem_2,
            "expired cache must trigger regeneration"
        );
    }

    #[test]
    fn cert_status_reports_not_generated_when_cache_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let status = cert_status_in(dir.path());
        assert!(!status.generated);
        assert!(status.not_after_unix.is_none());
    }

    #[test]
    fn cert_status_reports_expiry_after_generation() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];
        ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        let status = cert_status_in(dir.path());
        assert!(status.generated);
        assert!(status.not_after_unix.unwrap() > time::OffsetDateTime::now_utc().unix_timestamp());
    }

    #[test]
    fn clear_cached_cert_forces_regeneration() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];
        let (cert_pem_1, _) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();

        clear_cached_cert_in(dir.path()).unwrap();
        assert!(!cert_status_in(dir.path()).generated);

        let (cert_pem_2, _) = ensure_self_signed_cert_in(dir.path(), &ips).unwrap();
        assert_ne!(
            cert_pem_1, cert_pem_2,
            "clearing the cache must force a fresh cert on next ensure"
        );
    }

    #[test]
    fn regenerates_when_cache_files_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let ips = [lan_ip(192, 168, 1, 50)];

        // No prior cache at all — must generate fresh rather than error.
        let result = ensure_self_signed_cert_in(dir.path(), &ips);
        assert!(result.is_ok());
    }
}
