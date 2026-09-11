//! Story 736-0afd: the `tuic-remote` named-instance CLI contract.
//!
//! The binary only has a real `main` without the desktop feature, so the whole
//! file is gated: with `--features desktop` the binary is a stub that always
//! exits 1 and every assertion here would be meaningless. CI runs it in the
//! `Remote daemon (no-default-features)` job.
//!
//! Each case runs in an isolated environment so a test cannot read or mutate the
//! developer's real TUIC configuration or keyring.
#![cfg(not(feature = "desktop"))]

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

const DEFAULT_ID: &str = "default";
const NAMED_ID: &str = "work-laptop";
const PASSWORD: &str = "red-test-password";

#[test]
fn binary_rejects_duplicate_missing_unknown_and_malformed_instance_arguments() {
    let env = IsolatedEnv::new();

    assert_cli_failure(&env, &["--instance", NAMED_ID, "--instance", "other"]);
    assert_cli_failure(&env, &["--instance"]);
    assert_cli_failure(&env, &["--instance", DEFAULT_ID]);
    assert_cli_failure(&env, &["--instance", "../escape"]);
    assert_cli_failure(&env, &["--instance", "Work-Laptop"]);
    assert_cli_failure(&env, &["--unknown-app-instance-flag"]);
}

#[test]
fn binary_default_set_password_remains_compatible() {
    let env = IsolatedEnv::new();

    assert_cli_success(
        &env,
        &["--set-password"],
        Some("test-user\n".to_owned() + PASSWORD + "\n"),
    );
    assert!(env.default_config().is_file());
    assert!(!env.named_config(NAMED_ID).exists());
}

#[test]
fn binary_named_set_password_writes_only_named_config() {
    let env = IsolatedEnv::new();

    assert_cli_success(
        &env,
        &["--instance", NAMED_ID, "--set-password"],
        Some("test-user\n".to_owned() + PASSWORD + "\n"),
    );
    assert!(env.named_config(NAMED_ID).is_file());
    assert!(!env.default_config().exists());
}

#[test]
fn named_config_ignores_seeded_legacy_directories() {
    let env = IsolatedEnv::new();
    env.seed_legacy_config("legacy-value");

    assert_cli_success(
        &env,
        &["--instance", NAMED_ID, "--set-password"],
        Some("test-user\n".to_owned() + PASSWORD + "\n"),
    );
    assert_eq!(
        fs::read_to_string(env.legacy_config()).unwrap(),
        "legacy-value"
    );
    assert!(env.named_config(NAMED_ID).is_file());
}

#[test]
fn named_debug_vault_ignores_and_does_not_mutate_default_legacy_entries() {
    let env = IsolatedEnv::new();
    env.seed_legacy_vault_entries();
    let legacy_path = env.debug_vault();
    let legacy_before = fs::read(&legacy_path).unwrap();

    run_cli_until_killed(&env, &["--instance", NAMED_ID], Duration::from_secs(5));

    assert_eq!(fs::read(&legacy_path).unwrap(), legacy_before);
    assert_eq!(legacy_path, env.default_root().join("credentials.json"));
}

#[test]
fn default_set_password_preserves_legacy_config() {
    let env = IsolatedEnv::new();
    env.seed_legacy_config("legacy-value");
    let legacy_before = fs::read(env.legacy_config()).unwrap();

    assert_cli_success(
        &env,
        &["--set-password"],
        Some("test-user\n".to_owned() + PASSWORD + "\n"),
    );

    assert_eq!(fs::read(env.legacy_config()).unwrap(), legacy_before);
    assert!(env.default_config().is_file());
}

struct IsolatedEnv {
    _dir: TempDir,
    home: PathBuf,
    config: PathBuf,
    appdata: PathBuf,
    userprofile: PathBuf,
}

