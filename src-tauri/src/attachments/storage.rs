//! The only attachment filesystem boundary; repository paths never enter here.
use std::path::{Path, PathBuf};

fn leaf(kind: &str, id: &str) -> Result<String, String> {
    if !["pty", "acp"].contains(&kind)
        || id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid attachment session id".into());
    }
    Ok(format!("{kind}-{id}"))
}

fn checked_directory(path: &Path, create: bool) -> Result<(), String> {
    if create && !path.exists() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("create attachment directory: {error}")),
        }
    }
    let meta = path.symlink_metadata().map_err(|e| e.to_string())?;
    if !meta.file_type().is_dir() {
        return Err("attachment directory is not a real directory".into());
    }
    Ok(())
}

fn root(data_dir: &Path, create: bool) -> Result<PathBuf, String> {
    let data = std::fs::canonicalize(data_dir).map_err(|e| e.to_string())?;
    let root = data.join("attachments");
    checked_directory(&root, create)?;
    Ok(root)
}

pub(crate) fn prepare(data_dir: &Path, kind: &str, id: &str) -> Result<PathBuf, String> {
    let name = leaf(kind, id)?;
    let path = root(data_dir, true)?.join(name);
    checked_directory(&path, true)?;
    Ok(path)
}

pub(crate) fn remove(data_dir: &Path, kind: &str, id: &str) {
    let Ok(name) = leaf(kind, id) else { return };
    let Ok(root) = root(data_dir, false) else {
        return;
    };
    let path = root.join(name);
    // Never follow a root or session symlink. remove_dir_all itself does not
    // follow symlinks inside a real session directory either.
    if checked_directory(&path, false).is_err() {
        return;
    }
    if let Err(error) = std::fs::remove_dir_all(&path) {
        tracing::warn!(path = %path.display(), %error, "Could not remove session attachments");
    }
}

/// Called before any sessions exist. Only our names beneath our root qualify.
pub(crate) fn cleanup_orphans(data_dir: &Path) {
    let Ok(root) = root(data_dir, false) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some((kind, id)) = name.split_once('-') else {
            continue;
        };
        remove(data_dir, kind, id);
    }
}
