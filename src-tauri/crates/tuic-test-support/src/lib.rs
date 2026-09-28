//! Platform-neutral helpers shared by Rust tests.

// Cargo's libtest runner has no per-binary setup hook. This crate is linked
// only into test binaries, so install their temp root before the harness starts.
#[ctor::ctor]
fn install_test_temp_root() {
    let root = test_temp_root();
    // Freeze the checkout selected at process start: tests may change cwd.
    // SAFETY: this constructor runs before libtest creates worker threads.
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &root) };
    for key in ["TMPDIR", "TMP", "TEMP"] {
        // SAFETY: a process constructor runs before main and before libtest
        // creates its worker threads. No test can read the environment yet.
        unsafe { std::env::set_var(key, &root) };
    }
}

/// Scratch space for Rust tests, overridable by the test runner.
pub fn test_temp_root() -> std::path::PathBuf {
    let root = std::env::var_os("TUIC_TEST_TMP_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            // A cached test-support crate may have been compiled in another
            // worktree. Find the checkout running this test, not its build path.
            let checkout = std::env::current_dir().ok().and_then(|cwd| {
                cwd.ancestors()
                    .find(|dir| {
                        dir.join(".git").exists() && dir.join("src-tauri/Cargo.toml").is_file()
                    })
                    .map(std::path::Path::to_path_buf)
            });
            checkout
                .unwrap_or_else(|| {
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
                })
                .join(".tmp/tuic-tests")
        });
    std::fs::create_dir_all(&root).expect("create repository test temp root");
    root
}

/// Return a short scratch path for Unix-domain socket tests.
#[cfg(unix)]
pub fn short_socket_test_temp_root() -> std::path::PathBuf {
    let requested = test_temp_root();
    if requested
        .join("sXXXXXX/home/.mdkb/daemon-hook.sock.4294967295.tmp")
        .as_os_str()
        .len()
        < 104
    {
        return requested;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "Gits"))
        .map(|gits| gits.join(".tmp/tuic-tests"))
        .unwrap_or(requested);
    std::fs::create_dir_all(&root).expect("create short socket test root");
    root
}

/// Return the host shell and its script argument.
pub fn host_shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// Print a file's contents in the host shell.
pub fn print_file_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type {path}")
    } else {
        format!("cat {path}")
    }
}

/// Create an empty file in the host shell.
pub fn touch_script(path: &str) -> String {
    if cfg!(windows) {
        format!("type nul > {path}")
    } else {
        format!("touch {path}")
    }
}

/// Copy a file in the host shell.
pub fn copy_file_script(source: &str, destination: &str) -> String {
    if cfg!(windows) {
        format!("copy /y {source} {destination} >nul")
    } else {
        format!("cp {source} {destination}")
    }
}

/// Run one host-shell script after another.
pub fn chain(first: &str, second: &str) -> String {
    if cfg!(windows) {
        format!("{first}& {second}")
    } else {
        format!("{first}; {second}")
    }
}

/// Print an environment variable in the host shell.
pub fn print_var_script(key: &str) -> String {
    if cfg!(windows) {
        format!("if defined {key} (echo %{key}%)")
    } else {
        format!("echo \"${key}\"")
    }
}

/// Return a command that sleeps longer than test timeouts.
pub fn sleep_argv() -> (String, Vec<String>) {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        (
            format!("{root}\\System32\\ping.exe"),
            vec!["-n".to_string(), "61".to_string(), "127.0.0.1".to_string()],
        )
    } else {
        ("sleep".to_string(), vec!["60".to_string()])
    }
}

/// Return the same sleep command as a host-shell script.
pub fn sleep_script() -> String {
    let (program, args) = sleep_argv();
    let call = format!("{program} {}", args.join(" "));
    if cfg!(windows) {
        format!("{call} >nul")
    } else {
        call
    }
}

/// Write a fake SSH executable to a stable path and warm it before use.
pub fn fake_ssh_script(name: &str, posix: &str, windows: &str) -> std::path::PathBuf {
    const WARMUP_VAR: &str = "TUIC_FAKE_SSH_WARMUP";

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fake-ssh");
    std::fs::create_dir_all(&dir).expect("create fake-ssh dir");
    let (extension, desired) = if cfg!(windows) {
        (
            "cmd",
            format!("@echo off\r\nif defined {WARMUP_VAR} exit /b 0\r\n{windows}\r\n"),
        )
    } else {
        (
            "sh",
            format!("#!/bin/sh\n[ -n \"${WARMUP_VAR}\" ] && exit 0\n{posix}\n"),
        )
    };
    let path = dir.join(format!("{name}.{extension}"));

    if std::fs::read_to_string(&path).is_ok_and(|found| found == desired) {
        return path;
    }

    let staging = dir.join(format!("{name}.{extension}.{}", std::process::id()));
    std::fs::write(&staging, &desired).expect("write fake ssh script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake ssh script");
    }
    std::fs::rename(&staging, &path).expect("install fake ssh script");

    let _ = std::process::Command::new(&path)
        .env(WARMUP_VAR, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    path
}

/// Write a message to stderr and exit with the given code.
pub fn fail_with_stderr_script(line: &str, code: i32) -> String {
    if cfg!(windows) {
        format!("echo {line} 1>&2& exit /b {code}")
    } else {
        format!("echo {line} >&2; exit {code}")
    }
}

/// Normalize Windows CRLF output to LF.
pub fn normalize_newlines(output: &str) -> String {
    output.replace("\r\n", "\n")
}

/// Build a host-shell command that replays a file into a PTY.
pub fn replay_file_command(path: &std::path::Path) -> portable_pty::CommandBuilder {
    let (shell, flag) = host_shell();
    let spelled = if cfg!(windows) {
        assert!(
            !path.to_string_lossy().contains(' '),
            "a space in {} cannot survive `cmd /C` through CommandBuilder; \
             give the test a temp directory without one",
            path.display(),
        );
        path.display().to_string().replace('/', "\\")
    } else {
        format!("\"{}\"", path.display())
    };
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.arg(flag);
    command.arg(print_file_script(&spelled));
    command
}

/// Return a path using forward slashes on every platform.
pub fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// Return an existing directory outside the current user's home directory.
pub fn dir_outside_home() -> std::path::PathBuf {
    if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
        )
    } else {
        std::path::PathBuf::from("/tmp")
    }
}
