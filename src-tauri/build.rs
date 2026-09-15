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
        tauri_build::build();

        // tauri_build embeds the Windows application manifest through
        // `cargo:rustc-link-arg-bins`, which reaches binaries and not test
        // targets. A test binary therefore binds comctl32 5.82 from System32,
        // which exports none of the four version-6 symbols tao and the dialog
        // plugin import, and the loader kills it with STATUS_ENTRYPOINT_NOT_FOUND
        // before any test runs. Give the test binaries the same manifest.
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("windows-tests.manifest");
            println!("cargo:rerun-if-changed={}", manifest.display());
            println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
                manifest.display()
            );
        }
    }
}