impl IsolatedEnv {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temporary test directory");
        let home = dir.path().join("home");
        let config = dir.path().join("xdg-config");
        let appdata = dir.path().join("appdata");
        let userprofile = dir.path().join("userprofile");
        fs::create_dir_all(&home).expect("temporary HOME");
        fs::create_dir_all(&config).expect("temporary XDG_CONFIG_HOME");
        fs::create_dir_all(&appdata).expect("temporary APPDATA");
        fs::create_dir_all(&userprofile).expect("temporary USERPROFILE");
        Self {
            _dir: dir,
            home,
            config,
            appdata,
            userprofile,
        }
    }

    fn default_root(&self) -> PathBuf {
        self.home.join(".tuicommander-dev")
    }

    fn default_config(&self) -> PathBuf {
        self.platform_config()
            .join("com.tuic.commander")
            .join("config.json")
    }

    fn named_config(&self, id: &str) -> PathBuf {
        self.platform_config()
            .join("com.tuic.commander")
            .join("instances")
            .join(id)
            .join("config.json")
    }

    fn legacy_config(&self) -> PathBuf {
        self.home.join(".tuicommander").join("config.json")
    }

    fn debug_vault(&self) -> PathBuf {
        self.default_root().join("credentials.json")
    }

    fn platform_config(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.home.join("Library/Application Support")
        } else if cfg!(target_os = "windows") {
            self.home.join("AppData/Roaming")
        } else {
            self.config.clone()
        }
    }

    fn seed_legacy_config(&self, value: &str) {
        let path = self.legacy_config();
        fs::create_dir_all(path.parent().unwrap()).expect("legacy config directory");
        fs::write(path, value).expect("legacy config");
    }

    fn seed_legacy_vault_entries(&self) {
        let path = self.debug_vault();
        fs::create_dir_all(path.parent().unwrap()).expect("legacy vault directory");
        fs::write(
            path,
            r#"{
  "tuicommander-ai-chat/api-key": "legacy-ai-key",
  "tuicommander-mcp/upstream": "legacy-mcp-upstream"
}"#,
        )
        .expect("legacy vault entries");
    }
}

fn assert_cli_success(env: &IsolatedEnv, args: &[&str], stdin: Option<String>) {
    let output = run_cli(env, args, stdin);
    assert!(
        output.status.success(),
        "CLI failed: status={:?}, stdout={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_cli_failure(env: &IsolatedEnv, args: &[&str]) {
    let output = run_cli(env, args, None);
    assert!(
        !output.status.success(),
        "CLI unexpectedly succeeded: {args:?}"
    );
}

fn run_cli(env: &IsolatedEnv, args: &[&str], stdin: Option<String>) -> std::process::Output {
    let port = isolated_port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic-remote"));
    command
        .args(args)
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config)
        .env("APPDATA", &env.appdata)
        .env("USERPROFILE", &env.userprofile)
        .env("TUIC_PORT", port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn isolated CLI");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(input.as_bytes())
            .expect("write password input");
    }
    reap_with_timeout(child, Duration::from_secs(5))
}

fn run_cli_until_killed(env: &IsolatedEnv, args: &[&str], timeout: Duration) {
    let port = isolated_port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tuic-remote"));
    command
        .args(args)
        .env("HOME", &env.home)
        .env("XDG_CONFIG_HOME", &env.config)
        .env("APPDATA", &env.appdata)
        .env("USERPROFILE", &env.userprofile)
        .env("TUIC_PORT", port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("spawn isolated CLI");
    if !wait_for_health(port, timeout) {
        let _ = child.kill();
        let _ = reap_with_timeout(child, Duration::from_secs(5));
        panic!("isolated CLI did not become healthy within {:?}", timeout);
    }
    let _ = child.kill();
    let _ = reap_with_timeout(child, Duration::from_secs(5));
}

fn isolated_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind isolated port");
    listener.local_addr().expect("read isolated port").port()
}

fn wait_for_health(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(100),
        ) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
            if stream
                .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .is_ok()
            {
                let mut response = Vec::new();
                if stream.read_to_end(&mut response).is_ok()
                    && response.starts_with(b"HTTP/1.1 200")
                {
                    return true;
                }
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    false
}

fn reap_with_timeout(mut child: Child, timeout: Duration) -> std::process::Output {
    let started = Instant::now();
    loop {
        if child.try_wait().expect("poll isolated CLI").is_some() {
            return child.wait_with_output().expect("collect CLI output");
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated CLI exceeded {:?} timeout", timeout);
        }
        thread::sleep(Duration::from_millis(10));
    }
}
