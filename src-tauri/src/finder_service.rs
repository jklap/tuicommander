//! macOS Finder Service — "New TUICommander Tab Here".
//!
//! Ships as a hand-authored Automator `.workflow` bundle under
//! `services/` (verified to run correctly via `automator -i`, both for a
//! single item and a multi-item selection, before being committed). Installing
//! it is just copying that bundle into `~/Library/Services/` — the standard OS
//! mechanism for a Finder-menu Service — which needs no Cocoa
//! `NSApplication.servicesProvider` code in this app. The bundle's own shell
//! action is a static bash script (`services/*/Contents/document.wflow`) that
//! independently hardcodes `/usr/local/bin/tuic` with its own `command -v
//! tuic` PATH fallback — it cannot call into this compiled binary, so it does
//! NOT actually go through `tuic_cli::resolve_install_path`; that function's
//! macOS branch happens to resolve to the same path today, but nothing keeps
//! the two in sync if it ever changes. The installed `tuic`'s `open-here`
//! subcommand then fires the `tuic://open-terminal` deep link the frontend's
//! placement ladder consumes.
//!
//! Modelled directly on `tuic_cli.rs`'s install/uninstall/status/dismiss shape.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Bundle directory name — must match `services/<name>` in the repo and the
/// `resources` entry in `tauri.conf.json`.
const BUNDLE_NAME: &str = "New TUICommander Tab Here.workflow";

const DISMISS_MARKER: &str = ".finder-service-prompt-dismissed";

#[derive(Serialize)]
pub(crate) struct FinderServiceStatus {
    installed: bool,
    prompt_dismissed: bool,
}

/// Check Finder Service installation status.
#[tauri::command]
pub(crate) fn get_finder_service_status(app: tauri::AppHandle) -> FinderServiceStatus {
    let _ = &app;
    let prompt_dismissed = crate::config::config_dir().join(DISMISS_MARKER).exists();

    #[cfg(target_os = "macos")]
    let installed = services_dir().map(|dir| status_in(&dir)).unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    let installed = false;

    FinderServiceStatus {
        installed,
        prompt_dismissed,
    }
}

/// Install the Finder Service into `~/Library/Services/`.
#[tauri::command]
pub(crate) fn install_finder_service(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Finder services are only available on macOS".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        let source = resolve_bundle_source(&app)?;
        let target_dir =
            services_dir().ok_or_else(|| "Could not resolve ~/Library/Services".to_string())?;
        let signed = install_into(&source, &target_dir)?;
        refresh_services_menu();
        tracing::info!(
            source = "finder_service",
            signed,
            "Finder Service installed"
        );
        Ok(())
    }
}

/// Remove the Finder Service from `~/Library/Services/`, if present.
#[tauri::command]
pub(crate) fn uninstall_finder_service() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let target_dir =
            services_dir().ok_or_else(|| "Could not resolve ~/Library/Services".to_string())?;
        uninstall_from(&target_dir)?;
        refresh_services_menu();
        tracing::info!(source = "finder_service", "Finder Service uninstalled");
        Ok(())
    }
}

/// Dismiss the first-run Finder Service install prompt (persisted to disk).
#[tauri::command]
pub(crate) fn dismiss_finder_service_prompt() {
    let marker = crate::config::config_dir().join(DISMISS_MARKER);
    let _ = std::fs::write(&marker, "");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn services_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("Library").join("Services"))
}

