//! Platform-neutral helpers shared by Rust tests.

fn checkout_root() -> std::path::PathBuf {
    // A cached test-support crate may have been compiled in another worktree.
    // Find the checkout running this test, not its build path.
    std::env::current_dir()
        .ok()
        .and_then(|cwd| {
            cwd.ancestors()
                .find(|dir| dir.join(".git").exists() && dir.join("src-tauri/Cargo.toml").is_file())
                .map(std::path::Path::to_path_buf)
        })
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."))
}

// Cargo's libtest runner has no per-binary setup hook. This crate is linked
// only into test binaries, so install their temp root before the harness starts.
#[ctor::ctor]
fn install_test_temp_root() {
    // Remember the temp dir this process was given before it is redirected
    // below, so host_temp_dir() and nested processes still see it.
    if non_empty_env("TUIC_TEST_HOST_TMPDIR").is_none() {
        let host = without_trailing_separator(std::env::temp_dir());
        // SAFETY: this constructor runs before libtest creates worker threads.
        unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host) };
    }
    let root = test_temp_root();
    // Freeze the checkout selected at process start: tests may change cwd.
    // SAFETY: this constructor runs before libtest creates worker threads.
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &root) };
    for key in ["TMPDIR", "TMP", "TEMP"] {
        // SAFETY: a process constructor runs before main and before libtest
        // creates its worker threads. No test can read the environment yet.
        unsafe { std::env::set_var(key, &root) };
    }
    isolate_git_config(&root);
}

/// The only git configuration a test process sees besides each fixture
/// repo's own `.git/config`.
const TEST_GITCONFIG: &str = "\
# Written by tuic-test-support: tests never read the developer's git config.
[user]
\tname = TUIC Test
\temail = tests@tuic.invalid
[init]
\tdefaultBranch = main
[commit]
\tgpgSign = false
[tag]
\tgpgSign = false
[core]
\tautocrlf = false
";

/// Point every git process this test binary starts — the fixtures' own `git`
/// calls and the production code under test alike — at [`TEST_GITCONFIG`]
/// instead of the developer's global and system config. A global
/// `merge.ff = only` turned the fixtures' diverged merges into refusals, and a
/// `url.<base>.insteadOf` made `git remote get-url` disagree with the raw
/// `.git/config` read it is compared against; rerere, hooks paths, excludes
/// files and signing settings leak in the same way. A test that needs a
/// specific global setting still sets `GIT_CONFIG_GLOBAL` itself, after this.
fn isolate_git_config(root: &std::path::Path) {
    let path = root.join("gitconfig");
    // Every test process of a nextest run writes this at once: write a private
    // copy and rename it into place, so no git ever reads a half-written file.
    let staging = root.join(format!("gitconfig.{}.tmp", std::process::id()));
    std::fs::write(&staging, TEST_GITCONFIG).expect("write the test gitconfig");
    std::fs::rename(&staging, &path).expect("install the test gitconfig");
    // SAFETY: called from the process constructor, before libtest starts threads.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &path);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::remove_var("GIT_CONFIG_SYSTEM");
        // Command-scope config an invoking `git -c …` or shell may export.
        std::env::remove_var("GIT_CONFIG_PARAMETERS");
        std::env::remove_var("GIT_CONFIG_COUNT");
    }
}

fn non_empty_env(key: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(|value| without_trailing_separator(value.into()))
}

/// `"/var/folders/…/T/"` and `"/var/folders/…/T"` are the same directory, but
/// only one spelling keeps the socket budget arithmetic exact.
fn without_trailing_separator(path: std::path::PathBuf) -> std::path::PathBuf {
    if path.as_os_str().len() > 1 {
        path.components().collect()
    } else {
        path
    }
}

