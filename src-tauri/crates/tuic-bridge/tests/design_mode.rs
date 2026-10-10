#![cfg(unix)]
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};

fn install(dir: &Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    assert!(
        Command::new("git")
            .arg("init")
            .arg(&repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    let socket = dir.join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    let health = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /health "));
        let body = r#"{"instance_id":"design-test"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let profile = dir.join("profile");
    let result = Command::new(env!("CARGO_BIN_EXE_tuic-bridge"))
        .arg("--design-install")
        .arg(&profile)
        .arg(&repo)
        .arg(&socket)
        .arg("launching-session")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    health.join().unwrap();
    profile
}
fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(tuic_test_support::short_socket_test_temp_root()).unwrap()
}
// Catches: installer omitting the manifest, admitting another origin, or naming an absent executable.
#[test]
fn design_mode_installs_a_manifest_pinned_to_its_extension_and_executable() {
    let dir = scratch();
    let profile = install(dir.path());
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(profile.join("NativeMessagingHosts/com.tuic.design_mode.json")).unwrap(),
    )
    .unwrap();
    let host = profile.join("tuic-design-host/tuic-design-host");
    assert_eq!(manifest["path"], json!(host));
    assert!(host.is_file());
    assert_eq!(manifest["type"], "stdio");
    assert_eq!(
        manifest["allowed_origins"],
        json!(["chrome-extension://eefbgjmmpaimpeaflioojmdipmfchefo/"])
    );
    let binding: Value = serde_json::from_slice(
        &std::fs::read(profile.join("tuic-design-extension/binding.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(binding["sessionId"], "launching-session");
}
// Catches: the host loop bypassing version validation and relaying an incompatible request.
#[test]
fn design_mode_native_host_replies_to_a_framed_version_mismatch() {
    let dir = scratch();
    let profile = install(dir.path());
    // Installation's health listener is gone; a bypass would return an IPC failure instead.
    let mut host = Command::new(profile.join("tuic-design-host/tuic-design-host"))
        .arg("chrome-extension://eefbgjmmpaimpeaflioojmdipmfchefo/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request =
        serde_json::to_vec(&json!({"id":"old-client","version":"1.0.0","action":"hello"})).unwrap();
    let mut input = host.stdin.take().unwrap();
    input
        .write_all(&(request.len() as u32).to_ne_bytes())
        .unwrap();
    input.write_all(&request).unwrap();
    drop(input);
    let output = host.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let length = u32::from_ne_bytes(output.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(output.stdout.len(), length + 4);
    let reply: Value = serde_json::from_slice(&output.stdout[4..]).unwrap();
    assert_eq!(reply["id"], "old-client");
    assert_eq!(reply["ok"], false);
    assert_eq!(
        reply["error"],
        "Design Mode version mismatch; reinstall the extension"
    );
}
