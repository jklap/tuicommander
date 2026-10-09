//! `imgcat`/`imgls`/`divider` PATH shims (color-tools plan, Phase 9).
//!
//! `tuic imgcat`/`tuic imgls`/`tuic divider` (`crates/tuic-cli/src/imgcat.rs`)
//! are already complete, tested, clean-room reimplementations of iTerm2's
//! utility scripts of the same name. The only gap this module closes is
//! discoverability: a user coming from iTerm2 (or a script that assumes
//! these scripts exist, like many dotfiles/READMEs do) expects a bare
//! `imgcat`, not `tuic imgcat`, to be on `PATH`. Each shim is a one-line
//! shell script that `exec`s the resolved `tuic` sidecar with the right
//! subcommand — Unix only, matching `inject_unix_terminal_env`'s own scope.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SHIM_NAMES: [&str; 3] = ["imgcat", "imgls", "divider"];

static SHIM_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The directory to prepend to a spawned PTY's `PATH`, or `None` if the
/// `tuic` sidecar couldn't be resolved (a shim pointing at nothing is worse
/// than no shim at all). Computed once per process and cached — never
/// worth failing or delaying a PTY spawn over.
pub(crate) fn shim_dir() -> Option<&'static Path> {
    SHIM_DIR.get_or_init(build_shims).as_deref()
}

fn build_shims() -> Option<PathBuf> {
    // In a test binary, only proceed if some test has explicitly opted into
    // a real `config_dir()` (via `set_config_dir_override`) — otherwise ANY
    // pre-existing test that merely calls `build_shell_command`/
    // `inject_unix_terminal_env` for unrelated reasons (most don't even know
    // this module exists) would silently write real files into the user's
    // actual, shared config directory the first time it runs in a given test
    // process. Production builds have no `CONFIG_DIR_OVERRIDE` at all (see
    // `config.rs`), so this check compiles away entirely outside `cfg(test)`.
    #[cfg(test)]
    if !crate::config::has_config_dir_override() {
        return None;
    }

    let sidecar_path = crate::tuic_cli::resolve_sidecar_path().ok()?;
    let dir = crate::config::config_dir().join("image-cli-shims");
    std::fs::create_dir_all(&dir).ok()?;
    write_shims(&dir, &sidecar_path);
    Some(dir)
}

/// Separated from `build_shims` so tests can exercise the actual script
/// contents/permissions against a scratch directory without touching the
/// real config dir or depending on a real sidecar binary existing.
fn write_shims(dir: &Path, sidecar_path: &str) {
    let quoted_sidecar = shell_single_quote(sidecar_path);
    for name in SHIM_NAMES {
        let script_path = dir.join(name);
        let script = format!("#!/usr/bin/env bash\nexec {quoted_sidecar} {name} \"$@\"\n");
        // Only rewritten when the content actually changed (e.g. the app
        // updated and the sidecar moved) — avoids needlessly touching the
        // file's mtime/perms on every call.
        if std::fs::read_to_string(&script_path).ok().as_deref() != Some(script.as_str())
            && let Ok(mut f) = std::fs::File::create(&script_path)
        {
            let _ = f.write_all(script.as_bytes());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
        }
    }
}

/// POSIX single-quote a shell argument: wrap in `'...'`, and for any
/// embedded `'` close the quote, emit an escaped literal quote, reopen —
/// the standard `'\''` trick. Safe against every shell metacharacter,
/// unlike `{:?}` Rust-debug-formatting a path (which does not escape `$`,
/// backticks, etc. the way POSIX single-quoting does).
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_a_plain_path_safely() {
        assert_eq!(
            shell_single_quote("/usr/local/bin/tuic"),
            "'/usr/local/bin/tuic'"
        );
    }

    #[test]
    fn quotes_a_path_containing_a_single_quote() {
        assert_eq!(
            shell_single_quote("/Users/o'brien/tuic"),
            r"'/Users/o'\''brien/tuic'"
        );
    }

    #[test]
    fn quotes_a_path_containing_shell_metacharacters_inertly() {
        // $(...) / `...` / $VAR must never be interpreted inside a single-quoted
        // shell string — this is exactly the class of injection {:?} debug-
        // formatting would NOT protect against.
        let quoted = shell_single_quote("/tmp/$(whoami)/`id`/$HOME/tuic");
        assert_eq!(quoted, "'/tmp/$(whoami)/`id`/$HOME/tuic'");
    }

    #[test]
    fn writes_all_three_shims_executable_and_pointing_at_the_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        write_shims(dir.path(), "/opt/tuic/tuic");

        for name in SHIM_NAMES {
            let path = dir.path().join(name);
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("'/opt/tuic/tuic'"));
            assert!(content.contains(&format!("{name} \"$@\"")));

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&path).unwrap().permissions().mode();
                assert_eq!(mode & 0o111, 0o111, "{name} must be executable");
            }
        }
    }

    #[test]
    fn does_not_rewrite_an_unchanged_shim() {
        let dir = tempfile::tempdir().unwrap();
        write_shims(dir.path(), "/opt/tuic/tuic");
        let path = dir.path().join("imgcat");
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        write_shims(dir.path(), "/opt/tuic/tuic");
        let after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after, "unchanged content must not touch the file");
    }

    #[test]
    fn rewrites_a_shim_when_the_sidecar_path_changes() {
        let dir = tempfile::tempdir().unwrap();
        write_shims(dir.path(), "/opt/tuic/tuic");
        write_shims(dir.path(), "/opt/tuic-new/tuic");

        let content = std::fs::read_to_string(dir.path().join("imgcat")).unwrap();
        assert!(content.contains("'/opt/tuic-new/tuic'"));
    }
}