/// This checkout's 16-hex-digit hash. FNV-1a (`tuic_checkout_hash` in
/// scripts/test-tmp-lib.sh computes the same): stable across Rust releases,
/// unlike `DefaultHasher`, so a toolchain bump never orphans the previous
/// run's per-checkout directories.
pub fn checkout_hash() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in checkout_root().as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The temp directory this test process was handed before the constructor
/// pointed `TMPDIR`/`TMP`/`TEMP` at its per-run root: `TUIC_TEST_HOST_TMPDIR`
/// (exported by `scripts/with-test-tmp.sh`, the nextest setup script and the
/// constructor itself), else the OS temp dir. Never derived from `$HOME`.
pub fn host_temp_dir() -> std::path::PathBuf {
    non_empty_env("TUIC_TEST_HOST_TMPDIR")
        .unwrap_or_else(|| without_trailing_separator(std::env::temp_dir()))
}

/// Parent of every per-run test root: `TUIC_TEST_TMP_BASE` (opt-in, e.g.
/// `<checkout>/.tmp/tuic-tests` for the old in-checkout layout), else
/// `<host temp>/tuic-tests`.
pub fn test_base() -> std::path::PathBuf {
    non_empty_env("TUIC_TEST_TMP_BASE").unwrap_or_else(|| host_temp_dir().join("tuic-tests"))
}

/// Scratch space for Rust tests: `TUIC_TEST_TMP_ROOT` when a runner chose it,
/// else `<`[`test_base`]`>/tuic-co-<checkout hash>` — the same per-checkout
/// default `scripts/test-tmp-lib.sh` gives the shell entry points, and outside
/// the checkout, so `find_repo_root` from a fixture never walks up into it.
pub fn test_temp_root() -> std::path::PathBuf {
    let root = non_empty_env("TUIC_TEST_TMP_ROOT")
        .unwrap_or_else(|| test_base().join(format!("tuic-co-{}", checkout_hash())));
    std::fs::create_dir_all(&root).expect("create test temp root");
    root
}

/// macOS `sun_path` size, NUL included (Linux allows 108; the smaller wins).
pub const SUN_PATH_MAX: usize = 104;
/// The longest socket path Rust's std binds or connects: one byte for the NUL
/// (a 104-byte path fails with "path must be shorter than SUN_LEN").
pub const SUN_PATH_USABLE: usize = SUN_PATH_MAX - 1;
/// Socket roots are `<host temp>/t.XXXXXX`: this prefix plus
/// [`SOCKET_ROOT_RANDOM`] random characters.
pub const SOCKET_ROOT_PREFIX: &str = "t.";
pub const SOCKET_ROOT_RANDOM: usize = 6;
/// Per-test socket directories ([`socket_dir`]) are this many random
/// characters below the socket root.
pub const SOCKET_DIR_RANDOM: usize = 4;
/// The longest socket name a test binds inside a [`socket_dir`]:
/// `mcp-<pid>.sock` with a 7-digit Linux pid (`mcp-4194304.sock`).
pub const MAX_TEST_SOCKET_NAME: usize = 16;
/// Longest socket root that still holds `/<dir>/<name>`:
/// 103 − `/` − 4 − `/` − 16 = 81 bytes.
pub const MAX_SOCKET_ROOT_LEN: usize =
    SUN_PATH_USABLE - 1 - SOCKET_DIR_RANDOM - 1 - MAX_TEST_SOCKET_NAME;
/// Longest host temp dir a default socket root fits under:
/// 81 − `/` − `t.` − 6 = 72 bytes (the agent sandbox's `TMPDIR`, exactly).
pub const MAX_SOCKET_HOST_LEN: usize =
    MAX_SOCKET_ROOT_LEN - 1 - SOCKET_ROOT_PREFIX.len() - SOCKET_ROOT_RANDOM;
/// Marks a socket root this crate created; `scripts/with-test-tmp.sh` prunes
/// only stale `t.??????` directories that carry it.
pub const SOCKET_ROOT_MARKER: &str = ".tuic-socket-root";

