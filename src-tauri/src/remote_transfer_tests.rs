use super::*;
use std::io::Write;

fn archive(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes).unwrap();
    builder.into_inner().unwrap()
}

// Catches: an upload route missing from tuic-remote, bypassing its auth, or using caller-supplied roots.
#[tokio::test]
#[serial_test::serial]
async fn daemon_router_upload_requires_existing_auth_and_registered_destination() {
    use tower::ServiceExt;
    let config = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    std::fs::write(config.path().join("repositories.json"), serde_json::json!({
        "repos": { repo.path().to_str().unwrap(): { "path": repo.path().to_str().unwrap(), "branches": {} } }
    }).to_string()).unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = "existing-token".into();
    state.config.write().services.auth.lan_auth_bypass = false;
    let app = crate::mcp_http::build_remote_router(state);
    let query = format!(
        "/fs/upload-copy?destDir={}&name=file&directory=false",
        repo.path().display()
    );
    for (suffix, expected) in [
        ("", axum::http::StatusCode::UNAUTHORIZED),
        ("&token=existing-token", axum::http::StatusCode::OK),
    ] {
        let mut request = axum::http::Request::post(format!("{query}{suffix}"))
            .body(Body::from(archive("file", b"remote bytes")))
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        if suffix.is_empty() {
            assert!(!repo.path().join("file").exists());
        }
    }
    assert_eq!(
        std::fs::read(repo.path().join("file")).unwrap(),
        b"remote bytes"
    );
}

// Catches: upload resolving its destination on the sending host, or publishing partial bytes.
#[tokio::test]
async fn uploaded_copy_publishes_binary_bytes_and_preserves_existing_names() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    let query = UploadQuery {
        dest_dir: roots[0].clone(),
        name: "image.bin".into(),
        directory: false,
    };
    let bytes = [0, 255, 10, 128];
    let result = receive_copy(
        query.clone(),
        &roots,
        axum::body::Body::from(archive("image.bin", &bytes)),
    )
    .await
    .unwrap();
    assert_eq!(result.moved, 1);
    assert_eq!(std::fs::read(repo.path().join("image.bin")).unwrap(), bytes);
    let result = receive_copy(
        query,
        &roots,
        axum::body::Body::from(archive("image.bin", b"replacement")),
    )
    .await
    .unwrap();
    assert_eq!(result.skipped, 1);
    assert_eq!(std::fs::read(repo.path().join("image.bin")).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 1);
}

// Catches: remote writes outside registered roots, traversal, or cross-platform absolute names.
#[tokio::test]
async fn remote_receiver_rejects_unregistered_paths_and_traversal() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let outside = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    for (dest, name) in [
        (outside.path().to_path_buf(), "file"),
        (repo.path().join(".."), "file"),
        (repo.path().to_path_buf(), "../file"),
        (repo.path().to_path_buf(), "C:\\escape"),
    ] {
        let query = UploadQuery {
            dest_dir: dest.to_string_lossy().into_owned(),
            name: name.into(),
            directory: false,
        };
        assert!(
            receive_copy(
                query,
                &roots,
                axum::body::Body::from(archive("file", b"evil"))
            )
            .await
            .is_err()
        );
    }
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

// Catches: dereferencing an escaped parent or replacing a symbolic-link target.
#[cfg(unix)]
#[tokio::test]
async fn remote_receiver_rejects_symlink_destination_and_target() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let outside = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(outside.path().join("victim"), b"untouched").unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("victim"), repo.path().join("victim")).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    for (dest, name) in [
        (repo.path().join("escape"), "new"),
        (repo.path().to_path_buf(), "victim"),
    ] {
        let query = UploadQuery {
            dest_dir: dest.to_string_lossy().into_owned(),
            name: name.into(),
            directory: false,
        };
        assert!(
            receive_copy(
                query,
                &roots,
                axum::body::Body::from(archive(name, b"evil"))
            )
            .await
            .is_err()
        );
    }
    assert_eq!(
        std::fs::read(outside.path().join("victim")).unwrap(),
        b"untouched"
    );
    assert!(!outside.path().join("new").exists());
}

