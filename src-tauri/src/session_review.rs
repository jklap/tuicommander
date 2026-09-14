//! Session Diff Review — reconstructs a step-by-step, per-file edit review
//! from a Claude Code session transcript.
//!
//! See `docs/backend/session-review.md` for the full format reference: the
//! transcript record shapes, the two format traps (an empty `structuredPatch`
//! on file creation, and a bare-string `toolUseResult` for Bash calls), the
//! base-resolution strategy, and the two revert mechanisms.
//!
//! # Design notes (deviations from the original design doc, made during
//! implementation for simplicity/robustness — the wire contract is unchanged)
//!
//! - **Base resolution does not key off `file-history-delta`'s `backupFileName`
//!   being present/absent.** The transcript itself already carries an
//!   unambiguous, always-available signal: a `Write` tool result's own `type`
//!   field is `"create"` only when the file did not exist before that write.
//!   So the *first* recorded edit for a path having [`StepKind::Create`] is
//!   both necessary and sufficient to know the file did not exist at session
//!   start — no `file-history` lookup required. `file-history` backups are
//!   still read (best-effort, since Claude Code prunes them) and used for two
//!   things: (a) the byte-exact `restore_backup` revert method, and (b) as
//!   tier 1 of base resolution when a plain-text diff base is wanted for a
//!   file that existed before the session (an `Edit` was the first touch).
//! - **A cache is a simple `(len, mtime)`-validated full-review cache**, not
//!   an incremental byte-offset-resume cache. Transcripts are re-scanned in
//!   full on a cache miss. Given the byte-level pre-filter in
//!   [`line_is_interesting`] (which rejects the vast majority of a
//!   transcript's lines — `attachment`/`assistant`/`system` records — before
//!   any JSON parsing), a full re-scan is fast enough that incremental resume
//!   was not worth the added complexity and failure surface for a first
//!   version. The cache still avoids re-parsing on every poll of a session
//!   that hasn't changed.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ─────────────────────────── Wire types ─────────────────────────────────────

/// One Claude Code session available for review in a given repo.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionSummary {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub size_bytes: u64,
    pub edit_count: Option<u32>,
    pub file_count: Option<u32>,
    pub has_subagents: bool,
}

/// What one tool call did to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StepKind {
    /// `Write` with `toolUseResult.type == "create"` — the file did not exist.
    Create,
    /// `Write` with `toolUseResult.type == "update"` — a whole-file overwrite.
    Overwrite,
    /// `Edit` — a single `old_string` → `new_string` substitution.
    Edit,
}

/// One file mutation, in transcript order.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct EditStep {
    /// 0-based position in the session-wide chronological order. Display
    /// only — `tool_use_id` is the stable handle used for revert.
    pub step_index: u32,
    pub tool_use_id: String,
    pub timestamp: Option<String>,
    pub kind: StepKind,
    pub abs_path: String,
    pub rel_path: Option<String>,
    pub in_repo: bool,
    /// A `git apply`-able unified diff for this step alone. Empty when the
    /// step was a no-op (e.g. an Edit whose `old_string`/`new_string` were
    /// identical).
    pub patch: String,
    pub additions: u32,
    pub deletions: u32,
    pub is_sidechain: bool,
    pub agent_name: Option<String>,
    pub user_modified: bool,
    pub replace_all: bool,
}

/// Where the session-start content of a file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BaseSource {
    /// A byte-exact `~/.claude/file-history/<session>/<name>@v1` backup.
    Backup,
    /// The first recorded edit for this path was a [`StepKind::Create`].
    CreatedInSession,
    /// The first edit's `toolUseResult.originalFile` was present.
    ToolResult,
    /// Reverse-folded from the current on-disk content.
    Reconstructed,
    /// None of the above worked — no cumulative diff is offered.
    Unknown,
}

