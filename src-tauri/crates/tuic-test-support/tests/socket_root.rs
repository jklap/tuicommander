#![cfg(unix)]
// Nextest runs every test in this binary in its own process, so each test may
// change the environment and the working directory without restoring them.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn checkout(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(root.join(".git")).expect("git marker");
    std::fs::create_dir_all(root.join("src-tauri")).expect("Cargo directory");
    std::fs::write(root.join("src-tauri/Cargo.toml"), "[workspace]\n").expect("Cargo marker");
    root
}

/// A short scratch directory under `/tmp`, removed on drop. Short host temp
/// dirs only exist there on macOS, where the default `$TMPDIR` is 48 chars.
fn short_scratch(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("tuic-{tag}"))
        .tempdir_in("/tmp")
        .expect("short scratch under /tmp")
}

/// Force the requested per-run root past the socket budget, so the resolver
/// has to pick one of its short candidates.
fn request_long_root() -> PathBuf {
    let requested = tuic_test_support::test_temp_root()
        .join("a-long-test-root-that-needs-a-short-unix-socket-path");
    assert!(!tuic_test_support::socket_root_fits(&requested));
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &requested) };
    requested
}

fn assert_binds_longest_socket(root: &Path) {
    let dir = tempfile::Builder::new()
        .prefix("s")
        .tempdir_in(root)
        .expect("socket directory");
    let socket = dir.path().join(".mdkb/daemon-hook.sock.4294967295.tmp");
    std::fs::create_dir_all(socket.parent().unwrap()).expect("mdkb directory");
    assert!(socket.as_os_str().len() + tuic_test_support::HOME_MARGIN < 104);
    // Creating the path is what the sandbox allows; binding is the real proof
    // where loopback/unix binds are permitted.
    if let Ok(listener) = std::os::unix::net::UnixListener::bind(&socket) {
        drop(listener);
    }
}

// Exact math: macOS sun_path is 104 bytes including the NUL; mdkb's staging
// socket is the longest name any test binds; HOME may be 8 bytes longer on CI.
#[test]
fn socket_budget_is_exactly_forty_nine_chars() {
    assert_eq!(tuic_test_support::SUN_PATH_MAX, 104);
    assert_eq!(
        tuic_test_support::LONGEST_TEST_SOCKET_SUFFIX,
        "sXXXXXX/.mdkb/daemon-hook.sock.4294967295.tmp"
    );
    assert_eq!(tuic_test_support::MAX_SOCKET_ROOT_LEN, 49);
    let fits = PathBuf::from(format!("/{}", "x".repeat(48)));
    let too_long = PathBuf::from(format!("/{}", "x".repeat(49)));
    assert!(tuic_test_support::socket_root_fits(&fits));
    assert!(!tuic_test_support::socket_root_fits(&too_long));
    // The full longest socket under the longest allowed root, plus the HOME
    // margin, leaves exactly the NUL byte.
    let longest = fits.join(tuic_test_support::LONGEST_TEST_SOCKET_SUFFIX);
    assert_eq!(
        longest.as_os_str().len() + tuic_test_support::HOME_MARGIN,
        tuic_test_support::SUN_PATH_MAX - 1
    );
}

// Catches: a test socket name outgrowing the suffix the budget is computed from.
#[test]
fn every_test_socket_name_is_shorter_than_the_budgeted_suffix() {
    let pid = u32::MAX;
    // A per-process counter; no test binds anywhere near this many sockets.
    let n = 99_999;
    for name in [
        "mcp-4294967295.sock".to_string(),
        "story.sock".to_string(),
        format!("tuic-mcp-{pid}-{n}.sock"),
        format!("tuic-wait-{pid}-{n}.sock"),
        format!("tuic-nonwait-{pid}-{n}.sock"),
        format!("tuic-drop-{pid}-{n}.sock"),
        format!("no-mcp-{pid}.sock"),
    ] {
        assert!(
            name.len() <= tuic_test_support::LONGEST_TEST_SOCKET_SUFFIX.len(),
            "{name} is longer than the budgeted suffix"
        );
    }
}

#[test]
fn socket_root_is_stable_and_distinct_per_checkout_and_ignores_gits_ancestors() {
    // The current directory reads back canonical (`/private/tmp` on macOS),
    // so build the fake `Gits` parent there and keep it short enough that the
    // old ancestor search would have picked it.
    let scratch = tempfile::Builder::new()
        .prefix("g")
        .rand_bytes(4)
        .tempdir_in("/tmp")
        .expect("short scratch");
    request_long_root();
    let gits = scratch.path().canonicalize().unwrap().join("Gits");
    assert!(tuic_test_support::socket_root_fits(
        &gits.join(".tmp").join("s0123456789abcdef")
    ));
    let first_checkout = checkout(&gits, "first-checkout");
    let second_checkout = checkout(&gits, "second-checkout");
    std::env::set_current_dir(&first_checkout).expect("first checkout");
    let first = tuic_test_support::short_socket_test_temp_root();
    assert_eq!(first, tuic_test_support::short_socket_test_temp_root());
    std::env::set_current_dir(&second_checkout).expect("second checkout");
    let second = tuic_test_support::short_socket_test_temp_root();
    assert_ne!(
        first, second,
        "parallel checkouts must not share socket scratch"
    );
    for root in [&first, &second] {
        assert!(
            !root.starts_with(&gits),
            "socket root {} sits under a Gits ancestor",
            root.display()
        );
        assert!(root.is_dir());
        assert!(tuic_test_support::socket_root_fits(root));
        assert_binds_longest_socket(root);
    }
}

