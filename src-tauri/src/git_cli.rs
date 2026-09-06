//! Unified git subprocess helper.
//!
//! Every git CLI invocation in the app should go through this module.
//! It wraps `Command::new(resolve_cli("git"))`, captures output, and
//! returns typed results with consistent error handling.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::cli::{enriched_path, resolve_cli};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error from a git subprocess call.
#[derive(Debug)]
pub(crate) enum GitError {
    /// The git process could not be spawned (missing binary, permission error).
    SpawnFailed(std::io::Error),
    /// Git exited with a non-zero status code.
    NonZeroExit { code: Option<i32>, stderr: String },
    /// Git outlived its deadline and was killed. Only reachable when the caller
    /// set one with [`GitCmd::timeout`].
    TimedOut { after: Duration },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "Failed to spawn git: {e}"),
            Self::NonZeroExit { code, stderr } => {
                let code_str = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                if stderr.is_empty() {
                    write!(f, "git exited with code {code_str}")
                } else {
                    write!(f, "git exited with code {code_str}: {stderr}")
                }
            }
            Self::TimedOut { after } => {
                write!(
                    f,
                    "git timed out after {:.1}s and was killed",
                    after.as_secs_f64()
                )
            }
        }
    }
}

impl From<GitError> for String {
    fn from(e: GitError) -> String {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Successful output from a git subprocess.
#[derive(Debug)]
pub(crate) struct GitOutput {
    pub stdout: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for configuring and running a git subprocess.
///
/// # Examples
/// ```ignore
/// let out = git_cmd(repo_path)
///     .args(&["log", "--oneline", "-5"])
///     .run()?;
/// ```
pub(crate) struct GitCmd {
    cmd: Command,
    cwd: PathBuf,
    /// Deadline for the whole invocation. `None` (the default) waits forever,
    /// which is right for the local reads that dominate this module and wrong
    /// for anything that touches a network or a user script.
    timeout: Option<Duration>,
}

impl GitCmd {
    /// Add multiple arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    /// Set an environment variable for the subprocess.
    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.cmd.env(key, val);
        self
    }

    /// Kill the invocation if it has not finished within `timeout`.
    ///
    /// Use it for anything that can block on something outside this machine's
    /// control — a network fetch, a user-supplied setup script — where the
    /// alternative to a deadline is a thread parked forever.
    ///
    /// Every `git fetch` in the app passes [`FETCH_TIMEOUT`].
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Run the command to completion, honoring [`GitCmd::timeout`] when set.
    fn output(&mut self) -> Result<std::process::Output, GitError> {
        match self.timeout {
            Some(t) => output_with_deadline(&mut self.cmd, t),
            None => self.cmd.output().map_err(GitError::SpawnFailed),
        }
    }

    /// Run the git command, requiring success (non-zero exit → `Err`).
    ///
    /// Returns `GitOutput` containing raw (untrimmed) stdout on success.
    pub fn run(mut self) -> Result<GitOutput, GitError> {
        let output = self.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(GitOutput { stdout })
    }

    /// Run the git command, returning `None` on non-zero exit.
    /// Spawn failures are logged to stderr (they indicate a broken git
    /// installation, not normal git behavior).
    pub fn run_silent(self) -> Option<GitOutput> {
        let cwd = self.cwd.clone();
        match self.run() {
            Ok(o) => Some(o),
            Err(GitError::SpawnFailed(e)) => {
                // Use warn for "No such file or directory" — stale worktree entries are expected.
                // Reserve error for unexpected spawn failures (broken git installation).
                if e.kind() == std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        source = "git_cli",
                        "Spawn failed (dir missing): {}",
                        cwd.display()
                    );
                } else {
                    tracing::error!(source = "git_cli", "Spawn failed in {}: {e}", cwd.display());
                }
                None
            }
            Err(GitError::TimedOut { after }) => {
                tracing::warn!(
                    source = "git_cli",
                    "Timed out after {:.1}s in {}",
                    after.as_secs_f64(),
                    cwd.display()
                );
                None
            }
            Err(GitError::NonZeroExit { .. }) => None,
        }
    }

    /// Run the git command, returning the full `Output` struct regardless
    /// of exit code. Use for callsites that need to inspect exit code and
    /// stderr independently (e.g. `run_git_command` which never returns Err).
    pub fn run_raw(mut self) -> Result<std::process::Output, GitError> {
        self.output()
    }
}

