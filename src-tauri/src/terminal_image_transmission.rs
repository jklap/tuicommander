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
//! *read* here grants no new capability. The real new capability a naive
//! implementation would introduce is `t=t`'s *delete-after-read*: nothing
//! else in a terminal emulator's normal operation lets escape-sequence
//! output cause a file to be deleted. That is the one thing this module
//! actually restricts — deletion only proceeds if the resolved path lies
//! inside the system temp directory, mirroring real Kitty's own guard.

use crate::terminal_images::MAX_SESSION_IMAGE_BYTES;
use std::path::Path;

/// Kitty `t=f`/`t=t`: read the file at `path` (already base64-decoded from
/// the wire — a raw path, not pixel data). Refuses cleanly (`None`) for a
/// missing file, a non-regular file (a directory, device, FIFO, etc. — never
/// worth reading as image bytes and a cheap way to dodge a `/dev/zero`-style
/// denial of service), or one over `MAX_SESSION_IMAGE_BYTES` — checked via
/// `metadata` *before* reading, so an oversized file is never pulled into
/// memory just to be rejected afterward.
///
/// `delete_after` (`t=t`) deletes the file afterward, but ONLY if its
/// canonicalized path resolves inside the system temp directory
/// (`std::env::temp_dir()`) — real Kitty clients always write their `t=t`
/// payload there by construction, so this never rejects a legitimate
/// sender; it exists solely to stop a wire-driven delete from reaching a
/// file outside temp. Failing that check (or the delete itself failing)
/// is silent: the read/display already succeeded, and there is no protocol
/// field to report a partial "displayed but not deleted" outcome.
pub(crate) fn read_file_medium(path: &[u8], delete_after: bool) -> Option<Vec<u8>> {
    let path_str = std::str::from_utf8(path).ok()?;
    let path = Path::new(path_str);

    let metadata = std::fs::symlink_metadata(path).ok()?;
    // `symlink_metadata` reports the *link* itself as non-regular even when
    // it points at a normal file — resolve once more via `metadata` (which
    // follows symlinks) for the file-type/size check that actually matters,
    // while still refusing a symlink to something that isn't a regular file.
    let resolved_metadata = if metadata.file_type().is_symlink() {
        std::fs::metadata(path).ok()?
    } else {
        metadata
    };
    if !resolved_metadata.is_file() {
        return None;
    }
    if resolved_metadata.len() as usize > MAX_SESSION_IMAGE_BYTES {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;

    if delete_after {
        try_delete_if_under_temp_dir(path);
    }

    Some(bytes)
}

fn try_delete_if_under_temp_dir(path: &Path) {
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return;
    };
    let Ok(canonical_temp) = std::fs::canonicalize(std::env::temp_dir()) else {
        return;
    };
    if canonical.starts_with(&canonical_temp) {
        let _ = std::fs::remove_file(&canonical);
    }
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

#[cfg(unix)]
pub(crate) fn read_shm_medium(name: &[u8]) -> Option<Vec<u8>> {
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

    // SAFETY: `shm_open` opens an existing (client-created) POSIX shared
    // memory object read-only — we never create or unlink one, matching the
    // "reader, not owner" contract Kitty's own spec expects of the
    // terminal. `fstat` on the resulting fd reports the OS's own view of the
    // segment's size, which is what gets mapped and capped — never the
    // caller's separately-claimed `S=`/`v=` size, so a name pointing at a
    // segment larger than declared can't be used to over-read. Every exit
    // path below closes `fd` exactly once via `FdGuard`, and `mmap`'s
    // returned pointer is unmapped before returning.
    unsafe {
        let fd = libc::shm_open(cname.as_ptr(), libc::O_RDONLY, 0);
        if fd < 0 {
            return None;
        }
        struct FdGuard(libc::c_int);
        impl Drop for FdGuard {
            fn drop(&mut self) {
                unsafe {
                    libc::close(self.0);
                }
            }
        }
        let _guard = FdGuard(fd);

        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut stat) != 0 {
            return None;
        }
        if stat.st_size <= 0 {
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
            fd,
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
pub(crate) struct KittyJobOutcome {
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
            read_file_medium(&decoded, delete_after).ok_or(KittyDecodeError {
                code: "ENOENT",
                message: "could not read the requested file",
            })?
        }
        Medium::SharedMemory => read_shm_medium(&decoded).ok_or(KittyDecodeError {
            code: "ENOENT",
            message: "could not read the requested shared memory segment",
        })?,
    };

    finish_decode(
        job.format,
        job.compressed,
        job.width_px,
        job.height_px,
        bytes,
    )
}

/// Format the deferred OK/error PTY reply for one completed Kitty decode
/// job, honoring `quiet` exactly like `Term::kitty_respond_ok`/
/// `kitty_respond_error` already do for the still-synchronous path. `None`
/// if `quiet` suppresses it. Shared between `pty.rs::process_chunk` (writes
/// it directly to the PTY) and `TerminalGrid::process` (pushes it onto the
/// same `TermEvent::PtyWrite` queue an immediate reply would use, so
/// `drain_pty_write_events` sees a deferred reply exactly like it always
/// saw an immediate one).
pub(crate) fn format_kitty_reply(outcome: &KittyJobOutcome) -> Option<String> {
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
pub(crate) fn drain_and_run_pending_kitty_decode_jobs(
    image_store: &std::sync::Mutex<crate::terminal_images::ImageStore>,
    pending_jobs: &std::sync::Mutex<Vec<alacritty_terminal::term::kitty::PendingKittyDecodeJob>>,
) -> Vec<KittyJobOutcome> {
    let jobs = std::mem::take(&mut *pending_jobs.lock().unwrap());
    jobs.into_iter()
        .map(|job| {
            let image_id = job.placeholder.image_id;
            let reply_image_id = job.reply_image_id;
            let reply_placement_id = job.reply_placement_id;
            let quiet = job.quiet;
            let result = match execute_pending_kitty_job(&job) {
                Ok(payload) => {
                    let store = image_store.lock().unwrap();
                    match store.try_complete(&job.placeholder, std::sync::Arc::from(payload.bytes))
                    {
                        Ok(()) => Ok(()),
                        Err(_) => {
                            job.placeholder.mark_failed();
                            Err(("ENOSPC", "over the per-session image byte cap"))
                        }
                    }
                }
                Err(err) => {
                    job.placeholder.mark_failed();
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
        let path = dir.join("delete-me.bin");
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
        let path = dir.join("must-survive.bin");
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
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr as *mut u8, payload.len());
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
    #[test]
    fn shm_missing_segment_returns_none_not_a_panic() {
        let name = format!("/tuic-test-shm-missing-{}", std::process::id());
        assert_eq!(read_shm_medium(name.as_bytes()), None);
    }
}
