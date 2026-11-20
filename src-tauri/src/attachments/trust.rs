//! Read-only check of whether Claude Code trusts TUIC's attachments directory.
//!
//! A Claude launch gets `--add-dir=<attachments>/pty-<id>` (see
//! `agent_hook_launch::launch_args_for`). The directory is new for every session, so
//! Claude asks "Is this a directory you created or one you trust?" each time. Claude
//! keeps a persistent trust per project path in `<profile>/.claude.json`
//! (`projects[<path>].hasTrustDialogAccepted`) and treats a trusted ancestor as
//! covering everything below it. Trusting the attachments root once therefore silences
//! the prompt for every future session, without widening what Claude may read: access
//! still comes only from the per-session `--add-dir` grant.
//!
//! This module only ever READS those files. TUIC never writes Claude's trust store: it
//! is another tool's safety prompt, and a forged entry could stop working whenever
//! Claude changes how it validates one.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Trust state of one Claude profile for the attachments root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ProfileTrust {
    /// `CLAUDE_CONFIG_DIR` the profile needs; `None` for the default profile.
    pub config_dir: Option<String>,
    /// The `.claude.json` that was read.
    pub config_file: String,
    pub trusted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AttachmentTrustStatus {
    /// Canonical attachments root: the directory to trust.
    pub root: String,
    /// Empty when the claude agent is not installed. Same shape on IPC and HTTP.
    pub profiles: Vec<ProfileTrust>,
}

/// True when `config_json` (the text of a profile's `.claude.json`) marks `root`, or any
/// ancestor of it, as trusted. A missing file, unparsable JSON, a missing `projects` key
/// and an explicit `false` all count as untrusted.
pub(crate) fn is_trusted(config_json: Option<&str>, root: &Path) -> bool {
    let Some(text) = config_json else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    let Some(projects) = config.get("projects").and_then(|p| p.as_object()) else {
        return false;
    };
    // DEFERRED (2026-11-19) — Windows: Claude's key spelling (separators, drive-letter
    // case) was not captured, so only an exact string match counts there.
    root.ancestors().any(|dir| {
        projects
            .get(dir.to_string_lossy().as_ref())
            .and_then(|entry| entry.get("hasTrustDialogAccepted"))
            .and_then(|value| value.as_bool())
            == Some(true)
    })
}

/// Reads each profile file once and reports its trust state. Blocking I/O.
pub(crate) fn profile_trust(
    root: &Path,
    profiles: &[(Option<PathBuf>, PathBuf)],
) -> Vec<ProfileTrust> {
    profiles
        .iter()
        .map(|(dir, file)| {
            let text = std::fs::read_to_string(file).ok();
            ProfileTrust {
                config_dir: dir.as_ref().map(|d| d.to_string_lossy().into_owned()),
                config_file: file.to_string_lossy().into_owned(),
                trusted: is_trusted(text.as_deref(), root),
            }
        })
        .collect()
}

