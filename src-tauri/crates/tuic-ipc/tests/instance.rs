use std::path::Path;
use tuic_ipc::app_instance::AppInstance;

// Catches: changing default paths, losing instance config isolation or changing the deployed hash.
#[test]
fn ipc_instance_paths_preserve_default_and_named_namespaces() {
    let default = AppInstance::default();
    let named = AppInstance::named("ipc-cli-regression").unwrap();
    let base = Path::new("/platform");
    let home = Path::new("/home");
    let temp = Path::new("/scratch");
    assert_eq!(
        default.config_dir_from(Some(base), home),
        Path::new("/platform/com.tuic.commander")
    );
    assert_eq!(
        default.config_dir_from(None, home),
        Path::new("/home/.tuicommander")
    );
    let config = named.config_dir_from(Some(base), home);
    assert_eq!(
        config,
        Path::new("/platform/com.tuic.commander/instances/ipc-cli-regression")
    );
    assert_eq!(
        tuic_ipc::socket_path(&default, &default.config_dir_from(Some(base), home), temp),
        Path::new("/platform/com.tuic.commander/mcp.sock")
    );
    assert_eq!(
        tuic_ipc::socket_path(&named, &config, temp),
        Path::new("/scratch/tuic-mcp-1953e60b4e4d5340.sock")
    );
    for invalid in ["", "default", "UPPER", "../escape", "-start", "end-"] {
        assert!(AppInstance::named(invalid).is_err(), "{invalid}");
    }
}

// Catches: the alternate drifting from the `tuic-mcp-<hash>-<pid>.sock` shape
// the bridge's fallback scan and the Local-connect identity check accept.
#[test]
fn named_alternate_socket_keeps_the_primary_stem_and_appends_the_pid() {
    let temp = Path::new("/scratch");
    assert_eq!(
        tuic_ipc::named_alternate_socket_path("ipc-cli-regression", temp, 4242),
        Path::new("/scratch/tuic-mcp-1953e60b4e4d5340-4242.sock")
    );
}

// Catches: an over-long socket path reaching bind() as an opaque SUN_LEN error,
// and an off-by-one at the 103/104-byte boundary.
#[test]
fn unix_socket_path_check_is_exact_at_the_boundary_and_names_the_way_out() {
    assert_eq!(tuic_ipc::UNIX_SOCKET_PATH_MAX, 103);
    let name = "tuic-mcp-1953e60b4e4d5340-4242.sock";
    let dir = |len: usize| format!("/{}", "d".repeat(len - 1));
    // 103 bytes fits, 104 does not (Rust std refuses it: SUN_LEN).
    let fits = Path::new(&dir(103 - 1 - name.len())).join(name);
    assert_eq!(fits.as_os_str().len(), 103);
    assert!(tuic_ipc::check_unix_socket_path(&fits).is_ok());
    let over = Path::new(&dir(104 - 1 - name.len())).join(name);
    assert_eq!(over.as_os_str().len(), 104);
    let message = tuic_ipc::check_unix_socket_path(&over).unwrap_err();
    for needle in [
        "104 bytes",
        "103-byte",
        "TMPDIR",
        "at most 67 bytes",
        "TUIC_SOCKET",
    ] {
        assert!(message.contains(needle), "{needle} missing: {message}");
    }
    // The 72-byte sandbox TMPDIR holds the primary with no headroom, never
    // the alternate.
    let sandbox = format!("/{}", "s".repeat(71));
    let primary = tuic_ipc::named_socket_path("ipc-cli-regression", Path::new(&sandbox));
    assert_eq!(primary.as_os_str().len(), 103);
    assert!(tuic_ipc::check_unix_socket_path(&primary).is_ok());
    let alternate =
        tuic_ipc::named_alternate_socket_path("ipc-cli-regression", Path::new(&sandbox), 4242);
    assert!(tuic_ipc::check_unix_socket_path(&alternate).is_err());
}