#[test]
fn short_host_temp_dir_gets_a_private_per_checkout_socket_root() {
    let host = short_scratch("th");
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host.path()) };
    request_long_root();
    let root = tuic_test_support::short_socket_test_temp_root();
    assert_eq!(
        root,
        host.path().join(tuic_test_support::socket_dir_name()),
        "a short host TMPDIR must hold the socket root"
    );
    let mode = std::fs::metadata(&root).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "socket root must be private, got {mode:o}");
    assert_binds_longest_socket(&root);
}

// Catches: a length-only check that picks a candidate it cannot write.
#[test]
fn unwritable_host_candidate_is_skipped() {
    let host = short_scratch("tu");
    std::fs::set_permissions(host.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host.path()) };
    request_long_root();
    let root = tuic_test_support::short_socket_test_temp_root();
    std::fs::set_permissions(host.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        !root.starts_with(host.path()),
        "picked {} inside the read-only host temp dir",
        root.display()
    );
    assert!(root.is_dir());
    assert!(tuic_test_support::socket_root_fits(&root));
}

// Catches: following a symlink another user planted at the predictable path.
#[test]
fn symlinked_candidate_is_refused() {
    let host = short_scratch("tl");
    let elsewhere = short_scratch("tx");
    std::os::unix::fs::symlink(
        elsewhere.path(),
        host.path().join(tuic_test_support::socket_dir_name()),
    )
    .unwrap();
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host.path()) };
    request_long_root();
    let root = tuic_test_support::short_socket_test_temp_root();
    assert!(
        !root.starts_with(host.path()),
        "followed {}",
        root.display()
    );
}

// Catches: a pre-existing lax-mode socket dir staying world-readable.
#[test]
fn pre_existing_lax_socket_root_is_tightened() {
    let host = short_scratch("tm");
    let lax = host.path().join(tuic_test_support::socket_dir_name());
    std::fs::create_dir(&lax).unwrap();
    std::fs::set_permissions(&lax, std::fs::Permissions::from_mode(0o755)).unwrap();
    unsafe { std::env::set_var("TUIC_TEST_HOST_TMPDIR", host.path()) };
    request_long_root();
    assert_eq!(tuic_test_support::short_socket_test_temp_root(), lax);
    let mode = std::fs::metadata(&lax).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn explicit_socket_root_override_is_honoured() {
    let scratch = short_scratch("te");
    let explicit = scratch.path().join("sockets");
    unsafe { std::env::set_var("TUIC_TEST_SOCKET_ROOT", &explicit) };
    request_long_root();
    assert_eq!(tuic_test_support::short_socket_test_temp_root(), explicit);
    assert!(explicit.is_dir());
}

// Catches: a mystery EACCES or a silent fallback instead of naming the fix.
#[test]
fn unusable_override_fails_naming_the_budget_and_the_variable() {
    let too_long = PathBuf::from(format!("/tmp/{}", "y".repeat(60)));
    unsafe { std::env::set_var("TUIC_TEST_SOCKET_ROOT", &too_long) };
    let panic = std::panic::catch_unwind(tuic_test_support::short_socket_test_temp_root)
        .expect_err("an over-budget override must fail");
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(message.contains("TUIC_TEST_SOCKET_ROOT"), "{message}");
    assert!(message.contains("49"), "{message}");
    assert!(
        message.contains(&too_long.display().to_string()),
        "{message}"
    );
    assert!(!too_long.exists());
}

#[test]
fn socket_root_leaves_room_for_mdkb_socket_with_longer_home() {
    let root = tuic_test_support::short_socket_test_temp_root();
    let daemon_socket = root.join("sXXXXXX/.mdkb/daemon-hook.sock.4294967295.tmp");
    assert!(
        daemon_socket.as_os_str().len() + 8 < 104,
        "mdkb socket path needs eight bytes of HOME margin: {}",
        daemon_socket.display()
    );
}

// Catches: discarding an explicitly supplied root even when it fits the socket budget.
#[test]
fn socket_root_preserves_a_requested_root_that_fits() {
    let root = tuic_test_support::short_socket_test_temp_root();
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &root) };
    assert_eq!(tuic_test_support::short_socket_test_temp_root(), root);
}

#[test]
fn short_socket_path_stays_under_the_socket_root() {
    let path = tuic_test_support::short_socket_path("story.sock");
    assert_eq!(
        path.parent(),
        Some(tuic_test_support::short_socket_test_temp_root().as_path())
    );
    assert!(path.as_os_str().len() < tuic_test_support::SUN_PATH_MAX);
}

// Catches: resolving any root from $HOME (the old `~/Gits` convention).
#[test]
fn no_root_is_derived_from_home() {
    let fake_home = short_scratch("hh");
    unsafe { std::env::set_var("HOME", fake_home.path()) };
    for root in [
        tuic_test_support::host_temp_dir(),
        tuic_test_support::test_base(),
        tuic_test_support::test_temp_root(),
        tuic_test_support::short_socket_test_temp_root(),
    ] {
        assert!(
            !root.starts_with(fake_home.path()),
            "{} derives from HOME",
            root.display()
        );
    }
}