/// Locate the bundled `.workflow` via Tauri's resource resolver.
#[cfg(target_os = "macos")]
fn resolve_bundle_source(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let resolved = app
        .path()
        .resolve(
            format!("services/{BUNDLE_NAME}"),
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve bundled Finder service: {e}"))?;
    if !resolved.exists() {
        return Err(format!(
            "Bundled Finder service not found at {} — packaging is missing the `services` resource",
            resolved.display()
        ));
    }
    Ok(resolved)
}

/// Ask Launch Services to re-scan Services providers. The Finder Services menu
/// is cached, so an install/uninstall would otherwise need a Finder restart to
/// show up. Best-effort: a missing/failing `pbs` never fails the install —
/// the service is still on disk and Finder will pick it up eventually.
#[cfg(target_os = "macos")]
fn refresh_services_menu() {
    let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
        .arg("-flush")
        .status();
}

/// True when the bundle is present under `services_dir`. Path-injected so
/// tests never touch the real `~/Library/Services`.
fn status_in(services_dir: &Path) -> bool {
    services_dir.join(BUNDLE_NAME).exists()
}

/// Copy `source` (the bundle directory) into `target_dir/<BUNDLE_NAME>`,
/// replacing any existing copy. Path-injected for tests. Returns whether the
/// copy was successfully ad-hoc signed afterward (see [`ad_hoc_sign`]) — a
/// `false` here means the bundle is installed but may still hit the
/// Gatekeeper rejection this signing step exists to prevent, which is worth
/// surfacing to the caller rather than only a buried log line.
fn install_into(source: &Path, target_dir: &Path) -> Result<bool, String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create {}: {e}", target_dir.display()))?;
    let dest = target_dir.join(BUNDLE_NAME);
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| {
            format!(
                "Failed to remove existing service at {}: {e}",
                dest.display()
            )
        })?;
    }
    copy_dir_recursive(source, &dest)?;
    Ok(ad_hoc_sign(&dest))
}

/// Absolute path to `codesign` — this function only ever runs behind
/// `#[cfg(target_os = "macos")]` at the real call site, where this path is
/// part of the OS itself, never PATH-dependent. Mirrors `refresh_services_menu`
/// in this same file, which hardcodes `pbs`'s path for the same reason.
const CODESIGN_PATH: &str = "/usr/bin/codesign";

/// Ad-hoc code-sign the installed bundle. A hand-copied `.workflow` bundle
/// carries no signature at all (`codesign -dv` reports "code object is not
/// signed at all"), and with Gatekeeper assessments enabled this is the
/// documented cause of Finder's generic "The Service cannot be run because
/// it is not configured correctly" dialog for third-party Automator Services
/// that run a shell script — `automator -i` (this bundle's own verification
/// method, see the module doc comment) does not go through the same
/// Gatekeeper-gated Services-menu dispatch path, so it never caught this.
///
/// `--deep` is required here, not just a defensive extra: verified empirically
/// that plain `codesign --force --sign -` on this exact bundle fails outright
/// ("code object is not signed at all — In subcomponent:
/// .../Contents/document.wflow", exit 1) — `.workflow`'s `document.wflow`
/// apparently counts as a nested component codesign wants sealed, even though
/// it's a data file, not executable code. Apple's TN2206 guidance is that
/// developer-authored signing should generally avoid `--deep` (it can produce
/// an invalid/incorrectly scoped signature for nested *code*), but that
/// doesn't hold for this bundle shape — don't "fix" this by removing it again
/// without re-testing a fresh copy the same way.
///
/// Best-effort: never fails the install — `codesign` may be absent (non-macOS
/// test hosts) or a fake test-fixture bundle's plist may not parse, and the
/// bundle already works when copied by hand and re-signed manually, so a
/// failure here just leaves it as unsigned as it always was. Returns whether
/// signing succeeded so the caller can log it alongside the install itself,
/// rather than this warning being the only trace of a real, silent failure.
fn ad_hoc_sign(bundle: &Path) -> bool {
    match std::process::Command::new(CODESIGN_PATH)
        .args(["--force", "--deep", "--sign", "-"])
        .arg(bundle)
        .output()
    {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            tracing::warn!(
                source = "finder_service",
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Ad-hoc signing of Finder Service bundle failed"
            );
            false
        }
        Err(e) => {
            tracing::warn!(source = "finder_service", error = %e, "Could not run codesign");
            false
        }
    }
}

