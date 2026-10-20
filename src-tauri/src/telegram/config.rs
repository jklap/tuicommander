use super::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// File limits bound input allocation before parsing untrusted local contents.
const FILE_LIMIT: u64 = 64 * 1024;

#[derive(Clone)]
pub(crate) struct Paths {
    pub(crate) directory: PathBuf,
}
impl Paths {
    pub(crate) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }
    pub(crate) fn allowlist(&self) -> Result<BTreeSet<i64>, Error> {
        let ids = self.allowlist_entries()?;
        if ids.is_empty() {
            return Err(Error::Config);
        }
        Ok(ids)
    }
    pub(crate) fn allowlist_entries(&self) -> Result<BTreeSet<i64>, Error> {
        let text = private_text(&self.file("allowed_chat_ids"))?;
        let mut ids = BTreeSet::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Only private chats are supported. No comments, usernames or IDs
            // learned from /start can grant authorization.
            if !line.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Error::Config);
            }
            let id: i64 = line.parse().map_err(|_| Error::Config)?;
            if id <= 0 {
                return Err(Error::Config);
            }
            ids.insert(id);
        }
        Ok(ids)
    }
    pub(crate) fn token(&self) -> Result<Zeroizing<String>, Error> {
        let text = private_text(&self.file("bot.token"))?;
        let token = text.trim();
        if token.is_empty()
            || token.len() > 512
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":_-".contains(&b))
        {
            return Err(Error::PrivateFile);
        }
        Ok(Zeroizing::new(token.to_string()))
    }
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    #[serde(default)]
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates enabled for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) enabled: bool,
    #[schemars(extend("x-setting" = {"sensitivity":"public","apply":"live","effect":"Updates bot alias for the current owner.","operations":["get","set"],"confirmationClass":"ordinary"}))]
    pub(crate) bot_alias: String,
}
impl Config {
    /// Missing configuration is disabled and never reads allowlist or token.
    pub(crate) fn load(paths: &Paths) -> Result<Option<Self>, Error> {
        match std::fs::symlink_metadata(paths.file("config.json")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::PrivateFile),
            Ok(_) => {}
        }
        let text = private_text(&paths.file("config.json"))?;
        let config: Self = serde_json::from_str(&text).map_err(|_| Error::Config)?;
        if !config.enabled {
            return Ok(None);
        }
        if config.bot_alias.is_empty()
            || config.bot_alias.len() > 64
            || !config
                .bot_alias
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(Error::Config);
        }
        paths.allowlist_entries()?;
        Ok(Some(config))
    }
}

/// The open file holds the OS lock until the owner is dropped; never unlink it.
pub(crate) struct Owner {
    _file: File,
}
impl Owner {
    pub(crate) fn acquire(paths: &Paths) -> Result<Self, Error> {
        check_directory(&paths.directory)?;
        let file = private_open(&paths.file("owner.lock"), true)?;
        file.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => Error::AlreadyOwned,
            TryLockError::Error(_) => Error::PrivateFile,
        })?;
        Ok(Self { _file: file })
    }
}

pub(super) fn check_directory(path: &Path) -> Result<(), Error> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| Error::PrivateFile)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(Error::PrivateFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments or memory preconditions.
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(Error::PrivateFile);
        }
    }
    Ok(())
}

pub(super) fn private_open(path: &Path, create: bool) -> Result<File, Error> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        // Refuse links/reparse points rather than follow a secret elsewhere.
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(test)]
    {
        *PRIVATE_FILE_OPENS.entry(path.to_path_buf()).or_default() += 1;
    }
    let file = options.open(path).map_err(|_| Error::PrivateFile)?;
    let meta = file.metadata().map_err(|_| Error::PrivateFile)?;
    if !meta.is_file() {
        return Err(Error::PrivateFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments or memory preconditions.
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(Error::PrivateFile);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            // FILE_ATTRIBUTE_REPARSE_POINT
            return Err(Error::PrivateFile);
        }
    }
    Ok(file)
}

pub(super) fn private_text(path: &Path) -> Result<Zeroizing<String>, Error> {
    let file = private_open(path, false)?;
    let mut text = Zeroizing::new(String::new());
    file.take(FILE_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|_| Error::PrivateFile)?;
    if text.len() as u64 > FILE_LIMIT {
        return Err(Error::PrivateFile);
    }
    Ok(text)
}

#[cfg(test)]
static PRIVATE_FILE_OPENS: std::sync::LazyLock<dashmap::DashMap<PathBuf, usize>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

#[cfg(test)]
mod file_read_tests {
    use super::*;

    // Catches: every bus event and one-second tick reopens unchanged config and allowlist.
    #[test]
    fn telegram_idle_enabled_checks_do_not_reopen_unchanged_files() {
        let (_dir, paths) = crate::telegram::tests::setup();
        let config = Config::load(&paths).unwrap().unwrap();
        let runtime = crate::telegram::runtime::Runtime::new(
            std::sync::Arc::new(crate::state::tests_support::make_test_app_state()),
            config,
            paths.clone(),
            "bounded-telegram".into(),
        )
        .unwrap();
        assert!(runtime.enabled());
        let opens = || {
            ["config.json", "allowed_chat_ids"]
                .iter()
                .map(|name| {
                    PRIVATE_FILE_OPENS
                        .get(&paths.file(name))
                        .map(|n| *n)
                        .unwrap_or(0)
                })
                .sum::<usize>()
        };
        let before = opens();
        for _ in 0..100 {
            assert!(runtime.enabled());
        }
        assert_eq!(
            opens() - before,
            0,
            "idle enabled checks must use the watched snapshot"
        );
    }
}