/// Net effect of the whole session on one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NetChange {
    Added,
    Modified,
    /// Base == final. Every step cancelled out. Steps are still listed.
    Unchanged,
    /// The file is gone from disk now.
    Deleted,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileReview {
    pub abs_path: String,
    pub rel_path: Option<String>,
    pub in_repo: bool,
    pub display_path: String,
    pub net_change: NetChange,
    pub base_source: BaseSource,
    pub cumulative_patch: String,
    pub additions: u32,
    pub deletions: u32,
    pub step_indices: Vec<u32>,
    /// True when the file's current on-disk content doesn't match what
    /// folding the transcript forward from `base` would produce — i.e.
    /// something outside this session touched it since.
    pub drifted_from_disk: bool,
    /// True when a `@v1` backup exists on disk right now.
    pub backup_available: bool,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionReview {
    pub session_id: String,
    pub transcript_path: String,
    pub repo_path: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub title: Option<String>,
    pub steps: Vec<EditStep>,
    pub files: Vec<FileReview>,
    /// Non-fatal parse problems — render as a dismissible banner, never swallow.
    pub warnings: Vec<String>,
    pub included_subagents: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RevertResult {
    pub applied: bool,
    /// `"git_apply_reverse"` | `"string_substitution"` | `"restore_backup"`
    /// | `"write_base"` | `"delete_file"`.
    pub method: String,
    pub abs_path: String,
    pub message: Option<String>,
}

// ─────────────────────────── Discovery helpers ──────────────────────────────

/// `<projects>/<slug>/` for a repo path, honoring `CLAUDE_CONFIG_DIR`.
fn project_dir_for(repo_path: &str, cfg: Option<&str>) -> Option<PathBuf> {
    let path =
        crate::agent_session::claude_project_dir(repo_path.to_string(), cfg.map(String::from))
            .ok()?;
    Some(PathBuf::from(path))
}

/// `<projects>/<slug>/<session_id>/subagents/*.jsonl`, sorted; empty when absent.
fn subagent_transcripts(project_dir: &Path, session_id: &str) -> Vec<PathBuf> {
    let dir = project_dir.join(session_id).join("subagents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    files
}

/// A subagent transcript's display name, derived from its filename
/// (`agent-<id>.jsonl` → `<id>`) — a bare filename stem would read as
/// "agent-worker1" instead of the friendlier "worker1".
fn subagent_name_from_path(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_str()?;
    Some(stem.strip_prefix("agent-").unwrap_or(stem).to_string())
}

/// `~/.claude/file-history/<session_id>/<backup_file_name>` (or the
/// `CLAUDE_CONFIG_DIR` equivalent).
fn backup_path(session_id: &str, backup_file_name: &str, cfg: Option<&str>) -> Option<PathBuf> {
    let base = if let Some(dir) = cfg {
        PathBuf::from(dir)
    } else {
        dirs::home_dir()?.join(".claude")
    };
    Some(
        base.join("file-history")
            .join(session_id)
            .join(backup_file_name),
    )
}

// ─────────────────────────── Parsing ────────────────────────────────────────

/// Cheap byte pre-filter. A transcript line is worth JSON-parsing only if it
/// could carry an edit or a backup delta — rejects the vast majority of a
/// multi-megabyte transcript (attachment/assistant/system records) without
/// allocating.
#[inline]
fn line_is_interesting(line: &str) -> bool {
    line.contains("\"toolUseResult\"") || line.contains("\"file-history-delta\"")
}

/// Raw per-step facts lifted straight out of one `toolUseResult`.
#[derive(Debug, Clone)]
struct RawEdit {
    tool_use_id: String,
    timestamp: Option<String>,
    file_path: String,
    kind: StepKind,
    old_string: Option<String>,
    new_string: Option<String>,
    /// The full post-write content, for Create/Overwrite.
    content: Option<String>,
    original_file: Option<String>,
    replace_all: bool,
    user_modified: bool,
    is_sidechain: bool,
    agent_name: Option<String>,
}

/// A `file-history-delta` record's backup pointer for one tracked path.
#[derive(Debug, Clone)]
struct BackupRef {
    /// `None` means the file did not exist at the time of this delta.
    backup_file_name: Option<String>,
}

/// Extract a [`RawEdit`] from a decoded transcript record, or `None` if it
/// isn't one. Tolerates `toolUseResult` being a bare string (Bash results).
fn raw_edit_from_record(
    v: &serde_json::Value,
    is_sidechain: bool,
    agent_name: Option<&str>,
) -> Option<RawEdit> {
    if v.get("type").and_then(|t| t.as_str()) != Some("user") {
        return None;
    }
    let tur = v.get("toolUseResult")?;
    let tur = tur.as_object()?;

    let tool_use_id = v
        .get("message")?
        .get("content")?
        .get(0)?
        .get("tool_use_id")?
        .as_str()?
        .to_string();
    let timestamp = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .map(String::from);
    let file_path = tur.get("filePath").and_then(|s| s.as_str())?.to_string();
    let replace_all = tur
        .get("replaceAll")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    let user_modified = tur
        .get("userModified")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    let original_file = tur
        .get("originalFile")
        .and_then(|s| s.as_str())
        .map(String::from);
    let agent_name = agent_name.map(String::from);

    // Write result: has its own "type" field, "create" or "update".
    if let Some(write_kind) = tur.get("type").and_then(|s| s.as_str()) {
        let kind = if write_kind == "create" {
            StepKind::Create
        } else {
            StepKind::Overwrite
        };
        let content = tur
            .get("content")
            .and_then(|s| s.as_str())
            .map(String::from);
        return Some(RawEdit {
            tool_use_id,
            timestamp,
            file_path,
            kind,
            old_string: None,
            new_string: None,
            content,
            original_file,
            replace_all,
            user_modified,
            is_sidechain,
            agent_name,
        });
    }

    // Edit result: oldString/newString. Anything else (e.g. ExitPlanMode's
    // {filePath, plan, ...}) is not an edit we can replay — skip it.
    let old_string = tur
        .get("oldString")
        .and_then(|s| s.as_str())
        .map(String::from);
    let new_string = tur
        .get("newString")
        .and_then(|s| s.as_str())
        .map(String::from);
    if old_string.is_none() && new_string.is_none() {
        return None;
    }
    Some(RawEdit {
        tool_use_id,
        timestamp,
        file_path,
        kind: StepKind::Edit,
        old_string,
        new_string,
        content: None,
        original_file,
        replace_all,
        user_modified,
        is_sidechain,
        agent_name,
    })
}

/// Extract a `file-history-delta` record's backup pointer, keyed by the
/// tracked path exactly as `toolUseResult.filePath` would report it — i.e.
/// resolved to an absolute path via `realParentDir`.
fn backup_ref_from_record(v: &serde_json::Value) -> Option<(String, BackupRef)> {
    if v.get("type").and_then(|t| t.as_str()) != Some("file-history-delta") {
        return None;
    }
    let tracking_path = v.get("trackingPath").and_then(|s| s.as_str())?;
    let backup = v.get("backup")?;
    let backup_file_name = backup
        .get("backupFileName")
        .and_then(|s| s.as_str())
        .map(String::from);
    let real_parent_dir = backup.get("realParentDir").and_then(|s| s.as_str());

    // trackingPath is cwd-relative for in-project files, absolute otherwise.
    let abs_path = if Path::new(tracking_path).is_absolute() {
        tracking_path.to_string()
    } else if let Some(parent) = real_parent_dir {
        // realParentDir is the directory the tracked file's basename lives
        // in; trackingPath may include leading path components already
        // relative to the project root, but joining the basename onto
        // realParentDir is the one thing guaranteed correct in both cases.
        let name = Path::new(tracking_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        match name {
            Some(n) => PathBuf::from(parent).join(n).to_string_lossy().to_string(),
            None => tracking_path.to_string(),
        }
    } else {
        tracking_path.to_string()
    };

    Some((abs_path, BackupRef { backup_file_name }))
}

/// Accumulated results of scanning one or more transcripts.
#[derive(Debug, Default)]
struct TranscriptScan {
    edits: Vec<RawEdit>,
    /// path -> first-seen backup ref (the earliest delta for a path is the
    /// one describing session-start state; later deltas for the same path
    /// describe later per-turn snapshots we don't need).
    backups: HashMap<String, BackupRef>,
    title: Option<String>,
    last_prompt: Option<String>,
    first_ts: Option<String>,
    last_ts: Option<String>,
}

/// One streaming pass over one transcript, appending into `out`.
fn scan_transcript(
    path: &Path,
    is_sidechain: bool,
    agent_name: Option<&str>,
    out: &mut TranscriptScan,
    warnings: &mut Vec<String>,
) -> std::io::Result<()> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    for (lineno, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warnings.push(format!(
                    "{}: line {} could not be read ({e})",
                    path.display(),
                    lineno + 1
                ));
                continue;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || !line_is_interesting(trimmed) {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                // A malformed or truncated line (e.g. a live session's final,
                // not-yet-flushed line) — skip, don't abort the whole scan.
                warnings.push(format!(
                    "{}: line {} could not be parsed as JSON (skipped)",
                    path.display(),
                    lineno + 1
                ));
                continue;
            }
        };

        if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
            if out.first_ts.is_none() {
                out.first_ts = Some(ts.to_string());
            }
            out.last_ts = Some(ts.to_string());
        }

        match v.get("type").and_then(|t| t.as_str()) {
            Some("custom-title") => {
                if let Some(t) = v.get("customTitle").and_then(|s| s.as_str()) {
                    out.title = Some(t.to_string());
                }
                continue;
            }
            Some("last-prompt") => {
                if let Some(p) = v.get("lastPrompt").and_then(|s| s.as_str()) {
                    out.last_prompt = Some(p.to_string());
                }
                continue;
            }
            _ => {}
        }

        if let Some((abs_path, backup_ref)) = backup_ref_from_record(&v) {
            out.backups.entry(abs_path).or_insert(backup_ref);
            continue;
        }

        if let Some(edit) = raw_edit_from_record(&v, is_sidechain, agent_name) {
            out.edits.push(edit);
        }
    }
    Ok(())
}

// ─────────────────────────── Replay / diffing ───────────────────────────────

/// Apply one step forward against `content`. `Err` names the mismatch.
fn apply_forward(content: &str, e: &RawEdit) -> Result<String, String> {
    match e.kind {
        StepKind::Create | StepKind::Overwrite => Ok(e.content.clone().unwrap_or_default()),
        StepKind::Edit => {
            let old = e.old_string.as_deref().unwrap_or("");
            let new = e.new_string.as_deref().unwrap_or("");
            if e.replace_all {
                if !content.contains(old) {
                    return Err("substitution text not found".into());
                }
                Ok(content.replace(old, new))
            } else {
                match content.find(old) {
                    Some(idx) => {
                        let mut s = String::with_capacity(content.len());
                        s.push_str(&content[..idx]);
                        s.push_str(new);
                        s.push_str(&content[idx + old.len()..]);
                        Ok(s)
                    }
                    None => Err("substitution text not found".into()),
                }
            }
        }
    }
}

/// Undo one step against `content` (the inverse of [`apply_forward`]).
/// [`StepKind::Overwrite`] can never be reversed without already knowing its
/// prior content — this is the deliberate stop condition that makes
/// reverse-folding bottom out at [`BaseSource::Unknown`] for a file whose
/// history includes an overwrite with no earlier anchor.
fn apply_reverse(content: &str, e: &RawEdit) -> Result<String, String> {
    match e.kind {
        StepKind::Create => {
            if e.content.as_deref() == Some(content) {
                Ok(String::new())
            } else {
                Err("content diverged; cannot reverse create".into())
            }
        }
        StepKind::Overwrite => {
            Err("cannot reverse an overwrite without a known prior state".into())
        }
        StepKind::Edit => {
            let old = e.old_string.as_deref().unwrap_or("");
            let new = e.new_string.as_deref().unwrap_or("");
            if e.replace_all {
                if !content.contains(new) {
                    return Err("substitution text not found (reverse)".into());
                }
                Ok(content.replace(new, old))
            } else {
                match content.find(new) {
                    Some(idx) => {
                        let mut s = String::with_capacity(content.len());
                        s.push_str(&content[..idx]);
                        s.push_str(old);
                        s.push_str(&content[idx + new.len()..]);
                        Ok(s)
                    }
                    None => Err("substitution text not found (reverse)".into()),
                }
            }
        }
    }
}

fn backward_fold(disk: &str, edits: &[RawEdit]) -> Result<String, String> {
    let mut cur = disk.to_string();
    for e in edits.iter().rev() {
        cur = apply_reverse(&cur, e)?;
    }
    Ok(cur)
}

/// git's own binary heuristic: a NUL byte within the first 8000 bytes.
fn is_binary_str(data: &str) -> bool {
    data.as_bytes().iter().take(8000).any(|&b| b == 0)
}

fn is_binary_bytes(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&b| b == 0)
}

/// Build a `git apply`-able unified diff between `old` and `new`, plus
/// (additions, deletions). Returns an empty patch (and zero counts) when the
/// two are identical.
fn unified_patch(
    old: &str,
    new: &str,
    old_exists: bool,
    new_exists: bool,
    display_path: &str,
) -> (String, u32, u32) {
    if old == new {
        return (String::new(), 0, 0);
    }
    if is_binary_str(old) || is_binary_str(new) {
        return (
            format!("Binary files a/{display_path} and b/{display_path} differ\n"),
            0,
            0,
        );
    }

    use gix::diff::blob::{
        Algorithm, BasicLineDiffPrinter, InternedInput, UnifiedDiffConfig,
        diff_with_slider_heuristics,
    };
    let input = InternedInput::new(old, new);
    let diff = diff_with_slider_heuristics(Algorithm::Histogram, &input);
    let additions = diff.count_additions();
    let deletions = diff.count_removals();
    if additions == 0 && deletions == 0 {
        return (String::new(), 0, 0);
    }
    let body = diff
        .unified_diff(
            &BasicLineDiffPrinter(&input.interner),
            UnifiedDiffConfig::default(),
            &input,
        )
        .to_string();

    let mut patch = String::new();
    patch.push_str(&format!("diff --git a/{display_path} b/{display_path}\n"));
    if !old_exists {
        patch.push_str("new file mode 100644\n");
    } else if !new_exists {
        patch.push_str("deleted file mode 100644\n");
    }
    let a_label = if old_exists {
        format!("a/{display_path}")
    } else {
        "/dev/null".to_string()
    };
    let b_label = if new_exists {
        format!("b/{display_path}")
    } else {
        "/dev/null".to_string()
    };
    patch.push_str(&format!("--- {a_label}\n"));
    patch.push_str(&format!("+++ {b_label}\n"));
    patch.push_str(&body);
    (patch, additions, deletions)
}

