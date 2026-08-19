//! Cross-platform session file discovery for AI coding agents.
//!
//! When a user launches an agent manually (without TUICommander's `--session-id`
//! injection), the session ID is unknown. This module scans known session storage
//! directories to discover the most recently created, unclaimed session file.
//!
//! Supported agents and their storage layouts:
//!
//! | Agent  | Path                                        | ID format         |
//! |--------|---------------------------------------------|-------------------|
//! | claude | `~/.claude/projects/<cwd-slug>/<UUID>.jsonl`| UUID filename stem|
//! | gemini | `~/.gemini/tmp/<hash>/chats/session-*.json` | JSON `sessionId` field |
//! | codex  | `~/.codex/sessions/YYYY/MM/DD/rollout-*-<UUID>.jsonl` | UUID in filename |
//! | goose  | SQLite `~/Library/Application Support/Block/goose/sessions/sessions.db` | name field (TUIC_SESSION) |
//! | fx     | `~/.fx/sessions/<id>/session.json`        | directory name    |

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Session discovery ignores session files/dirs older than this. Discovery
/// fires on every idle↔busy transition (~every agent turn), not just the 30s
/// poll — so without a bound its cost grows with the user's entire session
/// history × open terminals. 300s (5 min) matches the Claude/Grok bound and is
/// comfortably longer than the gap between an agent starting and writing its
/// first session file, so a just-launched session is never aged out.
const SESSION_MAX_AGE: Duration = Duration::from_secs(300);

/// Scan a directory for agent session files and return the ID of the newest
/// unclaimed session, or `None` if none can be found.
///
/// # Parameters
/// - `agent_type`: one of `"claude"`, `"gemini"`, `"codex"`, `"goose"`, `"grok"`, `"fx"`
/// - `cwd`: the terminal's working directory (used to compute project-scoped paths)
/// - `claimed_ids`: session IDs already assigned to other terminals — excluded from results
/// - `agent_pid`: PID of the running agent process. When provided, env vars that affect
///   session storage paths (`CLAUDE_CONFIG_DIR`, `GEMINI_CLI_HOME`, `CODEX_HOME`, `HOME`) are read
///   directly from the process's initial environment — the ground-truth source.
/// - `env_overrides`: fallback env overrides from the TUIC run config. Only used for keys
///   NOT found in the process env (i.e. process env takes precedence).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn discover_agent_session(
    agent_type: String,
    cwd: String,
    claimed_ids: Vec<String>,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> Option<String> {
    let env = resolve_env_overrides(&agent_type, agent_pid, &env_overrides);
    match agent_type.as_str() {
        "claude" => discover_claude_session(
            &cwd,
            &claimed_ids,
            env.get("CLAUDE_CONFIG_DIR").map(|s| s.as_str()),
        ),
        "gemini" => discover_gemini_session(
            &cwd,
            &claimed_ids,
            env.get("GEMINI_CLI_HOME").map(|s| s.as_str()),
        ),
        "codex" => discover_codex_session(
            &cwd,
            &claimed_ids,
            env.get("CODEX_HOME").map(|s| s.as_str()),
        ),
        // Goose stores sessions in SQLite — no filesystem discovery.
        // Shell wrapper injects --name $TUIC_SESSION for deterministic binding.
        "goose" => None,
        "grok" => discover_grok_session(&cwd, &claimed_ids),
        "fx" => discover_fx_session(&cwd, &claimed_ids, env.get("HOME").map(String::as_str)),
        _ => None,
    }
}

/// Merge env overrides: process env (ground truth) takes precedence over run config fallback.
fn resolve_env_overrides(
    agent_type: &str,
    agent_pid: Option<u32>,
    run_config_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = run_config_env.clone();
    if let Some(pid) = agent_pid {
        let process_env = read_agent_env_overrides(agent_type, pid);
        merged.extend(process_env);
    }
    merged
}

/// Env vars that affect session storage paths, keyed by agent type.
///
/// Agents without entries here have no known env var that changes their
/// session storage path (amp, cursor, droid use cloud or hardcoded paths;
/// aider writes to CWD; goose has no override).
const AGENT_ENV_VARS: &[(&str, &[&str])] = &[
    ("claude", &["CLAUDE_CONFIG_DIR"]),
    ("gemini", &["GEMINI_CLI_HOME"]),
    ("codex", &["CODEX_HOME"]),
    ("opencode", &["OPENCODE_DATA_DIR"]),
    ("fx", &["HOME"]),
];

/// Read session-relevant env vars from a running agent process.
///
/// Uses the process's initial environment (set at exec time) via platform-specific
/// APIs: `KERN_PROCARGS2` on macOS, `/proc/pid/environ` on Linux, PEB on Windows.
///
/// Returns a map suitable for passing to `discover_agent_session`/`verify_agent_session`.
pub(crate) fn read_agent_env_overrides(agent_type: &str, pid: u32) -> HashMap<String, String> {
    let mut overrides = HashMap::new();
    let vars = AGENT_ENV_VARS
        .iter()
        .find(|(t, _)| *t == agent_type)
        .map(|(_, vars)| *vars)
        .unwrap_or(&[]);
    for var in vars {
        if let Some(val) = crate::process_env::read_process_env_var(pid, var) {
            overrides.insert((*var).to_string(), val);
        }
    }
    overrides
}

/// Return the absolute path to Claude Code's project directory for a given CWD.
/// E.g. `/Users/foo/bar` → `~/.claude/projects/-Users-foo-bar`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn claude_project_dir(
    cwd: String,
    claude_config_dir: Option<String>,
) -> Result<String, String> {
    let base = claude_projects_dir(claude_config_dir.as_deref())
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    let path = base.join(path_to_claude_slug(&cwd));
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Project path contains non-UTF-8 characters".to_string())
}

// ─── Claude ──────────────────────────────────────────────────────────────────

/// Base directory for Claude Code session transcripts.
///
/// When `config_dir_override` is set (from `CLAUDE_CONFIG_DIR` in the agent's
/// run config), uses `<override>/projects/`. Otherwise defaults to `~/.claude/projects/`.
fn claude_projects_dir(config_dir_override: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = config_dir_override {
        Some(PathBuf::from(dir).join("projects"))
    } else {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }
}

/// Encode a filesystem path to the slug Claude Code uses as a directory name.
///
/// Claude encodes a path by replacing `/`, `.`, and `_` with `-`,
/// prepending a leading `-` to represent the root.
///
/// Example: `/Users/foo.bar/my_project` → `-Users-foo-bar-my-project`
fn path_to_claude_slug(path: &str) -> String {
    // Normalise separators so this works on Windows too
    let normalised = path.replace('\\', "/");
    // Strip trailing separator to avoid a trailing dash in the slug
    let trimmed = normalised.trim_end_matches('/');
    // Replace `/`, `.`, and `_` — Claude treats all three as slug delimiters
    trimmed.replace(['/', '.', '_'], "-")
}