/// Shared by the Tauri command and `GET /attachments/trust`.
pub(crate) async fn status(data_dir: &Path) -> Result<AttachmentTrustStatus, String> {
    let root = super::storage::ensure_root(data_dir)?;
    let root_text = root.to_string_lossy().into_owned();
    // DEFERRED (2026-11-20) — full detection runs `claude --version` on every status
    // call (startup, Settings expand, Re-check). `agent::detect_binary_path_only` is
    // cheaper but only probes fixed install dirs, so it could miss an nvm/custom
    // install and silently hide the feature; needs a PATH-aware cheap variant first.
    let installed = crate::agent::detect_agent_binary("claude".to_string())
        .await
        .path
        .is_some();
    if !installed {
        return Ok(AttachmentTrustStatus {
            root: root_text,
            profiles: Vec::new(),
        });
    }
    let profiles = crate::agent_mcp::claude_profile_configs(
        &crate::config::load_agents_config(),
        std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref(),
    );
    let scan_root = root.clone();
    let profiles = tokio::task::spawn_blocking(move || profile_trust(&scan_root, &profiles))
        .await
        .map_err(|error| error.to_string())?;
    Ok(AttachmentTrustStatus {
        root: root_text,
        profiles,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_attachment_trust_status(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<AttachmentTrustStatus, String> {
    status(&state.data_dir).await
}

pub(crate) async fn trust_status_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match status(&state.data_dir).await {
        Ok(status) => axum::Json(status).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/Users/me/Library/Application Support/com.tuic.commander/attachments";

    fn config(entries: &[(&str, serde_json::Value)]) -> String {
        let projects: serde_json::Map<_, _> = entries
            .iter()
            .map(|(path, value)| ((*path).to_string(), value.clone()))
            .collect();
        serde_json::json!({ "projects": projects, "other": 1 }).to_string()
    }

    // Catches: the user trusted the root and TUIC keeps nagging anyway.
    #[test]
    fn exact_entry_is_trusted() {
        let text = config(&[(ROOT, serde_json::json!({ "hasTrustDialogAccepted": true }))]);
        assert!(is_trusted(Some(&text), Path::new(ROOT)));
    }

    // Catches: a per-session child directory is not covered by its trusted parent.
    #[test]
    fn ancestor_entry_covers_the_root() {
        let text = config(&[(
            "/Users/me/Library/Application Support",
            serde_json::json!({ "hasTrustDialogAccepted": true }),
        )]);
        assert!(is_trusted(Some(&text), Path::new(ROOT)));
    }

    // Catches: a trusted sibling or a descendant is mistaken for trust of the root.
    #[test]
    fn sibling_and_descendant_entries_do_not_count() {
        let text = config(&[
            (
                "/Users/me/Library/Application Support/com.tuic.commander/other",
                serde_json::json!({ "hasTrustDialogAccepted": true }),
            ),
            (
                &format!("{ROOT}/pty-1"),
                serde_json::json!({ "hasTrustDialogAccepted": true }),
            ),
        ]);
        assert!(!is_trusted(Some(&text), Path::new(ROOT)));
    }

    // Catches: an entry Claude recorded with trust declined reads as trusted.
    #[test]
    fn explicit_false_and_missing_flag_are_untrusted() {
        let declined = config(&[(ROOT, serde_json::json!({ "hasTrustDialogAccepted": false }))]);
        assert!(!is_trusted(Some(&declined), Path::new(ROOT)));
        let no_flag = config(&[(ROOT, serde_json::json!({ "allowedTools": [] }))]);
        assert!(!is_trusted(Some(&no_flag), Path::new(ROOT)));
        let wrong_type = config(&[(
            ROOT,
            serde_json::json!({ "hasTrustDialogAccepted": "true" }),
        )]);
        assert!(!is_trusted(Some(&wrong_type), Path::new(ROOT)));
    }

    // Catches: a fresh profile (no file / no projects / garbage) panics or reads as trusted.
    #[test]
    fn missing_file_missing_projects_and_garbage_are_untrusted() {
        assert!(!is_trusted(None, Path::new(ROOT)));
        assert!(!is_trusted(Some("{}"), Path::new(ROOT)));
        assert!(!is_trusted(Some(r#"{"projects": []}"#), Path::new(ROOT)));
        assert!(!is_trusted(Some("not json"), Path::new(ROOT)));
    }

    // Catches: the lookup uses a non-canonical path the config never contains.
    #[test]
    fn a_non_canonical_root_is_not_matched_by_its_symlink_spelling() {
        let text = config(&[(ROOT, serde_json::json!({ "hasTrustDialogAccepted": true }))]);
        assert!(!is_trusted(
            Some(&text),
            Path::new("/private/var/other/attachments")
        ));
    }

    // Catches: profiles read the wrong file, or the default profile gets a config dir.
    #[test]
    fn profile_trust_reads_each_file_and_reports_the_config_dir() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let root = dir.path().join("attachments");
        let trusted_file = dir.path().join("trusted.json");
        std::fs::write(
            &trusted_file,
            config(&[(
                root.to_str().unwrap(),
                serde_json::json!({ "hasTrustDialogAccepted": true }),
            )]),
        )
        .unwrap();
        let missing_file = dir.path().join("private/.claude.json");
        let result = profile_trust(
            &root,
            &[
                (None, trusted_file.clone()),
                (Some(dir.path().join("private")), missing_file.clone()),
            ],
        );
        assert_eq!(result[0].config_dir, None);
        assert!(result[0].trusted);
        assert_eq!(
            result[1].config_dir.as_deref(),
            Some(dir.path().join("private").to_str().unwrap())
        );
        assert!(!result[1].trusted);
    }
}
