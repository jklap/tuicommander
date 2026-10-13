//! How a host path is spelled: absolute on any platform, and the portable
//! `/`-separated form that git and the rest of TUIC read back.

/// Is this string an absolute path on *any* platform TUIC runs on?
///
/// `Path::is_absolute` answers for the host only, and the validators that call
/// this decide whether a path may escape a boundary. Judging a foreign shape
/// as relative is the dangerous half of that: on Windows `Path::join` replaces
/// the root when the joined path has one, so a repo path joined with
/// `/etc/passwd` lands at the root of the repo's drive rather than inside the
/// repo. So each shape the other platform uses gets an explicit string check —
/// `C:\…` and `\\…` do not parse as absolute on unix, and a leading `/` does
/// not parse as absolute on Windows, where a path without a drive letter is
/// merely rooted.
pub fn is_absolute_on_any_platform(path: &str) -> bool {
    let drive_rooted = path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && matches!(path.get(1..3), Some(":/") | Some(":\\"));
    std::path::Path::new(path).is_absolute()
        || drive_rooted
        || path.starts_with("\\\\")
        || path.starts_with('/')
}

/// Rewrite a host path string the way everything outside the Windows API
/// spells one: no `\\?\` prefix, `/` separators.
///
/// Two consumers need it and they need the same answer. `git` is one — the gix
/// and CLI adapters behind `GitReads` have to return identical bytes, and on
/// Windows `fs::canonicalize` does not oblige: it returns a verbatim `\\?\`
/// path with `\`, while `git worktree list` prints `C:/Users/…`, so a consumer
/// comparing the two finds no match at all. A git *config value* is the other:
/// there `\` starts an escape sequence, so a Windows path written verbatim
/// reaches git mangled. `/` is also the separator the rest of TUIC carries in a
/// path string (see `fs::DirEntry::path` in the app crate).
///
/// Only Windows paths are rewritten: `\` is a legal character in a unix file
/// name, so the same rewrite there would corrupt paths rather than normalise
/// them. The rewrite itself is in [`windows_portable_spelling`], which is
/// compiled and tested on every platform.
pub fn portable_spelling(path: &str) -> String {
    if cfg!(windows) {
        windows_portable_spelling(path)
    } else {
        path.to_string()
    }
}

