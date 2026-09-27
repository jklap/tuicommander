#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

fn test_path(name: &str) -> std::path::PathBuf {
    let root =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Gits/.tmp/tuic-tests");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "bg-{name}-{}-{}",
        std::process::id(),
        NEXT_JOB.fetch_add(1, Ordering::Relaxed)
    ))
}

fn read_request(stream: &mut std::os::unix::net::UnixStream) -> (String, serde_json::Value) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request_line = line.trim_end().to_string();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (
        request_line,
        if body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        },
    )
}

fn reply(stream: &mut std::os::unix::net::UnixStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(path.exists(), "{} was not written", path.display());
}

#[test]
fn bg_returns_before_command_exits_and_queues_one_exact_wake() {
    let log = test_path("wake.log");
    std::fs::write(&log, "previous\n").unwrap();
    let socket = test_path("wake.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let second = read_request(&mut stream);
        reply(&mut stream, r#"{"typed":false,"queued":1}"#);
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        assert!(listener.accept().is_err(), "duplicate wake request");
        (first, second)
    });
    // Warm the freshly linked executable before measuring only launcher behavior.
    Command::new(env!("CARGO_BIN_EXE_tuic"))
        .arg("--version")
        .output()
        .unwrap();
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "sleep 2; echo body; echo problem >&2; exit 7",
        ])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    let launch_time = start.elapsed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        launch_time < Duration::from_secs(1),
        "launcher blocked for {launch_time:?}"
    );
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    assert!(
        !exit_file.exists(),
        "launcher wrote the exit before the command ended"
    );
    wait_for(&exit_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "7");
    let log_text = std::fs::read_to_string(&log).unwrap();
    assert!(
        log_text.contains("previous\n")
            && log_text.contains("body")
            && log_text.contains("problem")
    );
    let (first, second) = server.join().unwrap();
    assert!(
        !std::fs::read_to_string(&log)
            .unwrap()
            .contains("wake failed")
    );
    assert_eq!(first.0, "GET /sessions HTTP/1.1");
    assert_eq!(second.0, "POST /sessions/pty-1/queue HTTP/1.1");
    let wake = second.1["text"].as_str().unwrap();
    assert!(
        wake.starts_with(&format!("BG DONE exit=7 log={} cmd=sh -c", log.display())),
        "{wake}"
    );
    std::fs::remove_file(exit_file).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_refuses_to_start_without_a_managed_session() {
    let log = test_path("missing.log");
    let marker = test_path("missing.marker");
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "touch \"$1\"",
            "_",
            marker.to_str().unwrap(),
        ])
        .env_remove("TUIC_SESSION")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists() && !log.exists());
}

#[test]
fn bg_creates_missing_log_directories_and_wakes_caller() {
    let root = test_path("new-parent");
    let log = root.join("nested/job.log");
    let socket = test_path("new-parent.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let first = read_request(&mut stream);
        reply(
            &mut stream,
            r#"[{"session_id":"pty-1","tuic_session":"caller-1"}]"#,
        );
        let (mut stream, _) = listener.accept().unwrap();
        let second = read_request(&mut stream);
        reply(&mut stream, r#"{"typed":false,"queued":1}"#);
        (first, second)
    });
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["bg", log.to_str().unwrap(), "--", "sh", "-c", "echo five"])
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    wait_for(&std::path::PathBuf::from(format!("{}.exit", log.display())));
    assert!(std::fs::read_to_string(&log).unwrap().contains("five"));
    let (first, second) = server.join().unwrap();
    assert_eq!(first.0, "GET /sessions HTTP/1.1");
    assert_eq!(second.0, "POST /sessions/pty-1/queue HTTP/1.1");
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn bg_refuses_an_uncreatable_log_before_running_command() {
    let parent_file = test_path("blocked-parent");
    let marker = test_path("blocked-marker");
    std::fs::write(&parent_file, "not a directory").unwrap();
    let log = parent_file.join("job.log");
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "touch \"$1\"",
            "_",
            marker.to_str().unwrap(),
        ])
        .env("TUIC_SESSION", "caller-1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists(), "command ran despite unusable log");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Cannot create background log directory")
    );
    std::fs::remove_file(parent_file).unwrap();
}

#[test]
fn bg_command_survives_killing_its_launchers_process_group() {
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    let log = test_path("group.log");
    let marker = test_path("group.marker");
    let mut shell = Command::new("sh");
    shell.args([
        "-c",
        "\"$1\" bg \"$2\" -- sh -c 'echo started; sleep 2; touch \"$1\"' _ \"$3\"; sleep 30",
        "sh",
        env!("CARGO_BIN_EXE_tuic"),
        log.to_str().unwrap(),
        marker.to_str().unwrap(),
    ]);
    shell
        .env("TUIC_SESSION", "caller-1")
        .env("TUIC_SOCKET", test_path("absent.sock"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = shell.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("started")
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("started")
    );
    assert_eq!(unsafe { kill(-(child.id() as i32), 9) }, 0);
    let _ = child.wait();
    wait_for(&std::path::PathBuf::from(format!("{}.exit", log.display())));
    assert!(
        marker.exists(),
        "detached command died with the launcher group"
    );
    std::fs::remove_file(format!("{}.exit", log.display())).unwrap();
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(marker).unwrap();
}