/// Classify an absolute path against the (canonicalized) repo root.
fn classify_path(canonical_repo: &Option<PathBuf>, abs: &Path) -> (Option<String>, bool) {
    let Some(repo) = canonical_repo else {
        return (None, false);
    };
    let canon = abs.canonicalize().ok();
    let target = canon.as_deref().unwrap_or(abs);
    match target.strip_prefix(repo) {
        Ok(rel) => (Some(rel.to_string_lossy().replace('\\', "/")), true),
        Err(_) => (None, false),
    }
}

// ─────────────────────────── Per-file base resolution ───────────────────────

/// Session-start content resolution + per-step before/after fold for one file.
struct FileFold {
    base: Option<String>,
    base_source: BaseSource,
    /// One entry per edit in `edits`, in order.
    steps: Vec<StepFold>,
}

struct StepFold {
    before: Option<String>,
    after: Option<String>,
}

/// Resolve the session-start content of a file and fold every edit forward
/// from it, re-anchoring on any Create/Overwrite step (whose post-content is
/// always fully known) so a run of unresolvable Edits doesn't poison steps
/// that come after a later whole-file write.
fn fold_file(
    edits: &[RawEdit],
    backup_content: Option<&str>,
    disk_content: Option<&str>,
) -> FileFold {
    let (base, base_source) = if let Some(b) = backup_content {
        (Some(b.to_string()), BaseSource::Backup)
    } else if edits.first().map(|e| e.kind) == Some(StepKind::Create) {
        (Some(String::new()), BaseSource::CreatedInSession)
    } else if let Some(orig) = edits.first().and_then(|e| e.original_file.clone()) {
        (Some(orig), BaseSource::ToolResult)
    } else if let Some(disk) = disk_content {
        match backward_fold(disk, edits) {
            Ok(reconstructed) => (Some(reconstructed), BaseSource::Reconstructed),
            Err(_) => (None, BaseSource::Unknown),
        }
    } else {
        (None, BaseSource::Unknown)
    };

    let mut cur = base.clone();
    let mut steps = Vec::with_capacity(edits.len());
    for e in edits {
        let before = cur.clone();
        let after = match (cur.as_ref(), e.kind) {
            (_, StepKind::Create | StepKind::Overwrite) => {
                let c = e.content.clone().unwrap_or_default();
                cur = Some(c.clone());
                Some(c)
            }
            (Some(content), StepKind::Edit) => match apply_forward(content, e) {
                Ok(new_content) => {
                    cur = Some(new_content.clone());
                    Some(new_content)
                }
                Err(_) => {
                    cur = None;
                    None
                }
            },
            (None, StepKind::Edit) => None,
        };
        steps.push(StepFold { before, after });
    }

    FileFold {
        base,
        base_source,
        steps,
    }
}

// ─────────────────────────── Assembly ───────────────────────────────────────

/// Everything the revert commands need beyond the wire `SessionReview`.
pub(crate) struct BuildResult {
    pub review: SessionReview,
    /// abs_path -> reconstructed session-start text (only for paths where a
    /// text base was resolved — i.e. `base_source != Unknown`).
    file_bases: HashMap<String, String>,
    /// abs_path -> raw `@v1` backup bytes (only when one was found on disk).
    backup_bytes: HashMap<String, Vec<u8>>,
}

fn build_session_review_full(
    repo_path: &Path,
    transcript: &Path,
    subagents: &[PathBuf],
    session_id: &str,
    claude_config_dir: Option<&str>,
) -> Result<BuildResult, String> {
    let mut warnings = Vec::new();
    let mut scan = TranscriptScan::default();
    scan_transcript(transcript, false, None, &mut scan, &mut warnings)
        .map_err(|e| format!("Failed to read transcript: {e}"))?;

    let mut included_subagents = false;
    for sub in subagents {
        included_subagents = true;
        let agent_name = subagent_name_from_path(sub);
        if let Err(e) = scan_transcript(sub, true, agent_name.as_deref(), &mut scan, &mut warnings)
        {
            warnings.push(format!(
                "Failed to read subagent transcript {}: {e}",
                sub.display()
            ));
        }
    }

    // Stable chronological order: transcript append-order is already
    // chronological within one file; interleave main + subagent by
    // timestamp (falling back to keeping relative order for ties/missing
    // timestamps via a stable sort).
    let mut indexed: Vec<(usize, RawEdit)> = scan.edits.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp).then(a.0.cmp(&b.0)));
    let edits: Vec<RawEdit> = indexed.into_iter().map(|(_, e)| e).collect();

    let canonical_repo = repo_path.canonicalize().ok();

    // Group edit indices by path, preserving first-touch order.
    let mut path_order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in edits.iter().enumerate() {
        groups
            .entry(e.file_path.clone())
            .or_insert_with(|| {
                path_order.push(e.file_path.clone());
                Vec::new()
            })
            .push(i);
    }

    let mut wire_steps: Vec<Option<EditStep>> = (0..edits.len()).map(|_| None).collect();
    let mut files = Vec::with_capacity(path_order.len());
    let mut file_bases = HashMap::new();
    let mut backup_bytes = HashMap::new();

    for path in path_order {
        let idxs = groups.remove(&path).unwrap_or_default();
        let edits_for_file: Vec<RawEdit> = idxs.iter().map(|&i| edits[i].clone()).collect();

        let abs = PathBuf::from(&path);
        let (rel_path, in_repo) = classify_path(&canonical_repo, &abs);
        let display_path = rel_path.clone().unwrap_or_else(|| path.clone());

        let raw_backup_bytes = scan
            .backups
            .get(&path)
            .and_then(|b| b.backup_file_name.as_ref())
            .and_then(|name| backup_path(session_id, name, claude_config_dir))
            .and_then(|p| std::fs::read(&p).ok());
        let backup_available = raw_backup_bytes.is_some();
        let backup_content = raw_backup_bytes.as_ref().and_then(|bytes| {
            (!is_binary_bytes(bytes)).then(|| String::from_utf8_lossy(bytes).into_owned())
        });
        if let Some(bytes) = raw_backup_bytes {
            backup_bytes.insert(path.clone(), bytes);
        }

        let disk_exists = abs.exists();
        let disk_bytes = std::fs::read(&abs).ok();
        let disk_is_binary = disk_bytes.as_deref().map(is_binary_bytes).unwrap_or(false);
        let disk_content = disk_bytes
            .as_ref()
            .filter(|b| !is_binary_bytes(b))
            .map(|b| String::from_utf8_lossy(b).into_owned());

        let fold = fold_file(
            &edits_for_file,
            backup_content.as_deref(),
            disk_content.as_deref(),
        );
        if let Some(base) = &fold.base {
            file_bases.insert(path.clone(), base.clone());
        }

        for (local_i, e) in edits_for_file.iter().enumerate() {
            let sf = &fold.steps[local_i];
            let (patch, additions, deletions) = match (&sf.before, &sf.after) {
                (Some(b), Some(a)) => {
                    let old_exists = !matches!(e.kind, StepKind::Create);
                    unified_patch(b, a, old_exists, true, &display_path)
                }
                _ => match e.kind {
                    StepKind::Edit => {
                        let old = e.old_string.as_deref().unwrap_or("");
                        let new = e.new_string.as_deref().unwrap_or("");
                        unified_patch(old, new, true, true, &display_path)
                    }
                    StepKind::Create | StepKind::Overwrite => {
                        let content = e.content.as_deref().unwrap_or("");
                        unified_patch("", content, false, true, &display_path)
                    }
                },
            };

            let overall_index = idxs[local_i];
            wire_steps[overall_index] = Some(EditStep {
                step_index: overall_index as u32,
                tool_use_id: e.tool_use_id.clone(),
                timestamp: e.timestamp.clone(),
                kind: e.kind,
                abs_path: path.clone(),
                rel_path: rel_path.clone(),
                in_repo,
                patch,
                additions,
                deletions,
                is_sidechain: e.is_sidechain,
                agent_name: e.agent_name.clone(),
                user_modified: e.user_modified,
                replace_all: e.replace_all,
            });
        }

        let final_content = fold
            .steps
            .last()
            .and_then(|s| s.after.clone())
            .or_else(|| fold.base.clone());
        let (cumulative_patch, additions, deletions) = match (&fold.base, &final_content) {
            (Some(b), Some(f)) => {
                let old_exists = fold.base_source != BaseSource::CreatedInSession;
                unified_patch(b, f, old_exists, disk_exists, &display_path)
            }
            _ => (String::new(), 0, 0),
        };

        let net_change = if !disk_exists {
            NetChange::Deleted
        } else if fold.base_source == BaseSource::CreatedInSession {
            NetChange::Added
        } else if fold.base.as_deref() == final_content.as_deref() {
            NetChange::Unchanged
        } else {
            NetChange::Modified
        };

        // Drift: does folding forward from base reproduce the current disk
        // content? Only meaningful when both sides are known; otherwise we
        // can't tell and default to "no drift" rather than a false alarm.
        let drifted_from_disk = match (&final_content, &disk_content) {
            (Some(f), Some(d)) => f != d,
            _ => false,
        };

        files.push(FileReview {
            abs_path: path.clone(),
            rel_path: rel_path.clone(),
            in_repo,
            display_path,
            net_change,
            base_source: fold.base_source,
            cumulative_patch,
            additions,
            deletions,
            step_indices: idxs.iter().map(|&i| i as u32).collect(),
            drifted_from_disk,
            backup_available,
            is_binary: disk_is_binary,
        });
    }

    let steps: Vec<EditStep> = wire_steps.into_iter().flatten().collect();

    Ok(BuildResult {
        review: SessionReview {
            session_id: session_id.to_string(),
            transcript_path: transcript.to_string_lossy().to_string(),
            repo_path: repo_path.to_string_lossy().to_string(),
            started_at: scan.first_ts,
            ended_at: scan.last_ts,
            title: scan.title,
            steps,
            files,
            warnings,
            included_subagents,
        },
        file_bases,
        backup_bytes,
    })
}