/// Spawn `cmd`, collect its output, and kill it if it outlives `timeout`.
///
/// The pipes are drained by two reader threads so a child that fills a pipe
/// buffer cannot deadlock against our own wait. On the timeout path those
/// threads are deliberately NOT joined: a killed git can leave a grandchild
/// (a credential helper, a `core.askpass`) holding the write end open, and
/// joining would reintroduce exactly the unbounded wait the deadline exists to
/// prevent. They exit on their own once the last writer closes.
///
/// Not git-specific: the owner probe below runs `lsof` through it, and
/// `worktree.rs` runs the user's setup scripts through it.
pub(crate) fn output_with_deadline(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, GitError> {
    use std::io::Read;

    // `Command::output()` nulls stdin; `spawn()` inherits it. Match `output()`,
    // or a deadlined child could park on a read of the app's stdin — the exact
    // unbounded wait this function exists to bound.
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::SpawnFailed)?;
    let mut out_pipe = child.stdout.take().expect("stdout piped above");
    let mut err_pipe = child.stderr.take().expect("stderr piped above");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    // Backs off from 1ms to 50ms so a fast command is not delayed by the poll
    // granularity while a slow one costs almost no wakeups.
    let mut poll = Duration::from_millis(1);
    let status = loop {
        match child.try_wait().map_err(GitError::SpawnFailed)? {
            Some(status) => break status,
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    let _ = child.kill();
                    // Reap it, so a timeout never leaves a zombie behind.
                    let _ = child.wait();
                    return Err(GitError::TimedOut { after: timeout });
                }
                std::thread::sleep(poll.min(remaining));
                poll = (poll * 2).min(Duration::from_millis(50));
            }
        }
    };

    Ok(std::process::Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

/// Deadline for every `git fetch` in the app.
///
/// A fetch is the only git call here that waits on something off this machine,
/// so it is the only one that can park a blocking thread forever: a
/// `credential.helper` sitting on a prompt, a half-open TCP connection with no
/// keepalive, a wedged network mount. `GIT_TERMINAL_PROMPT=0` in [`git_cmd`]
/// stops git's own prompt but not any of those.
///
/// Three minutes is deliberately generous. It clears a dual-stack connect
/// timeout (~75s per address family), so a genuinely unreachable host still
/// reports git's own error rather than ours, and it leaves room for a large
/// incremental fetch on a slow link. Every fetch below is a single refspec into
/// an existing clone, never a clone, so the transfer is a branch delta — killing
/// one that would have succeeded is a worse outcome than waiting for it.
#[cfg(not(test))]
pub(crate) const FETCH_TIMEOUT: Duration = Duration::from_secs(180);

/// Tests exercise the shipped wiring through a deadline they can afford to wait
/// for. Only the number differs from the value above.
#[cfg(test)]
pub(crate) const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Remove a stale `.git/index.lock` left behind by a crashed process so the
/// next git invocation isn't blocked with `Unable to create '.git/index.lock':
/// File exists` / `could not write index`.
///
/// Staleness is age-based, with two thresholds because the two crash modes
/// produce locks of different sizes and we want a wide margin over any live
/// git process:
///
/// - **Empty lock** (0 bytes): git created the lock but crashed before writing
///   the new index. A real in-progress write fills the lock almost immediately,
///   so a 0-byte lock older than [`EMPTY_LOCK_STALE_SECS`] is certainly orphaned.
/// - **Non-empty lock**: git wrote the new index into the lock but died before
///   renaming it over `.git/index` (e.g. Claude Code killed mid-`git stash`).
///   A legitimate index write finishes in well under a second; we wait
///   [`NONEMPTY_LOCK_STALE_SECS`] to stay safely clear of even large
///   `stash`/`add` operations before reclaiming.
///
/// A 0-byte lock is reclaimed after this many seconds (early crash, no index written yet).
const EMPTY_LOCK_STALE_SECS: u64 = 5;
/// A non-empty lock (index written, rename never happened) is reclaimed after this
/// many seconds — wide margin over even large `stash`/`add` index writes.
const NONEMPTY_LOCK_STALE_SECS: u64 = 30;

/// Pure staleness rule for an `index.lock` of the given byte size and age.
/// Split out from [`remove_stale_index_lock`] so the thresholds are unit-testable
/// without touching the filesystem clock.
fn is_index_lock_stale(len: u64, age_secs: u64) -> bool {
    let threshold = if len == 0 {
        EMPTY_LOCK_STALE_SECS
    } else {
        NONEMPTY_LOCK_STALE_SECS
    };
    age_secs >= threshold
}

/// How long the owner probe may take before we give up.
///
/// Past this we have **no answer**, which is not the same fact as "nobody holds
/// the lock" — see [`UnknownOwner::DeadlineExceeded`].
#[cfg(unix)]
const LOCK_OWNER_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// What the owner probe established about an `index.lock`.
///
/// Three outcomes, because two of them used to be one. The probe returned
/// `Option<Vec<u32>>`, and `None` meant both "nothing holds this lock" and "we
/// could not find out" — so an `lsof` that merely ran slowly was read downstream
/// as permission to delete a lock a live `git add` was holding.
#[derive(Debug, PartialEq, Eq)]
enum LockOwnership {
    /// The probe ran and named the live processes holding the lock open.
    HeldBy(Vec<u32>),
    /// The probe ran and found no holder. The only outcome that is evidence.
    Unowned,
    /// The probe could not answer. Says nothing either way about a holder.
    Unknown(UnknownOwner),
}

/// Why the owner probe has no answer. The two cases are not interchangeable:
/// one is permanent, the other is a bad minute.
#[derive(Debug, PartialEq, Eq)]
enum UnknownOwner {
    /// The probe could not be run at all — no `lsof` on `PATH`, exec refused, or
    /// a platform with no probe. Nothing can be determined here, ever, so
    /// retrying costs a fork and buys nothing.
    Unavailable(String),
    /// `lsof` was installed, ran, and outlived the deadline. An answer existed
    /// and we ran out of patience for it, so the next attempt may well get one.
    /// Measured on this machine at 3.7s against a 2s deadline under several
    /// concurrent agents, and at 0.8s an hour later — the latency is variable,
    /// not a constant we can size a deadline against once and forget.
    DeadlineExceeded(Duration),
}

impl fmt::Display for UnknownOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(detail) => write!(f, "the owner probe could not run: {detail}"),
            Self::DeadlineExceeded(after) => {
                write!(f, "the owner probe outlived its {after:?} deadline")
            }
        }
    }
}

