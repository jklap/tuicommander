#![cfg(windows)]

use std::process::Command;
use std::time::{Duration, Instant};

fn test_root() -> std::path::PathBuf {
    let root = std::env::var_os("TUIC_TEST_TMP_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp/bg-tests")
        });
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(path.exists(), "{} was not written", path.display());
}

fn remove_when_closed(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if std::fs::remove_file(path).is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "could not remove {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn bg_windows_parent_returns_before_detached_command_and_records_its_exit() {
    let root = test_root();
    let log = root.join(format!("bg-windows-{}.log", std::process::id()));
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    let _ = std::fs::remove_file(&exit_file);
    let _ = std::fs::remove_file(&log);
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
            "cmd",
            "/C",
            "ping -n 3 127.0.0.1 >NUL & echo body & exit /B 7",
        ])
        .env("TUIC_SESSION", "windows-caller")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "launcher waited for the command"
    );
    assert!(
        !exit_file.exists(),
        "command had not finished when launcher returned"
    );
    wait_for(&exit_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "7");
    assert!(std::fs::read_to_string(&log).unwrap().contains("body"));
    remove_when_closed(&exit_file);
    remove_when_closed(&log);
}

#[test]
fn bg_windows_without_managed_session_runs_nothing() {
    let root = test_root();
    let log = root.join(format!("bg-windows-no-session-{}.log", std::process::id()));
    let marker = root.join("bg-windows-ran.txt");
    let _ = std::fs::remove_file(&marker);
    let _ = std::fs::remove_file(&log);
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "cmd",
            "/C",
            "echo ran > bg-windows-ran.txt",
        ])
        .current_dir(&root)
        .env_remove("TUIC_SESSION")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists() && !log.exists());
}

#[test]
fn bg_windows_creates_missing_log_directories() {
    let root = test_root().join(format!("bg-windows-new-parent-{}", std::process::id()));
    let log = root.join("nested/job.log");
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args([
            "bg",
            log.to_str().unwrap(),
            "--",
            "cmd",
            "/C",
            "echo created",
        ])
        .env("TUIC_SESSION", "windows-caller")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exit_file = std::path::PathBuf::from(format!("{}.exit", log.display()));
    wait_for(&exit_file);
    assert_eq!(std::fs::read_to_string(&exit_file).unwrap().trim(), "0");
    assert!(std::fs::read_to_string(&log).unwrap().contains("created"));
    remove_when_closed(&exit_file);
    remove_when_closed(&log);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bg_windows_refuses_an_uncreatable_log_before_running_command() {
    let root = test_root();
    let parent_file = root.join(format!("bg-windows-blocked-parent-{}", std::process::id()));
    let marker = root.join(format!("bg-windows-blocked-marker-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    std::fs::write(&parent_file, "not a directory").unwrap();
    let log = parent_file.join("job.log");
    let command = format!("echo ran > \"{}\"", marker.display());
    let output = Command::new(env!("CARGO_BIN_EXE_tuic"))
        .args(["bg", log.to_str().unwrap(), "--", "cmd", "/C", &command])
        .current_dir(&root)
        .env("TUIC_SESSION", "windows-caller")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!marker.exists() && !log.exists());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Cannot create background log directory")
    );
    std::fs::remove_file(parent_file).unwrap();
}
