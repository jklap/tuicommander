use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TARGETS: &[(&str, &str)] = &[
    ("Linux x86_64", "x86_64-unknown-linux-gnu"),
    ("Linux aarch64", "aarch64-unknown-linux-gnu"),
    ("Darwin arm64", "aarch64-apple-darwin"),
];
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalAsset {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
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

pub(crate) async fn ensure_local(version: &str, target: &str) -> Result<LocalAsset, String> {
    let url = asset_url(version, target);
    ensure_local_from_url(version, target, &url).await
}

async fn ensure_local_from_url(
    version: &str,
    target: &str,
    url: &str,
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

    let result = download_and_promote(version, target, url, &staging, &destination).await;
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

        let first = ensure_local_from_url("1.2.3", "test-target", &url)
            .await
            .expect("download succeeds");
        let second = ensure_local_from_url("1.2.3", "test-target", "http://127.0.0.1:1")
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
        )
        .await
        .expect_err("404 is a named error");

        assert_eq!(
            error,
            "no tuic-remote release asset for x86_64-unknown-linux-gnu at v9.8.7"
        );
    }
}