// Catches: leaking a partially uploaded file/tree after disconnect or malformed tar.
#[tokio::test]
async fn truncated_upload_leaves_no_destination_or_staging_files() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    let mut data = archive("file", &[1; 2048]);
    data.truncate(800);
    let query = UploadQuery {
        dest_dir: roots[0].clone(),
        name: "file".into(),
        directory: false,
    };
    assert!(
        receive_copy(query, &roots, axum::body::Body::from(data))
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
}

// Catches: trusting tar's declared length and extracting an oversized payload.
#[tokio::test]
async fn oversized_declared_entry_is_rejected_before_extraction() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    let mut header = tar::Header::new_gnu();
    header.set_path("file").unwrap();
    header.set_mode(0o644);
    header.set_size(268_435_457);
    header.set_cksum();
    let query = UploadQuery {
        dest_dir: roots[0].clone(),
        name: "file".into(),
        directory: false,
    };
    let error = receive_copy(
        query,
        &roots,
        axum::body::Body::from(header.as_bytes().to_vec()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("exceeds 256 MiB"), "{error}");
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
}

// Catches: a body stream failure being treated as a completed upload.
#[tokio::test]
async fn disconnected_body_stream_never_publishes_a_partial_copy() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    let chunks = futures_util::stream::iter([
        Ok(vec![0u8; 128]),
        Err(std::io::Error::other("disconnected")),
    ]);
    let query = UploadQuery {
        dest_dir: roots[0].clone(),
        name: "file".into(),
        directory: false,
    };
    assert!(
        receive_copy(query, &roots, axum::body::Body::from_stream(chunks))
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
}

// Catches: treating directory recursion authorization as permission to install tar symlinks.
#[tokio::test]
async fn remote_receiver_rejects_archive_links_and_foreign_top_level_paths() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots = vec![repo.path().to_string_lossy().into_owned()];
    let mut builder = tar::Builder::new(Vec::new());
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_size(0);
    h.set_link_name("/etc/passwd").unwrap();
    h.set_cksum();
    builder
        .append_data(&mut h, "folder/link", std::io::empty())
        .unwrap();
    for data in [
        builder.into_inner().unwrap(),
        archive("other/file", b"evil"),
    ] {
        let query = UploadQuery {
            dest_dir: roots[0].clone(),
            name: "folder".into(),
            directory: true,
        };
        assert!(
            receive_copy(query, &roots, axum::body::Body::from(data))
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
    }
}

