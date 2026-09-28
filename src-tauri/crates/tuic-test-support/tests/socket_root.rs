#![cfg(unix)]

use std::path::{Path, PathBuf};

fn checkout(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(root.join(".git")).expect("git marker");
    std::fs::create_dir_all(root.join("src-tauri")).expect("Cargo directory");
    std::fs::write(root.join("src-tauri/Cargo.toml"), "[workspace]\n").expect("Cargo marker");
    root
}

#[test]
fn socket_root_is_stable_and_distinct_for_long_checkout_paths() {
    let sandbox = tempfile::tempdir_in(tuic_test_support::test_temp_root()).expect("sandbox");
    let original_dir = std::env::current_dir().expect("working directory");
    let original_root = std::env::var_os("TUIC_TEST_TMP_ROOT");
    let requested = sandbox
        .path()
        .join("a-long-test-root-that-needs-a-short-unix-socket-path");
    // This test binary is run one test per process by Nextest.
    unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", &requested) };

    let fake_gits = sandbox.path().join("Gits");
    let first_checkout = checkout(&fake_gits, "first-checkout");
    let second_checkout = checkout(&fake_gits, "second-checkout");
    std::env::set_current_dir(&first_checkout).expect("first checkout");
    let first = tuic_test_support::short_socket_test_temp_root();
    assert_eq!(first, tuic_test_support::short_socket_test_temp_root());

    std::env::set_current_dir(&second_checkout).expect("second checkout");
    let second = tuic_test_support::short_socket_test_temp_root();
    assert_ne!(
        first, second,
        "parallel checkouts must not share socket scratch"
    );
    assert!(first.starts_with(fake_gits.join(".tmp/tuic-tests")));
    assert!(second.starts_with(fake_gits.join(".tmp/tuic-tests")));
    assert!(first.is_dir());
    assert!(second.is_dir());
    assert!(first.as_os_str().len() < requested.as_os_str().len());
    assert!(second.as_os_str().len() < requested.as_os_str().len());

    std::env::set_current_dir(original_dir).expect("restore checkout");
    if let Some(root) = original_root {
        unsafe { std::env::set_var("TUIC_TEST_TMP_ROOT", root) };
    } else {
        unsafe { std::env::remove_var("TUIC_TEST_TMP_ROOT") };
    }
}