/// Find the most recently created, unclaimed `.jsonl` session file under
/// `~/.claude/projects/<cwd-slug>/`.
fn discover_claude_session(
    cwd: &str,
    claimed_ids: &[String],
    config_dir: Option<&str>,
) -> Option<String> {
    let slug = path_to_claude_slug(cwd);
    let project_dir = claude_projects_dir(config_dir)?.join(&slug);

    newest_unclaimed_file(
        &project_dir,
        |name| {
            // Filename must be `<UUID>.jsonl`
            name.strip_suffix(".jsonl")
                .filter(|stem| is_uuid(stem))
                .map(|stem| stem.to_string())
        },
        claimed_ids,
        Some(SESSION_MAX_AGE),
    )
}

// ─── Gemini ──────────────────────────────────────────────────────────────────

/// Base directory for Gemini session temp files.
///
/// When `cli_home` is set (from `GEMINI_CLI_HOME`), uses `<cli_home>/.gemini/tmp/`.
/// Otherwise defaults to `~/.gemini/tmp/`.
fn gemini_tmp_dir(cli_home: Option<&str>) -> Option<PathBuf> {
    if let Some(home) = cli_home {
        Some(PathBuf::from(home).join(".gemini").join("tmp"))
    } else {
        dirs::home_dir().map(|h| h.join(".gemini").join("tmp"))
    }
}

/// Gemini CLI stores sessions under `~/.gemini/tmp/<project-hash>/chats/`.
/// The hash is a SHA-256 of the absolute project path. Rather than recomputing
/// the hash (which would require adding sha2 as a dependency), we scan ALL
/// project directories under `~/.gemini/tmp/` and look for the newest session
/// file across all of them. This is correct because Gemini is project-scoped:
/// a session in a different project dir won't be in a directory we visit.
///
/// When `cli_home` is set (from `GEMINI_CLI_HOME` in the agent's process env),
/// uses `<cli_home>/.gemini/tmp/`. Otherwise defaults to `~/.gemini/tmp/`.
fn discover_gemini_session(
    _cwd: &str,
    claimed_ids: &[String],
    cli_home: Option<&str>,
) -> Option<String> {
    let tmp_dir = gemini_tmp_dir(cli_home)?;
    if !tmp_dir.exists() {
        return None;
    }

    // Collect (mtime, sessionId) from all session-*.json files across all project dirs
    let now = SystemTime::now();
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();

    let Ok(project_entries) = std::fs::read_dir(&tmp_dir) else {
        return None;
    };
    for proj in project_entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        collect_gemini_session_files(&chats_dir, now, &mut candidates);
    }

    // Sort newest first, return first unclaimed
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn collect_gemini_session_files(
    chats_dir: &PathBuf,
    now: SystemTime,
    out: &mut Vec<(SystemTime, String)>,
) {
    let Ok(entries) = std::fs::read_dir(chats_dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("session-") || !name.ends_with(".json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(mtime) = meta.modified() else { continue };
        // Skip stale files by mtime BEFORE reading/parsing their JSON — discovery
        // fires ~every agent turn, so this caps I/O to recently-touched sessions
        // instead of the user's full history.
        if now.duration_since(mtime).unwrap_or_default() > SESSION_MAX_AGE {
            continue;
        }
        // Read the sessionId from the JSON content
        if let Ok(contents) = std::fs::read_to_string(entry.path())
            && let Some(session_id) = extract_json_string_field(&contents, "sessionId")
            && is_uuid(&session_id)
        {
            out.push((mtime, session_id));
        }
    }
}

/// Extract a top-level string field from JSON.
fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()?
        .get(field)?
        .as_str()
        .map(str::to_string)
}

// ─── Codex ───────────────────────────────────────────────────────────────────

/// Base directory for Codex session files.
///
/// When `codex_home` is set (from `CODEX_HOME`), uses `<codex_home>/sessions/`.
/// Otherwise defaults to `~/.codex/sessions/`.
fn codex_sessions_dir(codex_home: Option<&str>) -> Option<PathBuf> {
    if let Some(home) = codex_home {
        Some(PathBuf::from(home).join("sessions"))
    } else {
        dirs::home_dir().map(|h| h.join(".codex").join("sessions"))
    }
}

/// Codex CLI stores sessions under `~/.codex/sessions/YYYY/MM/DD/`.
/// Files are named `rollout-<timestamp>-<UUID>.jsonl`.
///
/// Unlike Claude and Grok, Codex does not partition its sessions by project — every
/// project's rollouts land in the same date tree. So the walk is filtered twice, and
/// both filters are load-bearing:
///
/// - **age**: `SESSION_MAX_AGE`, per file, matching the Claude/Gemini/Grok siblings.
///   The date-dir prune alone bounds the walk to 24-48h, which is not a recency check:
///   a terminal opened now could bind to a session abandoned this morning.
/// - **cwd**: the rollout's own recorded working directory must match this session's.
///   Without it a fresh terminal in project A took the globally-newest unclaimed
///   session and resumed project B's history.
fn discover_codex_session(
    cwd: &str,
    claimed_ids: &[String],
    codex_home: Option<&str>,
) -> Option<String> {
    let sessions_root = codex_sessions_dir(codex_home)?;

    if !sessions_root.exists() {
        return None;
    }

    // Walk only today's/yesterday's YYYY/MM/DD subtrees — a session younger than
    // SESSION_MAX_AGE can only live there. This prunes the entire historical tree
    // by date before walking, so cost stays bounded regardless of how much
    // lifetime history has accumulated.
    let now = SystemTime::now();
    let wanted_cwd = normalize_cwd(cwd);
    let mut candidates: Vec<(SystemTime, String)> = Vec::new();
    for day_dir in codex_recent_day_dirs(&sessions_root, chrono::Local::now()) {
        collect_codex_files(&day_dir, now, &wanted_cwd, &mut candidates);
    }

    // Sort newest first
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

/// Canonical form for comparing two working directories.
///
/// Falls back to the lexical path when the directory cannot be canonicalized (it may
/// have been deleted since the session started) so a missing directory degrades to an
/// exact string match rather than matching everything.
fn normalize_cwd(cwd: &str) -> PathBuf {
    let path = PathBuf::from(cwd);
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Read the working directory Codex recorded in a rollout file.
///
/// The first JSONL record is `{"type":"session_meta","payload":{...,"cwd":"…"}}` —
/// verified against Codex 0.122 and 0.142, which agree on the field's location.
/// Only that first line is read: the payload embeds the full base instructions, so a
/// rollout is easily megabytes, and only the head is needed.
fn read_codex_rollout_cwd(path: &Path) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader, Read};

    let file = std::fs::File::open(path).ok()?;
    // Cap the read: a corrupt file with no newline must not pull an unbounded amount
    // of it into memory on a path that runs every agent turn.
    let mut head = String::new();
    BufReader::new(file.take(256 * 1024))
        .read_line(&mut head)
        .ok()?;

    let meta: serde_json::Value = serde_json::from_str(head.trim_end()).ok()?;
    let cwd = meta.get("payload")?.get("cwd")?.as_str()?;
    Some(normalize_cwd(cwd))
}