/// Read an `lsof -t` run into a [`LockOwnership`].
///
/// Split from the spawn so both failure shapes are testable without an `lsof`
/// that misbehaves on cue.
#[cfg(unix)]
fn classify_owner_probe(probe: Result<std::process::Output, GitError>) -> LockOwnership {
    let out = match probe {
        Ok(out) => out,
        Err(GitError::TimedOut { after }) => {
            return LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(after));
        }
        // The io error is unwrapped rather than printed through `GitError`'s
        // Display, which opens with "Failed to spawn git". This is not git.
        Err(GitError::SpawnFailed(e)) => {
            return LockOwnership::Unknown(UnknownOwner::Unavailable(e.to_string()));
        }
        Err(e) => return LockOwnership::Unknown(UnknownOwner::Unavailable(e.to_string())),
    };

    // The exit code is not consulted: a file nobody has open exits non-zero with
    // empty stdout, so stdout carries the whole answer.
    //
    // DEFERRED (2026-09-06) — an `lsof` that runs and *fails* (permission
    // denied, a path that vanished under us) also exits non-zero with empty
    // stdout, so it still classifies as `Unowned` here rather than `Unknown`.
    // Under today's fail-open policy that changes no outcome, and `lsof`'s
    // stderr contract on a clean "not found" could only be verified on macOS
    // from here, not on every platform we ship. Revisit if the `Unknown` path is
    // ever made to fail closed, where the difference starts to decide deletions.
    let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
        .collect();
    if pids.is_empty() {
        LockOwnership::Unowned
    } else {
        LockOwnership::HeldBy(pids)
    }
}