// Catches: local source deletion, binary corruption, missing empty directories, auth bypass,
// reset instead of skipped, and Rust errors losing the remote host/destination context.
#[tokio::test]
#[serial_test::serial]
async fn sender_streams_real_files_through_http_and_keeps_local_sources() {
    let src = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let dest = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::create_dir_all(src.path().join("folder/empty")).unwrap();
    let mut file = std::fs::File::create(src.path().join("folder/file")).unwrap();
    file.write_all(&[0, 255, 1]).unwrap();
    let config = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    std::fs::write(config.path().join("repositories.json"), serde_json::json!({
        "repos": { dest.path().to_str().unwrap(): { "path": dest.path().to_str().unwrap(), "branches": {} } }
    }).to_string()).unwrap();
    let daemon = Arc::new(crate::state::tests_support::make_test_app_state());
    *daemon.session_token.write() = "existing-token".into();
    daemon.config.write().services.auth.lan_auth_bypass = false;
    let router = crate::mcp_http::build_remote_router(daemon);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let state = crate::state::tests_support::make_test_app_state();
    state
        .remote
        .force_connected_for_test("mint", &url, Some("existing-token"));
    let request = |allow_recursive| RemoteTransferRequest {
        connection_id: "mint".into(),
        dest_dir: dest.path().to_string_lossy().into_owned(),
        paths: vec![src.path().join("folder").to_string_lossy().into_owned()],
        allow_recursive,
    };
    let result = transfer_remote(&state, request(false)).await.unwrap();
    assert!(result.needs_confirm);
    assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
    let missing = transfer_remote(
        &state,
        RemoteTransferRequest {
            connection_id: "mint".into(),
            dest_dir: dest.path().to_str().unwrap().into(),
            paths: vec![src.path().join("absent").to_str().unwrap().into()],
            allow_recursive: false,
        },
    )
    .await
    .unwrap_err();
    assert!(
        missing.starts_with(&format!(
            "Remote mint (127.0.0.1) {}:",
            dest.path().display()
        )),
        "{missing}"
    );
    let result = transfer_remote(&state, request(true)).await.unwrap();
    assert_eq!(result.moved, 1);
    assert_eq!(
        std::fs::read(dest.path().join("folder/file")).unwrap(),
        [0, 255, 1]
    );
    assert!(dest.path().join("folder/empty").is_dir());
    assert!(src.path().join("folder/file").is_file());
    // A body larger than HTTP buffers must finish sending before skipped is returned.
    std::fs::write(src.path().join("large"), vec![3; 2 * 1024 * 1024]).unwrap();
    std::fs::write(dest.path().join("large"), b"original").unwrap();
    let skipped = send_copies(
        reqwest::Client::new(),
        &url,
        Some("existing-token"),
        dest.path().to_str().unwrap(),
        vec![src.path().join("large").to_str().unwrap().into()],
        true,
    )
    .await
    .unwrap();
    assert_eq!(skipped.skipped, 1);
    assert!(skipped.errors.is_empty(), "{:?}", skipped.errors);
    assert_eq!(
        std::fs::read(dest.path().join("large")).unwrap(),
        b"original"
    );

    let rejected = transfer_remote(
        &state,
        RemoteTransferRequest {
            connection_id: "mint".into(),
            dest_dir: "/not-registered".into(),
            paths: vec![src.path().join("large").to_str().unwrap().into()],
            allow_recursive: true,
        },
    )
    .await
    .unwrap();
    assert!(
        rejected.errors[0].contains("Remote mint (127.0.0.1) /not-registered:"),
        "{:?}",
        rejected.errors
    );
    server.abort();
}

// Catches: tar header overhead crossing the cap after an accepted source-size precheck.
#[test]
fn source_size_precheck_accounts_for_tar_headers() {
    let src = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let path = src.path().join("large");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_UPLOAD_BYTES - 1024).unwrap();
    let error = source_entries(&path).unwrap_err().to_string();
    assert_eq!(error, UPLOAD_SIZE_ERROR);
    file.set_len(MAX_UPLOAD_BYTES - 1536).unwrap();
    assert!(source_entries(&path).is_ok());
}

// Catches: losing script executability or installing privilege and group-write bits from tar.
#[cfg(unix)]
#[tokio::test]
async fn tar_permissions_preserve_exec_but_strip_privileged_and_write_bits() {
    use std::os::unix::fs::PermissionsExt;
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots: Vec<String> = vec![repo.path().to_str().unwrap().into()];
    for (name, mode, maximum) in [("script", 0o7777, 0o755), ("plain", 0o6666, 0o644)] {
        let mut builder = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_mode(mode);
        h.set_size(3);
        h.set_cksum();
        builder.append_data(&mut h, name, &b"abc"[..]).unwrap();
        receive_copy(
            UploadQuery {
                dest_dir: roots[0].clone(),
                name: name.into(),
                directory: false,
            },
            &roots,
            Body::from(builder.into_inner().unwrap()),
        )
        .await
        .unwrap();
        let actual = std::fs::metadata(repo.path().join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(actual & !maximum, 0);
        assert_eq!(actual & 0o700, maximum & 0o700);
    }
}

// Catches: process termination leaving upload staging forever, restrictive modes blocking cleanup,
// or cleanup following an escaped link and changing its target permissions.
#[test]
fn startup_sweep_removes_only_upload_directories_under_registered_roots() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let outside = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::create_dir_all(repo.path().join("nested/.tuic-upload-abandoned")).unwrap();
    std::fs::write(
        repo.path().join("nested/.tuic-upload-abandoned/archive"),
        b"partial",
    )
    .unwrap();
    std::fs::create_dir(repo.path().join(".other-stage")).unwrap();
    std::fs::write(repo.path().join(".tuic-upload-user-file"), b"keep").unwrap();
    std::fs::create_dir(outside.path().join(".tuic-upload-outside")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), repo.path().join("escape")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(
            repo.path()
                .join("nested/.tuic-upload-restricted/data/locked"),
        )
        .unwrap();
        std::fs::write(
            repo.path()
                .join("nested/.tuic-upload-restricted/data/locked/file"),
            b"partial",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            outside.path(),
            repo.path().join("nested/.tuic-upload-restricted/escape"),
        )
        .unwrap();
        for relative in [
            "nested/.tuic-upload-restricted/data/locked",
            "nested/.tuic-upload-restricted/data",
        ] {
            std::fs::set_permissions(
                repo.path().join(relative),
                std::fs::Permissions::from_mode(0o000),
            )
            .unwrap();
        }
        let before = std::fs::metadata(outside.path())
            .unwrap()
            .permissions()
            .mode();
        sweep_staging(&[repo.path().to_str().unwrap().into()]);
        assert!(!repo.path().join("nested/.tuic-upload-restricted").exists());
        assert_eq!(
            std::fs::metadata(outside.path())
                .unwrap()
                .permissions()
                .mode(),
            before
        );
    }
    #[cfg(not(unix))]
    sweep_staging(&[repo.path().to_str().unwrap().into()]);
    assert!(!repo.path().join("nested/.tuic-upload-abandoned").exists());
    assert!(repo.path().join(".other-stage").exists());
    assert!(repo.path().join(".tuic-upload-user-file").exists());
    assert!(outside.path().join(".tuic-upload-outside").exists());
}