/// Do two host path strings name the same filesystem location?
///
/// The strings `git` prints are resolved: `git worktree list` reports
/// `/private/var/…` on macOS where the user (or `$TMPDIR`) spells the same
/// directory `/var/…` or through any other symlinked folder. A comparison of
/// spellings alone then refuses a path that is exactly the registered one. So
/// equal [`portable_spelling`]s match outright, and otherwise both sides are
/// resolved with [`resolved_spelling`] and compared again.
///
/// Resolution never aliases two distinct locations: two paths that resolve to
/// the same canonical string are the same directory entry. A literal `\` in a
/// unix file name stays a file-name character, exactly as in
/// [`portable_spelling`].
pub fn same_path(a: &str, b: &str) -> bool {
    if portable_spelling(a) == portable_spelling(b) {
        return true;
    }
    match (resolved_spelling(a), resolved_spelling(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The canonical, [`portable_spelling`] form of `path`, resolving symlinks.
///
/// A path that does not exist (a checkout whose directory was deleted while
/// git still lists it) is resolved through its deepest existing ancestor, and
/// the missing components are rejoined verbatim. `None` when the path is
/// relative, when no ancestor resolves, or when a missing component is `.` or
/// `..` — rejoining those lexically could name a different place than the
/// filesystem would. Only a component that is really missing (`NotFound`) is
/// rejoined: any other failure (permission denied, not a directory, a loop)
/// means the component may exist as something this process cannot see — a
/// symlink behind an unreadable directory, say — so the answer is `None`
/// rather than a guess.
pub fn resolved_spelling(path: &str) -> Option<String> {
    use std::path::{Component, Path, PathBuf};

    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    let mut missing: Vec<&std::ffi::OsStr> = Vec::new();
    let mut ancestor = path;
    let canonical = loop {
        match std::fs::canonicalize(ancestor) {
            Ok(canonical) => break canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        let name = match ancestor.components().next_back()? {
            Component::Normal(name) => name,
            _ => return None,
        };
        missing.push(name);
        ancestor = ancestor.parent()?;
    };
    let full: PathBuf = missing
        .iter()
        .rev()
        .fold(canonical, |acc, name| acc.join(name));
    Some(portable_spelling(&full.to_string_lossy()))
}

pub fn windows_portable_spelling(path: &str) -> String {
    // `\\?\UNC\host\share` is `\\host\share` written verbatim, so the prefix
    // cannot simply be cut off that one.
    let simplified = match path.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => path.strip_prefix(r"\\?\").unwrap_or(path).to_string(),
    };
    simplified.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_rooted_forward_slash_path_is_absolute_on_every_host() {
        assert!(is_absolute_on_any_platform("C:/Users/me/repo"));
        assert!(is_absolute_on_any_platform("c:/Users/me/repo"));
        assert!(is_absolute_on_any_platform(r"C:\Users\me\repo"));
        assert!(!is_absolute_on_any_platform("C:relative/repo"));
        assert!(!is_absolute_on_any_platform("1:/Users/me/repo"));
    }

    /// The spelling half of the parity the `git_reads` shootout asserts, where
    /// it can be checked on every platform rather than only on the one that
    /// breaks.
    #[test]
    fn windows_paths_are_rewritten_the_way_git_prints_them() {
        // What `fs::canonicalize` hands back on Windows.
        assert_eq!(
            windows_portable_spelling(r"\\?\C:\Users\me\repo"),
            "C:/Users/me/repo"
        );
        // A plain host path: separators only.
        assert_eq!(
            windows_portable_spelling(r"C:\Users\me\repo"),
            "C:/Users/me/repo"
        );
        // The verbatim UNC form is `\\host\share`, not `UNC\host\share`.
        assert_eq!(
            windows_portable_spelling(r"\\?\UNC\host\share\repo"),
            "//host/share/repo"
        );
        // Already in the portable spelling: unchanged.
        assert_eq!(
            windows_portable_spelling("C:/Users/me/repo"),
            "C:/Users/me/repo"
        );
    }

    /// Catches `||` -> `&&` in `is_absolute_on_any_platform`: each shape is
    /// absolute by exactly one disjunct, so a host-independent verdict fails if
    /// any `||` becomes `&&` (`\\host\share` is relative on unix, `/etc/passwd`
    /// is merely rooted on Windows).
    #[test]
    fn each_foreign_absolute_shape_is_absolute_on_every_host() {
        assert!(is_absolute_on_any_platform(r"\\host\share\repo"));
        assert!(is_absolute_on_any_platform("/etc/passwd"));
        assert!(!is_absolute_on_any_platform("repo/sub"));
        assert!(!is_absolute_on_any_platform(r"repo\sub"));
    }

    /// A drive root is the one Windows path `fs::canonicalize` answers with a
    /// trailing separator (`\\?\C:\`). Its portable spelling must be the one
    /// `git` prints for it, from either input form, and a child under it must
    /// not gain a doubled separator.
    #[test]
    fn a_windows_drive_root_round_trips_through_the_portable_spelling() {
        assert_eq!(windows_portable_spelling(r"\\?\C:\"), "C:/");
        assert_eq!(windows_portable_spelling(r"C:\"), "C:/");
        assert_eq!(windows_portable_spelling("C:/"), "C:/");
        assert_eq!(
            windows_portable_spelling(r"\\?\C:\tuic\repo"),
            "C:/tuic/repo"
        );
    }

    /// The same round trip through the filesystem: the drive the system root
    /// lives on resolves to its portable root, and a missing path under it is
    /// rejoined once, verbatim.
    #[cfg(windows)]
    #[test]
    fn resolved_spelling_of_a_drive_root_is_its_portable_root() {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let root = format!(r"{drive}\");
        let portable_root = format!("{drive}/");
        assert_eq!(
            resolved_spelling(&root).as_deref(),
            Some(portable_root.as_str())
        );
        assert!(same_path(&root, &portable_root));
        let missing = format!(r"{drive}\tuic-no-such-dir-{}\x", std::process::id());
        assert_eq!(
            resolved_spelling(&missing),
            Some(format!("{drive}/tuic-no-such-dir-{}/x", std::process::id()))
        );
    }

    /// Catches the walk treating every `canonicalize` failure as "missing": a
    /// component behind an unreadable directory may be a symlink to anywhere,
    /// so rejoining it verbatim could alias two different places.
    #[cfg(unix)]
    #[test]
    fn resolved_spelling_refuses_a_component_it_cannot_inspect() {
        use std::os::unix::fs::PermissionsExt;
        let base =
            std::env::temp_dir().join(format!("tuic-spelling-eacces-{}", std::process::id()));
        let locked = base.join("locked");
        std::fs::create_dir_all(locked.join("inner")).unwrap();
        let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
        // Restores the mode even when an assertion fails, so the scratch dir
        // can always be removed.
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
                let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
            }
        }
        let _restore = Restore(locked.clone());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::canonicalize(locked.join("inner")).is_ok() {
            eprintln!("UNCHECKED: mode 000 does not stop this process (root?)");
            return;
        }

        assert_eq!(resolved_spelling(&s(&locked.join("inner"))), None);
        assert_eq!(resolved_spelling(&s(&locked.join("inner/gone"))), None);
        // The locked directory itself is still readable as an entry.
        assert!(resolved_spelling(&s(&locked)).is_some());
        // Two spellings that could only be matched by guessing are not "same".
        assert!(!same_path(
            &s(&locked.join("inner")),
            &s(&base.join("./locked/inner"))
        ));
    }

    /// Catches `portable_spelling` replaced by `String::new()` or `"xyzzy"`.
    /// Only Windows hosts rewrite; elsewhere `\` is a legal file-name character
    /// and the path must come back byte-identical.
    #[test]
    fn portable_spelling_rewrites_only_on_windows() {
        let (windows_path, posix_path) = (r"C:\Users\me\repo", "/home/me/repo");
        assert_eq!(portable_spelling(posix_path), posix_path);
        let expected = if cfg!(windows) {
            "C:/Users/me/repo"
        } else {
            windows_path
        };
        assert_eq!(portable_spelling(windows_path), expected);
    }
}
