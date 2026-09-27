#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

fn socket() -> (std::path::PathBuf, UnixListener) {
    let root =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Gits/.tmp/tuic-tests");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "tuic-mcp-{}-{}.sock",
        std::process::id(),
        NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    (path, listener)
}

fn read_request(stream: &mut std::os::unix::net::UnixStream) -> (Vec<String>, serde_json::Value) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
        headers.push(line);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (
        headers,
        if body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        },
    )
}

fn respond(stream: &mut std::os::unix::net::UnixStream, body: &str, init: bool) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{body}",
        body.len(),
        if init {
            "Mcp-Session-Id: test-session\r\n"
        } else {
            ""
        }
    )
    .unwrap();
}

fn tool_response(text: &str, is_error: bool) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "result": {"isError": is_error, "content": [{"type": "text", "text": text}]}
    })
    .to_string()
}

fn run_with_stub(
    args: &[&str],
    stdin: Option<&str>,
    session: Option<&str>,
    tool_text: &str,
    is_error: bool,
) -> (Output, Vec<(Vec<String>, serde_json::Value)>) {
    run_with_stub_delay(args, stdin, session, tool_text, is_error, Duration::ZERO)
}

fn run_with_stub_delay(
    args: &[&str],
    stdin: Option<&str>,
    session: Option<&str>,
    tool_text: &str,
    is_error: bool,
    delay: Duration,
) -> (Output, Vec<(Vec<String>, serde_json::Value)>) {
    let (path, listener) = socket();
    let response = tool_response(tool_text, is_error);
    let external = session.is_none();
    let with_health = args.first() != Some(&"mcp");
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..(if external { 3 } else { 2 }) + usize::from(with_health) {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream));
            let protocol_index = index.saturating_sub(usize::from(with_health));
            let body = if with_health && index == 0 {
                r#"{"ok":true}"#.to_string()
            } else if protocol_index == 0 {
                r#"{"jsonrpc":"2.0","result":{}}"#.to_string()
            } else if external && protocol_index == 1 {
                tool_response(r#"{"ok":true}"#, false)
            } else {
                std::thread::sleep(delay);
                response.clone()
            };
            respond(&mut stream, &body, index == usize::from(with_health));
        }
        requests
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic"));
    command
        .args(args)
        .env("TUIC_SOCKET", &path)
        .env_remove("TUIC_SESSION");
    if let Some(sid) = session {
        command.env("TUIC_SESSION", sid);
    }
    let output = if let Some(input) = stdin {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.output().unwrap()
    };
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("TUICommander did not start"),
        "CLI did not call MCP: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(
        output.status.code(),
        Some(2),
        "CLI rejected valid MCP command: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = server.join().unwrap();
    std::fs::remove_file(path).unwrap();
    (output, requests)
}

