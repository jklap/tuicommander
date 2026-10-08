//! Kitty graphics protocol `t=f`/`t=t`/`t=s` transmission-medium readers
//! (color-tools plan, Phase 6).
//!
//! Deliberately a separate module from `terminal_images.rs` (the in-memory
//! store): this one does real OS I/O — filesystem reads and POSIX/Windows
//! shared-memory mapping — rather than bookkeeping, and is exactly the kind
//! of "OS/filesystem-specific, security-sensitive operation" the vendored
//! `alacritty_terminal` crate's `EventListener::read_file_medium`/
//! `read_shm_medium` doc comments say belongs to the embedding application.
//!
//! **The threat model for `t=f`/`t=t` is narrower than "arbitrary file
//! read" might suggest.** The process driving these escape sequences is
//! already running as this user inside this PTY — it could read anything
//! readable by this user simply by executing `cat`. So an unrestricted file
//! *read* here grants no new capability to a LOCAL program. Two things are
//! new, and both are restricted here:
//!
//! - **Deletion.** `t=t` (delete-after-read) and the `t=s` unlink are the
//!   only ways escape-sequence output could make a file or segment vanish.
//!   Both follow the Kitty spec's own guard: only a name containing
//!   `tty-graphics-protocol` (`KITTY_TEMP_MARKER`) is ever removed, and a
//!   `t=t` file additionally must be a regular file (never a symlink) whose
//!   resolved parent lies inside a known temp directory (`temp_roots`).
//! - **Probing from a remote sender.** Output from `ssh`/`cat` of a hostile
//!   file names paths on THIS machine. Every failure on a local medium
//!   replies with the single `kitty::MEDIUM_ERROR`, so a reply can't
//!   distinguish "missing" from "exists but wrong shape/size".
//!
//! Reads are bounded and never block: the file is opened non-blocking and
//! checked with `fstat` on the opened handle (no stat-then-open race; a
//! FIFO or device swapped in is refused), must be a non-empty regular file
//! (zero-length pseudo-files like `/proc/*` are refused) no larger than
//! `MAX_SESSION_IMAGE_BYTES`, and is read through `take(len)`.

use crate::terminal_images::MAX_SESSION_IMAGE_BYTES;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The substring the Kitty graphics spec requires in a `t=t` file's path
/// before the terminal may delete it ("the file has the string
/// tty-graphics-protocol in its full file path"); real clients (`kitty
/// +kitten icat`, `timg`, ...) name their temp files and shm segments with
/// it. Checked here against the file NAME (stricter than the full path: a
/// directory named after the marker doesn't make everything in it
/// deletable) and, for `t=s`, against the segment name before unlinking.
pub(crate) const KITTY_TEMP_MARKER: &str = "tty-graphics-protocol";

/// Kitty `t=f`/`t=t`: read the file at `path` (already base64-decoded from
/// the wire — a raw path, not pixel data). Refuses cleanly (`None`) for a
/// relative path (it would resolve against this app's cwd, not the
/// sender's), a missing file, anything that isn't a non-empty regular file
/// once opened, or one over `MAX_SESSION_IMAGE_BYTES` — checked via `fstat`
/// of the opened handle *before* reading, so an oversized file is never
/// pulled into memory just to be rejected afterward. A symlink to a regular
/// file is still read (exactly what `cat` would do).
///
/// `delete_after` (`t=t`) deletes the file afterward only when
/// `delete_kitty_temp_file`'s policy allows it; a refused delete is silent
/// (the read/display already succeeded, and there is no protocol field to
/// report a partial "displayed but not deleted" outcome).
pub(crate) fn read_file_medium(path: &[u8], delete_after: bool) -> Option<Vec<u8>> {
    let path_str = std::str::from_utf8(path).ok()?;
    let path = Path::new(path_str);
    if !path.is_absolute() {
        return None;
    }

    let bytes = read_regular_file_bounded(path)?;

    if delete_after {
        delete_kitty_temp_file(path);
    }

    Some(bytes)
}