fn collect_codex_files(
    dir: &PathBuf,
    now: SystemTime,
    wanted_cwd: &Path,
    out: &mut Vec<(SystemTime, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_codex_files(&path, now, wanted_cwd, out);
        } else if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string())
            && let Some(uuid) = extract_codex_uuid(&name)
            && let Ok(meta) = entry.metadata()
            && let Ok(mtime) = meta.modified()
        {
            // Age first: it is a stat we already have, and it rejects almost
            // everything before any file is opened.
            if now.duration_since(mtime).unwrap_or_default() > SESSION_MAX_AGE {
                continue;
            }
            // A rollout whose cwd cannot be read is REJECTED, not accepted. If Codex
            // ever moves the field, resume-after-restart quietly stops working —
            // annoying but obvious. Accepting instead would silently reinstate
            // cross-project binding, which resumes the wrong history.
            if read_codex_rollout_cwd(&path).as_deref() != Some(wanted_cwd) {
                continue;
            }
            out.push((mtime, uuid));
        }
    }
}

/// Resolve the `<root>/YYYY/MM/DD` session dirs that can hold a session younger
/// than `SESSION_MAX_AGE`. With a 5-min bound only today matters, but yesterday
/// is kept too to stay correct across the midnight boundary and minor
/// clock/timezone skew between TUIC and the Codex writer (Codex partitions by
/// local date). Bounds the walk to ≤2 day-dirs regardless of total history.
fn codex_recent_day_dirs(root: &Path, now: chrono::DateTime<chrono::Local>) -> Vec<PathBuf> {
    use chrono::Datelike;
    let today = now.date_naive();
    let mut dates = vec![today];
    if let Some(yesterday) = today.pred_opt() {
        dates.push(yesterday);
    }
    dates
        .into_iter()
        .map(|d| {
            root.join(format!("{:04}", d.year()))
                .join(format!("{:02}", d.month()))
                .join(format!("{:02}", d.day()))
        })
        .filter(|p| p.is_dir())
        .collect()
}

/// Extract the UUID from a Codex session filename: `rollout-<ts>-<UUID>.jsonl`
/// The UUID is the last `-`-separated segment before `.jsonl`.
fn extract_codex_uuid(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".jsonl")?;
    if !stem.starts_with("rollout-") {
        return None;
    }
    // UUID is 36 chars: 8-4-4-4-12 hex + dashes = 36
    if stem.len() < 37 {
        return None;
    }
    let candidate = &stem[stem.len() - 36..];
    if is_uuid(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

// ─── Session verification ────────────────────────────────────────────────────

/// Check whether a session file exists on disk for the given agent type and UUID.
///
/// Used at restore time to decide if `--resume <uuid>` is safe: if the session
/// file doesn't exist, the resume command would fail.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn verify_agent_session(
    agent_type: String,
    session_id: String,
    cwd: String,
    agent_pid: Option<u32>,
    env_overrides: HashMap<String, String>,
) -> bool {
    let env = resolve_env_overrides(&agent_type, agent_pid, &env_overrides);
    match agent_type.as_str() {
        "claude" => verify_claude_session(
            &session_id,
            &cwd,
            env.get("CLAUDE_CONFIG_DIR").map(|s| s.as_str()),
        ),
        "gemini" => verify_gemini_session(
            &session_id,
            &cwd,
            env.get("GEMINI_CLI_HOME").map(|s| s.as_str()),
        ),
        "codex" => verify_codex_session(&session_id, env.get("CODEX_HOME").map(|s| s.as_str())),
        "goose" => verify_goose_session(),
        "grok" => verify_grok_session(&session_id, &cwd),
        "fx" => verify_fx_session(&session_id, &cwd, env.get("HOME").map(String::as_str)),
        _ => false,
    }
}

/// Check if `~/.claude/projects/<slug>/<uuid>.jsonl` exists.
fn verify_claude_session(session_id: &str, cwd: &str, config_dir: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(project_dir) =
        claude_projects_dir(config_dir).map(|d| d.join(path_to_claude_slug(cwd)))
    else {
        return false;
    };
    project_dir.join(format!("{session_id}.jsonl")).exists()
}

/// Check if any session file under `~/.gemini/tmp/*/chats/` contains this sessionId.
fn verify_gemini_session(session_id: &str, _cwd: &str, cli_home: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(tmp_dir) = gemini_tmp_dir(cli_home) else {
        return false;
    };
    if !tmp_dir.exists() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(&tmp_dir) else {
        return false;
    };
    for proj in entries.filter_map(|e| e.ok()) {
        let chats_dir = proj.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&chats_dir) else {
            continue;
        };
        for f in files.filter_map(|e| e.ok()) {
            if let Ok(contents) = std::fs::read_to_string(f.path())
                && let Some(found_id) = extract_json_string_field(&contents, "sessionId")
                && found_id == session_id
            {
                return true;
            }
        }
    }
    false
}

/// Check if any Codex session file has this UUID in its filename.
fn verify_codex_session(session_id: &str, codex_home: Option<&str>) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(sessions_root) = codex_sessions_dir(codex_home) else {
        return false;
    };
    if !sessions_root.exists() {
        return false;
    }
    codex_session_exists(&sessions_root, session_id)
}

fn codex_session_exists(dir: &std::path::Path, target_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if codex_session_exists(&path, target_id) {
                return true;
            }
        } else if let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string())
            && let Some(uuid) = extract_codex_uuid(&name)
            && uuid == target_id
        {
            return true;
        }
    }
    false
}

// ─── Goose ────────────────────────────────────────────────────────────────────

/// Goose stores sessions in SQLite — we can't query it without a dependency.
/// Optimistic check: return true if the sessions DB file exists, meaning the
/// user has used Goose. The resume command handles missing sessions gracefully.
fn verify_goose_session() -> bool {
    goose_db_path().map(|p| p.exists()).unwrap_or(false)
}

fn goose_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| {
        d.join("Block")
            .join("goose")
            .join("sessions")
            .join("sessions.db")
    })
}

