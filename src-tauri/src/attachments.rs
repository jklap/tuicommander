//! Session-owned binary attachment uploads.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::AppState;

type UploadError = (StatusCode, Json<serde_json::Value>);

#[derive(Deserialize)]
pub(crate) struct UploadQuery {
    kind: String,
    id: String,
    name: String,
}

#[derive(Serialize)]
pub(crate) struct AttachmentReceipt {
    pub(crate) path: String,
    pub(crate) size: u64,
}

fn error(status: StatusCode, message: &str) -> UploadError {
    (status, Json(serde_json::json!({ "error": message })))
}

fn session_cwd(state: &AppState, query: &UploadQuery) -> Result<PathBuf, UploadError> {
    if query.kind == "acp" {
        let id = serde_json::from_value(serde_json::json!(query.id))
            .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid ACP connection id"))?;
        return state
            .acp
            .connection_root(id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "ACP connection not found"));
    }
    if query.kind != "pty" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "unsupported attachment target",
        ));
    }
    let session = state
        .session_maps
        .sessions
        .get(&query.id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "session not found"))?;
    let cwd = session
        .value()
        .lock()
        .cwd
        .clone()
        .ok_or_else(|| error(StatusCode::CONFLICT, "session has no working directory"))?;
    Ok(PathBuf::from(cwd))
}

fn safe_name(name: &str) -> Result<String, UploadError> {
    let name: String = name
        .chars()
        .take(120)
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        return Err(error(StatusCode::BAD_REQUEST, "invalid attachment name"));
    }
    Ok(name)
}

fn exclude_from_git(cwd: &Path) -> Result<(), UploadError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--git-path", "info/exclude"])
        .output()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "could not run git"))?;
    if !output.status.success() {
        return Ok(()); // The global ACP root may not be a Git repository.
    }
    let prefix = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-prefix"])
        .output()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not inspect Git root",
            )
        })?;
    if !prefix.status.success() {
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not inspect Git root",
        ));
    }
    let pattern = format!(
        "{}.tuic/attachments/",
        String::from_utf8_lossy(&prefix.stdout).trim()
    );
    let raw = String::from_utf8_lossy(&output.stdout);
    let exclude = PathBuf::from(raw.trim());
    let exclude = if exclude.is_absolute() {
        exclude
    } else {
        cwd.join(exclude)
    };
    let old = std::fs::read_to_string(&exclude).unwrap_or_default();
    if !old.lines().any(|line| line == pattern) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&exclude)
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not exclude attachments from Git",
                )
            })?;
        if !old.is_empty() && !old.ends_with('\n') {
            file.write_all(b"\n").map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not update Git exclude",
                )
            })?;
        }
        file.write_all(format!("{pattern}\n").as_bytes())
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not update Git exclude",
                )
            })?;
    }
    Ok(())
}

fn prepare_directory(cwd: &Path) -> Result<PathBuf, UploadError> {
    let root = std::fs::canonicalize(cwd)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "working directory does not exist"))?;
    let tuic = root.join(".tuic");
    let attachments = tuic.join("attachments");
    for dir in [&tuic, &attachments] {
        if dir
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "attachment directory is a symlink",
            ));
        }
        std::fs::create_dir_all(dir).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create attachment directory",
            )
        })?;
    }
    let actual = std::fs::canonicalize(&attachments).map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not inspect attachment directory",
        )
    })?;
    if !actual.starts_with(&root) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "attachment directory escapes working directory",
        ));
    }
    exclude_from_git(&root)?;
    Ok(actual)
}

/// Best-effort retention at a session boundary. Only files with our generated
/// timestamp prefix are eligible; arbitrary files in `.tuic` belong to users.
pub(crate) fn cleanup_old(cwd: &Path, retention_days: u32) {
    let directory = cwd.join(".tuic/attachments");
    if !directory
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_dir())
    {
        return;
    }
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            u64::from(retention_days) * 86_400,
        ))
        .unwrap_or(std::time::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some((stamp, _)) = name.split_once('-') else {
            continue;
        };
        if stamp.is_empty() || !stamp.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        if !meta.file_type().is_file() || meta.modified().is_ok_and(|modified| modified >= cutoff) {
            continue;
        }
        if let Err(error) = std::fs::remove_file(entry.path()) {
            tracing::warn!(path = %entry.path().display(), %error, "Could not remove old attachment");
        }
    }
}

pub(crate) async fn upload_http(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<AttachmentReceipt>, UploadError> {
    upload(state, query, headers, body).await.map(Json)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn upload_attachment(
    state: tauri::State<'_, Arc<AppState>>,
    kind: String,
    id: String,
    name: String,
    bytes: Vec<u8>,
) -> Result<AttachmentReceipt, String> {
    // Desktop IPC carries a byte array. The HTTP peer streams the same bytes
    // through the shared writer without encoding them as JSON base64.
    upload(
        Arc::clone(&state),
        UploadQuery { kind, id, name },
        HeaderMap::new(),
        Body::from(bytes),
    )
    .await
    .map_err(|(_, json)| {
        json.0["error"]
            .as_str()
            .unwrap_or("attachment upload failed")
            .to_owned()
    })
}

async fn upload(
    state: Arc<AppState>,
    query: UploadQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<AttachmentReceipt, UploadError> {
    let cwd = session_cwd(&state, &query)?;
    let name = safe_name(&query.name)?;
    let max_bytes = state.config.read().attachment_max_bytes;
    if max_bytes == 0 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "attachment limit must be positive",
        ));
    }
    if headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > max_bytes)
    {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "attachment exceeds configured limit",
        ));
    }
    let directory = tokio::task::spawn_blocking(move || prepare_directory(&cwd))
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not prepare attachment directory",
            )
        })??;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = directory.join(format!("{timestamp}-{name}"));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not create attachment",
                )
            })?;
        let mut stream = body.into_data_stream();
        let mut size = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| error(StatusCode::BAD_REQUEST, "attachment upload failed"))?;
            size = size.checked_add(chunk.len() as u64).ok_or_else(|| {
                error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "attachment exceeds configured limit",
                )
            })?;
            if size > max_bytes {
                return Err(error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "attachment exceeds configured limit",
                ));
            }
            file.write_all(&chunk).await.map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not write attachment",
                )
            })?;
        }
        if size == 0 {
            return Err(error(StatusCode::BAD_REQUEST, "empty attachment"));
        }
        file.flush().await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not flush attachment",
            )
        })?;
        Ok(size)
    }
    .await;
    match result {
        Ok(size) => Ok(AttachmentReceipt {
            path: path.to_string_lossy().into_owned(),
            size,
        }),
        Err(problem) => {
            let _ = tokio::fs::remove_file(&path).await;
            Err(problem)
        }
    }
}
