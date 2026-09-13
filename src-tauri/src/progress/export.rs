use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::{
    ProgressEvent, ProgressExportInput, ProgressExportOptions, ProgressExportReceipt, ProgressStore,
};

const EXPORT_FILE: &str = "progress.md";
const EXPORT_LOCK: &str = ".tuic/progress-export.lock";

pub(crate) struct ExportWorkstream {
    pub id: String,
    pub name: String,
    pub state: String,
}

pub(crate) struct ExportData {
    pub revision: u64,
    pub collection_enabled: bool,
    pub workstreams: Vec<ExportWorkstream>,
    pub blockers: Vec<ProgressEvent>,
    pub events: Vec<ProgressEvent>,
}

pub(crate) fn progress_export(
    project_root: PathBuf,
    input: ProgressExportInput,
) -> Result<ProgressExportReceipt, String> {
    let (options, snapshot_time_ms, expected_snapshot, write, replace, expected_content) =
        match input {
            ProgressExportInput::Preview { options } => {
                (options, now_ms()?, None, false, false, None)
            }
            ProgressExportInput::Write {
                options,
                snapshot_id,
                snapshot_time_ms,
                replace,
                expected_content,
            } => (
                options,
                snapshot_time_ms,
                Some(snapshot_id),
                true,
                replace,
                expected_content,
            ),
        };
    let store = ProgressStore::open(&project_root)?;
    let data = store.export_data()?;
    let markdown = render(&project_root, &data, snapshot_time_ms, &options)?;
    let snapshot_id = snapshot_id(data.revision, snapshot_time_ms, &options, &markdown);
    if expected_snapshot
        .as_deref()
        .is_some_and(|expected| expected != snapshot_id)
    {
        return Err("progress_export_snapshot_changed: previewed snapshot is no longer current; preview again".into());
    }
    let target = project_root.join(EXPORT_FILE);
    let existing = inspect_target(&target)?;
    if !write {
        return Ok(receipt(
            &project_root,
            &target,
            snapshot_id,
            data.revision,
            snapshot_time_ms,
            markdown,
            existing,
            false,
        ));
    }
    let lock_path = project_root.join(EXPORT_LOCK);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "progress_export_unavailable: cannot open export lock '{}': {error}",
                lock_path.display()
            )
        })?;
    lock.lock().map_err(|error| {
        format!(
            "progress_export_unavailable: cannot lock export '{}': {error}",
            target.display()
        )
    })?;
    let result = (|| {
        let current = inspect_target(&target)?;
        match (&current, replace, expected_content.as_ref()) {
            (Some(_), false, _) => return Err("progress_export_exists: progress.md already exists; preview and explicitly replace it".into()),
            (Some(content), true, Some(expected)) if content == expected => {}
            (Some(_), true, _) => return Err("progress_export_content_changed: progress.md changed after preview; preview again".into()),
            (None, _, Some(_)) => return Err("progress_export_content_changed: progress.md was removed after preview; preview again".into()),
            (None, true, None) => return Err("progress_export_invalid_request: replace requires expectedContent from preview".into()),
            (None, false, None) => {}
        }
        atomic_replace(&target, markdown.as_bytes())?;
        Ok(receipt(
            &project_root,
            &target,
            snapshot_id,
            data.revision,
            snapshot_time_ms,
            markdown,
            current,
            true,
        ))
    })();
    let unlock = lock.unlock().map_err(|error| {
        format!(
            "progress_export_unavailable: cannot unlock export '{}': {error}",
            target.display()
        )
    });
    match (result, unlock) {
        (Ok(receipt), Ok(())) => Ok(receipt),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn receipt(
    root: &Path,
    target: &Path,
    snapshot_id: String,
    revision: u64,
    time: u64,
    markdown: String,
    existing: Option<String>,
    written: bool,
) -> ProgressExportReceipt {
    ProgressExportReceipt {
        project_root: root.to_string_lossy().into_owned(),
        path: target.to_string_lossy().into_owned(),
        snapshot_id,
        snapshot_revision: revision,
        snapshot_time_ms: time,
        markdown,
        file_exists: existing.is_some(),
        existing_content: existing,
        written,
    }
}

fn render(
    root: &Path,
    data: &ExportData,
    snapshot_time_ms: u64,
    options: &ProgressExportOptions,
) -> Result<String, String> {
    let project = root.file_name().and_then(|v| v.to_str()).ok_or_else(|| {
        "progress_export_unavailable: project root has no readable name".to_string()
    })?;
    let time = DateTime::<Utc>::from_timestamp_millis(
        i64::try_from(snapshot_time_ms)
            .map_err(|_| "progress_export_invalid_request: snapshotTimeMs is out of range")?,
    )
    .ok_or("progress_export_invalid_request: snapshotTimeMs is out of range")?;
    let mut out = format!(
        "# Project Progress: {}\n\n- Snapshot revision: {}\n- Snapshot time: {}\n- Collection: {}\n\n## Workstreams\n\n",
        escape(project),
        data.revision,
        time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        if data.collection_enabled {
            "active"
        } else {
            "paused"
        }
    );
    if data.workstreams.is_empty() {
        out.push_str("No workstreams recorded.\n");
    }
    for item in &data.workstreams {
        let active = data
            .blockers
            .iter()
            .filter(|event| event.workstream_id.as_deref() == Some(&item.id))
            .count();
        out.push_str(&format!(
            "- **{}** — {}{}\n",
            escape(&item.name),
            escape(&item.state),
            if active == 0 {
                String::new()
            } else {
                format!(
                    "; {active} active blocker{}",
                    if active == 1 { "" } else { "s" }
                )
            }
        ));
    }
    out.push_str("\n## Active blockers\n\n");
    if data.blockers.is_empty() {
        out.push_str("No active blockers.\n");
    }
    for event in &data.blockers {
        render_event(&mut out, event, options, true)?;
    }
    out.push_str("\n## History\n\n");
    if data.events.is_empty() {
        out.push_str("No progress recorded.\n");
    }
    for event in &data.events {
        render_event(&mut out, event, options, false)?;
    }
    Ok(out)
}

fn render_event(
    out: &mut String,
    event: &ProgressEvent,
    options: &ProgressExportOptions,
    blocker: bool,
) -> Result<(), String> {
    let time = DateTime::<Utc>::from_timestamp_millis(
        i64::try_from(event.created_at_ms)
            .map_err(|_| "progress_export_invalid_data: event time is out of range")?,
    )
    .ok_or("progress_export_invalid_data: event time is out of range")?;
    let scope = event.workstream.as_deref().unwrap_or("Project");
    let kind = if blocker {
        "Blocked".to_string()
    } else {
        let raw = event.kind.as_str();
        format!("{}{}", &raw[..1].to_uppercase(), &raw[1..])
    };
    out.push_str(&format!(
        "- {} — **{}** · {}: {}",
        time.format("%Y-%m-%d"),
        escape(scope),
        kind,
        escape(&event.summary)
    ));
    if options.include_provenance {
        let parts = [
            event
                .provenance
                .reporter_name
                .as_deref()
                .or(event.provenance.reporter_id.as_deref()),
            event.provenance.workspace_path.as_deref(),
            event.provenance.session_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(escape)
        .collect::<Vec<_>>();
        if !parts.is_empty() {
            out.push_str(&format!(" _(source: {})_", parts.join("; ")));
        }
    }
    out.push('\n');
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "\\|")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('`', "\\`")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn snapshot_id(
    revision: u64,
    time: u64,
    options: &ProgressExportOptions,
    markdown: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(revision.to_be_bytes());
    hash.update(time.to_be_bytes());
    hash.update([options.include_provenance as u8]);
    hash.update(markdown.as_bytes());
    format!("sha256:{}", hex::encode(hash.finalize()))
}

fn inspect_target(target: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "progress_export_unsafe_target: '{}' is a symlink",
            target.display()
        )),
        Ok(meta) if !meta.is_file() => Err(format!(
            "progress_export_unsafe_target: '{}' is not a regular file",
            target.display()
        )),
        Ok(_) => fs::read_to_string(target).map(Some).map_err(|error| {
            format!(
                "progress_export_unavailable: cannot read '{}': {error}",
                target.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "progress_export_unavailable: cannot inspect '{}': {error}",
            target.display()
        )),
    }
}

