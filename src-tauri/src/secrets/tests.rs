use super::*;
use std::collections::BTreeMap;

#[test]
fn guessed_or_replayed_nonce_cannot_replace_a_secret() {
    let store = SecretStore::default();
    let form = Form::request(
        vec![Field {
            name: "TOKEN".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "test".into(),
    )
    .unwrap();
    let opened = store.open(form).unwrap();
    assert!(
        store
            .submit(
                "guessed",
                Submission::stored(BTreeMap::from([("TOKEN".into(), "private".into())]))
            )
            .is_err()
    );
    assert!(
        store
            .submit(
                &opened.nonce,
                Submission::stored(BTreeMap::from([("EXTRA".into(), "private".into())]))
            )
            .is_err()
    );
    let result = store
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([("TOKEN".into(), "private".into())])),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        r#"{"names":["TOKEN"],"status":"stored"}"#
    );
    assert!(
        store
            .submit(
                &opened.nonce,
                Submission::stored(BTreeMap::from([("TOKEN".into(), "replacement".into())]))
            )
            .is_err()
    );
    assert_eq!(store.mask("private replacement"), "[REDACTED] replacement");
}

#[test]
fn declined_form_and_removed_secret_leave_no_retrievable_value() {
    let store = SecretStore::default();
    let opened = store
        .open(
            Form::request(
                vec![Field {
                    name: "OTP".into(),
                    kind: FieldKind::Otp,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let result = store.submit(&opened.nonce, Submission::declined()).unwrap();
    assert_eq!(result.status, "declined");
    assert!(store.environment(&["OTP".into()]).is_err());
    assert!(SecretStore::default().environment(&["OTP".into()]).is_err());
}

#[test]
fn encoded_and_line_wrapped_secret_cannot_escape_output_masking() {
    let store = SecretStore::default();
    let opened = store
        .open(
            Form::request(
                vec![Field {
                    name: "TOKEN".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([("TOKEN".into(), "ab +/".into())])),
        )
        .unwrap();
    for text in [
        "ab +/",
        "ab \r\n+/",
        "YWIgKy8=",
        "YWIg\nKy8=",
        "6162202b2f",
        "6162202B2F",
        "ab%20%2B%2F",
        "ab%20%2b%2f",
        "ab+%2B%2F",
        "%61%62%20%2b%2F",
        "6162202b2F",
        "ab \x1b[31m+/",
    ] {
        assert_eq!(
            store.mask(text),
            "[REDACTED]",
            "unmasked representation: {text:?}"
        );
    }
}

#[test]
fn template_placeholder_cannot_change_program_subcommand_or_inject_options() {
    let template = policy::Template::new(vec!["gh".into(), "api".into(), "{arg}".into()]).unwrap();
    for argv in [
        vec!["gh", "api", "user"],
        vec!["gh", "api", "repos/org/repo"],
    ] {
        assert!(template.matches(&argv.into_iter().map(String::from).collect::<Vec<_>>()));
    }
    for argv in [
        vec!["sh", "api", "user"],
        vec!["gh", "auth", "user"],
        vec!["gh", "api", "--hostname=evil"],
        vec!["gh", "api", "x y"],
        vec!["gh", "api", "x;y"],
        vec!["gh", "api", "$(x)"],
        vec!["gh", "api", "x", "y"],
    ] {
        assert!(!template.matches(&argv.into_iter().map(String::from).collect::<Vec<_>>()));
    }
    assert!(policy::Template::new(vec!["{arg}".into(), "api".into()]).is_err());
    assert!(policy::Template::new(vec!["gh".into(), "{arg}".into()]).is_err());
    assert!(policy::Template::new(vec!["gh".into(), "auth".into(), "{arg}".into()]).is_err());
    assert!(policy::Template::new(vec!["git".into(), "remote".into(), "{arg}".into()]).is_err());
}

#[test]
fn approval_cannot_override_shell_or_environment_dumper_rejection() {
    for argv in [
        vec!["sh", "-c", "true"],
        vec!["/bin/bash", "-lc", "true"],
        vec!["zsh", "--command", "true"],
        vec!["env"],
        vec!["printenv"],
        vec!["node", "-e", "0"],
        vec!["python3", "-cpass"],
        vec!["perl", "-we", "0"],
        vec!["node", "--eval=0"],
        vec!["renamed-interpreter", "-c", "pass"],
    ] {
        assert!(
            policy::validate_argv(&argv.into_iter().map(String::from).collect::<Vec<_>>()).is_err()
        );
    }
}

#[tokio::test]
async fn child_environment_and_pipe_capture_do_not_echo_secret_to_result() {
    let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let argv = vec![
        exe.to_string_lossy().into_owned(),
        "--exact".into(),
        "secrets::tests::secret_child_fixture".into(),
        "--ignored".into(),
        "--nocapture".into(),
    ];
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let cwd = std::fs::canonicalize(crate::test_support::test_temp_root()).unwrap();
    let opened = state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "TUIC_SECRET_FIXTURE".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    state
        .secrets
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([(
                "TUIC_SECRET_FIXTURE".into(),
                "ab +/".into(),
            )])),
        )
        .unwrap();
    let mut approval = Form::request(
        vec![Field {
            name: "TUIC_SECRET_FIXTURE".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    approval.argv = Some(argv.clone());
    approval.cwd = Some(cwd.to_string_lossy().into_owned());
    let approval = state.secrets.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(argv.clone());
    state.secrets.submit(&approval.nonce, submission).unwrap();
    let result = run::handle_secret(&state, &serde_json::json!({"action": "run", "names": ["TUIC_SECRET_FIXTURE"], "argv": argv, "cwd": cwd})).await;
    assert!(state.session_maps.output_buffers.is_empty());
    assert!(state.log_buffer.lock().get_entries(0).is_empty());
    let stdout = result["stdout"].as_str().unwrap();
    let stderr = result["stderr"].as_str().unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(
        stdout.contains("child-saw-environment [REDACTED]"),
        "{result}"
    );
    assert!(!serde_json::to_string(&result).unwrap().contains("YWIgKy8="));
    assert!(stderr.contains("[REDACTED]"));
    assert!(std::env::var("TUIC_SECRET_FIXTURE").is_err());
}

#[test]
#[ignore = "Child-process fixture: run only by child_environment_and_pipe_capture_do_not_echo_secret_to_result"]
fn secret_child_fixture() {
    let value = std::env::var("TUIC_SECRET_FIXTURE").unwrap();
    println!("child-saw-environment {value}");
    println!("YWIg\nKy8=");
    eprintln!("6162202B2F");
}

#[tokio::test]
async fn http_submission_rejects_guessed_and_replayed_nonce_without_echoing_values() {
    use axum::{
        Router,
        body::Body,
        http::Request,
        routing::{get, post},
    };
    use tower::ServiceExt;
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let form = state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    let app = Router::new()
        .route("/secrets/forms/{nonce}", get(form_http))
        .route("/secrets/forms/submit", post(submit_http))
        .with_state(state.clone());
    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/secrets/forms/wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), 404);
    for (nonce, expected) in [
        ("wrong", 400),
        (form.nonce.as_str(), 200),
        (form.nonce.as_str(), 400),
    ] {
        let body = serde_json::json!({"nonce": nonce, "status": "stored", "values": {"PASS": "synthetic-entry"}, "template": null});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/secrets/forms/submit")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("synthetic-entry"));
    }
}

#[tokio::test]
async fn open_form_blocks_native_and_proxied_inspection_tools() {
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    state.config.write().disabled_native_tools.clear();
    state
        .secrets
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    for (tool, args) in [
        (
            "ui",
            serde_json::json!({"action": "screenshot", "id": "any"}),
        ),
        (
            "debug",
            serde_json::json!({"action": "invoke_js", "script": "return document.body.innerHTML"}),
        ),
        ("maccontrol__screenshot", serde_json::json!({})),
    ] {
        let result = crate::mcp_http::mcp_transport::handle_mcp_tool_call(
            &state,
            "127.0.0.1:12345".parse().unwrap(),
            tool,
            &args,
            None,
        )
        .await;
        assert!(
            result.to_string().contains("inspection is disabled"),
            "{tool}: {result}"
        );
    }
}

#[test]
fn saved_template_cannot_approve_new_names_or_working_directory() {
    let store = SecretStore::default();
    let mut approval = Form::request(
        vec![Field {
            name: "PASS".into(),
            kind: FieldKind::Password,
            display: None,
        }],
        "Approve".into(),
    )
    .unwrap();
    approval.argv = Some(vec!["/usr/bin/gh".into(), "api".into(), "user".into()]);
    approval.cwd = Some("/trusted".into());
    let approval = store.open(approval).unwrap();
    let mut submission = Submission::declined();
    submission.status = "approved".into();
    submission.template = Some(vec!["/usr/bin/gh".into(), "api".into(), "{arg}".into()]);
    store.submit(&approval.nonce, submission).unwrap();
    let argv = vec!["/usr/bin/gh".into(), "api".into(), "user".into()];
    assert!(store.allowed(&argv, &["PASS".into()], "/trusted"));
    assert!(!store.allowed(&argv, &["OTHER".into()], "/trusted"));
    assert!(!store.allowed(&argv, &["PASS".into()], "/attacker"));
}

#[test]
fn removed_value_is_missing_and_epoch_survives_close_to_catch_pending_inspection() {
    let store = SecretStore::default();
    let initial = store.inspection_epoch();
    let opened = store
        .open(
            Form::request(
                vec![Field {
                    name: "PASS".into(),
                    kind: FieldKind::Password,
                    display: None,
                }],
                "test".into(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(store.tools_blocked());
    store
        .submit(
            &opened.nonce,
            Submission::stored(BTreeMap::from([("PASS".into(), "synthetic-entry".into())])),
        )
        .unwrap();
    assert!(!store.tools_blocked());
    assert_ne!(initial, store.inspection_epoch());
    store.remove(&["PASS".into()]);
    assert!(store.environment(&["PASS".into()]).is_err());
}

#[test]
fn utf8_split_by_a_wrap_is_masked_before_lossy_decoding() {
    let needles = mask::representations("café");
    assert_eq!(mask::mask_bytes(b"caf\xc3\r\n\xa9", &needles), "[REDACTED]");
}