/// Which live processes currently hold `lock` open.
///
/// Only ever consulted for a lock the age rule has already condemned, so the
/// `lsof` fork happens at most once per reclaim attempt and never on the hot
/// path of an ordinary git call.
#[cfg(unix)]
fn probe_index_lock_owner(lock: &Path) -> LockOwnership {
    // -t: PIDs only, one per line. -w: no warnings on unreadable mounts.
    //
    // Deadlined: this runs inside `git_cmd`, so an `lsof` stuck on a wedged
    // network mount would wedge every git call in the app.
    classify_owner_probe(output_with_deadline(
        Command::new("lsof").args([
            OsStr::new("-w"),
            OsStr::new("-t"),
            OsStr::new("--"),
            lock.as_os_str(),
        ]),
        LOCK_OWNER_PROBE_TIMEOUT,
    ))
}

/// No portable owner probe outside unix. Windows refuses to unlink a file another
/// process holds open, so the OS itself provides the protection `lsof` gives us here.
#[cfg(not(unix))]
fn probe_index_lock_owner(_lock: &Path) -> LockOwnership {
    LockOwnership::Unknown(UnknownOwner::Unavailable(
        "no owner probe on this platform".to_string(),
    ))
}

fn remove_stale_index_lock(cwd: &Path) {
    reclaim_stale_index_lock(cwd, probe_index_lock_owner);
}

/// [`remove_stale_index_lock`] with the owner probe injected, so each of the
/// probe's three answers can be driven from a test without an `lsof` that fails
/// on cue.
fn reclaim_stale_index_lock(cwd: &Path, probe: impl FnOnce(&Path) -> LockOwnership) {
    let lock = cwd.join(".git/index.lock");
    let Ok(meta) = std::fs::metadata(&lock) else {
        return;
    };

    // Without a reliable age we can't tell a stale lock from a live one — leave it.
    let Some(age_secs) = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
    else {
        return;
    };

    if !is_index_lock_stale(meta.len(), age_secs) {
        return;
    }

    // Age says "crashed", but age cannot see a git that is merely slow. On a large
    // monorepo an `add`/`stash` index write can outrun the threshold, and deleting
    // the lock under it corrupts the index. Ask who holds it before reclaiming.
    match probe(&lock) {
        LockOwnership::HeldBy(pids) => {
            tracing::info!(
                source = "git_cli",
                "Keeping index.lock in {} — still held by {pids:?}",
                cwd.display()
            );
            return;
        }
        LockOwnership::Unowned => {}
        // FAIL OPEN, deliberately: with no answer we reclaim, exactly as this
        // code did before the owner probe existed. Failing closed would strand a
        // repo behind a lock nothing can prove is dead, and on a host with no
        // `lsof` nothing ever could — the lock would outlive the process that
        // left it. To fail closed instead, `return` here; that is the whole
        // change, and `reason` already carries which of the two cases it is.
        //
        // The cost is real, so it is never silent: past this point the age rule
        // decides alone, and the age rule cannot see a git that is merely slow.
        LockOwnership::Unknown(reason) => {
            tracing::warn!(
                source = "git_cli",
                "index.lock ownership in {} could not be determined ({reason}) — \
                 reclaiming on age alone, which cannot tell a crashed git from a slow one",
                cwd.display()
            );
        }
    }

    match std::fs::remove_file(&lock) {
        Ok(()) => {
            tracing::info!(
                source = "git_cli",
                "Removed stale index.lock ({} bytes, {age_secs}s old) in {}",
                meta.len(),
                cwd.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                source = "git_cli",
                "Failed to remove stale index.lock in {}: {e}",
                cwd.display()
            );
        }
    }
}

/// Test-only count of git subprocesses built for a working directory.
///
/// Keyed by cwd rather than process-wide: the suite runs tests in parallel, so a
/// single counter would measure every other test's git calls. Each test owns its
/// own temp repo, so the key isolates it.
#[cfg(test)]
static GIT_CMD_FORKS: std::sync::LazyLock<dashmap::DashMap<PathBuf, usize>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// How many git subprocesses were built for `cwd`. See [`GIT_CMD_FORKS`].
#[cfg(test)]
pub(crate) fn git_cmd_forks(cwd: &Path) -> usize {
    GIT_CMD_FORKS.get(cwd).map(|n| *n).unwrap_or(0)
}

/// Create a git command builder rooted at the given directory.
pub(crate) fn git_cmd(cwd: &Path) -> GitCmd {
    #[cfg(test)]
    {
        *GIT_CMD_FORKS.entry(cwd.to_path_buf()).or_insert(0) += 1;
    }
    remove_stale_index_lock(cwd);
    let mut cmd = Command::new(resolve_cli("git"));
    cmd.current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("PATH", enriched_path());
    cmd.arg("--no-optional-locks");
    crate::cli::apply_no_window(&mut cmd);
    GitCmd {
        cmd,
        cwd: cwd.to_path_buf(),
        timeout: None,
    }
}

