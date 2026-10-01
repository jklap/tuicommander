#[allow(dead_code)]
mod build_sidecars;

fn main() {
    if cfg!(feature = "desktop") && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/native_notification.m")
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .compile("tuic_native_notification");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rerun-if-changed=src/native_notification.m");
    }
    // Expose git commit hash as BUILD_GIT_HASH for version checks (PWA update detection).
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=BUILD_GIT_HASH={}", hash.trim());

    // Expose target triple for sidecar path resolution at runtime
    println!(
        "cargo:rustc-env=TUIC_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );

    #[cfg(feature = "desktop")]
    {
        // One Windows manifest for every artifact we link, ours instead of the
        // tauri one. tauri_build embeds its copy through
        // `cargo:rustc-link-arg-bins`, which reaches binaries and not the unit
        // tests of the lib. Those test binaries then bind comctl32 5.82 from
        // System32, which does not export TaskDialogIndirect, and the loader
        // kills them with STATUS_ENTRYPOINT_NOT_FOUND before a test runs.
        //
        // Only `cargo:rustc-link-arg` without a suffix reaches the lib test
        // binary: `-tests` covers the `tests/` targets alone (measured). That
        // directive also reaches the binaries, and a second RT_MANIFEST there
        // is a duplicate resource the linker rejects, so the tauri manifest has
        // to go. Its content is this file, so the binaries keep what they had.
        let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
        skip_placeholder_sidecars();
        let attributes = tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        tauri_build::try_build(attributes).expect("failed to run tauri-build");

        if windows {
            let manifest =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app.manifest");
            println!("cargo:rerun-if-changed={}", manifest.display());
            println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
            println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        }
    }
}

/// tauri-build copies every `externalBin` source over `target/<profile>/<name>`
/// with `remove_file` + `fs::copy`. `tuic` and `tuic-bridge` are also workspace
/// binaries that cargo uplifts to that same path, so a placeholder source (the
/// empty CI/box stub, or the tracked non-executable blob) replaces the freshly
/// built binary whenever this script runs after the link, and the tests that
/// spawn `CARGO_BIN_EXE_tuic*` fail with EACCES (#1325-394f).
///
/// An `externalBin` entry whose source is empty or not executable is a
/// placeholder: leave it out of `externalBin` for this run. Real sidecars come
/// from `pnpm build:sidecar`, which writes executable files, so `make dev`
/// keeps its copy. An explicit `TAURI_CONFIG` (the tauri CLI `--config`) is
/// left alone.
#[cfg(feature = "desktop")]
fn skip_placeholder_sidecars() {
    use build_sidecars::{FileState, placeholder_entries};
    use std::collections::HashMap;

    println!("cargo:rerun-if-changed=binaries");
    if std::env::var_os("TAURI_CONFIG").is_some() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(external_bin) = std::fs::read_to_string(root.join("tauri.conf.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|conf| {
            conf["bundle"]["externalBin"].as_array().map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
        })
    else {
        return;
    };
    let triple = std::env::var("TARGET").unwrap_or_default();
    let ext = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        ".exe"
    } else {
        ""
    };
    let Ok(dir) = std::fs::read_dir(root.join("binaries")) else {
        return;
    };
    let files: HashMap<String, FileState> = dir
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let state = FileState {
                len: meta.len(),
                executable: is_executable(&meta),
            };
            Some((entry.file_name().to_string_lossy().into_owned(), state))
        })
        .collect();
    let placeholders = placeholder_entries(&external_bin, &triple, ext, &files);
    if placeholders.is_empty() {
        return;
    }
    let real: Vec<&String> = external_bin
        .iter()
        .filter(|entry| !placeholders.contains(&entry.as_str()))
        .collect();
    let patch = serde_json::json!({ "bundle": { "externalBin": real } });
    // SAFETY: a build script is single-threaded at this point.
    unsafe { std::env::set_var("TAURI_CONFIG", patch.to_string()) };
}

#[cfg(all(feature = "desktop", unix))]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(all(feature = "desktop", not(unix)))]
fn is_executable(_: &std::fs::Metadata) -> bool {
    true
}