/// Remove the bundle from `target_dir`, if present. Not an error when absent —
/// uninstalling something already gone is a no-op success, same convention as
/// `tuic_cli::uninstall_cli`.
fn uninstall_from(target_dir: &Path) -> Result<(), String> {
    let dest = target_dir.join(BUNDLE_NAME);
    if !dest.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dest).map_err(|e| format!("Failed to remove {}: {e}", dest.display()))
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;
    for entry in std::fs::read_dir(source)
        .map_err(|e| format!("Failed to read {}: {e}", source.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)
                .map_err(|e| format!("Failed to copy {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fake_bundle(root: &Path) -> PathBuf {
        let bundle = root.join(BUNDLE_NAME);
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(contents.join("Info.plist"), b"fake-info-plist").unwrap();
        std::fs::write(contents.join("document.wflow"), b"fake-document-wflow").unwrap();
        bundle
    }

    #[test]
    fn status_in_is_false_before_install_and_true_after() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();

        assert!(!status_in(target_root.path()));
        install_into(&source, target_root.path()).unwrap();
        assert!(status_in(target_root.path()));
    }

    #[test]
    fn install_into_copies_the_full_bundle_tree() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();

        install_into(&source, target_root.path()).unwrap();

        let dest = target_root.path().join(BUNDLE_NAME);
        assert_eq!(
            std::fs::read(dest.join("Contents").join("Info.plist")).unwrap(),
            b"fake-info-plist"
        );
        assert_eq!(
            std::fs::read(dest.join("Contents").join("document.wflow")).unwrap(),
            b"fake-document-wflow"
        );
    }

    /// `ad_hoc_sign` is best-effort: a bundle whose `Info.plist` isn't real
    /// plist XML (like this test fixture) makes `codesign` fail outright
    /// (confirmed empirically: `errSecCoreFoundationUnknown`, exit 1), and
    /// `install_into` must still report success — a signing failure must
    /// never turn a working install into a reported failure.
    #[test]
    #[cfg(target_os = "macos")]
    fn install_into_succeeds_even_when_the_bundle_cannot_be_code_signed() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();

        let dest = target_root.path().join(BUNDLE_NAME);
        assert!(
            !std::process::Command::new(CODESIGN_PATH)
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&source)
                .output()
                .unwrap()
                .status
                .success(),
            "test fixture assumption broken: codesign now accepts a fake plist"
        );

        assert!(install_into(&source, target_root.path()).is_ok());
        assert!(dest.join("Contents").join("Info.plist").exists());
    }

    #[test]
    fn install_into_replaces_an_existing_copy_rather_than_erroring() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();

        install_into(&source, target_root.path()).unwrap();
        // Mutate the installed copy, then reinstall — it must be replaced, not merged with.
        let dest_contents = target_root.path().join(BUNDLE_NAME).join("Contents");
        std::fs::write(dest_contents.join("stray-leftover-file"), b"leftover").unwrap();

        install_into(&source, target_root.path()).unwrap();

        assert!(!dest_contents.join("stray-leftover-file").exists());
        assert!(dest_contents.join("Info.plist").exists());
    }

    #[test]
    fn uninstall_from_removes_the_bundle() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();
        install_into(&source, target_root.path()).unwrap();

        uninstall_from(target_root.path()).unwrap();

        assert!(!status_in(target_root.path()));
    }

    #[test]
    fn uninstall_from_when_absent_is_a_no_op_success() {
        let target_root = tempfile::tempdir().unwrap();
        assert!(uninstall_from(target_root.path()).is_ok());
    }

    /// Well-formedness check on the REAL shipped bundle (not a fake one) —
    /// the plist internals are hand-authored, not Automator-generated, so this
    /// is the automated half of the risk called out in the feature plan. The
    /// functional half (the workflow actually runs and calls `tuic open-here`
    /// with every selected path) was verified manually via
    /// `automator -i <path> "services/New TUICommander Tab Here.workflow"`
    /// for a single item, multiple items, and a plain file — not repeatable
    /// here since it depends on the `automator` CLI and isn't hermetic.
    #[test]
    #[cfg(target_os = "macos")]
    fn shipped_bundle_plists_are_well_formed() {
        let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("services")
            .join(BUNDLE_NAME)
            .join("Contents");

        for file in ["Info.plist", "document.wflow"] {
            let path = bundle.join(file);
            assert!(path.exists(), "missing {}", path.display());
            let status = std::process::Command::new("plutil")
                .arg("-lint")
                .arg(&path)
                .status()
                .expect("plutil must be available on macOS");
            assert!(status.success(), "{} failed plutil -lint", path.display());
        }
    }

    #[test]
    fn install_into_creates_the_target_dir_if_missing() {
        let source_root = tempfile::tempdir().unwrap();
        let source = write_fake_bundle(source_root.path());
        let target_root = tempfile::tempdir().unwrap();
        let nested_target = target_root.path().join("Library").join("Services");
        assert!(!nested_target.exists());

        install_into(&source, &nested_target).unwrap();

        assert!(status_in(&nested_target));
    }
}