/// Whether `root` leaves room for every socket the tests bind below it.
pub fn socket_root_fits(root: &std::path::Path) -> bool {
    root.as_os_str().len() <= MAX_SOCKET_ROOT_LEN
}

/// `Ok` when `path` is short enough to bind or connect as a Unix socket,
/// else a message naming its length and the budget.
pub fn unix_socket_path_fits(path: &std::path::Path) -> Result<(), String> {
    let len = path.as_os_str().len();
    if len <= SUN_PATH_USABLE {
        Ok(())
    } else {
        Err(format!(
            "{} is {len} bytes, over the {SUN_PATH_USABLE}-byte Unix socket path budget \
             ({SUN_PATH_MAX}-byte sun_path − NUL) by {}",
            path.display(),
            len - SUN_PATH_USABLE
        ))
    }
}

/// For a test whose socket path is fixed by something it cannot shorten (an
/// external tool's own socket name, a production naming scheme): `true` when
/// `path` fits; otherwise prints an `UNCHECKED` line naming the reason and the
/// `TUIC_TEST_SOCKET_ROOT` override, and returns `false` so the test can return
/// early. Never silent: the line goes to stdout and stderr.
pub fn socket_test_can_run(test: &str, path: &std::path::Path) -> bool {
    match unix_socket_path_fits(path) {
        Ok(()) => true,
        Err(reason) => {
            let line = format!(
                "UNCHECKED {test}: {reason}. The host temp dir {} is {} bytes; set \
                 TUIC_TEST_SOCKET_ROOT=<short private dir> or a shorter TMPDIR to run it.",
                host_temp_dir().display(),
                host_temp_dir().as_os_str().len()
            );
            println!("{line}");
            eprintln!("{line}");
            false
        }
    }
}

#[cfg(unix)]
static PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Make `dir` usable as a socket root, or say why it is not. A `private` root
/// (one this crate creates) must be a real directory owned by us, mode 0700:
/// a symlink or a directory owned by another user is refused, never followed.
#[cfg(unix)]
pub fn prepare_socket_root(dir: &std::path::Path, private: bool) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !socket_root_fits(dir) {
        return Err(format!(
            "{} bytes, over the {MAX_SOCKET_ROOT_LEN}-byte socket-root budget",
            dir.as_os_str().len()
        ));
    }
    if !private {
        std::fs::create_dir_all(dir).map_err(|error| format!("cannot create: {error}"))?;
    }
    let meta = std::fs::symlink_metadata(dir).map_err(|error| format!("cannot stat: {error}"))?;
    if private && meta.file_type().is_symlink() {
        return Err("is a symlink".to_owned());
    }
    if !std::fs::metadata(dir).is_ok_and(|meta| meta.is_dir()) {
        return Err("is not a directory".to_owned());
    }
    // A file this process just created carries its effective uid, so the
    // probe both proves the directory is writable and tells us who we are.
    let probe = dir.join(format!(
        ".tuic-probe-{}-{}",
        std::process::id(),
        PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&probe, b"").map_err(|error| format!("not writable: {error}"))?;
    let own_uid = std::fs::symlink_metadata(&probe).map(|probe| probe.uid());
    let _ = std::fs::remove_file(&probe);
    let own_uid = own_uid.map_err(|error| format!("cannot stat probe: {error}"))?;
    if private {
        if meta.uid() != own_uid {
            return Err(format!("owned by uid {}, not {own_uid}", meta.uid()));
        }
        if meta.mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("cannot make private: {error}"))?;
        }
    }
    Ok(())
}

/// `n` random lowercase alphanumerics from std's per-process random hash keys
/// (no `rand`/`tempfile` dependency in this crate).
fn random_name(n: usize) -> String {
    use std::hash::{BuildHasher, Hasher};
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(now.as_nanos());
    }
    let mut bits = hasher.finish();
    (0..n)
        .map(|_| {
            let index = (bits % ALPHABET.len() as u64) as usize;
            bits /= ALPHABET.len() as u64;
            ALPHABET[index] as char
        })
        .collect()
}