fn read_regular_file_bounded(path: &Path) -> Option<Vec<u8>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK: opening a FIFO must never wait for a writer (it is
        // refused by the fstat check below anyway). O_NOCTTY: opening a tty
        // must never make it this process's controlling terminal.
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    if metadata.len() > MAX_SESSION_IMAGE_BYTES as u64 {
        return None;
    }
    let len = metadata.len();
    let mut bytes = Vec::with_capacity(len as usize);
    // `take(len)`: a file that grows after the fstat still yields at most
    // the size that was checked against the cap.
    file.take(len).read_to_end(&mut bytes).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

/// The directories a `t=t` file may be deleted from, canonicalized: the
/// process temp dir (`TMPDIR`) plus, on Unix, `/tmp` and `/dev/shm` — the
/// "known temporary directories" the Kitty spec names. Canonicalized so the
/// containment check compares resolved paths (macOS `/tmp` is a symlink to
/// `/private/tmp`).
fn temp_roots() -> Vec<PathBuf> {
    let mut candidates = vec![std::env::temp_dir()];
    #[cfg(unix)]
    {
        candidates.push(PathBuf::from("/tmp"));
        candidates.push(PathBuf::from("/dev/shm"));
    }
    let mut roots: Vec<PathBuf> = candidates
        .into_iter()
        .filter_map(|candidate| std::fs::canonicalize(candidate).ok())
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Kitty `t=t` delete-after-read, following the spec's guard. Deletes ONLY
/// when every condition holds, and otherwise leaves the file alone:
///
/// 1. the file NAME contains `KITTY_TEMP_MARKER`;
/// 2. the path itself is a regular file — `symlink_metadata`, so a symlink
///    (to anything, anywhere) is never deleted and never followed;
/// 3. its parent directory, canonicalized, lies inside one of `temp_roots`
///    (so `..` components and symlinked parent directories can't escape);
/// 4. the resolved `parent/name` is still the same file (same device and
///    inode on Unix) as the one checked in step 2.
///
/// Returns whether the file was removed (for tests; the caller ignores it).
fn delete_kitty_temp_file(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    if !name
        .to_str()
        .is_some_and(|name| name.contains(KITTY_TEMP_MARKER))
    {
        return false;
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_file() {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(canonical_parent) = std::fs::canonicalize(parent) else {
        return false;
    };
    if !temp_roots()
        .iter()
        .any(|root| canonical_parent.starts_with(root))
    {
        return false;
    }
    let target = canonical_parent.join(name);
    let Ok(target_metadata) = std::fs::symlink_metadata(&target) else {
        return false;
    };
    if !target_metadata.file_type().is_file() || !same_file(&metadata, &target_metadata) {
        return false;
    }
    std::fs::remove_file(&target).is_ok()
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

/// Validate a Kitty `t=s` shared-memory segment name before it ever reaches
/// an OS API: reject empty/oversized names, path traversal, and (per POSIX
/// `shm_open(3)` convention) anything with more than the one leading `/` a
/// name may optionally carry. Shared, platform-independent — the actual
/// `shm_open`/`CreateFileMappingA` calls differ, but a malformed name is
/// malformed on every platform.
fn is_safe_shm_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    if name.contains("..") {
        return false;
    }
    let rest = name.strip_prefix('/').unwrap_or(name);
    !rest.is_empty() && !rest.contains('/') && !rest.contains('\\')
}

/// Kitty `t=s` on Unix: read an existing POSIX shared-memory segment
/// (opened read-only; never created here), then unlink it as the Kitty
/// spec asks of the terminal ("read the data from the memory object and
/// then unlink and close it") — but only when its name contains
/// `KITTY_TEMP_MARKER`, the same guard `t=t` uses. Unlinking any
/// caller-named segment would let escape-sequence output remove another
/// program's live segment by name; a client that doesn't use the marker
/// keeps owning (and cleaning up) its own segment, as before.
///
/// The size comes from `fstat` on the opened fd — the OS's own view, never
/// the client's claimed `S=`/`v=` — and is capped at
/// `MAX_SESSION_IMAGE_BYTES` before any byte is copied.
#[cfg(unix)]
pub(crate) fn read_shm_medium(name: &[u8]) -> Option<Vec<u8>> {
    use std::os::fd::{FromRawFd, OwnedFd};

    let name_str = std::str::from_utf8(name).ok()?;
    if !is_safe_shm_name(name_str) {
        return None;
    }
    // POSIX shm_open names conventionally start with a single `/`.
    let normalized = if name_str.starts_with('/') {
        name_str.to_string()
    } else {
        format!("/{name_str}")
    };
    let cname = std::ffi::CString::new(normalized).ok()?;

    // SAFETY: `shm_open` with O_RDONLY opens an existing segment; a
    // non-negative return is a fresh fd this function now owns exclusively,
    // handed straight to `OwnedFd` so every exit path closes it exactly once.
    let fd = unsafe { libc::shm_open(cname.as_ptr(), libc::O_RDONLY, 0) };
    if fd < 0 {
        return None;
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let bytes = read_shm_fd(fd);

    if name_str.contains(KITTY_TEMP_MARKER) {
        // SAFETY: `cname` is a valid NUL-terminated name; unlinking only
        // removes the name — this process's fd/mapping is already gone.
        unsafe {
            libc::shm_unlink(cname.as_ptr());
        }
    }
    bytes
}

/// Copy a shared-memory segment out through `read(2)` rather than `mmap`
/// wherever the OS supports it (Linux, the BSDs): a local process that
/// shrinks a mapped segment while it is being copied turns the copy into
/// a SIGBUS that kills the whole app, whereas a shrinking `read` just comes
/// up short (refused below).
#[cfg(all(unix, not(target_os = "macos")))]
fn read_shm_fd(fd: std::os::fd::OwnedFd) -> Option<Vec<u8>> {
    let file = std::fs::File::from(fd);
    let size = file.metadata().ok()?.len();
    if size == 0 || size > MAX_SESSION_IMAGE_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(size as usize);
    (&file).take(size).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 == size).then_some(bytes)
}

/// macOS cannot `read(2)` a POSIX shared-memory object — `mmap` is the only
/// way in. That is still SIGBUS-safe there because macOS fixes a segment's
/// size at its first `ftruncate`: every later `ftruncate` fails with
/// `EINVAL`, so nobody can shrink the segment under the mapping (pinned by
/// `macos_shm_size_cannot_change_after_it_is_set`).
#[cfg(target_os = "macos")]
fn read_shm_fd(fd: std::os::fd::OwnedFd) -> Option<Vec<u8>> {
    use std::os::fd::AsRawFd;
    let raw = fd.as_raw_fd();
    // SAFETY: `raw` is a valid fd owned by `fd` for this whole scope. The
    // mapping is read-only, its length is the OS-reported size checked
    // against the cap, it is copied out and unmapped before returning, and
    // macOS segments cannot shrink once sized (see the doc comment).
    unsafe {
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(raw, &mut stat) != 0 || stat.st_size <= 0 {
            return None;
        }
        let size = stat.st_size as usize;
        if size > MAX_SESSION_IMAGE_BYTES {
            return None;
        }
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            raw,
            0,
        );
        if ptr == libc::MAP_FAILED {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
        libc::munmap(ptr, size);
        Some(bytes)
    }
}

/// Windows shared-memory read — **written against documented Win32
/// semantics but not verified on Windows** (this crate is developed on
/// macOS/Linux); flag any issue found testing it for real. Opens an
/// existing named file mapping (never creates one — same "reader, not
/// owner" contract as the Unix path), maps the whole object (`0` byte
/// count asks `MapViewOfFile` for the mapping's full size), then uses
/// `VirtualQuery` on the resulting view to learn how large it actually is
/// — the OS's own accounting, not the client's claimed size — before
/// copying out and unmapping.
#[cfg(windows)]
pub(crate) fn read_shm_medium(name: &[u8]) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Memory::{
        FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
        VirtualQuery,
    };

    let name_str = std::str::from_utf8(name).ok()?;
    if !is_safe_shm_name(name_str) {
        return None;
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(name_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `OpenFileMappingW` opens an existing (client-created) named
    // file mapping read-only. Every exit path closes the handle exactly
    // once via `HandleGuard`, and the view returned by `MapViewOfFile` is
    // unmapped before returning. `VirtualQuery` reports the OS's own view
    // size, capped against `MAX_SESSION_IMAGE_BYTES` before any copy.
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr());
        if handle.is_null() {
            return None;
        }
        struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);
        impl Drop for HandleGuard {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _guard = HandleGuard(handle);

        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0);
        if view.Value.is_null() {
            return None;
        }

        let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
        let queried = VirtualQuery(view.Value, &mut info, std::mem::size_of_val(&info));
        if queried == 0 {
            UnmapViewOfFile(view);
            return None;
        }
        let size = info.RegionSize;
        if size == 0 || size > MAX_SESSION_IMAGE_BYTES {
            UnmapViewOfFile(view);
            return None;
        }

        let bytes = std::slice::from_raw_parts(view.Value as *const u8, size).to_vec();
        UnmapViewOfFile(view);
        Some(bytes)
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn read_shm_medium(_name: &[u8]) -> Option<Vec<u8>> {
    None
}

/// The outcome of one deferred Kitty decode job (color-tools plan) — enough
/// for the caller (`pty.rs::process_chunk`, running outside the `vt_log`
/// lock) to write the deferred OK/error PTY reply and, on success, fire the
/// `image-decoded` signal the frontend needs to retry a fetch that may have
/// raced the still-in-flight decode.
pub struct KittyJobOutcome {
    /// The real, resolved image id (`placeholder.image_id`) — what the
    /// frontend's `image-decoded` event should carry. Not necessarily the
    /// same as `reply_image_id` (a client that omitted `i=` gets an
    /// auto-allocated id here, but the wire reply still echoes back `0`,
    /// matching this protocol's existing behavior for that case).
    pub image_id: u32,
    pub reply_image_id: u32,
    pub reply_placement_id: u32,
    pub quiet: u8,
    pub result: Result<(), (&'static str, &'static str)>,
}

/// Shared handle types for the two `Arc<Mutex<...>>`s
/// `drain_and_run_pending_kitty_decode_jobs` needs — named so every call
/// site (`pty.rs`, `terminal_grid.rs`, `state.rs`) spells the same type
/// instead of repeating the nested generics (which clippy's
/// `type_complexity` lint rejects at any local-variable declaration site).
pub type KittyImageStoreHandle =
    std::sync::Arc<parking_lot::Mutex<crate::terminal_images::ImageStore>>;
pub type KittyPendingJobsHandle =
    std::sync::Arc<parking_lot::Mutex<Vec<alacritty_terminal::term::kitty::PendingKittyDecodeJob>>>;

/// Base64-decode a Kitty transmission's wire payload, resolve its medium
/// (file/shared-memory read for `t=f`/`t=t`/`t=s`; the direct bytes
/// themselves for `t=d`), then hand off to
/// `alacritty_terminal::term::kitty::finish_decode` for the parts that don't
/// need OS I/O (zlib inflate, raw-format trim, PNG dimension sniff — shared
/// with the still-synchronous PNG-auto-size path in `term/mod.rs` so there
/// is one implementation, not two).
fn execute_pending_kitty_job(
    job: &alacritty_terminal::term::kitty::PendingKittyDecodeJob,
) -> Result<
    alacritty_terminal::term::kitty::DecodedImagePayload,
    alacritty_terminal::term::kitty::KittyDecodeError,
> {
    use alacritty_terminal::term::kitty::{KittyDecodeError, Medium, finish_decode};
    use base64::Engine;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&job.payload_b64)
        .map_err(|_| KittyDecodeError {
            code: "EINVAL",
            message: "payload is not valid base64",
        })?;
    if decoded.is_empty() {
        return Err(KittyDecodeError {
            code: "EINVAL",
            message: "empty payload",
        });
    }

    let bytes = match job.medium {
        Medium::Direct => decoded,
        Medium::File | Medium::TempFile => {
            let delete_after = job.medium == Medium::TempFile;
            read_file_medium(&decoded, delete_after).ok_or(medium_error())?
        }
        Medium::SharedMemory => read_shm_medium(&decoded).ok_or(medium_error())?,
    };

    // Any decode failure after a LOCAL medium was read is reported as the
    // same generic error as a failed read — see `kitty::MEDIUM_ERROR`.
    finish_decode(
        job.format,
        job.compressed,
        job.width_px,
        job.height_px,
        bytes,
    )
    .map_err(|err| {
        let (code, message) = job.medium.reply_error(err.code, err.message);
        KittyDecodeError { code, message }
    })
    .and_then(|payload| {
        job.medium.check_decoded(job.format, &payload)?;
        Ok(payload)
    })
}

fn medium_error() -> alacritty_terminal::term::kitty::KittyDecodeError {
    let (code, message) = alacritty_terminal::term::kitty::MEDIUM_ERROR;
    alacritty_terminal::term::kitty::KittyDecodeError { code, message }
}

/// Format the deferred OK/error PTY reply for one completed Kitty decode
/// job, honoring `quiet` exactly like `Term::kitty_respond_ok`/
/// `kitty_respond_error` already do for the still-synchronous path. `None`
/// if `quiet` suppresses it. Shared between `pty.rs::process_chunk` (writes
/// it directly to the PTY) and `TerminalGrid::process` (pushes it onto the
/// same `TermEvent::PtyWrite` queue an immediate reply would use, so
/// `drain_pty_write_events` sees a deferred reply exactly like it always
/// saw an immediate one).
pub fn format_kitty_reply(outcome: &KittyJobOutcome) -> Option<String> {
    match &outcome.result {
        Ok(()) if outcome.quiet == 0 => Some(alacritty_terminal::term::kitty::ok_response(
            outcome.reply_image_id,
            outcome.reply_placement_id,
        )),
        Ok(()) => None,
        Err((code, message)) if outcome.quiet < 2 => Some(
            alacritty_terminal::term::kitty::error_response(outcome.reply_image_id, code, message),
        ),
        Err(_) => None,
    }
}

/// Drain every Kitty decode job queued since the last drain and run each to
/// completion, resolving its placeholder `ImageData` one way or the other.
///
/// **This is the whole point of the color-tools plan's deferred-decode
/// design: the caller must invoke this OUTSIDE the session's `vt_log` lock.**
/// A security review found Kitty image decode (base64, zlib inflate, file/
/// shared-memory reads) running fully synchronously under that lock — fine
/// for an occasional image, but a real, continuous cost for a sustained
/// video stream (`mpv --vo=kitty`, `timg`) sending a new frame every
/// 16-33ms. Takes the store/queue as plain `&Mutex` references rather than
/// `&TerminalGrid`/`&VtLogBuffer` specifically so a caller can hold only
/// these two independent locks — never the grid lock — while this runs. See
/// `pty.rs::process_chunk`'s lock-free interlude for the real call site, and
/// `TerminalGrid::drain_and_run_pending_kitty_decode_jobs` for the
/// equivalent single-call convenience the test suite uses (safe there only
/// because a bare test `TerminalGrid` has no concurrent lock contention to
/// begin with).
pub fn drain_and_run_pending_kitty_decode_jobs(
    image_store: &parking_lot::Mutex<crate::terminal_images::ImageStore>,
    pending_jobs: &parking_lot::Mutex<Vec<alacritty_terminal::term::kitty::PendingKittyDecodeJob>>,
) -> Vec<KittyJobOutcome> {
    let jobs = std::mem::take(&mut *pending_jobs.lock());
    jobs.into_iter()
        .map(|job| {
            let image_id = job.placeholder.image_id;
            let reply_image_id = job.reply_image_id;
            let reply_placement_id = job.reply_placement_id;
            let quiet = job.quiet;
            let result = match execute_pending_kitty_job(&job) {
                Ok(payload) => {
                    let mut store = image_store.lock();
                    match store.try_complete(&job.placeholder, std::sync::Arc::from(payload.bytes))
                    {
                        Ok(()) => Ok(()),
                        Err(_) => {
                            // `store_pending` registers unconditionally (no
                            // bytes exist yet to check against a cap), so a
                            // permanently-failed placeholder must evict
                            // itself here — otherwise a stream of cheap
                            // (~30-byte) junk transmissions that each fail
                            // this same cap check would grow `ImageStore`'s
                            // HashMap without bound, independent of and
                            // unbounded by `MAX_SESSION_IMAGE_BYTES` (which
                            // only ever looks at bytes that resolved
                            // successfully). Cells that already reference
                            // this `Arc` (there are none here — reservation
                            // used the same placeholder, but nothing ever
                            // completed it) keep it alive independently,
                            // same as any other `forget`.
                            job.placeholder.mark_failed();
                            store.forget(image_id);
                            Err(job
                                .medium
                                .reply_error("ENOSPC", "over the per-session image byte cap"))
                        }
                    }
                }
                Err(err) => {
                    // Same reasoning as the cap-exceeded branch above: a
                    // decode failure (bad base64, missing file, corrupt
                    // zlib, ...) must not leave a dead entry in the store
                    // forever.
                    job.placeholder.mark_failed();
                    image_store.lock().forget(image_id);
                    Err((err.code, err.message))
                }
            };
            KittyJobOutcome {
                image_id,
                reply_image_id,
                reply_placement_id,
                quiet,
                result,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_real_file_under_the_cap() {
        let dir = std::env::temp_dir().join(format!("tuic-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.bin");
        std::fs::write(&path, b"hello world").unwrap();

        let bytes = read_file_medium(path.to_str().unwrap().as_bytes(), false);
        assert_eq!(bytes.as_deref(), Some(&b"hello world"[..]));
        assert!(path.exists(), "t=f (delete_after=false) must not delete");

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn t_equals_t_deletes_a_file_inside_the_temp_dir() {
        let dir = std::env::temp_dir().join(format!("tuic-test-tt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tty-graphics-protocol-delete-me.bin");
        std::fs::write(&path, b"ephemeral").unwrap();

        let bytes = read_file_medium(path.to_str().unwrap().as_bytes(), true);
        assert_eq!(bytes.as_deref(), Some(&b"ephemeral"[..]));
        assert!(!path.exists(), "t=t must delete a file under the temp dir");

        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn t_equals_t_refuses_to_delete_a_file_outside_the_temp_dir() {
        // Anything genuinely outside the OS temp dir — home directory is a
        // safe stand-in that this test suite can always write to.
        let dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(format!(".tuic-test-outside-temp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Carries the marker, so only the temp-dir containment check can
        // be what refuses the delete.
        let path = dir.join("tty-graphics-protocol-must-survive.bin");
        std::fs::write(&path, b"keep me").unwrap();

        let bytes = read_file_medium(path.to_str().unwrap().as_bytes(), true);
        assert_eq!(
            bytes.as_deref(),
            Some(&b"keep me"[..]),
            "still reads/displays even when the delete is refused"
        );
        assert!(
            path.exists(),
            "t=t must NOT delete a file outside the temp dir"
        );

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    /// Kitty spec: the terminal may only delete a `t=t` file whose name
    /// carries `tty-graphics-protocol`. Without that guard, `cat` of a
    /// hostile file (or SSH output) could delete any file in the temp dir.
    #[test]
    fn t_equals_t_refuses_to_delete_a_temp_file_without_the_spec_marker() {
        let dir = std::env::temp_dir().join(format!("tuic-test-tt-nomark-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("someone-elses-socket-or-lock.bin");
        std::fs::write(&path, b"not ours").unwrap();

        let bytes = read_file_medium(path.to_str().unwrap().as_bytes(), true);
        assert_eq!(bytes.as_deref(), Some(&b"not ours"[..]), "still displays");
        assert!(
            path.exists(),
            "a temp file without the marker must survive t=t"
        );

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    /// A marker-named symlink in the temp dir pointing at another temp
    /// file: neither the link nor its target may be deleted (before this
    /// fix the canonicalized TARGET was removed).
    #[cfg(unix)]
    #[test]
    fn t_equals_t_never_deletes_through_a_symlink() {
        let dir = std::env::temp_dir().join(format!("tuic-test-tt-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let victim = dir.join("victim.bin");
        std::fs::write(&victim, b"victim").unwrap();
        let link = dir.join("tty-graphics-protocol-link.bin");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        let bytes = read_file_medium(link.to_str().unwrap().as_bytes(), true);
        assert_eq!(
            bytes.as_deref(),
            Some(&b"victim"[..]),
            "reading via a link is fine"
        );
        assert!(victim.exists(), "the link's target must never be deleted");
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "the link itself must not be deleted either"
        );

        std::fs::remove_file(&link).ok();
        std::fs::remove_file(&victim).ok();
        std::fs::remove_dir(&dir).ok();
    }

    /// A marker-named file reached through a symlinked PARENT directory
    /// that leads out of the temp dir must not be deleted.
    #[cfg(unix)]
    #[test]
    fn t_equals_t_refuses_a_symlinked_parent_that_escapes_the_temp_dir() {
        let outside = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join(format!(".tuic-test-tt-escape-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("tty-graphics-protocol-x.bin");
        std::fs::write(&target, b"outside").unwrap();
        let link_dir =
            std::env::temp_dir().join(format!("tuic-test-tt-pdir-{}", std::process::id()));
        std::os::unix::fs::symlink(&outside, &link_dir).unwrap();

        let via_link = link_dir.join("tty-graphics-protocol-x.bin");
        assert!(read_file_medium(via_link.to_str().unwrap().as_bytes(), true).is_some());
        assert!(
            target.exists(),
            "must not delete outside the temp dir via a parent link"
        );

        std::fs::remove_file(&link_dir).ok();
        std::fs::remove_file(&target).ok();
        std::fs::remove_dir(&outside).ok();
    }

    #[test]
    fn refuses_a_relative_path() {
        assert_eq!(read_file_medium(b"Cargo.toml", false), None);
    }

    /// Zero-length "regular" files are refused: that is also what every
    /// Linux `/proc`/`/sys` pseudo-file reports, whose real content would
    /// otherwise be read past the size check.
    #[test]
    fn refuses_an_empty_file() {
        let dir = std::env::temp_dir().join(format!("tuic-test-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.bin");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(
            read_file_medium(path.to_str().unwrap().as_bytes(), false),
            None
        );
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    /// A FIFO must be refused immediately, never block the reader waiting
    /// for a writer (the open is non-blocking and fstat rejects it).
    #[cfg(unix)]
    #[test]
    fn refuses_a_fifo_without_blocking() {
        let dir = std::env::temp_dir().join(format!("tuic-test-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pipe");
        let cpath = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        assert_eq!(
            read_file_medium(path.to_str().unwrap().as_bytes(), false),
            None
        );
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn refuses_a_missing_file_cleanly() {
        assert_eq!(read_file_medium(b"/no/such/path/at/all", false), None);
    }

    #[test]
    fn refuses_a_directory_rather_than_reading_it_as_bytes() {
        let dir = std::env::temp_dir().join(format!("tuic-test-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            read_file_medium(dir.to_str().unwrap().as_bytes(), false),
            None
        );
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn refuses_a_file_over_the_size_cap_without_reading_it() {
        let dir = std::env::temp_dir().join(format!("tuic-test-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.bin");
        // Sparse file: report a huge length without actually writing/reading
        // that many bytes, so this test stays fast either way.
        let f = std::fs::File::create(&path).unwrap();
        f.set_len((MAX_SESSION_IMAGE_BYTES + 1) as u64).unwrap();
        drop(f);

        assert_eq!(
            read_file_medium(path.to_str().unwrap().as_bytes(), false),
            None
        );

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn shm_name_validation_rejects_traversal_and_nested_slashes() {
        assert!(is_safe_shm_name("myshm"));
        assert!(is_safe_shm_name("/myshm"));
        assert!(!is_safe_shm_name(""));
        assert!(!is_safe_shm_name("../etc/passwd"));
        assert!(!is_safe_shm_name("/a/b"));
        assert!(!is_safe_shm_name("a/b"));
        assert!(!is_safe_shm_name(&"x".repeat(256)));
    }

    #[cfg(unix)]
    #[test]
    fn shm_round_trip_reads_back_at_least_the_written_payload() {
        let name = format!("/tuic-test-shm-{}", std::process::id());
        let cname = std::ffi::CString::new(name.clone()).unwrap();
        let payload = b"shared memory payload";

        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600);
            assert!(fd >= 0, "shm_open (create) failed");
            assert_eq!(libc::ftruncate(fd, payload.len() as i64), 0);
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                payload.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            assert_ne!(ptr, libc::MAP_FAILED);
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.cast::<u8>(), payload.len());
            libc::munmap(ptr, payload.len());
            libc::close(fd);
        }

        let read_back = read_shm_medium(name.as_bytes());
        // NOT exact-length equality: `fstat` reports the OS's real segment
        // size, which some platforms round up to a page boundary regardless
        // of the exact `ftruncate` length (observed on macOS) — this
        // function intentionally never trims to a client-claimed length (see
        // its own doc comment), so trailing padding is expected here. Format-
        // specific trimming to `width*height*channels` happens one layer up,
        // in `Term::kitty_process`, which is what actually knows the
        // expected exact length for raw pixel formats.
        let read_back = read_back.expect("segment must be readable");
        assert!(
            read_back.starts_with(payload),
            "expected the written payload as a prefix, got {read_back:?}"
        );

        unsafe {
            libc::shm_unlink(cname.as_ptr());
        }
    }

    #[cfg(unix)]
    #[test]
    fn shm_rejects_a_segment_whose_real_os_reported_size_is_over_the_cap() {
        let name = format!("/tuic-test-shm-big-{}", std::process::id());
        let cname = std::ffi::CString::new(name.clone()).unwrap();

        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600);
            assert!(fd >= 0, "shm_open (create) failed");
            // Never actually write/map this many bytes — a bare `ftruncate`
            // claim is enough to make `fstat` (what this function trusts,
            // never the client's own claimed `S=`/`v=`) report an oversized
            // segment.
            assert_eq!(libc::ftruncate(fd, (MAX_SESSION_IMAGE_BYTES + 1) as i64), 0);
            libc::close(fd);
        }

        assert_eq!(read_shm_medium(name.as_bytes()), None);

        unsafe {
            libc::shm_unlink(cname.as_ptr());
        }
    }

    #[cfg(unix)]
    fn create_shm(name: &str, payload: &[u8]) {
        let cname = std::ffi::CString::new(name).unwrap();
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600);
            assert!(fd >= 0, "shm_open (create) failed");
            assert_eq!(libc::ftruncate(fd, payload.len() as i64), 0);
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                payload.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            assert_ne!(ptr, libc::MAP_FAILED);
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.cast::<u8>(), payload.len());
            libc::munmap(ptr, payload.len());
            libc::close(fd);
        }
    }

    #[cfg(unix)]
    fn shm_exists(name: &str) -> bool {
        let cname = std::ffi::CString::new(name).unwrap();
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_RDONLY, 0);
            if fd >= 0 {
                libc::close(fd);
            }
            fd >= 0
        }
    }

    /// Kitty spec: the terminal unlinks a `t=s` segment after reading it —
    /// for a marker-named segment (what real clients create).
    #[cfg(unix)]
    #[test]
    fn shm_marker_segment_is_unlinked_after_reading() {
        // macOS caps shm names at 31 bytes (PSHMNAMLEN).
        let name = format!("/tty-graphics-protocol-{}", std::process::id() % 100_000);
        create_shm(&name, b"frame");
        let read_back = read_shm_medium(name.as_bytes()).expect("readable");
        assert!(read_back.starts_with(b"frame"));
        let still_there = shm_exists(&name);
        if still_there {
            let cname = std::ffi::CString::new(name.clone()).unwrap();
            unsafe { libc::shm_unlink(cname.as_ptr()) };
        }
        assert!(
            !still_there,
            "a marker-named segment must be unlinked after reading"
        );
    }

    /// ...but escape-sequence output must not be able to unlink another
    /// program's segment by naming it.
    #[cfg(unix)]
    #[test]
    fn shm_segment_without_the_marker_is_left_alone() {
        let name = format!("/tuic-test-shm-keep-{}", std::process::id());
        create_shm(&name, b"someone else's");
        assert!(read_shm_medium(name.as_bytes()).is_some());
        let still_there = shm_exists(&name);
        let cname = std::ffi::CString::new(name.clone()).unwrap();
        unsafe { libc::shm_unlink(cname.as_ptr()) };
        assert!(still_there, "an unmarked segment must not be unlinked");
    }

    /// The premise `read_shm_fd`'s macOS `mmap` path relies on for SIGBUS
    /// safety: once sized, a macOS shm segment cannot be resized (so it
    /// can't shrink under the mapping). If a future macOS ever allows it,
    /// this fails and the mmap path must be revisited.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_shm_size_cannot_change_after_it_is_set() {
        let name = format!("/tuic-test-shm-resize-{}", std::process::id());
        let cname = std::ffi::CString::new(name.clone()).unwrap();
        unsafe {
            let fd = libc::shm_open(cname.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600);
            assert!(fd >= 0);
            assert_eq!(libc::ftruncate(fd, 16384), 0);
            let shrink = libc::ftruncate(fd, 4096);
            libc::close(fd);
            libc::shm_unlink(cname.as_ptr());
            assert_ne!(shrink, 0, "macOS must refuse to resize a sized shm segment");
        }
    }

    /// The deferred-decode path replies with the same generic error whether
    /// the local file is missing or exists with the wrong shape, so a PTY
    /// reply can't be used to probe local file existence.
    #[test]
    fn local_medium_failures_are_indistinguishable() {
        use alacritty_terminal::term::kitty::{
            Format, MEDIUM_ERROR, Medium, PendingKittyDecodeJob,
        };
        use base64::Engine;

        let dir = std::env::temp_dir().join(format!("tuic-test-oracle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exists = dir.join("exists.txt");
        std::fs::write(&exists, b"short text, not pixels").unwrap();

        let run = |path: &str, format: Format| {
            let job = PendingKittyDecodeJob {
                placeholder: std::sync::Arc::new(
                    alacritty_terminal::term::cell::ImageData::pending(1, String::new(), 0, 0),
                ),
                format,
                compressed: false,
                medium: Medium::File,
                width_px: 100,
                height_px: 100,
                payload_b64: base64::engine::general_purpose::STANDARD
                    .encode(path)
                    .into_bytes(),
                reply_image_id: 1,
                reply_placement_id: 0,
                quiet: 0,
            };
            execute_pending_kitty_job(&job)
                .map(|_| ())
                .map_err(|err| (err.code, err.message))
        };

        let missing = run("/no/such/path/at/all", Format::Rgb);
        let wrong_shape = run(exists.to_str().unwrap(), Format::Rgb);
        let not_a_png = run(exists.to_str().unwrap(), Format::Png);
        assert_eq!(missing, Err(MEDIUM_ERROR));
        assert_eq!(
            wrong_shape,
            Err(MEDIUM_ERROR),
            "too short for s/v must not differ from missing"
        );
        assert_eq!(
            not_a_png,
            Err(MEDIUM_ERROR),
            "f=100 must not answer OK for any file"
        );

        std::fs::remove_file(&exists).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn shm_missing_segment_returns_none_not_a_panic() {
        let name = format!("/tuic-test-shm-missing-{}", std::process::id());
        assert_eq!(read_shm_medium(name.as_bytes()), None);
    }
}
