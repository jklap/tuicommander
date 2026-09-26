use std::path::PathBuf;
use std::sync::Arc;

use crate::remote_connection::{DeployMode, RemoteTransport};
use crate::remote_deploy::assets::{BuildIdentity, resolve_update_asset};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct UpdatePreview {
    pub(crate) remote_build: Option<BuildIdentity>,
    pub(crate) desktop_build: BuildIdentity,
    pub(crate) source: &'static str,
    pub(crate) session_count: usize,
    pub(crate) out_of_date: bool,
}

pub(crate) fn build_is_out_of_date(remote: &BuildIdentity, selected: &BuildIdentity) -> bool {
    remote.sha256 != selected.sha256
}

pub(crate) async fn prepare(
    state: &Arc<crate::AppState>,
    id: &str,
) -> Result<UpdatePreview, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let base_url = state
        .remote
        .base_url(id)
        .ok_or("Remote connection is not connected")?;
    let client = state.remote.http_client();
    let health = crate::remote_runtime::read_health(&client, &base_url).await?;
    let remote_build = health.build;
    let target = if let Some(build) = &remote_build {
        build.target.clone()
    } else if let Some(profile) = crate::remote_runtime::ssh_profile(&connection) {
        let uname = crate::tunnels::exec::ssh_exec(
            &profile,
            "uname -sm",
            None,
            std::time::Duration::from_secs(10),
        )
        .await
        .map_err(|e| format!("SSH target probe failed: {e:?}"))?;
        crate::remote_deploy::assets::require_target(uname.stdout.trim())?.to_string()
    } else {
        return Err("Direct daemon does not report a build target; install a daemon with /health.build once before in-process updates".to_string());
    };
    let asset = resolve_update_asset(&target).await?;
    let session_count = health
        .session_count
        .ok_or("Remote daemon does not report live session count")?;
    let desktop_build = BuildIdentity {
        version: asset.version,
        target: asset.target,
        sha256: asset.binary.sha256,
    };
    Ok(UpdatePreview {
        out_of_date: remote_build
            .as_ref()
            .is_none_or(|build| build_is_out_of_date(build, &desktop_build)),
        remote_build,
        desktop_build,
        source: asset.source,
        session_count,
    })
}