fn finish_failed_operation_after_abort_result(
    operation: &str,
    failure_summary: &str,
    original_error: &str,
    abort_result: Result<(), GitError>,
) -> String {
    match abort_result {
        Ok(()) => format!("{failure_summary} (aborted): {original_error}"),
        Err(abort_error) => {
            tracing::error!(
                source = "git_cli",
                operation = %operation,
                original_error = %original_error,
                abort_error = %abort_error,
                "git {operation} --abort failed after failed {operation}"
            );
            format!(
                "{failure_summary}; repo left in a conflicted state, run git {operation} --abort manually: {abort_error}; original error: {original_error}"
            )
        }
    }
}

/// Abort a failed merge/rebase and describe the final state.
///
/// Returns a message that claims `(aborted)` only when the abort command
/// succeeds. If abort fails, the message tells the user the repository may still
/// be conflicted and includes the manual recovery command.
pub(crate) fn finish_failed_git_operation_after_abort(
    repo_path: &Path,
    operation: &str,
    failure_summary: &str,
    original_error: impl fmt::Display,
) -> String {
    let original_error = original_error.to_string();
    let abort_result = git_cmd(repo_path)
        .args([operation, "--abort"])
        .run()
        .map(|_| ());
    finish_failed_operation_after_abort_result(
        operation,
        failure_summary,
        &original_error,
        abort_result,
    )
}

/// Split a `git status --porcelain` v1 record into its XY status field and path.
///
/// The status codes are positional, so a path is only ever read from column 3
/// onward — never matched anywhere in the line. The mandatory space at index 2
/// also proves indices 2 and 3 are char boundaries, so the slices cannot panic.
fn split_porcelain_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    (bytes.len() >= 4 && bytes[2] == b' ').then(|| (&line[..2], &line[3..]))
}