// ─── Grok ─────────────────────────────────────────────────────────────────────

/// Base directory for grok session storage: `~/.grok/sessions/`.
fn grok_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".grok").join("sessions"))
}

/// Encode a filesystem path the way grok names its per-CWD session directory:
/// RFC-3986 percent-encoding of the absolute path, preserving the unreserved set
/// (ALPHA / DIGIT / `-` / `.` / `_` / `~`) and escaping everything else as `%XX`
/// (uppercase hex). E.g. `/Users/foo.bar/proj` → `%2FUsers%2Ffoo.bar%2Fproj`.
fn grok_path_encode(path: &str) -> String {
    let normalised = path.replace('\\', "/");
    let mut out = String::with_capacity(normalised.len());
    for b in normalised.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

/// grok stores sessions under `~/.grok/sessions/<percent-encoded-cwd>/<UUIDv7>/`.
/// Each session is a *directory* named with its UUIDv7 id (usable with
/// `grok --resume <id>`); the newest such directory is the active session.
fn discover_grok_session(cwd: &str, claimed_ids: &[String]) -> Option<String> {
    let dir = grok_sessions_dir()?.join(grok_path_encode(cwd));
    // DEFERRED (2026-06-13) — extractor accepts any UUID-named entry, not only
    // directories. grok only ever creates session *directories*, and
    // verify_grok_session() rejects non-dirs before resume, so a phantom
    // UUID-named file would at worst yield a no-op resume. A real is_dir guard
    // needs the entry kind threaded through newest_unclaimed_file (8 call sites).
    newest_unclaimed_file(
        &dir,
        |name| is_uuid(name).then(|| name.to_string()),
        claimed_ids,
        Some(SESSION_MAX_AGE),
    )
}

/// Check if `~/.grok/sessions/<percent-encoded-cwd>/<session_id>/` exists.
fn verify_grok_session(session_id: &str, cwd: &str) -> bool {
    if !is_uuid(session_id) {
        return false;
    }
    let Some(dir) = grok_sessions_dir().map(|d| d.join(grok_path_encode(cwd))) else {
        return false;
    };
    dir.join(session_id).is_dir()
}

// ─── fx ──────────────────────────────────────────────────────────────────────

/// fx stores every native session in `~/.fx/sessions/<id>/`. `HOME` is the
/// only profile-root override read by fx 0.0.3, so discovery follows the live
/// process environment when available.
fn fx_sessions_dir(home_override: Option<&str>) -> Option<PathBuf> {
    home_override
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .map(|home| home.join(".fx").join("sessions"))
}

/// Match fx's public session-id grammar from `session_layout.zig` before using
/// an on-disk directory name as a resume operand.
fn is_fx_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 255
        && session_id != "."
        && session_id != ".."
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Read the bounded v0.0.3 session manifest. New fx sessions use schema v3 and
/// cap `session.json` at 64 KiB; rejecting larger or incomplete files avoids an
/// unbounded read on a lifecycle path that runs every agent turn.
fn read_fx_session_identity(path: &Path) -> Option<(String, PathBuf)> {
    use std::io::Read;

    const FX_MANIFEST_MAX_BYTES: u64 = 64 * 1024;

    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > FX_MANIFEST_MAX_BYTES {
        return None;
    }
    let mut contents = String::new();
    file.take(FX_MANIFEST_MAX_BYTES + 1)
        .read_to_string(&mut contents)
        .ok()?;
    if contents.len() as u64 > FX_MANIFEST_MAX_BYTES {
        return None;
    }
    let manifest: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let id = manifest.get("id")?.as_str()?;
    let workspace_root = manifest.get("workspace_root")?.as_str()?;
    if !is_fx_session_id(id) {
        return None;
    }
    Some((id.to_string(), normalize_cwd(workspace_root)))
}

fn discover_fx_session(
    cwd: &str,
    claimed_ids: &[String],
    home_override: Option<&str>,
) -> Option<String> {
    let sessions_dir = fx_sessions_dir(home_override)?;
    let wanted_cwd = normalize_cwd(cwd);
    let now = SystemTime::now();
    let mut candidates: Vec<(SystemTime, String)> = std::fs::read_dir(&sessions_dir)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !is_fx_session_id(&id) {
                return None;
            }
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let recency = entry_recency(&entry.path(), &metadata);
            if now.duration_since(recency).unwrap_or_default() > SESSION_MAX_AGE {
                return None;
            }
            let (manifest_id, manifest_cwd) =
                read_fx_session_identity(&entry.path().join("session.json"))?;
            if manifest_id != id || manifest_cwd != wanted_cwd {
                return None;
            }
            Some((recency, id))
        })
        .collect();

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