pub(crate) async fn update_and_restart(
    state: &Arc<crate::AppState>,
    id: &str,
    confirmed_sessions: usize,
    expected_sha256: &str,
) -> Result<UpdatePreview, String> {
    let connection = crate::remote_runtime::load_connection(state, id)?;
    let base_url = state
        .remote
        .base_url(id)
        .ok_or("Remote connection is not connected")?;
    let client = state.remote.http_client();
    let token = state
        .remote
        .token(id)
        .ok_or("Remote connection has no session token")?;
    let preview = prepare(state, id).await?;
    if confirmed_sessions != preview.session_count {
        return Err(format!(
            "Live session count changed from {confirmed_sessions} to {}",
            preview.session_count
        ));
    }
    if expected_sha256 != preview.desktop_build.sha256 {
        return Err("Selected remote binary changed after confirmation".to_string());
    }
    let asset = resolve_update_asset(&preview.desktop_build.target).await?;
    if asset.binary.sha256 != preview.desktop_build.sha256 {
        return Err("Selected remote binary changed while preparing transfer".to_string());
    }
    match &connection.transport {
        RemoteTransport::Direct { .. } => {
            let file = tokio::fs::File::open(&asset.binary.path)
                .await
                .map_err(|e| format!("Could not open remote binary: {e}"))?;
            let stream = futures_util::stream::try_unfold(file, |mut file| async move {
                use tokio::io::AsyncReadExt;
                let mut buffer = [0u8; 64 * 1024];
                let count = file.read(&mut buffer).await?;
                if count == 0 {
                    Ok::<_, std::io::Error>(None)
                } else {
                    Ok(Some((
                        axum::body::Bytes::copy_from_slice(&buffer[..count]),
                        file,
                    )))
                }
            });
            let response = client
                .post(format!("{}/remote/update", base_url.trim_end_matches('/')))
                .query(&[("token", token.as_str())])
                .header("x-tuic-target", &preview.desktop_build.target)
                .header("x-tuic-sha256", &preview.desktop_build.sha256)
                .header("x-tuic-confirmed-sessions", confirmed_sessions)
                .body(reqwest::Body::wrap_stream(stream))
                .send()
                .await
                .map_err(|e| format!("Remote binary upload failed: {e}"))?;
            if !response.status().is_success() {
                return Err(format!(
                    "Remote binary upload rejected: {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ));
            }
        }
        RemoteTransport::Ssh {
            remote_daemon_port, ..
        } => {
            let profile = crate::remote_runtime::ssh_profile(&connection)
                .ok_or("Could not resolve SSH profile")?;
            let uname = crate::tunnels::exec::ssh_exec(
                &profile,
                "uname -sm",
                None,
                std::time::Duration::from_secs(10),
            )
            .await
            .map_err(|e| format!("SSH target probe failed: {e:?}"))?;
            let ssh_target = crate::remote_deploy::assets::require_target(uname.stdout.trim())?;
            if ssh_target != preview.desktop_build.target {
                return Err(format!(
                    "SSH target {ssh_target} differs from daemon target {}",
                    preview.desktop_build.target
                ));
            }
            let latest = crate::remote_runtime::read_health(&client, &base_url).await?;
            if latest.session_count != Some(confirmed_sessions) {
                return Err("Live session count changed before SSH deployment".to_string());
            }
            if connection.deploy == DeployMode::Installed {
                crate::remote_deploy::service::update_installed(
                    &profile,
                    *remote_daemon_port,
                    &token,
                )
                .await
                .map_err(|e| format!("Installed SSH update failed: {e}"))?;
            } else {
                crate::remote_deploy::deploy_ephemeral(
                    &profile,
                    *remote_daemon_port,
                    &token,
                    connection.survive_secs,
                )
                .await
                .map_err(|e| format!("SSH update failed: {e}"))?;
            }
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(health) = crate::remote_runtime::read_health(&client, &base_url).await {
            if health
                .build
                .as_ref()
                .is_some_and(|build| build.sha256 == preview.desktop_build.sha256)
            {
                crate::remote_runtime::record_updated_build(
                    state,
                    id,
                    preview.desktop_build.clone(),
                );
                return Ok(preview);
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Remote restart did not report build {} within 30s",
                preview.desktop_build.sha256
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn prepare_remote_update(
    state: tauri::State<'_, Arc<crate::AppState>>,
    id: String,
) -> Result<UpdatePreview, String> {
    prepare(state.inner(), &id).await
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn update_and_restart_remote(
    state: tauri::State<'_, Arc<crate::AppState>>,
    id: String,
    confirmed_sessions: usize,
    expected_sha256: String,
) -> Result<UpdatePreview, String> {
    update_and_restart(state.inner(), &id, confirmed_sessions, &expected_sha256).await
}

const MAX_UPDATE_BYTES: u64 = 512 * 1024 * 1024;

/// An update may touch only the executable selected when the daemon started.
pub(crate) struct RemoteUpdateState {
    pub(crate) executable: PathBuf,
    pub(crate) restart: Arc<tokio::sync::Notify>,
    pub(crate) in_progress: tokio::sync::Mutex<()>,
    pub(crate) installed: std::sync::atomic::AtomicBool,
}

pub(crate) async fn upload(
    State(state): State<Arc<crate::AppState>>,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> (StatusCode, String) {
    if !crate::mcp_http::auth::has_valid_token_query(&uri, &state.session_token.read()) {
        return (
            StatusCode::UNAUTHORIZED,
            "remote session token required".to_string(),
        );
    }
    match upload_inner(&state, &headers, body).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            "remote update staged; restarting".to_string(),
        ),
        Err((status, message)) => (status, message),
    }
}

async fn upload_inner(
    state: &crate::AppState,
    headers: &HeaderMap,
    body: Body,
) -> Result<(), (StatusCode, String)> {
    let error = |status, message: &str| (status, message.to_string());
    if cfg!(windows) {
        return Err(error(
            StatusCode::NOT_IMPLEMENTED,
            "in-process updates of Windows executables are not supported",
        ));
    }
    let target = header(headers, "x-tuic-target")?;
    if target != env!("TUIC_TARGET_TRIPLE") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "remote target does not match binary",
        ));
    }
    let digest = header(headers, "x-tuic-sha256")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid sha256 digest"));
    }
    let confirmed: usize = header(headers, "x-tuic-confirmed-sessions")?
        .parse()
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid confirmed session count"))?;
    if confirmed != state.session_maps.sessions.len() {
        return Err(error(StatusCode::CONFLICT, "live session count changed"));
    }
    if headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_UPDATE_BYTES)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "remote binary too large",
        ));
    }
    let Some(runtime) = state.remote_update.as_ref() else {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "not a remote daemon",
        ));
    };
    let _update_guard = runtime
        .in_progress
        .try_lock()
        .map_err(|_| error(StatusCode::CONFLICT, "another remote update is in progress"))?;
    if runtime.installed.load(std::sync::atomic::Ordering::Acquire) {
        return Err(error(
            StatusCode::CONFLICT,
            "remote restart is already pending",
        ));
    }

    let executable = std::fs::canonicalize(&runtime.executable).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not locate daemon executable",
        )
    })?;
    let metadata = std::fs::metadata(&executable).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not inspect daemon executable",
        )
    })?;
    if !metadata.is_file() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon executable is not a file",
        ));
    }
    let directory = executable.parent().ok_or_else(|| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "daemon executable has no directory",
        )
    })?;
    let staging = directory.join(format!(".tuic-remote-update-{}", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not stage remote binary",
                )
            })?;
        let mut stream = body.into_data_stream();
        let mut hasher = Sha256::new();
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "upload stream failed"))?;
            written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| error(StatusCode::PAYLOAD_TOO_LARGE, "remote binary too large"))?;
            if written > MAX_UPDATE_BYTES {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "remote binary too large",
                ));
            }
            hasher.update(&chunk);
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk).await.map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not write remote binary",
                )
            })?;
        }
        if written == 0 {
            return Err(error(StatusCode::BAD_REQUEST, "empty remote binary"));
        }
        use tokio::io::AsyncWriteExt;
        file.flush().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not flush remote binary",
            )
        })?;
        file.sync_all().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not sync remote binary",
            )
        })?;
        drop(file);
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if !actual.eq_ignore_ascii_case(digest) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "remote binary sha256 mismatch",
            ));
        }
        if confirmed != state.session_maps.sessions.len() {
            return Err(error(
                StatusCode::CONFLICT,
                "live session count changed during upload",
            ));
        }
        tokio::fs::set_permissions(&staging, metadata.permissions())
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not set executable permissions",
                )
            })?;
        tokio::fs::rename(&staging, &executable)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not replace daemon executable",
                )
            })?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    } else {
        runtime
            .installed
            .store(true, std::sync::atomic::Ordering::Release);
        tracing::info!(
            source = "remote",
            target,
            sha256 = digest,
            "Remote binary updated"
        );
        let restart = runtime.restart.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            restart.notify_one();
        });
    }
    result
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, (StatusCode, String)> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("missing or malformed {name}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn direct_update_waits_until_the_new_build_answers_health() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cache = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cache.path().to_path_buf());
        let target = env!("TUIC_TARGET_TRIPLE");
        let directory = cache
            .path()
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("tuic-remote-{target}")),
            b"remote binary",
        )
        .unwrap();
        let installed = Arc::new(AtomicBool::new(false));
        let advertised = Arc::new(AtomicBool::new(false));
        let upload_seen = Arc::new(tokio::sync::Notify::new());
        let router = axum::Router::new()
            .route("/health", axum::routing::get({
                let advertised = advertised.clone();
                move || {
                    let advertised = advertised.clone();
                    async move {
                        let sha256 = if advertised.load(Ordering::SeqCst) {
                            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f".to_string()
                        } else {
                            "a".repeat(64)
                        };
                        axum::Json(serde_json::json!({
                            "session_count": 0,
                            "build": { "version": "1.7.6", "target": env!("TUIC_TARGET_TRIPLE"), "sha256": sha256 }
                        }))
                    }
                }
            }))
            .route("/remote/update", axum::routing::post({
                let installed = installed.clone();
                let upload_seen = upload_seen.clone();
                move |uri: axum::http::Uri, headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let installed = installed.clone();
                    let upload_seen = upload_seen.clone();
                    async move {
                        assert_eq!(uri.query(), Some("token=test-token"));
                        assert_eq!(headers["x-tuic-confirmed-sessions"], "0");
                        assert_eq!(headers["x-tuic-target"], env!("TUIC_TARGET_TRIPLE"));
                        assert_eq!(body.as_ref(), b"remote binary");
                        installed.store(true, Ordering::SeqCst);
                        upload_seen.notify_one();
                        StatusCode::ACCEPTED
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "test remote",
            url.clone(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &url, Some("test-token"));

        let task = tokio::spawn(async move {
            update_and_restart(
                &state,
                &id,
                0,
                "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f",
            )
            .await
        });
        upload_seen.notified().await;
        assert!(installed.load(Ordering::SeqCst));
        assert!(
            !task.is_finished(),
            "upload alone must not count as a verified restart"
        );
        advertised.store(true, Ordering::SeqCst);
        let preview = task.await.unwrap().unwrap();
        assert_eq!(
            preview.desktop_build.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
        server.abort();
    }

    #[tokio::test]
    async fn prepare_reports_selected_release_and_live_sessions_for_direct() {
        let cache = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(cache.path().to_path_buf());
        let target = env!("TUIC_TARGET_TRIPLE");
        let directory = cache
            .path()
            .join("remote-bin")
            .join(env!("CARGO_PKG_VERSION"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("tuic-remote-{target}")),
            b"remote binary",
        )
        .unwrap();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "session_count": 2,
                    "build": { "version": "1.7.7", "target": target, "sha256": "a".repeat(64) }
                })
                .to_string(),
            )
            .create_async()
            .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "test remote",
            server.url(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &server.url(), Some("token"));

        let preview = prepare(&state, &id).await.unwrap();
        assert_eq!(preview.session_count, 2);
        assert_eq!(preview.source, "release");
        assert_eq!(preview.desktop_build.target, target);
        assert_eq!(
            preview.desktop_build.sha256,
            "7dee7cc2fcb3d9ee8394182fe8d23a1a3d7e5c80b869b281269df9215a5abf2f"
        );
        assert!(preview.out_of_date);
    }

    #[tokio::test]
    async fn older_direct_daemon_without_build_target_names_the_bootstrap_limit() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/health")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_count":1,"protocol_version":4}"#)
            .create_async()
            .await;
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let connection = crate::remote_connection::RemoteConnection::new_direct(
            "older daemon",
            server.url(),
            "boss",
        );
        let id = connection.id.clone();
        crate::remote_connection::RemoteConnectionStore::save(&state.data_dir, &[connection])
            .unwrap();
        state
            .remote
            .force_connected_for_test(&id, &server.url(), Some("token"));
        let error = prepare(&state, &id)
            .await
            .expect_err("old Direct daemon needs one manual bootstrap");
        assert!(error.contains("Direct daemon does not report a build target"));
    }

    #[test]
    fn identity_detects_an_older_binary_even_with_the_same_version() {
        let remote = BuildIdentity {
            version: "1.7.7".into(),
            target: "aarch64-apple-darwin".into(),
            sha256: "a".repeat(64),
        };
        let selected = BuildIdentity {
            version: "1.7.7".into(),
            target: "aarch64-apple-darwin".into(),
            sha256: "b".repeat(64),
        };
        assert!(build_is_out_of_date(&remote, &selected));
        assert!(!build_is_out_of_date(&selected, &selected));
    }
}
