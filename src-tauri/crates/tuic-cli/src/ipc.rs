//! HTTP-over-IPC client for communicating with a running TUICommander instance.
//!
//! Unix: connects via Unix domain socket at `<config_dir>/mcp.sock`
//! Windows: connects via named pipe at `\\.\pipe\tuicommander-mcp`

use std::io::{self, Read, Write};
use tuic_ipc::app_instance::current_app_instance;

pub(crate) fn config_dir() -> std::path::PathBuf {
    current_app_instance().config_dir_from(
        dirs::config_dir().as_deref(),
        &dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")),
    )
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("TUIC_SOCKET") {
        return std::path::PathBuf::from(path);
    }
    tuic_ipc::socket_path(current_app_instance(), &config_dir(), &std::env::temp_dir())
}

#[cfg(unix)]
fn connect() -> io::Result<std::os::unix::net::UnixStream> {
    let path = socket_path();
    std::os::unix::net::UnixStream::connect(&path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("Cannot connect to TUICommander at {}: {e}", path.display()),
        )
    })
}

#[cfg(windows)]
fn connect() -> io::Result<std::fs::File> {
    let path = tuic_ipc::PIPE_NAME;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("Cannot connect to TUICommander at {path}: {e}"),
            )
        })
}

/// HTTP response parsed by the shared IPC framing module.
#[derive(Debug)]
pub struct Response(tuic_ipc::http::Response);

impl From<tuic_ipc::http::Response> for Response {
    fn from(response: tuic_ipc::http::Response) -> Self {
        Self(response)
    }
}

impl std::ops::Deref for Response {
    type Target = tuic_ipc::http::Response;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Response {
    pub fn json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(&self.body)
    }
}

/// The default per-request socket read/write timeout. Callers whose server-side
/// handler can legitimately take longer than this (e.g. `materialize_pane`,
/// whose server-side shell-readiness gate can hold the response for its own
/// bounded wait) must use [`request_with_headers_and_timeout`]/[`post_with_timeout`] with a
/// value that exceeds the server-side bound by a real margin — never rely on
/// this default silently covering it.
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Send an HTTP request over the IPC socket and return the response.
pub fn request(method: &str, path: &str, body: Option<&str>) -> io::Result<Response> {
    request_with_headers(method, path, body, &[])
}

