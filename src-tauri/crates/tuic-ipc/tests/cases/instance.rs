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
