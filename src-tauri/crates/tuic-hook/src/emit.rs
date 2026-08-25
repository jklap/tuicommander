//! Deliver OSC 7770 `verb=payload` pairs: first through TUIC's local socket
//! side channel (`POST /sessions/<TUIC_SESSION>/hook`, #2749-bee7), so they
//! never share the tty with the agent TUI's redraw (#2747-636f), and when that
//! is unavailable or refuses, to the resolved tty in one `write_all`.

use crate::tty;
use std::io::Write;

/// One verb/payload pair to emit. `verbatim` is for fixed enum/numeric
/// payloads (`state`, `toolfail`) — kept byte-identical to the original shell
/// hooks' wire format for backward compatibility with any still-installed
/// old-format hook, and safe because they can never contain a byte
/// percent-encoding would need to touch. `encoded` percent-encodes at
/// construction time, for free-text payloads (`cwd`, `tool`, `notify`, …).
#[derive(Debug)]
pub struct Emission {
    pub verb: &'static str,
    pub payload: String,
}

impl Emission {
    pub fn verbatim(verb: &'static str, payload: impl Into<String>) -> Self {
        Self {
            verb,
            payload: payload.into(),
        }
    }

    pub fn encoded(verb: &'static str, payload: impl AsRef<str>) -> Self {
        Self {
            verb,
            payload: crate::payload::encode(payload.as_ref()),
        }
    }
}

/// Emit every pair as one OSC 7770 sequence per pair, all delivered in a
/// single `write_all` call against the resolved tty. Silent no-op (never an
/// error, never a panic) if the tty can't be resolved or opened — a hook must
/// never block or fail the agent.
pub fn emit(pairs: &[Emission]) {
    if pairs.is_empty() {
        return;
    }
    let path = tty::resolve();
    // Same inertness rule as the tty path: nothing is posted when no tty
    // resolved, so a hook that has no terminal to report on stays silent.
    //
    // DEFERRED (2026-11-17) — main's shell hooks also skip the post when the
    // parent's own tty is `/dev/tty` (a headless `codex exec` started from a
    // tool call inherits TUIC_SESSION/TUIC_SOCKET). Here `TUIC_PTY_TTY` is
    // inherited the same way, so the sidecar cannot tell that case apart from
    // a real tab hook; the pre-existing tty path has the same limitation.
    // Needs a reliable "is my agent headless" signal before it can be fixed.
    if path.is_some() && post_to_socket(pairs) != Posted::NotDelivered {
        return;
    }
    emit_to(path, pairs);
}

/// Outcome of a side-channel post.
#[derive(Debug, PartialEq, Eq)]
enum Posted {
    /// TUIC answered 2xx.
    Delivered,
    /// The post went out but no answer arrived in time. TUIC may have
    /// processed the frames already, so the tty fallback would double them.
    Unanswered,
    /// No socket, or TUIC refused: fall back to the tty.
    NotDelivered,
}

