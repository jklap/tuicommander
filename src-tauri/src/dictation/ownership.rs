//! One TUICommander instance per config directory owns dictation.
//!
//! Two desktop instances on one config directory (a debug or worktree build next
//! to the installed app) would both register the global hotkey and both open the
//! microphone. The owner is decided by an exclusive advisory lock on a file in
//! the config directory: the OS releases it when the process dies, so a crash
//! never leaves a stale owner behind, which a PID file would.
//!
//! A named instance (`TUIC_APP_INSTANCE`) has its own config directory and so
//! its own lock.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

/// Refusal text; the frontend matches on `OWNED_ELSEWHERE_MARKER`.
pub const OWNED_ELSEWHERE: &str = "Dictation is owned by another TUICommander instance";
pub const OWNED_ELSEWHERE_MARKER: &str = "owned by another TUICommander instance";

const LOCK_FILE: &str = "dictation.lock";

/// Result of one lock attempt. Holds the lock for as long as it lives.
#[derive(Debug)]
pub struct Ownership {
    owner: bool,
    path: PathBuf,
    _lock: Option<File>,
}

impl Ownership {
    /// Try to become the dictation owner for `config_dir`.
    ///
    /// Fails open: when the lock file cannot be created or locked for a reason
    /// other than "someone else holds it", this instance stays the owner. A
    /// filesystem quirk must not switch dictation off for the only instance.
    pub fn acquire(config_dir: &Path) -> Self {
        let path = config_dir.join(LOCK_FILE);
        let open = std::fs::create_dir_all(config_dir).and_then(|()| {
            File::options()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
        });
        let file = match open {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(source = "dictation", path = %path.display(), "Cannot open dictation lock, assuming ownership: {error}");
                return Self {
                    owner: true,
                    path,
                    _lock: None,
                };
            }
        };
        match file.try_lock() {
            Ok(()) => Self {
                owner: true,
                path,
                _lock: Some(file),
            },
            Err(TryLockError::WouldBlock) => Self {
                owner: false,
                path,
                _lock: None,
            },
            Err(TryLockError::Error(error)) => {
                tracing::warn!(source = "dictation", path = %path.display(), "Cannot lock dictation lock, assuming ownership: {error}");
                Self {
                    owner: true,
                    path,
                    _lock: None,
                }
            }
        }
    }

    pub fn is_owner(&self) -> bool {
        self.owner
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap()
    }

    #[test]
    fn a_second_instance_on_the_same_config_dir_is_not_the_owner() {
        // Bug caught: both instances register the hotkey and open the mic.
        let dir = dir();
        let primary = Ownership::acquire(dir.path());
        let secondary = Ownership::acquire(dir.path());
        assert!(primary.is_owner());
        assert!(!secondary.is_owner());
    }

    #[test]
    fn ownership_is_released_when_the_owner_goes_away() {
        // Bug caught: a stale owner after the primary quits or crashes.
        let dir = dir();
        let primary = Ownership::acquire(dir.path());
        drop(primary);
        assert!(Ownership::acquire(dir.path()).is_owner());
    }

    #[test]
    fn instances_on_different_config_dirs_each_own_dictation() {
        // Bug caught: a TUIC_APP_INSTANCE build locked out by the default one.
        let (a, b) = (dir(), dir());
        let first = Ownership::acquire(a.path());
        let second = Ownership::acquire(b.path());
        assert!(first.is_owner() && second.is_owner());
    }
}