/// True when an XY status field marks an unmerged (conflicted) path.
fn is_unmerged_code(code: &str) -> bool {
    matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU")
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_conflicted_files_porcelain(status: &str) -> Vec<String> {
    status
        .lines()
        .filter_map(|line| {
            let (code, path) = split_porcelain_line(line)?;
            is_unmerged_code(code).then(|| path.trim().to_string())
        })
        .filter(|path| !path.is_empty())
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_conflict_assist_prompt(pr_number: i64, base: &str, files: &[String]) -> String {
    let mut prompt = format!(
        "Resolve the merge conflicts for PR #{pr_number} after rebasing onto {base}. Do not push and do not merge. Edit only the conflicted files, run relevant checks, and stop for human review.\n\nConflicted files:"
    );
    for file in files {
        prompt.push_str(&format!("\n- {file}"));
    }
    prompt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: create a temp dir with `git init`.
    fn setup_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        Command::new("git")
            .current_dir(&path)
            .args(["init"])
            .output()
            .expect("git init");
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("git config email");
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.name", "Test"])
            .output()
            .expect("git config name");
        (dir, path)
    }

    #[test]
    fn parse_conflicted_files_porcelain_extracts_unmerged_paths() {
        let status = "\
UU src/lib.rs
AA src/new.rs
 M src/clean.rs
?? notes.txt
DU src/deleted.rs
";
        assert_eq!(
            parse_conflicted_files_porcelain(status),
            vec!["src/lib.rs", "src/new.rs", "src/deleted.rs"]
        );
    }

    /// The XY field lives in columns 1-2; a path is never read as a status. The
    /// `DD.rs` case is the one that bites — a real conflict code appearing in a
    /// filename, on a line whose actual code is a plain modification.
    #[test]
    fn parse_conflicted_files_porcelain_ignores_conflict_codes_inside_names() {
        assert!(parse_conflicted_files_porcelain("?? UUID.md\n").is_empty());
        assert!(parse_conflicted_files_porcelain("?? AAA.md\n M src/DD.rs\n").is_empty());
        assert!(parse_conflicted_files_porcelain("").is_empty());
        assert_eq!(
            parse_conflicted_files_porcelain("?? notes.txt\nUU file.rs\n"),
            vec!["file.rs".to_string()]
        );
    }

    #[test]
    fn build_conflict_assist_prompt_lists_files_and_gates_push() {
        let files = vec!["src/lib.rs".to_string(), "src/db.rs".to_string()];
        let prompt = build_conflict_assist_prompt(42, "main", &files);
        assert!(prompt.contains("PR #42"));
        assert!(prompt.contains("rebasing onto main"));
        assert!(prompt.contains("Do not push and do not merge"));
        assert!(prompt.contains("- src/lib.rs"));
        assert!(prompt.contains("- src/db.rs"));
    }

    #[test]
    fn finish_failed_operation_claims_aborted_only_when_abort_succeeds() {
        let msg = finish_failed_operation_after_abort_result(
            "rebase",
            "Rebase failed",
            "conflicts",
            Ok(()),
        );

        assert_eq!(msg, "Rebase failed (aborted): conflicts");
    }

    #[test]
    fn finish_failed_operation_surfaces_abort_failure() {
        let msg = finish_failed_operation_after_abort_result(
            "merge",
            "Merge failed",
            "conflicts",
            Err(GitError::NonZeroExit {
                code: Some(128),
                stderr: "fatal: There is no merge to abort".to_string(),
            }),
        );

        assert!(!msg.contains("(aborted)"));
        assert!(msg.contains("repo left in a conflicted state"));
        assert!(msg.contains("run git merge --abort manually"));
        assert!(msg.contains("original error: conflicts"));
    }

    #[test]
    fn test_empty_lock_kept_while_fresh_removed_when_old() {
        // 0-byte lock: kept under 5s, reclaimed at/after 5s.
        assert!(!is_index_lock_stale(0, 0));
        assert!(!is_index_lock_stale(0, 4));
        assert!(is_index_lock_stale(0, 5));
        assert!(is_index_lock_stale(0, 60));
    }

    #[test]
    fn test_nonempty_lock_kept_until_30s() {
        // Non-empty lock (index written, rename never happened): kept under 30s
        // so we never nuke a live large stash/add, reclaimed at/after 30s.
        assert!(!is_index_lock_stale(4096, 0));
        assert!(!is_index_lock_stale(4096, 29));
        assert!(is_index_lock_stale(4096, 30));
        assert!(is_index_lock_stale(4096, 120));
    }

    /// Backdate a file's mtime so the age-based staleness rule sees it as old,
    /// without sleeping in the test.
    fn age_file(path: &Path, secs: u64) {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for set_times");
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
        f.set_times(
            std::fs::FileTimes::new()
                .set_accessed(when)
                .set_modified(when),
        )
        .expect("set_times");
    }

    /// A lock past the age threshold that a live process still holds open must
    /// survive. Age alone cannot tell a crashed git from a slow one, and
    /// deleting the lock under a running `git add` on a large monorepo is the
    /// index-corruption path.
    #[test]
    fn stale_by_age_lock_with_a_live_owner_is_kept() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let held = std::fs::File::open(&lock).expect("open lock");
        remove_stale_index_lock(&path);
        assert!(
            lock.exists(),
            "a lock a live process holds open must not be reclaimed"
        );
        drop(held);
    }

    /// The control: nobody owns the lock, so the age rule still reclaims it.
    #[test]
    fn stale_lock_nobody_owns_is_reclaimed() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        remove_stale_index_lock(&path);
        assert!(!lock.exists(), "an unowned stale lock must be reclaimed");
    }

    /// Capture every `tracing` event emitted on this thread while `f` runs.
    fn capture_tracing<T>(f: impl FnOnce() -> T) -> (T, String) {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let sink = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(sink.clone()))
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let out = tracing::subscriber::with_default(subscriber, f);
        let text = String::from_utf8(sink.lock().expect("sink").clone()).expect("utf8");
        (out, text)
    }

    /// Build the `Output` an `lsof` run would have produced.
    #[cfg(unix)]
    fn probe_output(stdout: &[u8], code: i32) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    /// `lsof -t` exiting non-zero with empty stdout is a real answer — "nobody
    /// has this file open" — and is the only outcome that licenses a delete.
    #[cfg(unix)]
    #[test]
    fn probe_reads_an_empty_answer_as_unowned_and_pids_as_held() {
        assert_eq!(
            classify_owner_probe(Ok(probe_output(b"", 1))),
            LockOwnership::Unowned
        );
        assert_eq!(
            classify_owner_probe(Ok(probe_output(b"431\n7\n", 0))),
            LockOwnership::HeldBy(vec![431, 7])
        );
    }

    /// No `lsof` on `PATH`: the question cannot be asked, now or ever. That is
    /// not the same fact as "nobody owns the lock", and the type must not let
    /// the caller confuse the two.
    #[cfg(unix)]
    #[test]
    fn probe_reports_unavailable_when_the_tool_is_missing() {
        let mut missing = Command::new("tuic-no-such-owner-probe");
        match classify_owner_probe(output_with_deadline(&mut missing, LOCK_OWNER_PROBE_TIMEOUT)) {
            LockOwnership::Unknown(UnknownOwner::Unavailable(detail)) => {
                assert!(
                    !detail.is_empty(),
                    "the spawn error must survive into the log"
                );
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// The measured failure. `lsof` is installed and working and merely slow —
    /// 3.7s observed on this machine against a 2s deadline. An answer existed
    /// and we ran out of patience for it; reading that as "nobody owns the
    /// lock" is how a live `git add` loses its index.
    #[cfg(unix)]
    #[test]
    fn probe_reports_deadline_exceeded_when_the_tool_is_slow() {
        let slow = Duration::from_millis(50);
        match classify_owner_probe(output_with_deadline(Command::new("sleep").arg("30"), slow)) {
            LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(after)) => {
                assert_eq!(after, slow);
            }
            other => panic!("expected DeadlineExceeded, got {other:?}"),
        }
    }

    /// Fail-open is the recorded policy: an undetermined owner still reclaims.
    /// It must not do so silently — with no answer this is the age rule alone,
    /// and the log is the only place that weakening is visible.
    #[test]
    fn an_unavailable_probe_reclaims_and_records_why() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| {
                LockOwnership::Unknown(UnknownOwner::Unavailable("no lsof on PATH".to_string()))
            })
        });

        assert!(
            !lock.exists(),
            "fail-open: an undetermined owner still reclaims the lock"
        );
        assert!(logs.contains("could not be determined"), "logs: {logs}");
        assert!(
            logs.contains("could not run") && logs.contains("no lsof on PATH"),
            "the log must name the tool problem: {logs}"
        );
    }

    /// Same policy, different reason — and the reason is the whole point. A
    /// probe that timed out means a live owner may well exist; a probe that is
    /// absent means nothing can ever be known. One is worth chasing, the other
    /// is not.
    #[test]
    fn a_slow_probe_reclaims_and_names_the_deadline() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| {
                LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(Duration::from_secs(2)))
            })
        });

        assert!(
            !lock.exists(),
            "fail-open: a probe that timed out still reclaims the lock"
        );
        assert!(logs.contains("could not be determined"), "logs: {logs}");
        assert!(
            logs.contains("outlived its 2s deadline"),
            "the log must name the deadline, not just the failure: {logs}"
        );
    }

    /// A named owner is kept, and the log says who has it.
    #[test]
    fn a_named_owner_keeps_the_lock() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| LockOwnership::HeldBy(vec![4321]))
        });

        assert!(
            lock.exists(),
            "a lock with a live owner must not be reclaimed"
        );
        assert!(
            logs.contains("4321"),
            "the log must name the holder: {logs}"
        );
    }

    /// The probe stays off the hot path: only a lock the age rule has already
    /// condemned is worth a fork.
    #[test]
    fn a_fresh_lock_is_never_probed() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");

        reclaim_stale_index_lock(&path, |_| panic!("a fresh lock must not be probed"));
        assert!(lock.exists(), "a fresh lock is kept without asking anyone");
    }

    #[test]
    fn test_run_success() {
        let (_dir, path) = setup_test_repo();
        let out = git_cmd(&path).args(["status", "--porcelain"]).run();
        assert!(out.is_ok());
    }

    #[test]
    fn test_run_non_zero_exit() {
        let (_dir, path) = setup_test_repo();
        // Asking for log in a repo with no commits → non-zero exit
        let result = git_cmd(&path).args(["log", "--oneline"]).run();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            GitError::NonZeroExit { code, stderr: _ } => {
                assert!(code.is_some());
            }
            _ => panic!("Expected NonZeroExit, got {err:?}"),
        }
        // Display impl should produce a readable message
        let msg = err.to_string();
        assert!(msg.contains("git exited with code"));
    }

    #[test]
    fn test_run_spawn_failed() {
        let (_dir, path) = setup_test_repo();
        // Use a non-existent binary to trigger spawn failure
        let mut cmd = Command::new("/nonexistent/git-binary-that-does-not-exist");
        cmd.current_dir(&path);
        cmd.args(["status"]);
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        };
        let result = gc.run();
        assert!(result.is_err());
        match result.unwrap_err() {
            GitError::SpawnFailed(_) => {} // expected
            other => panic!("Expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_run_silent_returns_none_on_error() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero → None
        let result = git_cmd(&path).args(["log", "--oneline"]).run_silent();
        assert!(result.is_none());
    }

    #[test]
    fn test_run_silent_returns_some_on_success() {
        let (_dir, path) = setup_test_repo();
        let result = git_cmd(&path).args(["status", "--porcelain"]).run_silent();
        assert!(result.is_some());
    }

    #[test]
    fn test_run_raw_returns_output_on_failure() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero but raw still returns Ok(Output)
        let result = git_cmd(&path).args(["log", "--oneline"]).run_raw();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.status.success());
    }

    /// A command that hangs must be killed at the deadline, not waited on.
    #[test]
    fn run_kills_a_command_that_outlives_its_timeout() {
        let (_dir, path) = setup_test_repo();
        let mut cmd = Command::new("sleep");
        cmd.current_dir(&path);
        cmd.arg("30");
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        }
        .timeout(Duration::from_millis(200));

        let started = Instant::now();
        let err = gc.run().expect_err("must time out");
        let waited = started.elapsed();

        assert!(
            matches!(err, GitError::TimedOut { .. }),
            "expected TimedOut, got {err:?}"
        );
        assert!(
            waited < Duration::from_secs(5),
            "must not wait for the child; waited {waited:?}"
        );
    }

    /// run_silent must swallow a timeout the same way it swallows a non-zero exit.
    #[test]
    fn run_silent_returns_none_on_timeout() {
        let (_dir, path) = setup_test_repo();
        let mut cmd = Command::new("sleep");
        cmd.current_dir(&path);
        cmd.arg("30");
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        }
        .timeout(Duration::from_millis(200));

        assert!(gc.run_silent().is_none());
    }

    /// The deadline must not truncate a command that finishes inside it.
    #[test]
    fn timeout_does_not_fire_for_a_command_that_finishes_in_time() {
        let (_dir, path) = setup_test_repo();
        std::fs::write(path.join("untracked.txt"), "u\n").expect("write");
        let out = git_cmd(&path)
            .timeout(Duration::from_secs(30))
            .args(["status", "--porcelain"])
            .run()
            .expect("status must succeed inside the deadline");
        assert!(
            out.stdout.contains("untracked.txt"),
            "stdout must survive the piped path: {:?}",
            out.stdout
        );
    }

    #[test]
    fn timed_out_error_names_the_deadline() {
        let err = GitError::TimedOut {
            after: Duration::from_millis(2500),
        };
        assert_eq!(err.to_string(), "git timed out after 2.5s and was killed");
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::NonZeroExit {
            code: Some(128),
            stderr: "fatal: not a git repository".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "git exited with code 128: fatal: not a git repository"
        );

        let err_empty = GitError::NonZeroExit {
            code: Some(1),
            stderr: String::new(),
        };
        assert_eq!(err_empty.to_string(), "git exited with code 1");

        let err_signal = GitError::NonZeroExit {
            code: None,
            stderr: "killed".to_string(),
        };
        assert_eq!(
            err_signal.to_string(),
            "git exited with code signal: killed"
        );
    }

    #[test]
    fn test_git_error_into_string() {
        let err = GitError::NonZeroExit {
            code: Some(1),
            stderr: "oops".to_string(),
        };
        let s: String = err.into();
        assert!(s.contains("oops"));
    }

    #[test]
    fn test_env_is_passed() {
        let (_dir, path) = setup_test_repo();
        // GIT_AUTHOR_NAME env var should be visible in the subprocess
        let out = git_cmd(&path)
            .env("GIT_AUTHOR_NAME", "TestBot")
            .args(["status", "--porcelain"])
            .run();
        assert!(out.is_ok());
    }
}