/// Create a fresh, private (0700) directory `parent/<prefix><random>`. `mkdir`
/// never follows a symlink at the final component and fails on any existing
/// entry, so a name another user planted is skipped, not reused.
#[cfg(unix)]
fn create_private_dir(
    parent: &std::path::Path,
    prefix: &str,
    random: usize,
) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let mut last = String::new();
    for _ in 0..16 {
        let dir = parent.join(format!("{prefix}{}", random_name(random)));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last = format!("{} exists", dir.display());
            }
            Err(error) => return Err(format!("cannot create {}: {error}", dir.display())),
        }
    }
    Err(format!("no free name after 16 tries ({last})"))
}

/// Socket roots this process created, by host temp dir, removed at exit.
#[cfg(unix)]
static CREATED_SOCKET_ROOTS: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(unix)]
#[ctor::dtor]
fn remove_created_socket_roots() {
    let roots = CREATED_SOCKET_ROOTS
        .lock()
        .map(|mut roots| std::mem::take(&mut *roots))
        .unwrap_or_default();
    for (_, root) in roots {
        // Only our own marked directory: never a symlink, never unmarked.
        let is_ours = std::fs::symlink_metadata(&root).is_ok_and(|meta| meta.is_dir())
            && root.join(SOCKET_ROOT_MARKER).is_file();
        if is_ours {
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}

#[cfg(unix)]
fn over_budget_message(host: &std::path::Path) -> String {
    let host_len = host.as_os_str().len();
    let root_len = host_len + 1 + SOCKET_ROOT_PREFIX.len() + SOCKET_ROOT_RANDOM;
    format!(
        "no Unix-socket test root: the host temp dir {} is {host_len} bytes, so \
         `<host>/{SOCKET_ROOT_PREFIX}XXXXXX` would be {root_len} bytes; a socket root \
         must be at most {MAX_SOCKET_ROOT_LEN} bytes ({SUN_PATH_USABLE}-byte usable \
         sun_path − `/` − {SOCKET_DIR_RANDOM}-char per-test dir − `/` − \
         {MAX_TEST_SOCKET_NAME}-byte socket name), i.e. a host temp dir of at most \
         {MAX_SOCKET_HOST_LEN} bytes. There is no shared-temp-dir fallback. Set \
         TUIC_TEST_SOCKET_ROOT=<short, private, writable dir of at most \
         {MAX_SOCKET_ROOT_LEN} bytes> or run with a shorter TMPDIR.",
        host.display()
    )
}

/// The socket root for host temp dir `host`: a private `t.XXXXXX` this process
/// creates there once (marked, removed at exit), or a message saying why none
/// fits.
#[cfg(unix)]
pub fn socket_root_under(host: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let mut created = CREATED_SOCKET_ROOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, root)) = created.iter().find(|(seen, _)| seen == host) {
        return Ok(root.clone());
    }
    if host.as_os_str().len() > MAX_SOCKET_HOST_LEN {
        return Err(over_budget_message(host));
    }
    let root =
        create_private_dir(host, SOCKET_ROOT_PREFIX, SOCKET_ROOT_RANDOM).map_err(|reason| {
            format!(
                "no Unix-socket test root under {}: {reason}",
                host.display()
            )
        })?;
    if let Err(reason) = prepare_socket_root(&root, true).and_then(|()| {
        std::fs::write(root.join(SOCKET_ROOT_MARKER), b"")
            .map_err(|error| format!("cannot write marker: {error}"))
    }) {
        let _ = std::fs::remove_dir_all(&root);
        return Err(format!("{}: {reason}", root.display()));
    }
    created.push((host.to_path_buf(), root.clone()));
    Ok(root)
}

