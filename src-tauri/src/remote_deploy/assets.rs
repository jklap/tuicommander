use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TARGETS: &[(&str, &str)] = &[
    ("Linux x86_64", "x86_64-unknown-linux-gnu"),
    ("Linux aarch64", "aarch64-unknown-linux-gnu"),
    ("Darwin arm64", "aarch64-apple-darwin"),
];
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BuildIdentity {
    pub(crate) version: String,
    pub(crate) target: String,
    pub(crate) sha256: String,
}

/// Keep the digest of the executable that started this process. Reading the
/// path again after an update would describe the replacement, not this daemon.
pub(crate) fn running_build_identity() -> Result<&'static BuildIdentity, String> {
    static RUNNING: OnceLock<Result<BuildIdentity, String>> = OnceLock::new();
    RUNNING
        .get_or_init(|| {
            let path = std::env::current_exe()
                .map_err(|error| format!("could not locate running executable: {error}"))?;
            let mut file = std::fs::File::open(&path)
                .map_err(|error| format!("could not open running executable: {error}"))?;
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("could not hash running executable: {error}"))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Ok(BuildIdentity {
                version: env!("CARGO_PKG_VERSION").to_string(),
                target: env!("TUIC_TARGET_TRIPLE").to_string(),
                sha256: hex_digest(&hasher.finalize()),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalAsset {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateAsset {
    pub(crate) binary: LocalAsset,
    pub(crate) source: &'static str,
    pub(crate) target: String,
    pub(crate) version: String,
}

/// Prefer the desktop release asset. Development builds can use a sibling
/// tuic-remote binary, but only when the daemon target equals this build target.
pub(crate) async fn resolve_update_asset(target: &str) -> Result<UpdateAsset, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("could not locate desktop binary: {e}"))?;
    let sibling = executable
        .parent()
        .ok_or_else(|| "desktop executable has no directory".to_string())?
        .join(if cfg!(windows) {
            "tuic-remote.exe"
        } else {
            "tuic-remote"
        });
    resolve_update_asset_from_url(
        target,
        &asset_url(env!("CARGO_PKG_VERSION"), target),
        &sibling,
        None,
    )
    .await
}

async fn resolve_update_asset_from_url(
    target: &str,
    url: &str,
    local_path: &Path,
    known_local_target: Option<&str>,
) -> Result<UpdateAsset, String> {
    match ensure_local_from_url(env!("CARGO_PKG_VERSION"), target, url, DOWNLOAD_TIMEOUT).await {
        Ok(binary) => Ok(UpdateAsset {
            binary,
            source: "release",
            target: target.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        Err(error) if error.starts_with("no tuic-remote release asset") => {
            let local_identity = if known_local_target.is_none() {
                Some(probe_local_binary(local_path).await?)
            } else {
                None
            };
            let local_target = known_local_target
                .or_else(|| {
                    local_identity
                        .as_ref()
                        .map(|identity| identity.target.as_str())
                })
                .ok_or_else(|| "local daemon did not report its target".to_string())?;
            if target != local_target {
                return Err(format!(
                    "release asset unavailable for remote target {target}; local tuic-remote target is {local_target}"
                ));
            }
            if !local_path.is_file() {
                return Err(format!(
                    "release asset unavailable; locally built tuic-remote for {local_target} not found at {}",
                    local_path.display()
                ));
            }
            let binary = local_asset(local_path.to_path_buf()).await?;
            if local_identity
                .as_ref()
                .is_some_and(|identity| identity.sha256 != binary.sha256)
            {
                return Err("local daemon binary changed after build identity probe".to_string());
            }
            Ok(UpdateAsset {
                binary,
                source: "local",
                target: target.to_string(),
                version: local_identity.map_or_else(
                    || env!("CARGO_PKG_VERSION").to_string(),
                    |identity| identity.version,
                ),
            })
        }
        Err(error) => Err(error),
    }
}

async fn probe_local_binary(path: &Path) -> Result<BuildIdentity, String> {
    if !path.is_file() {
        return Err(format!(
            "locally built tuic-remote not found at {}",
            path.display()
        ));
    }
    let mut command = tokio::process::Command::new(path);
    command.arg("--build-info").kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(300), command.output())
        .await
        .map_err(|_| {
            format!(
                "local tuic-remote build probe timed out at {}",
                path.display()
            )
        })?
        .map_err(|error| {
            format!(
                "could not run local tuic-remote at {}: {error}",
                path.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("requires --no-default-features") {
            return Err(format!(
                "local tuic-remote at {} requires --no-default-features; run cargo build --bin tuic-remote --no-default-features from src-tauri",
                path.display()
            ));
        }
        return Err(format!(
            "local tuic-remote did not report build identity at {}",
            path.display()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid local tuic-remote build identity: {error}"))
}

pub(crate) fn target_for(uname: &str) -> Option<&'static str> {
    TARGETS
        .iter()
        .find_map(|(candidate, target)| (*candidate == uname.trim()).then_some(*target))
}

pub(crate) fn require_target(uname: &str) -> Result<&'static str, String> {
    target_for(uname).ok_or_else(|| format!("unsupported remote host: {uname}"))
}

pub(crate) fn asset_url(version: &str, target: &str) -> String {
    format!(
        "https://github.com/sstraus/tuicommander/releases/download/v{version}/tuic-remote-{target}"
    )
}

async fn ensure_local_from_url(
    version: &str,
    target: &str,
    url: &str,
    timeout: std::time::Duration,
) -> Result<LocalAsset, String> {
    validate_component("version", version)?;
    validate_component("target", target)?;

    let directory = crate::config::config_dir().join("remote-bin").join(version);
    let destination = directory.join(format!("tuic-remote-{target}"));
    if destination.is_file() {
        return local_asset(destination).await;
    }

    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    let staging = directory.join(format!(
        ".tuic-remote-{target}.tmp-{}",
        uuid::Uuid::new_v4()
    ));

    let result = tokio::time::timeout(
        timeout,
        download_and_promote(version, target, url, &staging, &destination),
    )
    .await
    .unwrap_or_else(|_| {
        Err(format!(
            "tuic-remote release download timed out for {target}"
        ))
    });
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    result
}

async fn download_and_promote(
    version: &str,
    target: &str,
    url: &str,
    staging: &Path,
    destination: &Path,
) -> Result<LocalAsset, String> {
    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| format!("could not download {url}: {error}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "no tuic-remote release asset for {target} at v{version}"
        ));
    }
    if !response.status().is_success() {
        return Err(format!(
            "could not download {url}: HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ASSET_BYTES)
    {
        return Err(format!("tuic-remote asset exceeds {MAX_ASSET_BYTES} bytes"));
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .await
        .map_err(|error| format!("could not create {}: {error}", staging.display()))?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("could not download {url}: {error}"))?;
        written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "tuic-remote asset size overflow".to_string())?;
        if written > MAX_ASSET_BYTES {
            return Err(format!("tuic-remote asset exceeds {MAX_ASSET_BYTES} bytes"));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("could not write {}: {error}", staging.display()))?;
    }
    file.flush()
        .await
        .map_err(|error| format!("could not flush {}: {error}", staging.display()))?;
    file.sync_all()
        .await
        .map_err(|error| format!("could not sync {}: {error}", staging.display()))?;
    drop(file);

    let sha256 = hex_digest(&hasher.finalize());
    match tokio::fs::rename(staging, destination).await {
        Ok(()) => Ok(LocalAsset {
            path: destination.to_path_buf(),
            sha256,
        }),
        Err(_) if destination.is_file() => {
            let _ = tokio::fs::remove_file(staging).await;
            local_asset(destination.to_path_buf()).await
        }
        Err(error) => Err(format!(
            "could not promote {} to {}: {error}",
            staging.display(),
            destination.display()
        )),
    }
}

async fn local_asset(path: PathBuf) -> Result<LocalAsset, String> {
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|error| format!("could not open cached {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| format!("could not read cached {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(LocalAsset {
        path,
        sha256: hex_digest(&hasher.finalize()),
    })
}

fn validate_component(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(format!("invalid release {label}: {value}"));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_uname_values_to_release_targets() {
        assert_eq!(target_for("Linux x86_64"), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(
            target_for("Linux aarch64"),
            Some("aarch64-unknown-linux-gnu")
        );
        assert_eq!(target_for("Darwin arm64"), Some("aarch64-apple-darwin"));
    }

    #[test]
    fn unknown_uname_is_rejected_by_name() {
        assert_eq!(target_for("Plan 9 mips"), None);
        let error = require_target("Plan 9 mips").expect_err("unsupported host");
        assert!(error.contains("Plan 9 mips"));
    }

    #[test]
    fn release_asset_url_uses_the_package_version() {
        assert_eq!(
            asset_url(env!("CARGO_PKG_VERSION"), "aarch64-apple-darwin"),
            format!(
                "https://github.com/sstraus/tuicommander/releases/download/v{}/tuic-remote-aarch64-apple-darwin",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[tokio::test]
    async fn download_is_hashed_promoted_and_then_served_from_cache() {
        let config = tempfile::tempdir().expect("temporary config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let mut server = mockito::Server::new_async().await;
        let route = server
            .mock("GET", "/tuic-remote-test-target")
            .with_status(200)
            .with_body(b"remote binary".as_slice())
            .create_async()
            .await;
        let url = format!("{}/tuic-remote-test-target", server.url());

        let first = ensure_local_from_url("1.2.3", "test-target", &url, DOWNLOAD_TIMEOUT)
            .await
            .expect("download succeeds");
        let second = ensure_local_from_url(
            "1.2.3",
            "test-target",
            "http://127.0.0.1:1",
            DOWNLOAD_TIMEOUT,
        )
        .await
        .expect("cached file needs no network");

        let expected_hash = "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f";
        assert_eq!(first.sha256, expected_hash);
        assert_eq!(second.sha256, expected_hash);
        assert_eq!(first.path, second.path);
        assert_eq!(std::fs::read(&first.path).unwrap(), b"remote binary");
        assert_eq!(
            first.path,
            config
                .path()
                .join("remote-bin/1.2.3/tuic-remote-test-target")
        );
        assert!(!config.path().join("remote-bin/1.2.3/.staging").exists());
        route.assert_async().await;
    }

    #[tokio::test]
    async fn stalled_release_download_times_out_without_caching_an_asset() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let entered = Arc::new(AtomicBool::new(false));
        let router = axum::Router::new().route(
            "/asset",
            axum::routing::get({
                let entered = entered.clone();
                move || {
                    let entered = entered.clone();
                    async move {
                        entered.store(true, Ordering::SeqCst);
                        std::future::pending::<axum::http::StatusCode>().await
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let error = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            ensure_local_from_url(
                "1.2.3",
                "test-target",
                &url,
                std::time::Duration::from_secs(5),
            ),
        )
        .await
        .expect("release download must have its own timeout")
        .expect_err("stalled response must not be cached");
        assert!(
            entered.load(Ordering::SeqCst),
            "request reached the stalled release endpoint"
        );
        assert!(error.contains("timed out"), "{error}");
        assert!(
            !config
                .path()
                .join("remote-bin/1.2.3/tuic-remote-test-target")
                .exists()
        );
        server.abort();
    }

    #[tokio::test]
    async fn not_found_names_the_target_and_version() {
        let config = tempfile::tempdir().expect("temporary config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .create_async()
            .await;

        let error = ensure_local_from_url(
            "9.8.7",
            "x86_64-unknown-linux-gnu",
            &format!("{}/missing", server.url()),
            DOWNLOAD_TIMEOUT,
        )
        .await
        .expect_err("404 is a named error");

        assert_eq!(
            error,
            "no tuic-remote release asset for x86_64-unknown-linux-gnu at v9.8.7"
        );
    }

    #[tokio::test]
    async fn update_source_falls_back_only_for_a_matching_local_target() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let local = config.path().join("tuic-remote");
        std::fs::write(&local, b"replacement executable").unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/missing")
            .with_status(404)
            .expect(2)
            .create_async()
            .await;
        let url = format!("{}/missing", server.url());

        let selected = resolve_update_asset_from_url(
            "aarch64-apple-darwin",
            &url,
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .expect("matching local binary");
        assert_eq!(selected.source, "local");
        assert_eq!(
            selected.binary.sha256,
            "74faa3811f5e551111ed370650ae6d6acf14f8f7141bc5c4f653eb52bf57bf16"
        );

        let error = resolve_update_asset_from_url(
            "x86_64-unknown-linux-gnu",
            &url,
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .expect_err("cross-target binary must be refused");
        assert!(error.contains("remote target x86_64-unknown-linux-gnu"));
        assert!(error.contains("local tuic-remote target is aarch64-apple-darwin"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn desktop_feature_stub_names_the_headless_build_command() {
        use std::os::unix::fs::PermissionsExt;
        let config = tempfile::tempdir().unwrap();
        let stub = config.path().join("tuic-remote");
        std::fs::write(
            &stub,
            "#!/bin/sh\necho 'tuic-remote requires --no-default-features' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = probe_local_binary(&stub).await.unwrap_err();
        assert!(error.contains("requires --no-default-features"), "{error}");
        assert!(
            error.contains("cargo build --bin tuic-remote --no-default-features"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn update_source_prefers_the_matching_release_asset() {
        let config = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let local = config.path().join("tuic-remote");
        std::fs::write(&local, b"replacement executable").unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/release")
            .with_status(200)
            .with_body(b"remote binary".as_slice())
            .create_async()
            .await;
        let selected = resolve_update_asset_from_url(
            "aarch64-apple-darwin",
            &format!("{}/release", server.url()),
            &local,
            Some("aarch64-apple-darwin"),
        )
        .await
        .unwrap();
        assert_eq!(selected.source, "release");
        assert_eq!(
            selected.binary.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
    }
}