/// The whole pipeline, with no Tauri/HTTP in sight — the unit-test entry
/// point and what [`get_session_review`] calls (through the cache).
pub(crate) fn build_session_review(
    repo_path: &Path,
    transcript: &Path,
    subagents: &[PathBuf],
    session_id: &str,
    claude_config_dir: Option<&str>,
) -> Result<SessionReview, String> {
    build_session_review_full(
        repo_path,
        transcript,
        subagents,
        session_id,
        claude_config_dir,
    )
    .map(|r| r.review)
}

/// Find one edit anywhere in the session (main thread or subagents) by its
/// `tool_use_id`, without building the whole review. Used by
/// [`revert_session_step`]'s out-of-repo fallback, which only needs this one
/// record's raw substitution, not the full assembled timeline.
fn find_raw_edit_by_tool_use_id(
    transcript: &Path,
    subagents: &[PathBuf],
    tool_use_id: &str,
) -> Option<RawEdit> {
    find_in_one(transcript, false, None, tool_use_id).or_else(|| {
        subagents.iter().find_map(|s| {
            let agent_name = subagent_name_from_path(s);
            find_in_one(s, true, agent_name.as_deref(), tool_use_id)
        })
    })
}

fn find_in_one(
    path: &Path,
    is_sidechain: bool,
    agent_name: Option<&str>,
    tool_use_id: &str,
) -> Option<RawEdit> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines() {
        let line = line.ok()?;
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains(tool_use_id) || !line_is_interesting(trimmed) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if let Some(e) = raw_edit_from_record(&v, is_sidechain, agent_name)
            && e.tool_use_id == tool_use_id
        {
            return Some(e);
        }
    }
    None
}

// ─────────────────────────── Session listing ────────────────────────────────

#[derive(Default)]
struct HeadInfo {
    cwd: Option<String>,
    git_branch: Option<String>,
    started_at: Option<String>,
}

#[derive(Default)]
struct TailInfo {
    title: Option<String>,
    last_prompt: Option<String>,
    ended_at: Option<String>,
}

/// Read the first 8 KB of a transcript for cheap session metadata.
fn read_session_head(path: &Path) -> HeadInfo {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return HeadInfo::default();
    };
    let mut buf = vec![0u8; 8192];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    let text = String::from_utf8_lossy(&buf);
    let mut out = HeadInfo::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if out.cwd.is_none() {
            out.cwd = v.get("cwd").and_then(|s| s.as_str()).map(String::from);
        }
        if out.git_branch.is_none() {
            out.git_branch = v
                .get("gitBranch")
                .and_then(|s| s.as_str())
                .map(String::from);
        }
        if out.started_at.is_none() {
            out.started_at = v
                .get("timestamp")
                .and_then(|s| s.as_str())
                .map(String::from);
        }
        if out.cwd.is_some() && out.git_branch.is_some() && out.started_at.is_some() {
            break;
        }
    }
    out
}

/// Read the last 64 KB of a transcript for cheap session metadata —
/// `custom-title`/`last-prompt` records recur every turn, so the last copy
/// is always near EOF.
fn read_session_tail(path: &Path) -> TailInfo {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(meta) = std::fs::metadata(path) else {
        return TailInfo::default();
    };
    const WINDOW: u64 = 64 * 1024;
    let start = meta.len().saturating_sub(WINDOW);
    let Ok(mut f) = std::fs::File::open(path) else {
        return TailInfo::default();
    };
    if f.seek(SeekFrom::Start(start)).is_err() {
        return TailInfo::default();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return TailInfo::default();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut out = TailInfo::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("custom-title") => {
                if let Some(t) = v.get("customTitle").and_then(|s| s.as_str()) {
                    out.title = Some(t.to_string());
                }
            }
            Some("last-prompt") => {
                if let Some(p) = v.get("lastPrompt").and_then(|s| s.as_str()) {
                    out.last_prompt = Some(p.to_string());
                }
            }
            _ => {}
        }
        if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
            out.ended_at = Some(ts.to_string());
        }
    }
    out
}

/// Full-scan edit/file counts — only run for the first `limit` sessions when
/// `include_counts` is requested.
fn count_edits_and_files(transcript: &Path, subagents: &[PathBuf]) -> (u32, u32) {
    let mut warnings = Vec::new();
    let mut scan = TranscriptScan::default();
    if scan_transcript(transcript, false, None, &mut scan, &mut warnings).is_err() {
        return (0, 0);
    }
    for sub in subagents {
        let agent_name = subagent_name_from_path(sub);
        let _ = scan_transcript(sub, true, agent_name.as_deref(), &mut scan, &mut warnings);
    }
    let mut paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for e in &scan.edits {
        paths.insert(&e.file_path);
    }
    (scan.edits.len() as u32, paths.len() as u32)
}

/// List recent Claude Code sessions whose transcripts live under the
/// project slug for `repo_path`. Newest first.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn list_review_sessions(
    repo_path: String,
    limit: Option<u32>,
    include_counts: Option<bool>,
    claude_config_dir: Option<String>,
) -> Result<Vec<SessionSummary>, String> {
    let limit = limit.unwrap_or(20).min(50) as usize;
    let include_counts = include_counts.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        let Some(project_dir) = project_dir_for(&repo_path, claude_config_dir.as_deref()) else {
            return Vec::new();
        };
        if !project_dir.is_dir() {
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&project_dir) else {
            return Vec::new();
        };
        let mut files: Vec<(PathBuf, std::fs::Metadata)> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .filter_map(|e| e.metadata().ok().map(|m| (e.path(), m)))
            .collect();
        files.sort_by_key(|(_, m)| std::cmp::Reverse(m.modified().ok()));

        let mut out = Vec::new();
        for (path, meta) in files.into_iter().take(limit) {
            let Some(session_id) = path.file_stem().and_then(|s| s.to_str()).map(String::from)
            else {
                continue;
            };
            let head = read_session_head(&path);
            let tail = read_session_tail(&path);
            let has_subagents = project_dir.join(&session_id).join("subagents").is_dir();
            let (edit_count, file_count) = if include_counts {
                let subs = subagent_transcripts(&project_dir, &session_id);
                let (e, f) = count_edits_and_files(&path, &subs);
                (Some(e), Some(f))
            } else {
                (None, None)
            };
            out.push(SessionSummary {
                session_id,
                transcript_path: path.to_string_lossy().to_string(),
                cwd: head.cwd,
                git_branch: head.git_branch,
                started_at: head.started_at,
                ended_at: tail.ended_at,
                title: tail.title,
                last_prompt: tail.last_prompt,
                size_bytes: meta.len(),
                edit_count,
                file_count,
                has_subagents,
            });
        }
        out
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))
}

// ─────────────────────────── Cache ──────────────────────────────────────────

struct CachedReview {
    len: u64,
    mtime: std::time::SystemTime,
    review: SessionReview,
}

const MAX_CACHED_REVIEWS: usize = 4;

fn review_cache() -> &'static Mutex<HashMap<PathBuf, CachedReview>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedReview>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_cached_review(transcript: &Path) -> Option<SessionReview> {
    let meta = std::fs::metadata(transcript).ok()?;
    let mtime = meta.modified().ok()?;
    let map = review_cache().lock().ok()?;
    let entry = map.get(transcript)?;
    (entry.len == meta.len() && entry.mtime == mtime).then(|| entry.review.clone())
}