#[cfg(unix)]
fn resolve_socket_root() -> Result<std::path::PathBuf, String> {
    if let Some(explicit) = non_empty_env("TUIC_TEST_SOCKET_ROOT") {
        return prepare_socket_root(&explicit, false)
            .map(|()| explicit.clone())
            .map_err(|reason| {
                format!(
                    "no Unix-socket test root: TUIC_TEST_SOCKET_ROOT is set but unusable: \
                     {}: {reason}. It must be a writable directory of at most \
                     {MAX_SOCKET_ROOT_LEN} bytes ({SUN_PATH_USABLE}-byte usable sun_path − \
                     `/` − {SOCKET_DIR_RANDOM}-char per-test dir − `/` − \
                     {MAX_TEST_SOCKET_NAME}-byte socket name).",
                    explicit.display()
                )
            });
    }
    socket_root_under(&host_temp_dir())
}

/// The Unix-domain socket root for this test process: `TUIC_TEST_SOCKET_ROOT`
/// when set (the one override; used as is), else a private, random
/// `<host temp>/t.XXXXXX` (mode 0700, created once per process, removed at
/// exit). Never `/tmp`, never the per-run test root (too long), never `$HOME`.
/// Panics naming the budget, the lengths and `TUIC_TEST_SOCKET_ROOT` when the
/// host temp dir is too long to hold one.
#[cfg(unix)]
pub fn short_socket_test_temp_root() -> std::path::PathBuf {
    resolve_socket_root().unwrap_or_else(|message| panic!("{message}"))
}

/// `name` directly inside [`short_socket_test_temp_root`], asserted to fit
/// `sun_path`. The root is private to this process, so names need no pid.
#[cfg(unix)]
pub fn short_socket_path(name: &str) -> std::path::PathBuf {
    let path = short_socket_test_temp_root().join(name);
    if let Err(reason) = unix_socket_path_fits(&path) {
        panic!("{reason}");
    }
    path
}

/// A private per-test directory for sockets, removed on drop.
#[cfg(unix)]
pub struct SocketDir(std::path::PathBuf);

#[cfg(unix)]
impl SocketDir {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

#[cfg(unix)]
impl Drop for SocketDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A fresh `<socket root>/xxxx` directory: any socket name of up to
/// [`MAX_TEST_SOCKET_NAME`] bytes inside it fits `sun_path`.
#[cfg(unix)]
pub fn socket_dir() -> SocketDir {
    let root = short_socket_test_temp_root();
    let dir = create_private_dir(&root, "", SOCKET_DIR_RANDOM)
        .unwrap_or_else(|reason| panic!("per-test socket dir: {reason}"));
    SocketDir(dir)
}

/// Serialises every working-directory change [`in_dir`] makes in this process.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with the working directory set to `dir`, then restore it, holding a
/// process-wide lock. For binding/connecting a Unix socket by a RELATIVE name
/// when its absolute path cannot fit `sun_path`.
///
/// The working directory is process-wide: a sibling test thread that resolves
/// a relative path or spawns a child during `f` would see `dir`. So this
/// refuses (panics) unless the process runs one test at a time — under
/// `cargo nextest` (`NEXTEST_RUN_ID`, one process per test) or libtest with
/// `RUST_TEST_THREADS=1` / `--test-threads=1`.
pub fn in_dir<R>(dir: &std::path::Path, f: impl FnOnce() -> R) -> R {
    let one_test_per_process = std::env::var_os("NEXTEST_RUN_ID").is_some()
        || std::env::var("RUST_TEST_THREADS").is_ok_and(|threads| threads == "1");
    assert!(
        one_test_per_process,
        "in_dir changes the process-wide working directory; run this test under \
         `cargo nextest` (one process per test) or with RUST_TEST_THREADS=1"
    );
    let _lock = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore the working directory");
        }
    }
    let _restore = Restore(std::env::current_dir().expect("read the working directory"));
    std::env::set_current_dir(dir).expect("enter the socket directory");
    f()
}

/// How a test and its children should spell sockets in one directory.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketSpelling {
    /// `dir/name` fits `sun_path`.
    Absolute,
    /// It does not: bind/connect `name` with the working directory at `dir`.
    Relative,
}

