fn main() {
    // whisper-rs-sys (ggml-metal) uses @available() which emits a call to
    // ___isPlatformVersionAtLeast from libclang_rt. Rust's -nodefaultlibs
    // strips it, so we must link it explicitly on macOS.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && let Ok(out) = std::process::Command::new("xcrun")
            .args([
                "--sdk",
                "macosx",
                "clang",
                "--print-file-name",
                "libclang_rt.osx.a",
            ])
            .output()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(dir) = std::path::Path::new(&path).parent() {
            println!("cargo:rustc-link-search=native={}", dir.display());
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
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
