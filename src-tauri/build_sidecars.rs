//! Placeholder detection for the `externalBin` sidecars, shared by `build.rs`
//! and (under `cfg(test)`) the lib so `cargo nextest` can run its tests: build
//! scripts have no test harness of their own.

use std::collections::HashMap;

/// Size and executable bit of a file in `binaries/`.
#[derive(Clone, Copy)]
pub struct FileState {
    pub len: u64,
    pub executable: bool,
}

/// The `externalBin` entries (e.g. `binaries/tuic`) whose source file
/// `<entry>-<triple><ext>` is a placeholder: empty or not executable. Files in
/// `binaries/` that are not an `externalBin` entry never matter, and a missing
/// source is left for tauri-build to report.
pub fn placeholder_entries<'a>(
    external_bin: &'a [String],
    triple: &str,
    ext: &str,
    files: &HashMap<String, FileState>,
) -> Vec<&'a str> {
    external_bin
        .iter()
        .filter(|entry| {
            let name = entry.rsplit('/').next().unwrap_or(entry);
            files
                .get(&format!("{name}-{triple}{ext}"))
                .is_some_and(|file| file.len == 0 || !file.executable)
        })
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRIPLE: &str = "aarch64-apple-darwin";

    fn entries() -> Vec<String> {
        vec!["binaries/tuic-bridge".into(), "binaries/tuic".into()]
    }

    fn files(list: &[(&str, u64, bool)]) -> HashMap<String, FileState> {
        list.iter()
            .map(|(name, len, executable)| {
                (
                    name.to_string(),
                    FileState {
                        len: *len,
                        executable: *executable,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn real_executable_sidecars_are_not_placeholders() {
        let files = files(&[
            ("tuic-aarch64-apple-darwin", 10, true),
            ("tuic-bridge-aarch64-apple-darwin", 10, true),
        ]);
        assert!(placeholder_entries(&entries(), TRIPLE, "", &files).is_empty());
    }

    #[test]
    fn an_empty_stub_is_a_placeholder() {
        let files = files(&[
            ("tuic-aarch64-apple-darwin", 0, true),
            ("tuic-bridge-aarch64-apple-darwin", 10, true),
        ]);
        assert_eq!(
            placeholder_entries(&entries(), TRIPLE, "", &files),
            ["binaries/tuic"]
        );
    }

    #[test]
    fn a_non_executable_blob_is_a_placeholder() {
        let files = files(&[
            ("tuic-aarch64-apple-darwin", 1275296, false),
            ("tuic-bridge-aarch64-apple-darwin", 10, true),
        ]);
        assert_eq!(
            placeholder_entries(&entries(), TRIPLE, "", &files),
            ["binaries/tuic"]
        );
    }

    /// Catches the first version of the fix: it scanned every file ending in
    /// `-<triple>`, so a stray empty `tuic-remote-<triple>` dropped the real
    /// `tuic` and `tuic-bridge` from `externalBin` as well.
    #[test]
    fn a_stray_file_outside_external_bin_is_ignored() {
        let files = files(&[
            ("tuic-aarch64-apple-darwin", 10, true),
            ("tuic-bridge-aarch64-apple-darwin", 10, true),
            ("tuic-remote-aarch64-apple-darwin", 0, false),
        ]);
        assert!(placeholder_entries(&entries(), TRIPLE, "", &files).is_empty());
    }

    #[test]
    fn a_missing_source_is_left_to_tauri_build() {
        let files = files(&[("tuic-bridge-aarch64-apple-darwin", 10, true)]);
        assert!(placeholder_entries(&entries(), TRIPLE, "", &files).is_empty());
    }

    #[test]
    fn windows_sources_carry_the_exe_suffix() {
        let triple = "x86_64-pc-windows-msvc";
        let files = files(&[
            ("tuic-x86_64-pc-windows-msvc.exe", 0, true),
            ("tuic-bridge-x86_64-pc-windows-msvc.exe", 10, true),
            ("tuic-x86_64-pc-windows-msvc", 10, true),
        ]);
        assert_eq!(
            placeholder_entries(&entries(), triple, ".exe", &files),
            ["binaries/tuic"]
        );
    }
}