#[cfg(unix)]
impl SocketSpelling {
    /// The longest name the caller will use in `dir` decides the spelling.
    pub fn for_dir(dir: &std::path::Path, longest_name: &str) -> Self {
        if unix_socket_path_fits(&dir.join(longest_name)).is_ok() {
            Self::Absolute
        } else {
            Self::Relative
        }
    }

    /// The temp dir to hand a child whose cwd is `dir` (`TMPDIR=.` resolves
    /// `temp_dir().join(name)` to `./name`).
    pub fn temp_dir(self, dir: &std::path::Path) -> std::path::PathBuf {
        match self {
            Self::Absolute => dir.to_path_buf(),
            Self::Relative => std::path::PathBuf::from("."),
        }
    }

    /// `name` as a child whose cwd is `dir` should spell it.
    pub fn path(self, dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        self.temp_dir(dir).join(name)
    }

    /// Bind `dir/name`, through [`in_dir`] when the absolute path cannot fit.
    pub fn bind(
        self,
        dir: &std::path::Path,
        name: &str,
    ) -> std::io::Result<std::os::unix::net::UnixListener> {
        match self {
            Self::Absolute => std::os::unix::net::UnixListener::bind(dir.join(name)),
            Self::Relative => in_dir(dir, || std::os::unix::net::UnixListener::bind(name)),
        }
    }
}

/// Return the host shell and its script argument.
pub fn host_shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Wait for stdin without spawning a child process; the parent keeps the pipe open.
pub fn wait_for_stdin_script() -> String {
    if cfg!(windows) {
        "set /p value=".to_owned()
    } else {
        "read value".to_owned()
    }
}

/// Print a file's contents in the host shell.
pub fn print_file_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type {path}")
    } else {
        format!("cat {path}")
    }
}

/// Create an empty file in the host shell.
pub fn touch_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type nul > {path}")
    } else {
        format!("touch {path}")
    }
}

/// Copy a file in the host shell.
pub fn copy_file_script(source: &str, destination: &str) -> String {
    if cfg!(windows) {
        format!("copy /y {source} {destination} >nul")
    } else {
        format!("cp {source} {destination}")
    }
}

/// Run one host-shell script after another.
pub fn chain(first: &str, second: &str) -> String {
    if cfg!(windows) {
        format!("{first}& {second}")
    } else {
        format!("{first}; {second}")
    }
}

/// Print an environment variable in the host shell.
pub fn print_var_script(key: &str) -> String {
    if cfg!(windows) {
        format!("if defined {key} (echo %{key}%)")
    } else {
        format!("echo \"${key}\"")
    }
}

/// Return a command that sleeps longer than test timeouts.
pub fn sleep_argv() -> (String, Vec<String>) {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        (
            format!("{root}\\System32\\ping.exe"),
            vec!["-n".to_string(), "61".to_string(), "127.0.0.1".to_string()],
        )
    } else {
        ("sleep".to_string(), vec!["60".to_string()])
    }
}

/// Return the same sleep command as a host-shell script.
pub fn sleep_script() -> String {
    let (program, args) = sleep_argv();
    let call = format!("{program} {}", args.join(" "));
    if cfg!(windows) {
        format!("{call} >nul")
    } else {
        call
    }
}