fn put_cached_review(transcript: &Path, review: &SessionReview) {
    let Ok(meta) = std::fs::metadata(transcript) else {
        return;
    };
    let Ok(mtime) = meta.modified() else {
        return;
    };
    let Ok(mut map) = review_cache().lock() else {
        return;
    };
    if map.len() >= MAX_CACHED_REVIEWS
        && !map.contains_key(transcript)
        && let Some(k) = map.keys().next().cloned()
    {
        map.remove(&k);
    }
    map.insert(
        transcript.to_path_buf(),
        CachedReview {
            len: meta.len(),
            mtime,
            review: review.clone(),
        },
    );
}

// ─────────────────────────── Commands ───────────────────────────────────────

/// Parse a session transcript into the full step timeline + per-file rollups.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn get_session_review(
    repo_path: String,
    session_id: String,
    include_subagents: Option<bool>,
    claude_config_dir: Option<String>,
) -> Result<SessionReview, String> {
    let include_subagents = include_subagents.unwrap_or(true);
    tokio::task::spawn_blocking(move || {
        let project_dir = project_dir_for(&repo_path, claude_config_dir.as_deref())
            .ok_or_else(|| "Could not determine Claude project directory".to_string())?;
        let transcript = project_dir.join(format!("{session_id}.jsonl"));
        if !transcript.is_file() {
            return Err(format!(
                "Session transcript not found: {}",
                transcript.display()
            ));
        }
        if let Some(cached) = get_cached_review(&transcript) {
            return Ok(cached);
        }
        let repo = PathBuf::from(&repo_path);
        let subs = if include_subagents {
            subagent_transcripts(&project_dir, &session_id)
        } else {
            Vec::new()
        };
        let review = build_session_review(
            &repo,
            &transcript,
            &subs,
            &session_id,
            claude_config_dir.as_deref(),
        )?;
        put_cached_review(&transcript, &review);
        Ok(review)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

fn no_match_result(abs_path: &str) -> RevertResult {
    RevertResult {
        applied: false,
        method: "string_substitution".into(),
        abs_path: abs_path.to_string(),
        message: Some("The edited text was not found — a later change may have already altered or reverted it".into()),
    }
}

fn drifted_result(abs_path: &str) -> RevertResult {
    RevertResult {
        applied: false,
        method: "string_substitution".into(),
        abs_path: abs_path.to_string(),
        message: Some("The file has changed since this step; cannot safely revert".into()),
    }
}

/// Out-of-repo fallback for [`revert_session_step`]: run the edit's own
/// substitution backwards against the file's *current* content, so a later,
/// unrelated edit to the same file doesn't block this one — mirroring what
/// `git apply --reverse`'s hunk-context matching gives the in-repo path.
fn revert_step_via_substitution(edit: &RawEdit, dry_run: bool) -> Result<RevertResult, String> {
    let path = PathBuf::from(&edit.file_path);
    let current = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    match edit.kind {
        StepKind::Edit => {
            let old = edit.old_string.as_deref().unwrap_or("");
            let new = edit.new_string.as_deref().unwrap_or("");
            let new_content = if edit.replace_all {
                if !current.contains(new) {
                    return Ok(no_match_result(&edit.file_path));
                }
                current.replace(new, old)
            } else {
                match current.find(new) {
                    Some(idx) => {
                        let mut s = String::with_capacity(current.len());
                        s.push_str(&current[..idx]);
                        s.push_str(old);
                        s.push_str(&current[idx + new.len()..]);
                        s
                    }
                    None => return Ok(no_match_result(&edit.file_path)),
                }
            };
            if !dry_run {
                std::fs::write(&path, &new_content)
                    .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
            }
            Ok(RevertResult {
                applied: !dry_run,
                method: "string_substitution".into(),
                abs_path: edit.file_path.clone(),
                message: dry_run.then(|| "This step can be reverted".into()),
            })
        }
        StepKind::Create => {
            let after = edit.content.as_deref().unwrap_or("");
            if current != after {
                return Ok(drifted_result(&edit.file_path));
            }
            if !dry_run {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
            }
            Ok(RevertResult {
                applied: !dry_run,
                method: "delete_file".into(),
                abs_path: edit.file_path.clone(),
                message: dry_run.then(|| "File would be deleted".into()),
            })
        }
        StepKind::Overwrite => {
            let after = edit.content.as_deref().unwrap_or("");
            if current != after {
                return Ok(drifted_result(&edit.file_path));
            }
            Ok(RevertResult {
                applied: false,
                method: "string_substitution".into(),
                abs_path: edit.file_path.clone(),
                message: Some(
                    "Cannot revert an overwrite step in isolation for a file outside the repository".into(),
                ),
            })
        }
    }
}

/// Undo one step, keeping every later step. `dry_run` reports feasibility
/// without touching the working tree.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn revert_session_step(
    repo_path: String,
    session_id: String,
    tool_use_id: String,
    dry_run: Option<bool>,
    claude_config_dir: Option<String>,
) -> Result<RevertResult, String> {
    let dry_run = dry_run.unwrap_or(false);
    let repo_for_bump = repo_path.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<RevertResult, String> {
        let project_dir = project_dir_for(&repo_path, claude_config_dir.as_deref())
            .ok_or_else(|| "Could not determine Claude project directory".to_string())?;
        let transcript = project_dir.join(format!("{session_id}.jsonl"));
        if !transcript.is_file() {
            return Err(format!(
                "Session transcript not found: {}",
                transcript.display()
            ));
        }
        let repo = PathBuf::from(&repo_path);
        let subs = subagent_transcripts(&project_dir, &session_id);
        let review = build_session_review(
            &repo,
            &transcript,
            &subs,
            &session_id,
            claude_config_dir.as_deref(),
        )?;

        let Some(step) = review.steps.iter().find(|s| s.tool_use_id == tool_use_id) else {
            return Err(format!(
                "No step with tool_use_id {tool_use_id} in this session"
            ));
        };

        if step.in_repo {
            if step.patch.trim().is_empty() {
                return Ok(RevertResult {
                    applied: false,
                    method: "git_apply_reverse".into(),
                    abs_path: step.abs_path.clone(),
                    message: Some("Nothing to revert — this step made no change".into()),
                });
            }
            match crate::git::apply_reverse_patch_impl(&repo_path, &step.patch, None, dry_run) {
                Ok(()) => Ok(RevertResult {
                    applied: !dry_run,
                    method: "git_apply_reverse".into(),
                    abs_path: step.abs_path.clone(),
                    message: dry_run.then(|| "This step can be reverted".into()),
                }),
                Err(e) => Ok(RevertResult {
                    applied: false,
                    method: "git_apply_reverse".into(),
                    abs_path: step.abs_path.clone(),
                    message: Some(e),
                }),
            }
        } else {
            let Some(edit) = find_raw_edit_by_tool_use_id(&transcript, &subs, &tool_use_id) else {
                return Err(format!(
                    "Could not re-locate step {tool_use_id} in the transcript"
                ));
            };
            revert_step_via_substitution(&edit, dry_run)
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))??;

    if result.applied {
        crate::git::bump_working_tree_epoch(&repo_for_bump);
    }
    Ok(result)
}

/// Restore one file to its pre-session content (or delete it if the session
/// created it). `force` overrides the `drifted_from_disk` guard.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn revert_file_to_session_start(
    repo_path: String,
    session_id: String,
    abs_path: String,
    force: Option<bool>,
    dry_run: Option<bool>,
    claude_config_dir: Option<String>,
) -> Result<RevertResult, String> {
    let force = force.unwrap_or(false);
    let dry_run = dry_run.unwrap_or(false);
    let repo_for_bump = repo_path.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<RevertResult, String> {
        let project_dir = project_dir_for(&repo_path, claude_config_dir.as_deref())
            .ok_or_else(|| "Could not determine Claude project directory".to_string())?;
        let transcript = project_dir.join(format!("{session_id}.jsonl"));
        if !transcript.is_file() {
            return Err(format!("Session transcript not found: {}", transcript.display()));
        }
        let repo = PathBuf::from(&repo_path);
        let subs = subagent_transcripts(&project_dir, &session_id);
        let built = build_session_review_full(&repo, &transcript, &subs, &session_id, claude_config_dir.as_deref())?;

        let Some(file) = built.review.files.iter().find(|f| f.abs_path == abs_path) else {
            return Err(format!("No file review entry for {abs_path} in this session"));
        };

        if file.drifted_from_disk && !force {
            return Ok(RevertResult {
                applied: false,
                method: "write_base".into(),
                abs_path: abs_path.clone(),
                message: Some(
                    "This file has changed outside the session since it was last touched; pass force to overwrite anyway"
                        .into(),
                ),
            });
        }

        match file.base_source {
            BaseSource::Unknown => Ok(RevertResult {
                applied: false,
                method: "write_base".into(),
                abs_path: abs_path.clone(),
                message: Some("Could not determine this file's content at session start".into()),
            }),
            BaseSource::CreatedInSession => {
                if !dry_run {
                    let _ = std::fs::remove_file(&abs_path);
                }
                Ok(RevertResult {
                    applied: !dry_run,
                    method: "delete_file".into(),
                    abs_path: abs_path.clone(),
                    message: dry_run.then(|| "File would be deleted (created this session)".into()),
                })
            }
            BaseSource::Backup if built.backup_bytes.contains_key(&abs_path) => {
                let bytes = &built.backup_bytes[&abs_path];
                if !dry_run {
                    std::fs::write(&abs_path, bytes).map_err(|e| format!("Failed to write {abs_path}: {e}"))?;
                }
                Ok(RevertResult {
                    applied: !dry_run,
                    method: "restore_backup".into(),
                    abs_path: abs_path.clone(),
                    message: dry_run.then(|| "File would be restored from its session-start backup".into()),
                })
            }
            _ => match built.file_bases.get(&abs_path) {
                Some(base_text) => {
                    if !dry_run {
                        std::fs::write(&abs_path, base_text).map_err(|e| format!("Failed to write {abs_path}: {e}"))?;
                    }
                    Ok(RevertResult {
                        applied: !dry_run,
                        method: "write_base".into(),
                        abs_path: abs_path.clone(),
                        message: dry_run.then(|| "File would be restored to its reconstructed session-start content".into()),
                    })
                }
                None => Ok(RevertResult {
                    applied: false,
                    method: "write_base".into(),
                    abs_path: abs_path.clone(),
                    message: Some("Could not determine this file's content at session start".into()),
                }),
            },
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))??;

    if result.applied {
        crate::git::bump_working_tree_epoch(&repo_for_bump);
    }
    Ok(result)
}

// ─────────────────────────── Tests ──────────────────────────────────────────

#[cfg(test)]
pub(crate) mod test_fixtures {
    use super::*;
    use tempfile::TempDir;

    /// Builder for a synthetic Claude environment: a temp `CLAUDE_CONFIG_DIR`
    /// with `projects/<slug>/<uuid>.jsonl`, optional `subagents/`, and a
    /// matching `file-history/<uuid>/` backup dir.
    pub(crate) struct TranscriptBuilder {
        config_dir: TempDir,
        repo_cwd: String,
        session_id: String,
        lines: Vec<String>,
        subagent_lines: HashMap<String, Vec<String>>,
        seq: u64,
        /// Every `tool_use_id` generated so far, in call order — so a test
        /// can chain builder calls (which must return `Self`) and still get
        /// back the ids it needs for revert-style assertions.
        tool_use_ids: Vec<String>,
    }

    fn ts(seq: u64) -> String {
        format!("2026-09-14T21:{:02}:{:02}.000Z", seq / 60, seq % 60)
    }

    impl TranscriptBuilder {
        pub(crate) fn new(repo_cwd: &str) -> Self {
            TranscriptBuilder {
                config_dir: tempfile::tempdir().expect("tempdir"),
                repo_cwd: repo_cwd.to_string(),
                session_id: uuid::Uuid::new_v4().to_string(),
                lines: Vec::new(),
                subagent_lines: HashMap::new(),
                seq: 0,
                tool_use_ids: Vec::new(),
            }
        }

        /// The `tool_use_id` generated by the Nth (0-based) edit/write call.
        pub(crate) fn tool_use_id(&self, n: usize) -> String {
            self.tool_use_ids[n].clone()
        }

        /// The `tool_use_id` generated by the most recent edit/write call.
        pub(crate) fn last_tool_use_id(&self) -> String {
            self.tool_use_ids
                .last()
                .cloned()
                .expect("no edit/write recorded yet")
        }

        fn next_ts(&mut self) -> String {
            self.seq += 1;
            ts(self.seq)
        }

        fn push_tool_result(&mut self, tool_use_id: &str, tur: serde_json::Value) {
            let timestamp = self.next_ts();
            let record = serde_json::json!({
                "type": "user",
                "timestamp": timestamp,
                "cwd": self.repo_cwd,
                "gitBranch": "main",
                "isSidechain": false,
                "message": {"content": [{"type": "tool_result", "tool_use_id": tool_use_id}]},
                "toolUseResult": tur,
            });
            self.lines.push(record.to_string());
        }

        /// Register a `file-history-delta` naming a `@v1` backup, and write
        /// that backup file's bytes to disk.
        pub(crate) fn backup(mut self, tracking_path: &str, pristine: &str) -> Self {
            let name = format!("{:x}@v1", md5_stub(tracking_path));
            let history_dir = self
                .config_dir
                .path()
                .join("file-history")
                .join(&self.session_id);
            std::fs::create_dir_all(&history_dir).unwrap();
            std::fs::write(history_dir.join(&name), pristine).unwrap();
            let ts = self.next_ts();
            let real_parent_dir = Path::new(tracking_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string());
            let record = serde_json::json!({
                "type": "file-history-delta",
                "messageId": uuid::Uuid::new_v4().to_string(),
                "snapshotMessageId": uuid::Uuid::new_v4().to_string(),
                "trackingPath": tracking_path,
                "backup": {"backupFileName": name, "version": 1, "backupTime": ts, "realParentDir": real_parent_dir},
                "timestamp": ts,
            });
            self.lines.push(record.to_string());
            self
        }

        /// Register a delta with `backupFileName: null` (file absent at
        /// session start).
        pub(crate) fn absent_at_start(mut self, tracking_path: &str) -> Self {
            let ts = self.next_ts();
            let real_parent_dir = Path::new(tracking_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string());
            let record = serde_json::json!({
                "type": "file-history-delta",
                "messageId": uuid::Uuid::new_v4().to_string(),
                "snapshotMessageId": uuid::Uuid::new_v4().to_string(),
                "trackingPath": tracking_path,
                "backup": {"backupFileName": serde_json::Value::Null, "version": 1, "backupTime": ts, "realParentDir": real_parent_dir},
                "timestamp": ts,
            });
            self.lines.push(record.to_string());
            self
        }

        pub(crate) fn edit(mut self, abs: &str, old: &str, new: &str, replace_all: bool) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            self.push_tool_result(
                &tool_use_id,
                serde_json::json!({
                    "filePath": abs, "oldString": old, "newString": new,
                    "originalFile": serde_json::Value::Null, "replaceAll": replace_all,
                    "structuredPatch": [], "userModified": false,
                }),
            );
            self.tool_use_ids.push(tool_use_id);
            self
        }

        pub(crate) fn edit_with_original(
            mut self,
            abs: &str,
            old: &str,
            new: &str,
            original_file: &str,
        ) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            self.push_tool_result(
                &tool_use_id,
                serde_json::json!({
                    "filePath": abs, "oldString": old, "newString": new,
                    "originalFile": original_file, "replaceAll": false,
                    "structuredPatch": [], "userModified": false,
                }),
            );
            self.tool_use_ids.push(tool_use_id);
            self
        }

        pub(crate) fn write_create(mut self, abs: &str, content: &str) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            self.push_tool_result(
                &tool_use_id,
                serde_json::json!({
                    "type": "create", "filePath": abs, "content": content,
                    "originalFile": serde_json::Value::Null, "structuredPatch": [], "userModified": false,
                }),
            );
            self.tool_use_ids.push(tool_use_id);
            self
        }

        pub(crate) fn write_update(mut self, abs: &str, content: &str) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            self.push_tool_result(
                &tool_use_id,
                serde_json::json!({
                    "type": "update", "filePath": abs, "content": content,
                    "originalFile": serde_json::Value::Null, "structuredPatch": [{"dummy": true}], "userModified": false,
                }),
            );
            self.tool_use_ids.push(tool_use_id);
            self
        }

        /// A Bash-shaped record whose `toolUseResult` is a bare string.
        pub(crate) fn bash_result(mut self, output: &str) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            let timestamp = self.next_ts();
            let record = serde_json::json!({
                "type": "user", "timestamp": timestamp,
                "message": {"content": [{"type": "tool_result", "tool_use_id": tool_use_id}]},
                "toolUseResult": output,
            });
            self.lines.push(record.to_string());
            self
        }

        /// Append an arbitrary raw line — for malformed/truncated cases.
        pub(crate) fn raw_line(mut self, line: &str) -> Self {
            self.lines.push(line.to_string());
            self
        }

        pub(crate) fn custom_title(mut self, title: &str) -> Self {
            let record = serde_json::json!({"type": "custom-title", "customTitle": title, "sessionId": self.session_id});
            self.lines.push(record.to_string());
            self
        }

        /// Append a record into `subagents/agent-<id>.jsonl` with isSidechain.
        pub(crate) fn subagent_edit(
            mut self,
            agent: &str,
            abs: &str,
            old: &str,
            new: &str,
        ) -> Self {
            let tool_use_id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            let timestamp = self.next_ts();
            let record = serde_json::json!({
                "type": "user", "timestamp": timestamp, "isSidechain": true,
                "message": {"content": [{"type": "tool_result", "tool_use_id": tool_use_id}]},
                "toolUseResult": {
                    "filePath": abs, "oldString": old, "newString": new,
                    "originalFile": serde_json::Value::Null, "replaceAll": false,
                    "structuredPatch": [], "userModified": false,
                },
            });
            self.subagent_lines
                .entry(agent.to_string())
                .or_default()
                .push(record.to_string());
            self.tool_use_ids.push(tool_use_id);
            self
        }

        pub(crate) fn build(self) -> (TempDir, PathBuf) {
            let project_dir_str = crate::agent_session::claude_project_dir(
                self.repo_cwd.clone(),
                Some(self.config_dir.path().to_string_lossy().to_string()),
            )
            .expect("claude_project_dir");
            let project_dir = PathBuf::from(project_dir_str);
            std::fs::create_dir_all(&project_dir).unwrap();
            let transcript = project_dir.join(format!("{}.jsonl", self.session_id));
            std::fs::write(&transcript, self.lines.join("\n") + "\n").unwrap();

            if !self.subagent_lines.is_empty() {
                let sub_dir = project_dir.join(&self.session_id).join("subagents");
                std::fs::create_dir_all(&sub_dir).unwrap();
                for (agent, lines) in &self.subagent_lines {
                    std::fs::write(
                        sub_dir.join(format!("agent-{agent}.jsonl")),
                        lines.join("\n") + "\n",
                    )
                    .unwrap();
                }
            }
            (self.config_dir, transcript)
        }
    }

    /// Tiny non-cryptographic stand-in for a stable backup filename — real
    /// uniqueness doesn't matter in tests, only determinism per input.
    fn md5_stub(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::TranscriptBuilder;
    use super::*;
    use crate::git_reads::test_fixtures::fixture_repo;

    fn build(repo: &Path, transcript: &Path, session_id: &str, config_dir: &str) -> SessionReview {
        build_session_review(repo, transcript, &[], session_id, Some(config_dir))
            .expect("build should succeed")
    }

    // ── Parser correctness ──────────────────────────────────────────────

    #[test]
    fn edit_step_has_exact_patch_and_counts() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "line1\nline2\nline3\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "line2\n", "line2-changed\n", false).build();
        // session id is embedded in the transcript path; recover it.
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert_eq!(review.steps.len(), 1);
        let step = &review.steps[0];
        assert!(step.patch.contains("-line2\n") || step.patch.contains("-line2"));
        assert!(step.patch.contains("+line2-changed"));
        assert_eq!(step.additions, 1);
        assert_eq!(step.deletions, 1);
        assert_eq!(step.kind, StepKind::Edit);
    }

    #[test]
    fn replace_all_true_replaces_every_occurrence() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("dup.txt").to_string_lossy().to_string();
        // Disk reflects post-edit state (all "x" replaced) — base is
        // reverse-folded from here since there's no backup/originalFile.
        std::fs::write(&abs, "y\ny\ny\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "x\n", "y\n", true).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.additions, 3);
        assert_eq!(file.deletions, 3);
    }

    #[test]
    fn replace_all_false_replaces_only_first() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("dup2.txt").to_string_lossy().to_string();
        // Disk reflects post-edit state (only the first "x" replaced).
        std::fs::write(&abs, "y\nx\nx\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "x\n", "y\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.additions, 1);
        assert_eq!(file.deletions, 1);
    }

    #[test]
    fn write_create_synthesizes_all_added_hunk() {
        // The single biggest trap in the format: structuredPatch is empty on
        // create, so the new content must come from `content` alone.
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("new_file.txt").to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.write_create(&abs, "hello\nworld\n").build();
        // The builder only records the transcript; simulate the real file the
        // create actually produced on disk.
        std::fs::write(&abs, "hello\nworld\n").unwrap();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert_eq!(review.steps.len(), 1);
        let step = &review.steps[0];
        assert_eq!(step.kind, StepKind::Create);
        assert!(step.patch.contains("+hello"));
        assert!(step.patch.contains("+world"));
        assert_eq!(step.additions, 2);
        assert_eq!(step.deletions, 0);
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.net_change, NetChange::Added);
        assert_eq!(file.base_source, BaseSource::CreatedInSession);
    }

    #[test]
    fn absent_at_start_delta_parses_without_affecting_base_resolution() {
        // A `file-history-delta` with `backupFileName: null` is real transcript
        // shape (the file didn't exist yet) — base resolution doesn't depend
        // on it (the first edit's own StepKind::Create is authoritative), but
        // the record must still parse cleanly and not raise a warning.
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("new_file2.txt").to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).absent_at_start(&abs);
        let (cfg, transcript) = tb.write_create(&abs, "content\n").build();
        std::fs::write(&abs, "content\n").unwrap();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert!(review.warnings.is_empty());
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.base_source, BaseSource::CreatedInSession);
    }

    #[test]
    fn write_update_overwrites_whole_file() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "brand new content\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.write_update(&abs, "brand new content\n").build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert_eq!(review.steps[0].kind, StepKind::Overwrite);
    }

    #[test]
    fn string_tool_use_result_is_skipped_not_fatal() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "line1\nline2\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).bash_result("some shell output\n");
        let (cfg, transcript) = tb.edit(&abs, "line1\n", "line1-changed\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert_eq!(review.steps.len(), 1);
        assert!(review.warnings.is_empty());
    }

    #[test]
    fn malformed_json_line_does_not_abort_scan() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "line1\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy())
            .raw_line("{\"type\":\"user\",\"toolUseResult\":{ this is not valid json");
        let (cfg, transcript) = tb.edit(&abs, "line1\n", "line1-changed\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        assert_eq!(review.steps.len(), 1);
        assert!(!review.warnings.is_empty());
    }

    #[test]
    fn non_edit_records_are_pre_filtered() {
        assert!(!line_is_interesting(r#"{"type":"assistant","message":{}}"#));
        assert!(!line_is_interesting(
            r#"{"type":"attachment","attachment":{}}"#
        ));
        assert!(line_is_interesting(r#"{"type":"user","toolUseResult":{}}"#));
        assert!(line_is_interesting(r#"{"type":"file-history-delta"}"#));
    }

    #[test]
    fn steps_from_subagent_are_flagged_and_named() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("sub.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "orig\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).subagent_edit(
            "worker1",
            &abs,
            "orig\n",
            "changed\n",
        );
        let (cfg, transcript) = tb.build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let project_dir = transcript.parent().unwrap();
        let subs = subagent_transcripts(project_dir, session_id);
        assert_eq!(subs.len(), 1);
        let review = build_session_review(
            &repo,
            &transcript,
            &subs,
            session_id,
            Some(&cfg.path().to_string_lossy()),
        )
        .unwrap();
        assert_eq!(review.steps.len(), 1);
        assert!(review.steps[0].is_sidechain);
        assert_eq!(review.steps[0].agent_name.as_deref(), Some("worker1"));
        assert!(review.included_subagents);
    }

    #[test]
    fn include_subagents_false_omits_them() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("sub2.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "orig\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).subagent_edit(
            "worker1",
            &abs,
            "orig\n",
            "changed\n",
        );
        let (_cfg, transcript) = tb.build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build_session_review(&repo, &transcript, &[], session_id, None).unwrap();
        assert_eq!(review.steps.len(), 0);
        assert!(!review.included_subagents);
    }

    // ── Base resolution ──────────────────────────────────────────────────

    #[test]
    fn base_from_v1_backup_is_byte_exact() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2\na3\na2-changed\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).backup(&abs, "a1\na2\na3\n");
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2-changed\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.base_source, BaseSource::Backup);
        assert_eq!(file.net_change, NetChange::Modified);
        assert!(file.backup_available);
    }

    #[test]
    fn missing_backup_falls_back_to_original_file() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2-changed\na3\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb
            .edit_with_original(&abs, "a2\n", "a2-changed\n", "a1\na2\na3\n")
            .build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.base_source, BaseSource::ToolResult);
    }

    #[test]
    fn no_backup_no_original_file_reverse_folds_from_disk() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2-changed\na3\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2-changed\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.base_source, BaseSource::Reconstructed);
        assert!(!file.drifted_from_disk);
    }

    #[test]
    fn net_zero_edits_report_unchanged() {
        // Edit a line, then edit it right back — the cumulative diff must be
        // empty and NetChange::Unchanged, even though two real steps happened.
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2\na3\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy())
            .edit(&abs, "a2\n", "a2-tmp\n", false)
            .edit(&abs, "a2-tmp\n", "a2\n", false);
        let (cfg, transcript) = tb.build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.net_change, NetChange::Unchanged);
        assert_eq!(file.cumulative_patch, "");
        assert_eq!(file.step_indices.len(), 2);
    }

    #[test]
    fn drift_detected_when_file_hand_edited_after_session() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        // Disk content reflects the session's edit PLUS an extra hand edit.
        std::fs::write(&abs, "a1\na2-changed\na3\nextra hand-edited line\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy()).backup(&abs, "a1\na2\na3\n");
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2-changed\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert!(file.drifted_from_disk);
    }

    #[test]
    fn deleted_file_reports_net_change_deleted() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("gone.txt").to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.write_create(&abs, "temp content\n").build();
        // Simulate the file the create produced, then its later deletion
        // (e.g. by a Bash `rm` the transcript doesn't otherwise record).
        std::fs::write(&abs, "temp content\n").unwrap();
        std::fs::remove_file(&abs).unwrap();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert_eq!(file.net_change, NetChange::Deleted);
    }

    #[test]
    fn binary_content_is_flagged_not_diffed() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("bin.dat").to_string_lossy().to_string();
        std::fs::write(&abs, [0u8, 1, 2, 3, 0, 4]).unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.write_create(&abs, "irrelevant text content").build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert!(file.is_binary);
    }

    // ── Path classification ──────────────────────────────────────────────

    #[test]
    fn in_repo_file_gets_rel_path() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2\na3\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2b\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert!(file.in_repo);
        assert_eq!(file.rel_path.as_deref(), Some("a.txt"));
    }

    #[test]
    fn outside_repo_file_has_no_rel_path() {
        let (_dir, repo) = fixture_repo();
        let outside_dir = tempfile::tempdir().unwrap();
        let abs = outside_dir
            .path()
            .join("notes.md")
            .to_string_lossy()
            .to_string();
        std::fs::write(&abs, "note v1\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "note v1\n", "note v2\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();
        let review = build(
            &repo,
            &transcript,
            session_id,
            &cfg.path().to_string_lossy(),
        );
        let file = review.files.iter().find(|f| f.abs_path == abs).unwrap();
        assert!(!file.in_repo);
        assert!(file.rel_path.is_none());
        assert_eq!(file.display_path, abs);
    }

    // ── Session listing ──────────────────────────────────────────────────

    #[tokio::test]
    async fn lists_sessions_newest_first_and_honors_config_dir_override() {
        let (_dir, repo) = fixture_repo();
        let cwd = repo.to_string_lossy().to_string();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\n").unwrap();

        let tb1 = TranscriptBuilder::new(&cwd).custom_title("First session");
        let (cfg1, _t1) = tb1.edit(&abs, "a1\n", "a1b\n", false).build();
        let cfg_path = cfg1.path().to_string_lossy().to_string();

        let sessions = list_review_sessions(cwd, None, Some(true), Some(cfg_path))
            .await
            .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title.as_deref(), Some("First session"));
        assert_eq!(sessions[0].edit_count, Some(1));
        assert_eq!(sessions[0].file_count, Some(1));
    }

    #[tokio::test]
    async fn include_counts_false_leaves_counts_none() {
        let (_dir, repo) = fixture_repo();
        let cwd = repo.to_string_lossy().to_string();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\n").unwrap();
        let tb = TranscriptBuilder::new(&cwd);
        let (cfg, _t) = tb.edit(&abs, "a1\n", "a1b\n", false).build();
        let sessions = list_review_sessions(
            cwd,
            None,
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].edit_count, None);
        assert_eq!(sessions[0].file_count, None);
    }

    #[tokio::test]
    async fn missing_project_dir_returns_empty_not_error() {
        let cwd = "/tmp/definitely-not-a-real-project-dir-for-this-test".to_string();
        let cfg = tempfile::tempdir().unwrap();
        let sessions = list_review_sessions(
            cwd,
            None,
            None,
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(sessions.is_empty());
    }

    // ── Cache ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cache_hit_returns_identical_review_for_unchanged_transcript() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2\n").unwrap();
        let tb = TranscriptBuilder::new(&repo.to_string_lossy());
        let (cfg, transcript) = tb.edit(&abs, "a1\n", "a1b\n", false).build();
        let session_id = transcript.file_stem().unwrap().to_str().unwrap();

        let r1 = get_session_review(
            repo.to_string_lossy().to_string(),
            session_id.to_string(),
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        let r2 = get_session_review(
            repo.to_string_lossy().to_string(),
            session_id.to_string(),
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert_eq!(r1.steps.len(), r2.steps.len());
        assert_eq!(r1.steps[0].tool_use_id, r2.steps[0].tool_use_id);
    }

    // ── Revert ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn revert_step_undoes_single_edit_keeping_later_ones() {
        // The two edited lines must sit farther apart than the diff's context
        // window (3 lines either side) — otherwise their hunks legitimately
        // overlap and reverting one always conflicts with the other, which is
        // the *intended* "same region" failure mode, not this test's case.
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("multi.txt").to_string_lossy().to_string();
        let lines: Vec<&str> = vec![
            "alpha", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "l9", "l10", "l11", "gamma",
        ];
        // Disk reflects the state AFTER both edits — the fixture builder only
        // records the transcript, it never touches disk itself.
        let after: Vec<String> = lines
            .iter()
            .map(|l| match *l {
                "alpha" => "ALPHA".to_string(),
                "gamma" => "GAMMA".to_string(),
                other => other.to_string(),
            })
            .collect();
        std::fs::write(&abs, after.join("\n") + "\n").unwrap();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str)
            .edit(&abs, "alpha\n", "ALPHA\n", false)
            .edit(&abs, "gamma\n", "GAMMA\n", false);
        let first_id = tb.tool_use_id(0);
        let (cfg, transcript) = tb.build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let result = revert_session_step(
            repo_str.clone(),
            session_id,
            first_id,
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(result.applied, "revert failed: {:?}", result.message);
        let content = std::fs::read_to_string(&abs).unwrap();
        assert!(
            content.contains("alpha\n"),
            "expected reverted line, got: {content}"
        );
        assert!(
            content.contains("GAMMA\n"),
            "later edit must survive: {content}"
        );
    }

    #[tokio::test]
    async fn revert_step_dry_run_does_not_touch_working_tree() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2\na3\n").unwrap();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str).edit(&abs, "a2\n", "a2-changed\n", false);
        let step_id = tb.last_tool_use_id();
        let (cfg, transcript) = tb.build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let before = std::fs::read_to_string(&abs).unwrap();

        let result = revert_session_step(
            repo_str,
            session_id,
            step_id,
            Some(true),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(!result.applied);
        let after = std::fs::read_to_string(&abs).unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn revert_file_restores_backup_byte_exactly() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        let pristine = "a1\na2\na3\n";
        std::fs::write(&abs, "a1\na2-changed\na3\n").unwrap();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str).backup(&abs, pristine);
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2-changed\n", false).build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let result = revert_file_to_session_start(
            repo_str,
            session_id,
            abs.clone(),
            None,
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(result.applied, "revert failed: {:?}", result.message);
        assert_eq!(result.method, "restore_backup");
        assert_eq!(std::fs::read_to_string(&abs).unwrap(), pristine);
    }

    #[tokio::test]
    async fn revert_file_deletes_file_created_in_session() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("created.txt").to_string_lossy().to_string();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str);
        let (cfg, transcript) = tb.write_create(&abs, "brand new\n").build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let result = revert_file_to_session_start(
            repo_str,
            session_id,
            abs.clone(),
            None,
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.method, "delete_file");
        assert!(!std::path::Path::new(&abs).exists());
    }

    #[tokio::test]
    async fn revert_file_refuses_on_drift_without_force() {
        let (_dir, repo) = fixture_repo();
        let abs = repo.join("a.txt").to_string_lossy().to_string();
        std::fs::write(&abs, "a1\na2-changed\na3\nextra hand-edited line\n").unwrap();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str).backup(&abs, "a1\na2\na3\n");
        let (cfg, transcript) = tb.edit(&abs, "a2\n", "a2-changed\n", false).build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let cfg_path = cfg.path().to_string_lossy().to_string();
        let result = revert_file_to_session_start(
            repo_str.clone(),
            session_id.clone(),
            abs.clone(),
            Some(false),
            Some(false),
            Some(cfg_path.clone()),
        )
        .await
        .unwrap();
        assert!(!result.applied);

        let result_forced = revert_file_to_session_start(
            repo_str,
            session_id,
            abs.clone(),
            Some(true),
            Some(false),
            Some(cfg_path),
        )
        .await
        .unwrap();
        assert!(result_forced.applied);
    }

    #[tokio::test]
    async fn revert_out_of_repo_step_uses_string_substitution() {
        let (_dir, repo) = fixture_repo();
        let outside_dir = tempfile::tempdir().unwrap();
        let abs = outside_dir
            .path()
            .join("plan.md")
            .to_string_lossy()
            .to_string();
        std::fs::write(&abs, "line one\nline two changed\n").unwrap();
        let repo_str = repo.to_string_lossy().to_string();
        let tb = TranscriptBuilder::new(&repo_str);
        let tb = tb.edit(&abs, "line two\n", "line two changed\n", false);
        let step_id = tb.last_tool_use_id();
        let (cfg, transcript) = tb.build();
        let session_id = transcript
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let result = revert_session_step(
            repo_str,
            session_id,
            step_id,
            Some(false),
            Some(cfg.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();
        assert!(result.applied, "revert failed: {:?}", result.message);
        assert_eq!(result.method, "string_substitution");
        assert_eq!(
            std::fs::read_to_string(&abs).unwrap(),
            "line one\nline two\n"
        );
    }
}