#[test]
fn agent_stats_uses_the_equivalent_single_request_http_route() {
    let (path, listener) = socket();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let health = read_request(&mut stream);
        respond(&mut stream, r#"{"ok":true}"#, false);
        let (mut stream, _) = listener.accept().unwrap();
        let stats = read_request(&mut stream);
        respond(
            &mut stream,
            r#"{"active_sessions":2,"max_sessions":4,"available_slots":2}"#,
            false,
        );
        (health, stats)
    });
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["agent", "stats", "--json"])
        .env("TUIC_SOCKET", &path)
        .output()
        .unwrap();
    let (health, stats) = server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        health.0[0].starts_with("GET /health HTTP/1.1"),
        "{:?}",
        health.0
    );
    assert!(
        stats.0[0].starts_with("GET /stats HTTP/1.1"),
        "{:?}",
        stats.0
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["available_slots"], 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn renamed_cli_inputs_reach_mcp_with_path_and_branch() {
    let (output, requests) = run_with_stub(
        &["agent", "list-peers", "--path", "/repo"],
        None,
        Some("peer-1"),
        r#"{"peers":[]}"#,
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests[2].1["params"]["arguments"]["path"], "/repo");
    assert!(
        requests[2].1["params"]["arguments"]
            .get("project")
            .is_none()
    );

    let (output, requests) = run_with_stub(
        &["repo", "worktree-remove", ".", "feature"],
        None,
        Some("peer-1"),
        r#"{"ok":true}"#,
        false,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(requests[2].1["params"]["arguments"]["branch"], "feature");
    assert!(
        requests[2].1["params"]["arguments"]
            .get("workspace_id")
            .is_none()
    );
}

#[test]
fn mcp_agent_wait_keeps_the_socket_open_for_eight_seconds() {
    let start = Instant::now();
    let (output, requests) = run_with_stub_delay(
        &["mcp", "agent", r#"{"action":"wait","timeout_ms":8000}"#],
        None,
        Some("peer-1"),
        r#"{"timed_out":true}"#,
        false,
        Duration::from_secs(8),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(start.elapsed() >= Duration::from_secs(8));
    assert_eq!(requests[1].1["params"]["arguments"]["timeout_ms"], 8000);
}

#[test]
fn mcp_prints_tool_text_without_reencoding() {
    let (output, requests) = run_with_stub(
        &["mcp", "session", r#"{"action":"list"}"#],
        None,
        Some("peer-1"),
        "{\"a\":1, \"b\":2}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "{\"a\":1, \"b\":2}\n"
    );
    assert_eq!(requests[1].1["params"]["name"], "session");
    assert_eq!(
        requests[1].1["params"]["arguments"],
        serde_json::json!({"action":"list"})
    );
}

#[test]
fn mcp_reports_is_error_on_stderr_with_failure_exit() {
    let (output, _) = run_with_stub(
        &["mcp", "agent", r#"{"action":"send"}"#],
        None,
        Some("peer-1"),
        r#"{"message":"Recipient not found"}"#,
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Recipient not found"));
}

#[test]
fn mcp_reports_payload_error_on_stderr_with_failure_exit() {
    let (output, _) = run_with_stub(
        &["mcp", "agent", r#"{"action":"send"}"#],
        None,
        Some("peer-1"),
        r#"{"error":"Recipient not found"}"#,
        false,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Recipient not found"));
}

#[test]
fn mcp_sends_managed_identity_header() {
    let (output, requests) = run_with_stub(
        &["mcp", "session", "{}"],
        None,
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|(headers, _)| {
        headers
            .iter()
            .any(|line| line.eq_ignore_ascii_case("x-tuic-session: peer-1\r\n"))
    }));
}

#[test]
fn mcp_registers_external_caller_before_tool() {
    let (output, requests) =
        run_with_stub(&["mcp", "session", "{}"], None, None, "{\"a\":1}", false);
    assert!(output.status.success());
    assert_eq!(requests[1].1["params"]["name"], "agent");
    assert_eq!(requests[1].1["params"]["arguments"]["action"], "register");
    assert_eq!(requests[2].1["params"]["name"], "session");
    assert!(requests.iter().all(|(headers, _)| {
        !headers
            .iter()
            .any(|line| line.to_ascii_lowercase().starts_with("x-tuic-session:"))
    }));
}

#[test]
fn mcp_reads_stdin_json_with_apostrophe() {
    let (output, requests) = run_with_stub(
        &["mcp", "agent", "-"],
        Some("{\"message\":\"it's ready\"}"),
        Some("peer-1"),
        "{\"a\":1}",
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        requests[1].1["params"]["arguments"]["message"],
        "it's ready"
    );
}

#[test]
fn mcp_rejects_bad_input_before_connecting() {
    let nonexistent = std::path::PathBuf::from(std::env::var("HOME").unwrap())
        .join("Gits/.tmp/tuic-tests/no-mcp.sock");
    for args in [
        vec!["mcp", "agent", "{"],
        vec!["mcp", "agent", "{}", "extra"],
        vec!["mcp"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
            .args(args)
            .env("TUIC_SOCKET", &nonexistent)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Cannot connect"));
    }
}