const POST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The frames as one OSC 7770 byte string, one sequence per pair.
fn wire_bytes(pairs: &[Emission]) -> Vec<u8> {
    let mut buf = Vec::new();
    for pair in pairs {
        buf.extend_from_slice(b"\x1b]7770;");
        buf.extend_from_slice(pair.verb.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(pair.payload.as_bytes());
        buf.extend_from_slice(b"\x1b\\");
    }
    buf
}

/// POST the frames to `$TUIC_SOCKET`. `$TUIC_HOOK_TTY` (the test seam) pins
/// delivery to the tty, so a test run inside a real TUIC terminal can never
/// reach that terminal's live socket.
#[cfg(unix)]
fn post_to_socket(pairs: &[Emission]) -> Posted {
    use std::io::Read;
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixStream;

    if std::env::var_os("TUIC_HOOK_TTY").is_some_and(|v| !v.is_empty()) {
        return Posted::NotDelivered;
    }
    let (Some(socket), Some(session)) = (
        std::env::var_os("TUIC_SOCKET").filter(|v| !v.is_empty()),
        std::env::var("TUIC_SESSION").ok().filter(|v| !v.is_empty()),
    ) else {
        return Posted::NotDelivered;
    };
    if !std::fs::metadata(&socket).is_ok_and(|m| m.file_type().is_socket()) {
        return Posted::NotDelivered;
    }
    let body = wire_bytes(pairs);
    let Ok(mut stream) = UnixStream::connect(&socket) else {
        return Posted::NotDelivered;
    };
    let _ = stream.set_write_timeout(Some(POST_TIMEOUT));
    let _ = stream.set_read_timeout(Some(POST_TIMEOUT));
    let mut request = format!(
        "POST /sessions/{session}/hook HTTP/1.1\r\nHost: tuic\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(&body);
    if stream.write_all(&request).is_err() {
        return Posted::NotDelivered;
    }
    let mut head = [0u8; 12];
    let mut filled = 0;
    while filled < head.len() {
        match stream.read(&mut head[filled..]) {
            Ok(0) => return Posted::NotDelivered,
            Ok(n) => filled += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Posted::Unanswered;
            }
            Err(_) => return Posted::NotDelivered,
        }
    }
    // "HTTP/1.1 204": the status code starts at byte 9.
    if head.starts_with(b"HTTP/1.") && head[9] == b'2' {
        Posted::Delivered
    } else {
        Posted::NotDelivered
    }
}

#[cfg(not(unix))]
fn post_to_socket(_pairs: &[Emission]) -> Posted {
    Posted::NotDelivered
}

/// Largest base64 receipt the tty fallback sends. One write is not atomic:
/// above the output high-water mark a macOS pty writer sleeps mid-write and
/// another writer gets in, so a bigger receipt is not sent over the tty.
const TTY_RECEIPT_MAX: usize = 32768;
/// `sh` printf writes in 1024-byte pieces and a redraw escape cut one large
/// frame (#2747-636f), so each tty receipt frame carries at most this much.
const TTY_RECEIPT_PIECE: usize = 768;

/// The tty flavour of [`wire_bytes`]: a `claude-hook=<tab>:<pid>:<base64>`
/// receipt is split into short frames (the reader joins them by tab and pid),
/// and dropped when it is too large for the tty.
fn tty_bytes(pairs: &[Emission]) -> Vec<u8> {
    let mut buf = Vec::new();
    for pair in pairs {
        if pair.verb == "claude-hook"
            && let Some((owner, body)) = pair.payload.rsplit_once(':')
            && !body.is_empty()
        {
            if body.len() <= TTY_RECEIPT_MAX {
                for piece in body.as_bytes().chunks(TTY_RECEIPT_PIECE) {
                    buf.extend_from_slice(b"\x1b]7770;claude-hook=");
                    buf.extend_from_slice(owner.as_bytes());
                    buf.push(b':');
                    buf.extend_from_slice(piece);
                    buf.extend_from_slice(b"\x1b\\");
                }
            }
            continue;
        }
        buf.extend(wire_bytes(std::slice::from_ref(pair)));
    }
    buf
}

/// The write side of [`emit`], taking an already-resolved path so the
/// "nothing resolved" branch is directly testable — `tty::resolve()` returning
/// `None` is not reliably reproducible from a test (`/dev/tty` exists as a
/// device node on every dev/CI machine actually used, so the fallback almost
/// never comes back empty; only the *open* can be forced to fail).
fn emit_to(path: Option<std::path::PathBuf>, pairs: &[Emission]) {
    let Some(path) = path else {
        return;
    };
    if std::env::var("TUIC_HOOK_DEBUG").is_ok_and(|v| !v.is_empty()) {
        eprintln!("tuic-hook: resolved tty = {}", path.display());
    }
    // Not `.create(true)` and not `.truncate(true)`: this must never create a
    // device node, and on the rare non-device target (the TUIC_HOOK_TTY test
    // seam) truncating would erase whatever another hook invocation in the
    // same test already wrote there.
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    // On Linux, a session leader with no controlling terminal that opens a
    // tty device acquires it as its ctty unless O_NOCTTY is passed — exactly
    // the state a Claude Code hook subprocess is in when TUIC_PTY_TTY or the
    // ancestor walk hands it a real device path. Harmless for the
    // TUIC_HOOK_TTY test seam, which points at a regular file rather than a
    // device (O_NOCTTY has no effect on non-tty files). macOS/BSD require an
    // explicit TIOCSCTTY ioctl to acquire a ctty, so this flag is a no-op
    // there rather than a second mechanism to keep in sync.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOCTTY);
    }
    let Ok(mut file) = options.open(&path) else {
        return;
    };

    let _ = file.write_all(&tty_bytes(pairs));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    // All TUIC_HOOK_TTY-touching cases share one `#[test]` fn, sequential
    // within it — the harness runs tests in parallel threads within this
    // binary, and TUIC_HOOK_TTY is a process-global env var (same hazard
    // `tty::tests::tuic_hook_tty_env_seam` documents for itself). Each case
    // below points at its own fresh tempfile so cases can't read back
    // another case's bytes even if this function's own env mutations were
    // somehow interleaved with a concurrent test elsewhere in the crate.
    //
    // `#[serial_test::serial]` (bare, no key — the crate's one shared group)
    // makes that "somehow" impossible rather than just unlikely: every
    // TUIC_HOOK_TTY-touching test in this binary (this fn, the one below, and
    // `tty::tests::tuic_hook_tty_env_seam`) carries the same attribute, so
    // cargo test's default parallel threads can't interleave any of them.
    // Without it, correctness depended entirely on nextest's one-process-
    // per-test default (`.config/nextest.toml`) rather than anything in code.
    #[test]
    #[serial_test::serial]
    fn emit_writes_expected_bytes_via_the_tty_seam() {
        let dir = std::env::temp_dir();

        // Golden bytes for a single verbatim emission.
        let path1 = dir.join(format!("tuic-hook-emit-test-{}-1", std::process::id()));
        fs::write(&path1, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path1) };
        emit(&[Emission::verbatim("state", "busy")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path1)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;state=busy\x1b\\");
        fs::remove_file(&path1).ok();

        // Two pairs concatenate into two complete sequences, in the order given.
        let path2 = dir.join(format!("tuic-hook-emit-test-{}-2", std::process::id()));
        fs::write(&path2, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path2) };
        emit(&[
            Emission::verbatim("toolfail", "1"),
            Emission::verbatim("state", "idle"),
        ]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path2)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;toolfail=1\x1b\\\x1b]7770;state=idle\x1b\\");
        fs::remove_file(&path2).ok();

        // `encoded()` percent-encodes at construction time — reserved OSC
        // delimiter and control bytes must not reach the wire literally.
        let path3 = dir.join(format!("tuic-hook-emit-test-{}-3", std::process::id()));
        fs::write(&path3, b"").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path3) };
        emit(&[Emission::encoded("notify", "a;b\x1bc")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path3)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "\x1b]7770;notify=a%3Bb%1Bc\x1b\\");
        fs::remove_file(&path3).ok();

        // Empty slice: no write at all, file stays exactly as it was.
        let path4 = dir.join(format!("tuic-hook-emit-test-{}-4", std::process::id()));
        fs::write(&path4, b"untouched").unwrap();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path4) };
        emit(&[]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        let mut got = String::new();
        fs::File::open(&path4)
            .unwrap()
            .read_to_string(&mut got)
            .unwrap();
        assert_eq!(got, "untouched");
        fs::remove_file(&path4).ok();
    }

    #[test]
    #[serial_test::serial]
    fn emit_is_a_silent_noop_when_the_tty_cannot_be_opened() {
        // A target that does not exist: OpenOptions::open (no .create(true))
        // fails, and emit() must swallow that rather than panicking.
        let path = std::env::temp_dir().join(format!(
            "tuic-hook-emit-test-{}-missing-{}",
            std::process::id(),
            std::process::id()
        ));
        fs::remove_file(&path).ok();
        unsafe { std::env::set_var("TUIC_HOOK_TTY", &path) };
        emit(&[Emission::verbatim("state", "busy")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
        assert!(!path.exists(), "must never create the target file");
    }

    /// `emit()` now opens with `O_NOCTTY` (added alongside `TUIC_PTY_TTY`,
    /// since both exist to make it safe to point a detached hook process at a
    /// real pty device). Every other test here writes to a regular file, which
    /// `O_NOCTTY` doesn't affect — this is the one test against a genuine pty
    /// slave, pinning that the flag doesn't break opening or writing to an
    /// actual tty device (e.g. an EINVAL from an unexpected flag encoding).
    /// It does not assert ctty-acquisition is avoided: that's only observable
    /// from a process that is a session leader with no ctty of its own, which
    /// a test running under `cargo test`/`cargo nextest` is not guaranteed
    /// (and generally is not) — the POSIX behavior `O_NOCTTY` prevents simply
    /// doesn't trigger here regardless of the flag.
    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn emit_writes_correctly_to_a_real_pty_slave_with_o_noctty_set() {
        use std::os::unix::io::FromRawFd;

        let mut master_fd: libc::c_int = -1;
        let mut slave_fd: libc::c_int = -1;
        let opened = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(
            opened,
            0,
            "openpty failed: {}",
            std::io::Error::last_os_error()
        );

        let mut name_buf = [0i8; 64];
        let named = unsafe { libc::ttyname_r(slave_fd, name_buf.as_mut_ptr(), name_buf.len()) };
        assert_eq!(named, 0, "ttyname_r failed");
        let slave_path = unsafe { std::ffi::CStr::from_ptr(name_buf.as_ptr()) }
            .to_str()
            .expect("tty path must be valid UTF-8")
            .to_string();

        unsafe { std::env::set_var("TUIC_HOOK_TTY", &slave_path) };
        emit(&[Emission::verbatim("state", "busy")]);
        unsafe { std::env::remove_var("TUIC_HOOK_TTY") };

        // SAFETY: master_fd came from openpty() above and is owned by this
        // test; File::drop closes it once, exactly.
        let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };
        let mut got = [0u8; 64];
        let n = master.read(&mut got).expect("read from pty master");
        assert_eq!(&got[..n], b"\x1b]7770;state=busy\x1b\\");

        unsafe { libc::close(slave_fd) };
    }

    /// The other half of `emit()`'s silent-no-op contract: nothing resolved at
    /// all (as opposed to the case above, something resolved but wouldn't
    /// open). No `TUIC_HOOK_TTY` env interaction here, so this doesn't need
    /// `#[serial_test::serial]` — it drives `emit_to` directly rather than
    /// going through `tty::resolve()`.
    #[test]
    fn emit_is_a_silent_noop_when_nothing_resolves() {
        emit_to(None, &[Emission::verbatim("state", "busy")]);
        // No assertion beyond "did not panic" is possible — there is no
        // target path to inspect when resolution itself came back empty.
        // That absence of any observable side effect *is* the contract.
    }

    // --- side-channel delivery (#2749-bee7) -------------------------------

    #[cfg(unix)]
    mod socket {
        use super::*;
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;

        /// One request read off the fake TUIC socket.
        struct Seen {
            request_line: String,
            body: Vec<u8>,
        }

        /// Serve exactly one connection on `listener`, answering `status`.
        fn serve_once(
            listener: UnixListener,
            status: &'static str,
        ) -> std::thread::JoinHandle<Seen> {
            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head_end, length) = loop {
                    let n = stream.read(&mut chunk).expect("read");
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        break (i + 4, length);
                    }
                    assert!(n > 0, "connection closed before the request head");
                };
                while buf.len() < head_end + length {
                    let n = stream.read(&mut chunk).expect("read body");
                    assert!(n > 0, "connection closed before the body");
                    buf.extend_from_slice(&chunk[..n]);
                }
                let request_line = String::from_utf8_lossy(&buf)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").expect("reply");
                Seen {
                    request_line,
                    body: buf[head_end..head_end + length].to_vec(),
                }
            })
        }

        /// Point the sidecar at a fresh socket and an empty tty file.
        fn setup() -> (
            tuic_test_support::SocketDir,
            UnixListener,
            tempfile::NamedTempFile,
        ) {
            let dir = tuic_test_support::socket_dir();
            let listener = UnixListener::bind(dir.path().join("s")).expect("bind");
            let tty = tempfile::NamedTempFile::new().expect("tty file");
            unsafe {
                std::env::remove_var("TUIC_HOOK_TTY");
                std::env::set_var("TUIC_SESSION", "tab-1");
                std::env::set_var("TUIC_SOCKET", dir.path().join("s"));
                std::env::set_var("TUIC_PTY_TTY", tty.path());
            }
            (dir, listener, tty)
        }

        fn teardown() {
            unsafe {
                std::env::remove_var("TUIC_SESSION");
                std::env::remove_var("TUIC_SOCKET");
                std::env::remove_var("TUIC_PTY_TTY");
            }
        }

        /// Catches: hook frames sharing the tty with the agent TUI's redraw, where a
        /// split write printed them over the screen (#2747-636f).
        #[test]
        #[serial_test::serial]
        fn frames_go_to_the_socket_and_not_the_tty() {
            let (_dir, listener, tty) = setup();
            let server = serve_once(listener, "204 No Content");
            emit(&[
                Emission::verbatim("toolfail", "1"),
                Emission::verbatim("state", "idle"),
            ]);
            let seen = server.join().unwrap();
            teardown();
            assert_eq!(seen.request_line, "POST /sessions/tab-1/hook HTTP/1.1");
            assert_eq!(
                String::from_utf8(seen.body).unwrap(),
                "\x1b]7770;toolfail=1\x1b\\\x1b]7770;state=idle\x1b\\"
            );
            assert_eq!(
                std::fs::read(tty.path()).unwrap(),
                b"",
                "nothing on the tty"
            );
        }

        /// Catches: a refusing TUIC (unknown session, rejected frames) silently
        /// dropping the state; the tty is the fallback.
        #[test]
        #[serial_test::serial]
        fn a_refused_post_falls_back_to_the_tty() {
            let (_dir, listener, tty) = setup();
            let server = serve_once(listener, "404 Not Found");
            emit(&[Emission::verbatim("state", "busy")]);
            server.join().unwrap();
            teardown();
            assert_eq!(
                std::fs::read(tty.path()).unwrap(),
                b"\x1b]7770;state=busy\x1b\\"
            );
        }

        /// Catches: no socket (an older TUIC, a remote shell) losing the frames.
        #[test]
        #[serial_test::serial]
        fn without_a_socket_the_tty_gets_the_frames() {
            let (_dir, listener, tty) = setup();
            drop(listener);
            unsafe { std::env::remove_var("TUIC_SOCKET") };
            emit(&[Emission::verbatim("state", "busy")]);
            teardown();
            assert_eq!(
                std::fs::read(tty.path()).unwrap(),
                b"\x1b]7770;state=busy\x1b\\"
            );
        }

        /// Catches: the test seam (and a hook run inside a TUIC terminal's own
        /// test suite) reaching the live socket.
        #[test]
        #[serial_test::serial]
        fn the_tty_seam_never_posts() {
            let (dir, listener, _tty) = setup();
            listener.set_nonblocking(true).unwrap();
            let seam = tempfile::NamedTempFile::new().unwrap();
            unsafe { std::env::set_var("TUIC_HOOK_TTY", seam.path()) };
            emit(&[Emission::verbatim("state", "busy")]);
            unsafe { std::env::remove_var("TUIC_HOOK_TTY") };
            teardown();
            assert!(
                listener.accept().is_err(),
                "no connection may reach the socket under the seam"
            );
            assert_eq!(
                std::fs::read(seam.path()).unwrap(),
                b"\x1b]7770;state=busy\x1b\\"
            );
            drop(dir);
        }
    }

    /// Catches: a large receipt cut by a redraw escape on the shared tty
    /// (#2747-636f): the tty fallback sends short pieces, and nothing at all
    /// when the receipt is too large to write atomically enough.
    #[test]
    fn tty_receipts_are_split_into_short_pieces_and_capped() {
        let body = "A".repeat(TTY_RECEIPT_PIECE * 2 + 5);
        let pairs = [
            Emission::verbatim("claude-hook", format!("tab:42:{body}")),
            Emission::verbatim("claude-hook", "tab:42:".to_string()),
            Emission::verbatim("state", "busy"),
        ];
        let text = String::from_utf8(tty_bytes(&pairs)).unwrap();
        let pieces: Vec<&str> = text.split("\x1b\\").collect();
        assert_eq!(
            pieces.len(),
            6,
            "3 pieces + terminator + state + trailing empty"
        );
        assert_eq!(
            pieces[0],
            format!(
                "\x1b]7770;claude-hook=tab:42:{}",
                "A".repeat(TTY_RECEIPT_PIECE)
            )
        );
        assert_eq!(pieces[3], "\x1b]7770;claude-hook=tab:42:");
        assert_eq!(pieces[4], "\x1b]7770;state=busy");

        let huge = Emission::verbatim(
            "claude-hook",
            format!("tab:42:{}", "A".repeat(TTY_RECEIPT_MAX + 1)),
        );
        assert_eq!(
            tty_bytes(&[huge]),
            b"",
            "an oversized receipt is not sent on the tty"
        );
        // The socket carries it whole: the cap is a tty-only limit.
        let whole = Emission::verbatim(
            "claude-hook",
            format!("tab:42:{}", "A".repeat(TTY_RECEIPT_MAX + 1)),
        );
        assert!(wire_bytes(&[whole]).len() > TTY_RECEIPT_MAX);
    }
}
