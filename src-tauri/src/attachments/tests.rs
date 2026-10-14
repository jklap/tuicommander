use super::*;

// Catches: closing one session removes sibling session files or legacy repository files.
#[test]
fn attachment_cleanup_removes_only_ended_session_and_startup_orphans() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let ended = prepare_session_directory(data.path(), "pty", "ended").unwrap();
    let live = prepare_session_directory(data.path(), "pty", "live").unwrap();
    std::fs::write(ended.join("fresh.txt"), b"ended").unwrap();
    std::fs::write(live.join("fresh.txt"), b"live").unwrap();
    let foreign = data.path().join("attachments/user-files");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("keep.txt"), b"user").unwrap();
    cleanup_session(data.path(), "pty", "ended");
    assert!(!ended.exists());
    assert_eq!(std::fs::read(live.join("fresh.txt")).unwrap(), b"live");
    // At startup there are no live PTYs: saved tabs will get fresh PTY ids.
    cleanup_orphans(data.path());
    assert!(!live.exists());
    assert_eq!(std::fs::read(foreign.join("keep.txt")).unwrap(), b"user");
    let resumed = prepare_session_directory(data.path(), "pty", "resumed").unwrap();
    assert_eq!(std::fs::read_dir(resumed).unwrap().count(), 0);
}

// Catches: a crafted target id escapes the app-owned attachment root on any host.
#[test]
fn attachment_directory_rejects_cross_platform_traversal() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    for id in [
        "",
        "..",
        "../repo",
        "/etc/passwd",
        "C:\\repo",
        "\\\\server\\share",
        "x/y",
    ] {
        assert!(
            prepare_session_directory(data.path(), "pty", id).is_err(),
            "{id}"
        );
        cleanup_session(data.path(), "pty", id);
    }
    assert!(!data.path().join("attachments").exists());
}

// Catches: prepare or startup cleanup follows a symlink and writes/deletes user data.
#[cfg(unix)]
#[test]
fn attachment_symlinks_never_reach_external_files() {
    let data = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let external = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(external.path().join("keep.txt"), b"outside").unwrap();
    std::os::unix::fs::symlink(external.path(), data.path().join("attachments")).unwrap();
    assert!(prepare_session_directory(data.path(), "pty", "escape").is_err());
    cleanup_orphans(data.path());
    assert_eq!(
        std::fs::read(external.path().join("keep.txt")).unwrap(),
        b"outside"
    );
    std::fs::remove_file(data.path().join("attachments")).unwrap();
    std::fs::create_dir(data.path().join("attachments")).unwrap();
    std::os::unix::fs::symlink(external.path(), data.path().join("attachments/pty-escape"))
        .unwrap();
    assert!(prepare_session_directory(data.path(), "pty", "escape").is_err());
    cleanup_session(data.path(), "pty", "escape");
    cleanup_orphans(data.path());
    assert_eq!(
        std::fs::read(external.path().join("keep.txt")).unwrap(),
        b"outside"
    );
}
