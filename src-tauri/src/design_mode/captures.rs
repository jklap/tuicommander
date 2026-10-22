//! Shared design-grabs persistence for extension and retained CDP captures.
use std::path::{Path, PathBuf};

pub(super) const MAX_PNG_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn save_png(grab_dir: &Path, bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
        return Ok(None);
    }
    std::fs::create_dir_all(grab_dir).map_err(|error| error.to_string())?;
    let path = grab_dir.join(format!("{}.png", uuid::Uuid::now_v7()));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(Some(path))
}
