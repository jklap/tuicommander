//! Shared instance namespaces and HTTP framing, independent of sync/async I/O.

pub mod app_instance;
pub mod http;

/// Backend deadlines shared with the CLI and stdio bridge.
pub const SECRET_FORM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
pub const SECRET_CHILD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Allow private entry/consent and execution to finish before transport expiry.
pub fn secret_response_timeout(action: &str) -> Option<std::time::Duration> {
    let margin = std::time::Duration::from_secs(5);
    match action {
        "request" => Some(SECRET_FORM_TIMEOUT + margin),
        "run" => Some(SECRET_FORM_TIMEOUT + SECRET_CHILD_TIMEOUT + margin),
        _ => None,
    }
}

use app_instance::AppInstance;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Resolve the server's Unix endpoint, including its short named-instance path.
pub fn socket_path(instance: &AppInstance, config_dir: &Path, temp_dir: &Path) -> PathBuf {
    match instance.named_id() {
        Some(id) => named_socket_path(id, temp_dir),
        None => config_dir.join("mcp.sock"),
    }
}

/// Preserve the deployed short socket name (first eight SHA-256 bytes).
pub fn named_socket_path(id: &str, temp_dir: &Path) -> PathBuf {
    let digest = Sha256::digest(id.as_bytes());
    let mut short_id = String::with_capacity(16);
    for byte in &digest[..8] {
        write!(short_id, "{byte:02x}").expect("writing to a String cannot fail");
    }
    temp_dir.join(format!("tuic-mcp-{short_id}.sock"))
}

/// Windows IPC endpoint. Kept identical to the server's existing pipe contract.
pub const PIPE_NAME: &str = r"\\.\pipe\tuicommander-mcp";