fn verify_fx_session(session_id: &str, cwd: &str, home_override: Option<&str>) -> bool {
    if !is_fx_session_id(session_id) {
        return false;
    }
    let Some(session_dir) = fx_sessions_dir(home_override).map(|root| root.join(session_id)) else {
        return false;
    };
    if !std::fs::symlink_metadata(&session_dir)
        .ok()
        .is_some_and(|metadata| metadata.is_dir())
    {
        return false;
    }
    read_fx_session_identity(&session_dir.join("session.json"))
        .is_some_and(|(id, workspace)| id == session_id && workspace == normalize_cwd(cwd))
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Return true if `s` matches the UUID format: 8-4-4-4-12 lowercase hex with dashes.
fn is_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    // Positions 8, 13, 18, 23 must be '-'
    if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

/// Recency of a session entry. For a *file* (Claude/Gemini/Codex `.jsonl`),
/// its own mtime. For a session *directory* (grok stores each session as a dir),
/// the newest of the directory's own mtime and its immediate children's — a
/// directory's own mtime only tracks entry creation/removal, not writes into
/// existing files, so an actively-written grok session would otherwise be aged
/// out of discovery by the `max_age` cap once it's 5 min old.
fn entry_recency(path: &Path, meta: &std::fs::Metadata) -> SystemTime {
    let own = meta.modified().ok();
    if !meta.is_dir() {
        return own.unwrap_or(SystemTime::UNIX_EPOCH);
    }
    let newest_child = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|c| c.ok()?.metadata().ok()?.modified().ok())
        .max();
    own.into_iter()
        .chain(newest_child)
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Scan `dir` for files matching `extract_id`, returning the newest unclaimed ID.
///
/// When `max_age` is set, files older than this duration are ignored. This
/// prevents discovering stale session files when an agent restarts in the same
/// terminal before the new session file is created.
fn newest_unclaimed_file<F>(
    dir: &PathBuf,
    extract_id: F,
    claimed_ids: &[String],
    max_age: Option<std::time::Duration>,
) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    if !dir.exists() {
        return None;
    }

    let now = SystemTime::now();

    let mut candidates: Vec<(SystemTime, String)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            let id = extract_id(&name)?;
            let meta = e.metadata().ok()?;
            let mtime = entry_recency(&e.path(), &meta);
            if max_age.is_some_and(|max| now.duration_since(mtime).unwrap_or_default() > max) {
                return None;
            }
            Some((mtime, id))
        })
        .collect();

    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));

    candidates
        .into_iter()
        .find(|(_, id)| !claimed_ids.contains(id))
        .map(|(_, id)| id)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    fn make_file(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"{}").unwrap();
        path
    }

    /// Write a Codex rollout whose first line is the real `session_meta` shape,
    /// which is where discovery reads the recorded working directory from.
    fn make_codex_rollout(dir: &std::path::Path, name: &str, cwd: &str) -> PathBuf {
        let path = dir.join(name);
        let head = serde_json::json!({
            "timestamp": "2026-07-01T08:31:54.793Z",
            "type": "session_meta",
            "payload": { "id": "ignored", "cwd": cwd, "originator": "codex-tui" },
        });
        fs::write(&path, format!("{head}\n{{\"type\":\"event_msg\"}}\n")).unwrap();
        path
    }

    fn make_fx_session(home: &std::path::Path, id: &str, cwd: &str) -> PathBuf {
        let session_dir = home.join(".fx").join("sessions").join(id);
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("session.json"),
            serde_json::json!({
                "schema_version": 3,
                "id": id,
                "workspace_root": cwd,
            })
            .to_string(),
        )
        .unwrap();
        session_dir
    }

    fn backdate_fx_session(session_dir: &std::path::Path, modified: SystemTime) {
        let manifest = session_dir.join("session.json");
        for path in [session_dir, manifest.as_path()] {
            fs::File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(modified))
                .unwrap();
        }
    }

    #[cfg(unix)]
    fn symlink_dir(target: &std::path::Path, link: &std::path::Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[cfg(windows)]
    fn symlink_dir(target: &std::path::Path, link: &std::path::Path) {
        std::os::windows::fs::symlink_dir(target, link).unwrap();
    }

    // ── is_uuid ──

    #[test]
    fn test_is_uuid_valid() {
        assert!(is_uuid("af467730-5e79-49d9-8a17-ebd94c99f262"));
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn test_is_uuid_invalid() {
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("af467730-5e79-49d9-8a17")); // too short
        assert!(!is_uuid("af467730-5e79-49d9-8a17-ebd94c99f262X")); // too long
        assert!(!is_uuid("zf467730-5e79-49d9-8a17-ebd94c99f262")); // non-hex
    }

    // ── grok ──

    #[test]
    fn test_grok_path_encode_matches_on_disk_layout() {
        // Captured live: grok names its per-CWD session dir by percent-encoding
        // the absolute path — '/' → %2F, '.' preserved.
        assert_eq!(
            grok_path_encode("/Users/stefano.straus/Gits/personal/tuicommander"),
            "%2FUsers%2Fstefano.straus%2FGits%2Fpersonal%2Ftuicommander"
        );
        // Unreserved set is preserved; spaces and other reserved chars escape.
        assert_eq!(grok_path_encode("/a b/c-d_e~f"), "%2Fa%20b%2Fc-d_e~f");
    }

    #[test]
    fn test_grok_path_encode_windows_separators() {
        assert_eq!(grok_path_encode(r"C:\Users\foo"), "C%3A%2FUsers%2Ffoo");
    }

    #[test]
    fn test_verify_grok_session_rejects_non_uuid() {
        assert!(!verify_grok_session("not-a-uuid", "/tmp/x"));
    }

    // ── fx ──

    #[test]
    fn test_fx_session_id_matches_upstream_grammar() {
        assert!(is_fx_session_id(
            "1770000000000-1770000000000000000-a1b2c3d4e5f60708"
        ));
        assert!(is_fx_session_id("session.v3"));
        for invalid in [
            "",
            ".",
            "..",
            "../outside",
            "nested/session",
            "nested\\session",
            "session with spaces",
        ] {
            assert!(!is_fx_session_id(invalid), "{invalid} must be rejected");
        }
        assert!(
            !is_fx_session_id(&"a".repeat(256)),
            "too-long IDs must be rejected"
        );
    }

    #[test]
    fn test_discover_fx_session_filters_workspace_and_claimed_ids() {
        let home = TempDir::new().unwrap();
        let wrong = "1770000000000-1770000000000000000-aaaaaaaaaaaaaaaa";
        let wanted = "1770000000001-1770000000000000001-bbbbbbbbbbbbbbbb";
        make_fx_session(home.path(), wrong, "/other/project");
        make_fx_session(home.path(), wanted, "/wanted/project");

        assert_eq!(
            discover_fx_session("/wanted/project", &[], Some(home.path().to_str().unwrap()),),
            Some(wanted.to_string())
        );
        assert!(
            discover_fx_session(
                "/wanted/project",
                &[wanted.to_string()],
                Some(home.path().to_str().unwrap()),
            )
            .is_none()
        );
    }

    #[test]
    fn test_verify_fx_session_checks_manifest_identity_and_workspace() {
        let home = TempDir::new().unwrap();
        let id = "1770000000001-1770000000000000001-bbbbbbbbbbbbbbbb";
        make_fx_session(home.path(), id, "/wanted/project");
        let home_path = home.path().to_str().unwrap();

        assert!(verify_fx_session(id, "/wanted/project", Some(home_path)));
        assert!(!verify_fx_session(id, "/other/project", Some(home_path)));
        assert!(!verify_fx_session(
            "../outside",
            "/wanted/project",
            Some(home_path)
        ));
    }

    #[test]
    fn test_discover_and_verify_fx_session_reject_symlinked_session_dir() {
        let home = TempDir::new().unwrap();
        let sessions = home.path().join(".fx").join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let target = home.path().join("real-session");
        fs::create_dir_all(&target).unwrap();
        fs::write(
            target.join("session.json"),
            serde_json::json!({
                "schema_version": 3,
                "id": "1770000000002-1770000000000000002-cccccccccccccccc",
                "workspace_root": "/wanted/project",
            })
            .to_string(),
        )
        .unwrap();
        let symlink_id = "1770000000003-1770000000000000003-dddddddddddddddd";
        let symlink = sessions.join(symlink_id);
        symlink_dir(&target, &symlink);

        let home_path = home.path().to_str().unwrap();
        assert!(
            discover_fx_session("/wanted/project", &[], Some(home_path)).is_none(),
            "symlinked session directories must not be discovered"
        );
        assert!(
            !verify_fx_session(symlink_id, "/wanted/project", Some(home_path)),
            "symlinked session directories must not be verified"
        );
    }

    #[test]
    fn test_discover_and_verify_fx_session_reject_malformed_or_oversized_manifest() {
        let home = TempDir::new().unwrap();
        let malformed_id = "1770000000004-1770000000000000004-eeeeeeeeeeeeeeee";
        let malformed = make_fx_session(home.path(), malformed_id, "/wanted/project");
        fs::write(malformed.join("session.json"), b"not-json").unwrap();

        let oversized_id = "1770000000005-1770000000000000005-ffffffffffffffff";
        let oversized = make_fx_session(home.path(), oversized_id, "/wanted/project");
        fs::write(oversized.join("session.json"), vec![b'x'; 64 * 1024 + 1]).unwrap();

        let home_path = home.path().to_str().unwrap();
        assert!(
            discover_fx_session("/wanted/project", &[], Some(home_path)).is_none(),
            "malformed and oversized manifests must not be discovered"
        );
        assert!(!verify_fx_session(
            malformed_id,
            "/wanted/project",
            Some(home_path)
        ));
        assert!(!verify_fx_session(
            oversized_id,
            "/wanted/project",
            Some(home_path)
        ));
    }

    #[test]
    fn test_discover_and_verify_fx_session_reject_manifest_directory_id_mismatch() {
        let home = TempDir::new().unwrap();
        let directory_id = "1770000000006-1770000000000000006-1111111111111111";
        let manifest_id = "1770000000007-1770000000000000007-2222222222222222";
        let session_dir = make_fx_session(home.path(), directory_id, "/wanted/project");
        fs::write(
            session_dir.join("session.json"),
            serde_json::json!({
                "schema_version": 3,
                "id": manifest_id,
                "workspace_root": "/wanted/project",
            })
            .to_string(),
        )
        .unwrap();

        let home_path = home.path().to_str().unwrap();
        assert!(discover_fx_session("/wanted/project", &[], Some(home_path)).is_none());
        assert!(!verify_fx_session(
            directory_id,
            "/wanted/project",
            Some(home_path)
        ));
    }

    #[test]
    fn test_discover_fx_session_rejects_stale_entry() {
        let home = TempDir::new().unwrap();
        let id = "1770000000008-1770000000000000008-3333333333333333";
        let session_dir = make_fx_session(home.path(), id, "/wanted/project");
        backdate_fx_session(
            &session_dir,
            SystemTime::now() - Duration::from_secs(SESSION_MAX_AGE.as_secs() + 60),
        );

        assert!(
            discover_fx_session("/wanted/project", &[], Some(home.path().to_str().unwrap()))
                .is_none(),
            "stale fx sessions must not be discovered"
        );
    }

    #[test]
    fn test_discover_fx_session_excludes_claimed_id() {
        let home = TempDir::new().unwrap();
        let id = "1770000000009-1770000000000000009-4444444444444444";
        make_fx_session(home.path(), id, "/wanted/project");

        assert_eq!(
            discover_fx_session(
                "/wanted/project",
                &[id.to_string()],
                Some(home.path().to_str().unwrap())
            ),
            None
        );
    }

    #[test]
    fn test_discover_fx_session_selects_newest_valid_candidate() {
        let home = TempDir::new().unwrap();
        let older_id = "1770000000010-1770000000000000010-5555555555555555";
        let newer_id = "1770000000011-1770000000000000011-6666666666666666";
        let older = make_fx_session(home.path(), older_id, "/wanted/project");
        let newer = make_fx_session(home.path(), newer_id, "/wanted/project");
        let now = SystemTime::now();
        backdate_fx_session(&older, now - Duration::from_secs(120));
        backdate_fx_session(&newer, now - Duration::from_secs(60));

        assert_eq!(
            discover_fx_session("/wanted/project", &[], Some(home.path().to_str().unwrap())),
            Some(newer_id.to_string())
        );
    }

    // ── path_to_claude_slug ──

    #[test]
    fn test_path_to_claude_slug_unix() {
        assert_eq!(path_to_claude_slug("/Users/foo/bar"), "-Users-foo-bar");
    }

    #[test]
    fn test_path_to_claude_slug_dots_in_username() {
        assert_eq!(
            path_to_claude_slug("/Users/stefano.straus/Gits/project"),
            "-Users-stefano-straus-Gits-project"
        );
    }

    #[test]
    fn test_path_to_claude_slug_underscores() {
        assert_eq!(
            path_to_claude_slug("/Users/foo/CC_Playground/my_project"),
            "-Users-foo-CC-Playground-my-project"
        );
    }

    #[test]
    fn test_path_to_claude_slug_hidden_dirs() {
        assert_eq!(
            path_to_claude_slug("/Users/foo/project/.claude-worktrees/feat"),
            "-Users-foo-project--claude-worktrees-feat"
        );
    }

    #[test]
    fn test_path_to_claude_slug_trailing_slash() {
        assert_eq!(path_to_claude_slug("/Users/foo/bar/"), "-Users-foo-bar");
    }

    #[test]
    fn test_path_to_claude_slug_windows() {
        assert_eq!(
            path_to_claude_slug("C:\\Users\\foo\\bar"),
            "C:-Users-foo-bar"
        );
    }

    // ── claude_project_dir ──

    #[test]
    fn test_claude_project_dir_returns_path_with_slug() {
        if let Ok(result) = claude_project_dir("/Users/foo/bar".to_string(), None) {
            assert!(
                result.ends_with("/.claude/projects/-Users-foo-bar"),
                "unexpected path: {result}"
            );
        }
        // None only if home dir unavailable (CI) — not a failure
    }

    #[test]
    fn test_claude_project_dir_dots_in_username() {
        if let Ok(result) = claude_project_dir("/Users/foo.bar/proj".to_string(), None) {
            assert!(
                result.ends_with("-Users-foo-bar-proj"),
                "unexpected path: {result}"
            );
        }
    }

    #[test]
    fn test_claude_project_dir_with_config_dir_override() {
        let dir = TempDir::new().unwrap();
        let override_path = dir.path().to_str().unwrap().to_string();
        let result = claude_project_dir("/Users/foo/bar".to_string(), Some(override_path.clone()));
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(
            path.starts_with(&override_path),
            "expected path to start with override dir, got: {path}"
        );
        assert!(
            path.ends_with("-Users-foo-bar"),
            "expected slug suffix, got: {path}"
        );
    }

    #[test]
    fn test_discover_claude_session_with_config_dir() {
        let dir = TempDir::new().unwrap();
        let projects_dir = dir.path().join("projects");
        let slug = path_to_claude_slug("/fake/project");
        let session_dir = projects_dir.join(&slug);
        fs::create_dir_all(&session_dir).unwrap();

        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(&session_dir, &format!("{uuid}.jsonl"));

        let result =
            discover_claude_session("/fake/project", &[], Some(dir.path().to_str().unwrap()));
        assert_eq!(result, Some(uuid.to_string()));
    }

    #[test]
    fn test_verify_claude_session_with_config_dir() {
        let dir = TempDir::new().unwrap();
        let projects_dir = dir.path().join("projects");
        let slug = path_to_claude_slug("/fake/project");
        let session_dir = projects_dir.join(&slug);
        fs::create_dir_all(&session_dir).unwrap();

        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(&session_dir, &format!("{uuid}.jsonl"));

        assert!(verify_claude_session(
            uuid,
            "/fake/project",
            Some(dir.path().to_str().unwrap())
        ));
        assert!(!verify_claude_session(uuid, "/fake/project", None));
    }

    #[test]
    fn test_newest_unclaimed_file_max_age_filter() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        let path = make_file(dir.path(), &format!("{uuid}.jsonl"));

        // Backdate the file mtime by 2 minutes
        let two_min_ago = std::time::SystemTime::now() - Duration::from_secs(120);
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(two_min_ago))
            .unwrap();

        // With 60s max age, the stale file should be filtered out
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            Some(Duration::from_secs(60)),
        );
        assert!(result.is_none(), "stale file should be filtered by max_age");

        // Without max age, should still find it
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid.to_string()));
    }

    // ── newest_unclaimed_file ──

    #[test]
    fn test_empty_dir_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_missing_dir_returns_none() {
        let result =
            newest_unclaimed_file(&PathBuf::from("/nonexistent/path/xyz"), |_| None, &[], None);
        assert!(result.is_none());
    }

    #[test]
    fn test_single_jsonl_returns_uuid() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(dir.path(), &format!("{uuid}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid.to_string()));
    }

    #[test]
    fn test_claimed_id_is_excluded() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        make_file(dir.path(), &format!("{uuid}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[uuid.to_string()],
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_newest_file_returned_when_multiple() {
        let dir = TempDir::new().unwrap();
        let uuid1 = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let uuid2 = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

        make_file(dir.path(), &format!("{uuid1}.jsonl"));
        // Sleep briefly so mtime differs; on fast filesystems use touch
        std::thread::sleep(Duration::from_millis(10));
        make_file(dir.path(), &format!("{uuid2}.jsonl"));

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert_eq!(result, Some(uuid2.to_string()));
    }

    #[test]
    fn test_non_uuid_filenames_skipped() {
        let dir = TempDir::new().unwrap();
        make_file(dir.path(), "not-a-uuid.jsonl");
        make_file(dir.path(), "some-other-file.txt");

        let result = newest_unclaimed_file(
            &dir.path().to_path_buf(),
            |name| {
                name.strip_suffix(".jsonl")
                    .filter(|s| is_uuid(s))
                    .map(|s| s.to_string())
            },
            &[],
            None,
        );
        assert!(result.is_none());
    }

    // ── gemini discovery mtime bound ──

    #[test]
    fn test_discover_gemini_session_filters_stale_by_mtime() {
        let dir = TempDir::new().unwrap();
        let chats = dir
            .path()
            .join(".gemini")
            .join("tmp")
            .join("projhash")
            .join("chats");
        fs::create_dir_all(&chats).unwrap();
        let cli_home = dir.path().to_str().unwrap();

        let stale_uuid = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let stale = chats.join("session-stale.json");
        fs::write(&stale, format!(r#"{{"sessionId":"{stale_uuid}"}}"#)).unwrap();
        // Backdate well beyond the 300s bound.
        let old = SystemTime::now() - Duration::from_secs(10 * 60);
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();

        // Only a stale file present → filtered out, nothing discovered.
        assert!(
            discover_gemini_session("/whatever", &[], Some(cli_home)).is_none(),
            "stale gemini session must be filtered by mtime"
        );

        // Add a fresh file → it (and only it) is discovered.
        let fresh_uuid = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        fs::write(
            chats.join("session-fresh.json"),
            format!(r#"{{"sessionId":"{fresh_uuid}"}}"#),
        )
        .unwrap();
        assert_eq!(
            discover_gemini_session("/whatever", &[], Some(cli_home)),
            Some(fresh_uuid.to_string()),
            "fresh gemini session should be discovered, stale ignored"
        );
    }

    // ── codex date-subtree pruning ──

    fn codex_day_dir(root: &std::path::Path, y: i32, m: u32, d: u32) -> PathBuf {
        root.join(format!("{y:04}"))
            .join(format!("{m:02}"))
            .join(format!("{d:02}"))
    }

    #[test]
    fn test_codex_recent_day_dirs_prunes_old_subtrees() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let now = chrono::Local::now();
        let today = now.date_naive();
        let yesterday = today.pred_opt().unwrap();

        for d in [
            today,
            yesterday,
            chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        ] {
            fs::create_dir_all(codex_day_dir(root.path(), d.year(), d.month(), d.day())).unwrap();
        }

        let dirs = codex_recent_day_dirs(root.path(), now);
        assert!(dirs.contains(&codex_day_dir(
            root.path(),
            today.year(),
            today.month(),
            today.day()
        )));
        assert!(dirs.contains(&codex_day_dir(
            root.path(),
            yesterday.year(),
            yesterday.month(),
            yesterday.day()
        )));
        assert!(
            !dirs.contains(&codex_day_dir(root.path(), 2020, 1, 1)),
            "stale date subtree must be pruned before walking"
        );
    }

    #[test]
    fn test_discover_codex_session_ignores_old_date_subtree() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();

        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        let old_dir = codex_day_dir(&sessions, 2020, 1, 1);
        fs::create_dir_all(&today_dir).unwrap();
        fs::create_dir_all(&old_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();
        let cwd = project.to_str().unwrap();

        let fresh = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let stale = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{fresh}.jsonl"),
            cwd,
        );
        // Create the stale file LAST so its mtime is newest — if the old subtree
        // weren't pruned it would win the newest-first sort, so returning `fresh`
        // proves the date prune worked.
        std::thread::sleep(Duration::from_millis(10));
        make_codex_rollout(
            &old_dir,
            &format!("rollout-2020-01-01T10-00-00-{stale}.jsonl"),
            cwd,
        );

        let result = discover_codex_session(cwd, &[], Some(root.path().to_str().unwrap()));
        assert_eq!(result, Some(fresh.to_string()));
    }

    /// The date-dir prune bounds the walk to 24-48h, which is NOT a recency check:
    /// a session abandoned this morning still sits in today's dir. Claude, Gemini and
    /// Grok all reject candidates older than SESSION_MAX_AGE per file; Codex did not.
    #[test]
    fn test_discover_codex_session_rejects_a_stale_session_in_todays_dir() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();
        let cwd = project.to_str().unwrap();

        let stale = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        let path = make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{stale}.jsonl"),
            cwd,
        );
        // Same day, but well past the 5-minute bound.
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();

        assert_eq!(
            discover_codex_session(cwd, &[], Some(root.path().to_str().unwrap())),
            None,
            "a session older than SESSION_MAX_AGE must not be resumed"
        );
    }

    /// Codex does not partition sessions by project, so without cwd scoping a fresh
    /// terminal in project A bound to the globally-newest unclaimed session and
    /// resumed project B's history.
    #[test]
    fn test_discover_codex_session_ignores_another_projects_session() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project_a = root.path().join("project-a");
        let project_b = root.path().join("project-b");
        fs::create_dir_all(&project_a).unwrap();
        fs::create_dir_all(&project_b).unwrap();

        let mine = "dddddddd-dddd-dddd-dddd-dddddddddddd";
        let theirs = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{mine}.jsonl"),
            project_a.to_str().unwrap(),
        );
        // Newest, so it wins the sort — only the cwd filter can keep it out.
        std::thread::sleep(Duration::from_millis(10));
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-01-{theirs}.jsonl"),
            project_b.to_str().unwrap(),
        );

        let home = Some(root.path().to_str().unwrap());
        assert_eq!(
            discover_codex_session(project_a.to_str().unwrap(), &[], home),
            Some(mine.to_string()),
            "the other project's newer session must not be selected"
        );
        assert_eq!(
            discover_codex_session(project_b.to_str().unwrap(), &[], home),
            Some(theirs.to_string()),
            "and project B still finds its own"
        );
    }

    /// A rollout whose cwd cannot be read is rejected. Accepting it would silently
    /// reinstate cross-project binding the moment Codex changed its file format.
    #[test]
    fn test_discover_codex_session_rejects_a_rollout_with_no_readable_cwd() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();

        let id = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        // `{}` — a well-formed JSON line with no payload, i.e. a format we don't know.
        make_file(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{id}.jsonl"),
        );

        assert_eq!(
            discover_codex_session(
                project.to_str().unwrap(),
                &[],
                Some(root.path().to_str().unwrap())
            ),
            None
        );
    }

    /// The recorded cwd and the session cwd may spell the same directory differently
    /// (symlinked /tmp on macOS, a trailing slash). Compare canonical paths.
    #[test]
    fn test_discover_codex_session_matches_an_equivalent_cwd_spelling() {
        use chrono::Datelike;
        let root = TempDir::new().unwrap();
        let sessions = root.path().join("sessions");
        let today = chrono::Local::now().date_naive();
        let today_dir = codex_day_dir(&sessions, today.year(), today.month(), today.day());
        fs::create_dir_all(&today_dir).unwrap();

        let project = root.path().join("project-a");
        fs::create_dir_all(&project).unwrap();

        let id = "12345678-1234-1234-1234-123456789abc";
        make_codex_rollout(
            &today_dir,
            &format!("rollout-2026-07-07T10-00-00-{id}.jsonl"),
            project.to_str().unwrap(),
        );

        // Same directory reached through a `.` component.
        let indirect = project.join(".");
        assert_eq!(
            discover_codex_session(
                indirect.to_str().unwrap(),
                &[],
                Some(root.path().to_str().unwrap())
            ),
            Some(id.to_string())
        );
    }

    // ── extract_codex_uuid ──

    #[test]
    fn test_extract_codex_uuid_valid() {
        let name = "rollout-2026-02-03T13-40-28-af467730-5e79-49d9-8a17-ebd94c99f262.jsonl";
        assert_eq!(
            extract_codex_uuid(name),
            Some("af467730-5e79-49d9-8a17-ebd94c99f262".to_string())
        );
    }

    #[test]
    fn test_extract_codex_uuid_wrong_prefix() {
        assert!(
            extract_codex_uuid("session-2026-af467730-5e79-49d9-8a17-ebd94c99f262.jsonl").is_none()
        );
    }

    #[test]
    fn test_extract_codex_uuid_no_suffix() {
        assert!(
            extract_codex_uuid("rollout-2026-af467730-5e79-49d9-8a17-ebd94c99f262.txt").is_none()
        );
    }

    // ── extract_json_string_field ──

    #[test]
    fn test_extract_json_string_field_present() {
        let json = r#"{"sessionId": "af467730-5e79-49d9-8a17-ebd94c99f262", "messages": []}"#;
        assert_eq!(
            extract_json_string_field(json, "sessionId"),
            Some("af467730-5e79-49d9-8a17-ebd94c99f262".to_string())
        );
    }

    #[test]
    fn test_extract_json_string_field_missing() {
        let json = r#"{"messages": []}"#;
        assert!(extract_json_string_field(json, "sessionId").is_none());
    }

    #[test]
    fn test_extract_json_string_field_non_string_value() {
        let json = r#"{"count": 42}"#;
        assert!(extract_json_string_field(json, "count").is_none());
    }

    // ── verify_claude_session ──

    #[test]
    fn test_verify_claude_session_exists() {
        let dir = TempDir::new().unwrap();
        let uuid = "af467730-5e79-49d9-8a17-ebd94c99f262";
        let slug = path_to_claude_slug("/fake/project");
        let project_dir = dir.path().join(&slug);
        fs::create_dir_all(&project_dir).unwrap();
        make_file(&project_dir, &format!("{uuid}.jsonl"));

        // Temporarily override the home dir by checking the file directly
        // (we can't mock dirs::home_dir, so test the inner logic)
        assert!(project_dir.join(format!("{uuid}.jsonl")).exists());
    }

    #[test]
    fn test_verify_claude_session_not_found() {
        let dir = TempDir::new().unwrap();
        let slug = path_to_claude_slug("/fake/project");
        let project_dir = dir.path().join(&slug);
        fs::create_dir_all(&project_dir).unwrap();

        assert!(!project_dir.join("nonexistent-uuid.jsonl").exists());
    }

    #[test]
    fn test_verify_agent_session_invalid_uuid() {
        // Invalid UUIDs should always return false
        assert!(!verify_claude_session("not-a-uuid", "/tmp", None));
    }

    #[test]
    fn test_verify_agent_session_unknown_agent() {
        assert!(!verify_agent_session(
            "unknown-agent".to_string(),
            "af467730-5e79-49d9-8a17-ebd94c99f262".to_string(),
            "/tmp".to_string(),
            None,
            HashMap::new(),
        ));
    }
}
