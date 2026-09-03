//! HTTP-over-IPC client for communicating with a running TUICommander instance.
//!
//! Unix: connects via Unix domain socket at `<config_dir>/mcp.sock`
//! Windows: connects via named pipe at `\\.\pipe\tuicommander-mcp`

use std::io::{self, BufRead, BufReader, Read, Write};

/// TUICommander's config directory. Not `#[cfg(unix)]` — the Windows
/// transport is a named pipe with no directory of its own, but the shim's
/// invocation log (`<config_dir>/logs/tmux-shim.log`) needs a real path on
/// every platform, so this is shared rather than reimplemented per caller.
///
/// This is a separate, simpler copy of the app's own `config_dir()`
/// (`src-tauri/src/config.rs`) — no legacy-dir migration, no test override
/// seam. Kept that way deliberately: this crate is a small sidecar binary,
/// not the desktop app.
pub(crate) fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|d| d.join("com.tuic.commander"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".tuicommander")
        })
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("TUIC_SOCKET") {
        return std::path::PathBuf::from(path);
    }
    config_dir().join("mcp.sock")
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
    let path = r"\\.\pipe\tuicommander-mcp";
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

/// HTTP response parsed from the IPC stream.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
    /// Response headers, names lowercased. Needed for `Mcp-Session-Id`, which
    /// carries the MCP session across the separate `Connection: close`
    /// connections this client opens per request.
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(&self.body)
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        let needle = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == needle)
            .map(|(_, v)| v.as_str())
    }
}

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
    let mut stream = connect()?;
    #[cfg(unix)]
    {
        let timeout = Some(std::time::Duration::from_secs(3));
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;
    }

    let content = body.unwrap_or("");
    let mut extra = String::new();
    for (name, value) in extra_headers {
        extra.push_str(&format!("{name}: {value}\r\n"));
    }
    let req = if content.is_empty() {
        format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             {extra}\
             Connection: close\r\n\
             \r\n"
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             {extra}\
             Connection: close\r\n\
             \r\n\
             {content}",
            content.len()
        )
    };

    stream.write_all(req.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(&mut stream);

    // Parse status line
    let mut status_line = String::new();
    reader.read_line(&mut status_line)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(500);

    // Parse headers
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(val) = lower.strip_prefix("content-length:") {
            content_length = val.trim().parse().ok();
        }
        if lower.contains("transfer-encoding") && lower.contains("chunked") {
            chunked = true;
        }
        // Keep the raw value: only the name is case-insensitive.
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }

    // Read body
    let body = if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        String::from_utf8_lossy(&buf).to_string()
    } else if chunked {
        read_chunked(&mut reader)?
    } else {
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf);
        buf
    };

    Ok(Response {
        status,
        body,
        headers,
    })
}

fn read_chunked(reader: &mut impl BufRead) -> io::Result<String> {
    let mut body = String::new();
    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line)?;
        let size = usize::from_str_radix(size_line.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)?;
        body.push_str(&String::from_utf8_lossy(&chunk));
        // Read trailing \r\n
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(body)
}

/// Convenience: GET request
pub fn get(path: &str) -> io::Result<Response> {
    request("GET", path, None)
}

/// Convenience: POST request with JSON body
pub fn post(path: &str, body: &str) -> io::Result<Response> {
    request("POST", path, Some(body))
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
    fn round_trip(response: &'static str) -> (Response, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
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

        let resp = client.join().unwrap().unwrap();
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
    fn malformed_status_line_defaults_to_500() {
        let (resp, _dir) = round_trip("NOT A STATUS LINE\r\n\r\n");
        assert_eq!(resp.status, 500);
        assert!(!resp.is_success());
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
        let dir = tempfile::tempdir().unwrap();
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
