//! Locate and keep fresh the `tuic-hook` sidecar's stable install copy.
//!
//! Hook commands written into an agent's settings file (`agent_hook.rs`) need
//! an absolute path that survives the app being moved and in-place updates —
//! unlike a path baked from `current_exe().parent()`, which breaks the moment
//! `TUICommander.app` relocates. So `tuic-hook` gets its own copy under the
//! config dir, refreshed whenever its version drifts from the bundled
//! sidecar, mirroring the version-drift check `tuic_cli.rs` already performs
//! for the `tuic` CLI (reusing its `resolve_sidecar_path_from`/`cli_version`/
//! `version_match` cores directly rather than re-implementing them).

use std::path::PathBuf;

const SIDECAR_PKG_NAME: &str = "tuic-hook";

fn sidecar_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "tuic-hook.exe"
    } else {
        "tuic-hook"
    }
}

/// The stable, install-location-independent path hook commands are written
/// against: `<config_dir>/bin/tuic-hook`.
pub(crate) fn stable_path() -> PathBuf {
    crate::config::config_dir()
        .join("bin")
        .join(sidecar_file_name())
}

fn resolve_bundled_sidecar() -> Result<String, String> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    crate::tuic_cli::resolve_sidecar_path_from(
        std::env::current_exe().ok().as_deref(),
        manifest,
        sidecar_file_name(),
    )
}

/// Ensure the stable copy exists and matches the bundled sidecar's version,
/// re-copying (atomic temp+rename, mode 0755) if either is stale or absent.
/// Called at app startup and again from `apply_at` before an install, so the
/// path is guaranteed valid at the moment it's written into a settings file.
///
/// Best-effort: any failure (bundled sidecar missing in a dev build without
/// `pnpm build:sidecar`, permission denied, …) is logged, not propagated —
/// hook install still proceeds and simply won't find a working binary at the
/// stable path, exactly as if the user hadn't enabled instrumentation yet.
pub(crate) fn ensure_current() {
    let Ok(bundled) = resolve_bundled_sidecar() else {
        tracing::debug!(
            source = "hook_binary",
            "tuic-hook sidecar not found; skipping refresh (expected in a dev build without `pnpm build:sidecar`)"
        );
        return;
    };
    let stable = stable_path();

    let up_to_date = stable.exists()
        && crate::tuic_cli::version_match(
            crate::tuic_cli::cli_version(&stable.to_string_lossy()),
            crate::tuic_cli::cli_version(&bundled),
        );
    if up_to_date {
        return;
    }

    if let Err(e) = install_stable_copy(&bundled, &stable) {
        tracing::warn!(source = "hook_binary", error = %e, "failed to refresh tuic-hook stable copy");
    } else {
        tracing::info!(source = "hook_binary", path = %stable.display(), "tuic-hook stable copy refreshed");
    }
}

/// Copy `bundled` to `stable` atomically (temp file in the same directory,
/// then rename) and mark it executable. No elevation needed: this directory
/// is ours (`<config_dir>/bin/`), always user-writable, unlike `tuic_cli`'s
/// system PATH install target.
fn install_stable_copy(bundled: &str, stable: &std::path::Path) -> Result<(), String> {
    let dir = stable
        .parent()
        .ok_or_else(|| "stable path has no parent directory".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let temp = dir.join(format!(".{SIDECAR_PKG_NAME}.tmp.{}", std::process::id()));
    std::fs::copy(bundled, &temp).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("copy {bundled} -> {}: {e}", temp.display())
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755)).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("chmod {}: {e}", temp.display())
        })?;
    }

    std::fs::rename(&temp, stable).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("rename {} -> {}: {e}", temp.display(), stable.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_file_name_matches_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(sidecar_file_name(), "tuic-hook.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(sidecar_file_name(), "tuic-hook");
    }

    #[test]
    fn stable_path_lives_under_config_dir_bin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let path = stable_path();
        assert_eq!(path, dir.path().join("bin").join(sidecar_file_name()));
    }

    /// Write an executable shell script that prints `stdout` and exits 0.
    #[cfg(unix)]
    fn write_fake_sidecar(path: &std::path::Path, stdout: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, format!("#!/bin/sh\nprintf '%s' \"{stdout}\"\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn install_stable_copy_creates_parent_dirs_and_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let src_dir = tempfile::tempdir().expect("src dir");
        let dst_dir = tempfile::tempdir().expect("dst dir");
        let bundled = src_dir.path().join("tuic-hook-fake");
        write_fake_sidecar(&bundled, "tuic-hook 9.9.9");
        let stable = dst_dir.path().join("nested/bin/tuic-hook");

        install_stable_copy(bundled.to_str().unwrap(), &stable).expect("install must succeed");

        assert!(stable.exists());
        let mode = std::fs::metadata(&stable).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        let output = std::process::Command::new(&stable).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "tuic-hook 9.9.9");
    }

    #[test]
    #[cfg(unix)]
    fn install_stable_copy_overwrites_an_existing_stale_copy() {
        let src_dir = tempfile::tempdir().expect("src dir");
        let dst_dir = tempfile::tempdir().expect("dst dir");
        let bundled = src_dir.path().join("tuic-hook-fake");
        let stable = dst_dir.path().join("tuic-hook");

        write_fake_sidecar(&bundled, "tuic-hook 1.0.0");
        install_stable_copy(bundled.to_str().unwrap(), &stable).unwrap();

        write_fake_sidecar(&bundled, "tuic-hook 2.0.0");
        install_stable_copy(bundled.to_str().unwrap(), &stable).unwrap();

        let output = std::process::Command::new(&stable).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "tuic-hook 2.0.0");
    }

    #[test]
    fn install_stable_copy_errs_when_bundled_sidecar_is_missing() {
        let dst_dir = tempfile::tempdir().expect("dst dir");
        let stable = dst_dir.path().join("tuic-hook");
        let err = install_stable_copy("/no/such/bundled/sidecar", &stable)
            .expect_err("must error, not panic, on a missing source");
        assert!(err.contains("copy"));
        assert!(!stable.exists());
    }
}
