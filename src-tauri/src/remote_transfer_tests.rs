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

// Catches: local sources being deleted, binary corruption, missing nested/empty directories.
#[tokio::test]
async fn sender_streams_real_files_through_http_and_keeps_local_sources() {
    let src = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    let dest = tempfile::TempDir::new_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::create_dir_all(src.path().join("folder/empty")).unwrap();
    let mut file = std::fs::File::create(src.path().join("folder/file")).unwrap();
    file.write_all(&[0, 255, 1]).unwrap();
    let roots = vec![dest.path().to_string_lossy().into_owned()];
    let router = axum::Router::new().route(
        "/fs/upload-copy",
        axum::routing::post(
            move |axum::extract::Query(q): axum::extract::Query<UploadQuery>,
                  axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
                  body: axum::body::Body| {
                let roots = roots.clone();
                async move {
                    assert!(uri.query().unwrap().contains("token=existing-token"));
                    axum::Json(receive_copy(q, &roots, body).await.unwrap())
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
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
    let result = transfer_remote(&state, request(true)).await.unwrap();
    assert_eq!(result.moved, 1);
    assert_eq!(
        std::fs::read(dest.path().join("folder/file")).unwrap(),
        [0, 255, 1]
    );
    assert!(dest.path().join("folder/empty").is_dir());
    assert!(src.path().join("folder/file").is_file());
    server.abort();
}