/// Same as [`request`], with caller-supplied extra request headers.
pub fn request_with_headers(
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> io::Result<Response> {
    request_with_headers_and_timeout(method, path, body, extra_headers, None)
}

pub fn request_with_headers_and_timeout(
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
    read_timeout: Option<std::time::Duration>,
) -> io::Result<Response> {
    let mut stream = connect()?;
    #[cfg(unix)]
    {
        let timeout = Some(read_timeout.unwrap_or(DEFAULT_TIMEOUT));
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;
    }
    #[cfg(windows)]
    let _ = read_timeout;

    stream.write_all(&tuic_ipc::http::request(method, path, body, extra_headers))?;
    stream.flush()?;
    let mut decoder = tuic_ipc::http::ResponseDecoder::default();
    let mut bytes = [0; 4096];
    loop {
        let count = stream.read(&mut bytes)?;
        decoder.push(&bytes[..count]);
        if let Some(response) = decoder.response(count == 0)? {
            return Ok(Response::from(response));
        }
    }
}

/// Convenience: GET request
pub fn get(path: &str) -> io::Result<Response> {
    request("GET", path, None)
}

/// Convenience: POST request with JSON body
pub fn post(path: &str, body: &str) -> io::Result<Response> {
    request("POST", path, Some(body))
}

/// Convenience: POST request with JSON body and an explicit timeout override —
/// for a call whose server-side handler can legitimately take longer than
/// [`DEFAULT_TIMEOUT`] (e.g. `materialize_pane`'s shell-readiness gate).
pub fn post_with_timeout(
    path: &str,
    body: &str,
    timeout: std::time::Duration,
) -> io::Result<Response> {
    request_with_headers_and_timeout("POST", path, Some(body), &[], Some(timeout))
}

/// Convenience: PUT request with JSON body
pub fn put(path: &str, body: &str) -> io::Result<Response> {
    request("PUT", path, Some(body))
}

/// Convenience: DELETE request
pub fn delete(path: &str) -> io::Result<Response> {
    request("DELETE", path, None)
}

/// Check if TUICommander is running
pub fn is_running() -> bool {
    get("/health").is_ok()
}

/// Try to launch TUICommander if not running
pub fn ensure_running() -> io::Result<()> {
    if is_running() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-a")
            .arg("TUICommander")
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        // Try desktop entry first, fall back to direct binary
        let result = std::process::Command::new("xdg-open")
            .arg("tuic://")
            .spawn();
        if result.is_err() {
            std::process::Command::new("tuicommander")
                .spawn()
                .map_err(|e| {
                    io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}"))
                })?;
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        std::process::Command::new(format!("{local_app_data}\\TUICommander\\TUICommander.exe"))
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("Failed to launch TUICommander: {e}")))?;
    }

    // Wait for socket to become available (up to 10s)
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if is_running() {
            return Ok(());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "TUICommander did not start within 10 seconds",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// Points `$TUIC_SOCKET` at a fresh Unix socket in a temp dir, spawns a
    /// client thread that issues `GET /health` (and joins it before
    /// returning — so exactly one connect() pairs with exactly one accept(),
    /// with no detached background thread whose scheduling could race a
    /// *different* test's mock server for the same process-global env var),
    /// and answers it with `response` verbatim.
    ///
    /// `#[serial]` on every caller still matters: `$TUIC_SOCKET` is
    /// process-global, so two of these running concurrently would still
    /// stomp on each other even though each one's own round trip is now
    /// internally race-free.
    fn round_trip(response: &'static str) -> (Response, tuic_test_support::SocketDir) {
        let (resp, dir) = round_trip_result(response);
        (resp.unwrap(), dir)
    }

    fn round_trip_result(
        response: &'static str,
    ) -> (io::Result<Response>, tuic_test_support::SocketDir) {
        let dir = crate::short_socket_tempdir();
        let sock_path = dir.path().join("mcp.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        unsafe {
            std::env::set_var("TUIC_SOCKET", &sock_path);
        }
        let client = std::thread::spawn(|| get("/health"));

        let (mut stream, _) = listener.accept().unwrap();
        // Drain the request so the client's write doesn't block on a full
        // pipe; content doesn't matter for these tests.
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        drop(stream);

        let resp = client.join().unwrap();
        (resp, dir)
    }

    #[test]
    #[serial_test::serial]
    fn parses_status_line_and_content_length_body() {
        let (resp, _dir) = round_trip(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
        );
        assert_eq!(resp.status, 200);
        assert!(resp.is_success());
        assert_eq!(resp.body, "{\"ok\":true}\r\n");
    }

    #[test]
    #[serial_test::serial]
    fn malformed_status_line_is_an_error() {
        // The shared `tuic_ipc::http` decoder rejects a status line it cannot
        // parse instead of inventing a 500 (the old in-crate parser's default).
        let (resp, _dir) = round_trip_result("NOT A STATUS LINE\r\n\r\n");
        assert!(
            resp.is_err(),
            "a malformed status line must not parse: {resp:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn headers_are_looked_up_case_insensitively() {
        let (resp, _dir) =
            round_trip("HTTP/1.1 200 OK\r\nMcp-Session-Id: abc-123\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(resp.header("mcp-session-id"), Some("abc-123"));
        assert_eq!(resp.header("MCP-SESSION-ID"), Some("abc-123"));
    }

    #[test]
    #[serial_test::serial]
    fn chunked_body_is_reassembled() {
        let (resp, _dir) = round_trip(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        );
        assert_eq!(resp.body, "hello world");
    }

    #[test]
    #[serial_test::serial]
    fn no_content_length_reads_to_eof() {
        let (resp, _dir) = round_trip("HTTP/1.1 200 OK\r\n\r\nplain body, no length");
        assert_eq!(resp.body, "plain body, no length");
    }

    #[test]
    #[serial_test::serial]
    fn is_running_reflects_transport_success_not_status_code() {
        // Documents the existing behavior: is_running() only checks that the
        // connection succeeded and a response came back, not that /health
        // returned 2xx.
        let dir = crate::short_socket_tempdir();
        let sock_path = dir.path().join("mcp.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        unsafe {
            std::env::set_var("TUIC_SOCKET", &sock_path);
        }
        let client = std::thread::spawn(is_running);
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        stream
            .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();
        drop(stream);
        assert!(client.join().unwrap());
    }

    #[test]
    #[serial_test::serial]
    fn connect_failure_names_the_socket_path() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("does-not-exist.sock");
        unsafe {
            std::env::set_var("TUIC_SOCKET", &sock_path);
        }
        let err = get("/health").unwrap_err();
        assert!(
            err.to_string()
                .contains(&sock_path.to_string_lossy().to_string()),
            "got: {err}"
        );
    }
}