// Catches: stalled upload bodies occupying a slot forever and leaving a partial staging directory.
#[tokio::test(start_paused = true)]
async fn idle_upload_aborts_and_cleans_staging() {
    let repo = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let roots: Vec<String> = vec![repo.path().to_str().unwrap().into()];
    let body = Body::from_stream(futures_util::stream::pending::<io::Result<Vec<u8>>>());
    let error = receive_copy(
        UploadQuery {
            dest_dir: roots[0].clone(),
            name: "file".into(),
            directory: false,
        },
        &roots,
        body,
    )
    .await
    .unwrap_err();
    assert!(error.contains("idle"), "{error}");
    assert_eq!(std::fs::read_dir(repo.path()).unwrap().count(), 0);
}

// Catches: the global response timeout aborting a progressing upload despite its chunk idle budget.
#[tokio::test(start_paused = true)]
async fn upload_route_uses_idle_budget_instead_of_global_response_deadline() {
    use tower::ServiceExt;
    let routes = axum::Router::new()
        .route(
            "/fs/upload-copy",
            axum::routing::post(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                axum::http::StatusCode::OK
            }),
        )
        .route(
            "/ordinary",
            axum::routing::post(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                axum::http::StatusCode::OK
            }),
        );
    let app = crate::mcp_http::with_server_limits(routes, std::time::Duration::from_secs(1));
    for (path, status) in [
        ("/fs/upload-copy", axum::http::StatusCode::OK),
        ("/ordinary", axum::http::StatusCode::REQUEST_TIMEOUT),
    ] {
        let response = app
            .clone()
            .oneshot(axum::http::Request::post(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
}

// Catches: authenticated HTTP callers exfiltrating arbitrary Finder/local source paths.
#[tokio::test]
#[serial_test::serial]
async fn remote_transfer_coordinator_has_no_http_route_even_with_valid_token() {
    use tower::ServiceExt;
    let config = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = "existing-token".into();
    state.config.write().services.auth.lan_auth_bypass = false;
    for app in [
        crate::mcp_http::build_remote_router(state.clone()),
        crate::mcp_http::build_router(state.clone(), true, false),
    ] {
        let mut request = axum::http::Request::post("/fs/transfer-remote?token=existing-token")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"connectionId":"mint","destDir":"/repo","paths":["/private/source"],"allowRecursive":true}"#)).unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        let response = app.oneshot(request).await.unwrap();
        assert!(
            matches!(
                response.status(),
                axum::http::StatusCode::NOT_FOUND | axum::http::StatusCode::METHOD_NOT_ALLOWED
            ),
            "HTTP coordinator must not be reachable: {}",
            response.status()
        );
    }
}
