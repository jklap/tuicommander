// How a test process picks its scratch roots when nothing (no wrapper, no
// nextest setup script) chose them first: the bare `cargo test` path.

use std::path::PathBuf;

const CHILD: &str = "TUIC_TEMP_ROOTS_CHILD";

fn run_child(test: &str, configure: impl FnOnce(&mut std::process::Command)) {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", test, "--nocapture"]).env(CHILD, "1");
    for key in [
        "TUIC_TEST_TMP_ROOT",
        "TUIC_TEST_TMP_BASE",
        "TUIC_TEST_HOST_TMPDIR",
        "TUIC_TEST_SOCKET_ROOT",
    ] {
        child.env_remove(key);
    }
    configure(&mut child);
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn host_fixture() -> tempfile::TempDir {
    tempfile::tempdir_in(tuic_test_support::test_temp_root()).unwrap()
}

#[test]
fn bare_test_process_derives_its_roots_from_the_incoming_tmpdir() {
    let host = host_fixture();
    run_child("child_sees_roots_under_host", |child| {
        child
            .env("TMPDIR", host.path())
            .env("TMP", host.path())
            .env("TEMP", host.path())
            .env("EXPECTED_HOST", host.path());
    });
}

#[test]
fn child_sees_roots_under_host() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let host = PathBuf::from(std::env::var_os("EXPECTED_HOST").unwrap());
    assert_eq!(tuic_test_support::host_temp_dir(), host);
    assert_eq!(tuic_test_support::test_base(), host.join("tuic-tests"));
    let root = tuic_test_support::test_temp_root();
    assert!(
        root.starts_with(host.join("tuic-tests")),
        "{} is not under the host temp dir",
        root.display()
    );
    // The constructor redirects the process temp dir at the per-run root.
    assert_eq!(std::env::temp_dir(), root);
}

// Catches: the default root landing inside the checkout, where
// find_repo_root() walks up from a fixture into the real repository.
#[test]
fn default_root_is_outside_the_checkout() {
    let root = tuic_test_support::test_temp_root();
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let checkout = checkout.canonicalize().unwrap();
    if std::env::var_os("TUIC_TEST_TMP_BASE").is_none() {
        assert!(
            !root.canonicalize().unwrap().starts_with(&checkout),
            "{} is inside the checkout {}",
            root.display(),
            checkout.display()
        );
    }
}

#[test]
fn tuic_test_tmp_base_opt_in_restores_a_chosen_base() {
    let base = host_fixture();
    run_child("child_sees_root_under_base", |child| {
        child
            .env("TUIC_TEST_TMP_BASE", base.path())
            .env("EXPECTED_BASE", base.path());
    });
}

#[test]
fn child_sees_root_under_base() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let base = PathBuf::from(std::env::var_os("EXPECTED_BASE").unwrap());
    assert_eq!(tuic_test_support::test_base(), base);
    assert!(tuic_test_support::test_temp_root().starts_with(&base));
}

#[test]
fn trailing_slash_on_tmpdir_is_dropped() {
    let host = host_fixture();
    let slashed = format!("{}/", host.path().display());
    run_child("child_sees_roots_under_host", |child| {
        child
            .env("TMPDIR", &slashed)
            .env("TMP", &slashed)
            .env("TEMP", &slashed)
            .env("EXPECTED_HOST", host.path());
    });
}