/// Write a fake SSH executable to a stable path and warm it before use.
pub fn fake_ssh_script(name: &str, posix: &str, windows: &str) -> std::path::PathBuf {
    const WARMUP_VAR: &str = "TUIC_FAKE_SSH_WARMUP";

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fake-ssh");
    std::fs::create_dir_all(&dir).expect("create fake-ssh dir");
    let (extension, desired) = if cfg!(windows) {
        (
            "cmd",
            format!("@echo off\r\nif defined {WARMUP_VAR} exit /b 0\r\n{windows}\r\n"),
        )
    } else {
        (
            "sh",
            format!("#!/bin/sh\n[ -n \"${WARMUP_VAR}\" ] && exit 0\n{posix}\n"),
        )
    };
    let path = dir.join(format!("{name}.{extension}"));

    if std::fs::read_to_string(&path).is_ok_and(|found| found == desired) {
        return path;
    }

    let staging = dir.join(format!("{name}.{extension}.{}", std::process::id()));
    std::fs::write(&staging, &desired).expect("write fake ssh script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake ssh script");
    }
    std::fs::rename(&staging, &path).expect("install fake ssh script");

    let _ = std::process::Command::new(&path)
        .env(WARMUP_VAR, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    path
}

/// Write a message to stderr and exit with the given code.
pub fn fail_with_stderr_script(line: &str, code: i32) -> String {
    if cfg!(windows) {
        format!("echo {line} 1>&2& exit /b {code}")
    } else {
        format!("echo {line} >&2; exit {code}")
    }
}

/// Normalize Windows CRLF output to LF.
pub fn normalize_newlines(output: &str) -> String {
    output.replace("\r\n", "\n")
}

/// Build a host-shell command that replays a file into a PTY.
pub fn replay_file_command(path: &std::path::Path) -> portable_pty::CommandBuilder {
    let (shell, flag) = host_shell();
    let spelled = if cfg!(windows) {
        assert!(
            !path.to_string_lossy().contains(' '),
            "a space in {} cannot survive `cmd /C` through CommandBuilder; \
             give the test a temp directory without one",
            path.display(),
        );
        path.display().to_string().replace('/', "\\")
    } else {
        format!("\"{}\"", path.display())
    };
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.arg(flag);
    command.arg(print_file_script(&spelled));
    command
}

/// Return a path using forward slashes on every platform.
pub fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// Return an existing directory outside the current user's home directory.
pub fn dir_outside_home() -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
        )
    } else {
        std::path::PathBuf::from("/tmp")
    }
}

/// HTTP request bytes captured by an IPC test server.
pub struct HttpRequest {
    pub request_line: String,
    /// Header lines retain their trailing CRLF for existing wire assertions.
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

/// Read one length-delimited request from a test connection.
pub fn read_http_request(reader: &mut impl std::io::Read) -> std::io::Result<HttpRequest> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(reader);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
    }
    let request_line = line.trim_end().to_string();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid Content-Length")
            })?;
        }
        headers.push(line.clone());
    }
    let mut body = vec![0; length];
    std::io::Read::read_exact(&mut reader, &mut body)?;
    Ok(HttpRequest {
        request_line,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    fn git_config_get(cwd: &std::path::Path, key: &str) -> Option<String> {
        let out = std::process::Command::new("git")
            .args(["config", "--get", key])
            .current_dir(cwd)
            .output()
            .expect("run git config");
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The constructor's config is the one git reads, whatever the invoking
    /// shell's `GIT_CONFIG_GLOBAL` or `~/.gitconfig` says. Run it under a
    /// global config that sets `merge.ff = only` to see it bite.
    #[test]
    fn git_in_a_test_process_reads_only_the_test_gitconfig() {
        let dir = tempfile::tempdir_in(super::test_temp_root()).expect("temp dir");
        // Its own repo, so no enclosing repository's local config can answer
        // (the temp root may sit inside a checkout via TUIC_TEST_TMP_BASE).
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(init.success(), "git init failed");
        assert_eq!(
            git_config_get(dir.path(), "user.email").as_deref(),
            Some("tests@tuic.invalid")
        );
        assert_eq!(
            git_config_get(dir.path(), "init.defaultBranch").as_deref(),
            Some("main")
        );
        for leaked in ["merge.ff", "pull.rebase", "rerere.enabled"] {
            assert_eq!(
                git_config_get(dir.path(), leaked),
                None,
                "{leaked} leaked in from the developer's git config"
            );
        }
    }
}