fn atomic_replace(target: &Path, content: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or("progress_export_unavailable: export target has no parent")?;
    let mut temp = tempfile::Builder::new()
        .prefix(".progress.md.")
        .tempfile_in(parent)
        .map_err(|error| {
            format!("progress_export_write_failed: cannot create temporary export: {error}")
        })?;
    temp.write_all(content)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|error| {
            format!("progress_export_write_failed: cannot write temporary export: {error}")
        })?;
    if let Ok(metadata) = fs::metadata(target) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|error| {
                format!("progress_export_write_failed: cannot preserve export permissions: {error}")
            })?;
    }
    temp.persist(target).map_err(|error| {
        format!(
            "progress_export_write_failed: cannot replace '{}': {}",
            target.display(),
            error.error
        )
    })?;
    if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn now_ms() -> Result<u64, String> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("progress_export_unavailable: system clock error: {error}"))?
        .as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{NewProgressEvent, ProgressKind, ProgressProvenance};

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::git_cli::git_cmd(dir.path())
            .args(["init"])
            .run()
            .unwrap();
        dir
    }

    fn record(root: &Path, kind: ProgressKind, summary: &str, workstream: Option<&str>) {
        ProgressStore::open(root)
            .unwrap()
            .record(&NewProgressEvent {
                kind,
                summary: summary.into(),
                workstream: workstream.map(str::to_string),
                provenance: ProgressProvenance {
                    reporter_name: Some("Agent *One*".into()),
                    workspace_path: Some("/tmp/work".into()),
                    ..Default::default()
                },
            })
            .unwrap();
    }

    fn preview(root: &Path, provenance: bool) -> ProgressExportReceipt {
        progress_export(
            root.to_path_buf(),
            ProgressExportInput::Preview {
                options: ProgressExportOptions {
                    include_provenance: provenance,
                },
            },
        )
        .unwrap()
    }

    fn write_input(preview: &ProgressExportReceipt, provenance: bool) -> ProgressExportInput {
        ProgressExportInput::Write {
            options: ProgressExportOptions {
                include_provenance: provenance,
            },
            snapshot_id: preview.snapshot_id.clone(),
            snapshot_time_ms: preview.snapshot_time_ms,
            replace: preview.file_exists,
            expected_content: preview.existing_content.clone(),
        }
    }

    #[test]
    fn renders_stable_english_markdown_and_escapes_untrusted_text() {
        let root = project();
        record(
            root.path(),
            ProgressKind::Blocked,
            "Need *approval*\nnow <soon>.",
            Some("API_[v2]"),
        );
        let data = ProgressStore::open(root.path())
            .unwrap()
            .export_data()
            .unwrap();
        let options = ProgressExportOptions {
            include_provenance: true,
        };
        let first = render(root.path(), &data, 1_700_000_000_000, &options).unwrap();
        let second = render(root.path(), &data, 1_700_000_000_000, &options).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("Snapshot time: 2023-11-14T22:13:20.000Z"));
        assert!(first.contains("API\\_\\[v2\\]"));
        assert!(first.contains("Need \\*approval\\* now &lt;soon&gt;."));
        assert!(first.contains("source: Agent \\*One\\*; /tmp/work"));
        assert!(!first.contains("reporterId"));
    }

    #[test]
    fn empty_paused_history_is_explicit_without_mutating_state() {
        let root = project();
        let store = ProgressStore::open(root.path()).unwrap();
        store.set_collection_enabled(false).unwrap();
        let before = store.status().unwrap();
        let preview = preview(root.path(), false);
        assert!(preview.markdown.contains("Collection: paused"));
        assert!(preview.markdown.contains("No workstreams recorded."));
        assert!(preview.markdown.contains("No active blockers."));
        assert!(preview.markdown.contains("No progress recorded."));
        assert_eq!(before, store.status().unwrap());
    }

    #[test]
    fn write_uses_previewed_snapshot_and_detects_database_or_file_changes() {
        let root = project();
        record(root.path(), ProgressKind::Milestone, "First.", None);
        let first = preview(root.path(), false);
        record(root.path(), ProgressKind::Milestone, "Raced.", None);
        assert!(
            progress_export(root.path().into(), write_input(&first, false))
                .unwrap_err()
                .starts_with("progress_export_snapshot_changed:")
        );

        let current = preview(root.path(), false);
        let written = progress_export(root.path().into(), write_input(&current, false)).unwrap();
        assert!(written.written);
        assert_eq!(
            fs::read_to_string(root.path().join(EXPORT_FILE)).unwrap(),
            current.markdown
        );
        let replace = preview(root.path(), false);
        let mut implicit = write_input(&replace, false);
        if let ProgressExportInput::Write { replace, .. } = &mut implicit {
            *replace = false;
        }
        assert!(
            progress_export(root.path().into(), implicit)
                .unwrap_err()
                .starts_with("progress_export_exists:")
        );
        fs::write(root.path().join(EXPORT_FILE), "human edit\n").unwrap();
        assert!(
            progress_export(root.path().into(), write_input(&replace, false))
                .unwrap_err()
                .starts_with("progress_export_content_changed:")
        );
        assert_eq!(
            fs::read_to_string(root.path().join(EXPORT_FILE)).unwrap(),
            "human edit\n"
        );
    }

    #[test]
    fn refuses_symlink_and_directory_targets() {
        let root = project();
        fs::create_dir(root.path().join(EXPORT_FILE)).unwrap();
        let error = progress_export(
            root.path().into(),
            ProgressExportInput::Preview {
                options: Default::default(),
            },
        )
        .unwrap_err();
        assert!(error.starts_with("progress_export_unsafe_target:"));
    }

    #[test]
    fn failed_atomic_replacement_preserves_original_target() {
        let root = project();
        let target = root.path().join("occupied");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("original"), "keep").unwrap();
        assert!(
            atomic_replace(&target, b"replacement")
                .unwrap_err()
                .starts_with("progress_export_write_failed:")
        );
        assert_eq!(fs::read_to_string(target.join("original")).unwrap(), "keep");
    }

    #[test]
    fn concurrent_reports_never_produce_a_torn_snapshot() {
        let root = project();
        let writer_root = root.path().to_path_buf();
        let writer = std::thread::spawn(move || {
            for index in 0..20 {
                record(
                    &writer_root,
                    ProgressKind::Milestone,
                    &format!("Concurrent {index}."),
                    None,
                );
            }
        });
        for _ in 0..20 {
            let snapshot = preview(root.path(), false);
            let count = snapshot.markdown.matches("Concurrent ").count() as u64;
            assert_eq!(snapshot.snapshot_revision, count);
        }
        writer.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_target_without_touching_destination() {
        use std::os::unix::fs::symlink;
        let root = project();
        let destination = root.path().join("owned.md");
        fs::write(&destination, "keep\n").unwrap();
        symlink(&destination, root.path().join(EXPORT_FILE)).unwrap();
        let error = progress_export(
            root.path().into(),
            ProgressExportInput::Preview {
                options: Default::default(),
            },
        )
        .unwrap_err();
        assert!(error.starts_with("progress_export_unsafe_target:"));
        assert_eq!(fs::read_to_string(destination).unwrap(), "keep\n");
    }
}
