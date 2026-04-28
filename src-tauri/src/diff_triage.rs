use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;

// ---------------------------------------------------------------------------
// Per-repo LLM classification cache — avoids re-sending unchanged diffs
// ---------------------------------------------------------------------------

struct CachedEntry {
    diff_hash: u64,
    classification: FileClassification,
}

type RepoCache = HashMap<String, CachedEntry>;

fn triage_cache() -> &'static Mutex<HashMap<String, RepoCache>> {
    static CACHE: OnceLock<Mutex<HashMap<String, RepoCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hash_diff(diff: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    diff.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileClassification {
    pub path: String,
    pub relevance: Relevance,
    pub category: Category,
    pub risk: Risk,
    pub summary: String,
    pub source: ClassificationSource,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Relevance {
    High = 0,
    Medium = 1,
    Low = 2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    BusinessLogic,
    ApiSurface,
    Schema,
    Config,
    Test,
    Boilerplate,
    Style,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    BreakingChange,
    BehavioralChange,
    Cosmetic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClassificationSource {
    Heuristic,
    Llm,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriageResult {
    pub summary: Option<String>,
    pub files: Vec<FileClassification>,
    pub llm_used: bool,
    pub llm_model: Option<String>,
}

// ---------------------------------------------------------------------------
// Heuristic classification rules
// ---------------------------------------------------------------------------
//
// Files matched here skip the LLM entirely — saves tokens and latency.
// Only classify files that are UNAMBIGUOUSLY a certain category regardless
// of what other files changed. Tests are deliberately NOT here because
// their relevance depends on context (a test for the main feature change
// is medium, not low).
//
// Categories:
//   1. Lock files         → low/boilerplate   (auto-generated dependency manifests)
//   2. Generated code     → low/boilerplate   (protobuf, codegen, etc.)
//   3. CI/CD configs      → low/config        (pipelines, workflows — unless large)
//   4. Documentation      → low/style         (markdown, txt, license)
//   5. Static assets      → low/style         (images, fonts, icons)
//   6. SQL migrations     → HIGH/schema       (always review database changes)
//   7. Minor config edits → low/config        (≤5 lines in known config files)
//   8. Formatting-only    → low/style         (prettier, rustfmt config files)
// ---------------------------------------------------------------------------

// 1. Lock files — auto-generated, never need human review
const LOCK_FILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Gemfile.lock",
    "poetry.lock",
    "go.sum",
    "Pipfile.lock",
    "composer.lock",
    "flake.lock",
    "bun.lockb",
    "shrinkwrap.json",
];

// 7. Config files — low relevance when edits are minor (≤5 lines)
const CONFIG_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "tsconfig.json",
    "tauri.conf.json",
    ".env.example",
    "Makefile",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "biome.json",
    ".eslintrc.json",
    ".eslintrc.js",
    ".eslintrc.yaml",
    ".prettierrc",
    ".prettierrc.json",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".nvmrc",
    ".node-version",
    ".tool-versions",
    "rust-toolchain.toml",
    "renovate.json",
    "dependabot.yml",
    "turbo.json",
    "nx.json",
    "lerna.json",
    "jest.config.ts",
    "jest.config.js",
    "vitest.config.ts",
    "vitest.config.js",
    "babel.config.js",
    "rollup.config.js",
    "vite.config.ts",
    "webpack.config.js",
];

// 3. CI/CD pipeline files — low relevance unless large changes
const CI_PATTERNS: &[&str] = &[
    ".github/workflows/",
    ".github/actions/",
    ".gitlab-ci",
    "Jenkinsfile",
    ".circleci/",
    ".travis.yml",
    "azure-pipelines",
    "bitbucket-pipelines",
    ".buildkite/",
];

// 8. Formatting/linting config — cosmetic by definition
const FORMAT_CONFIG_FILES: &[&str] = &[
    ".prettierrc",
    ".prettierrc.json",
    ".prettierrc.yaml",
    ".prettierignore",
    ".eslintignore",
    "rustfmt.toml",
    ".rustfmt.toml",
    "biome.json",
    ".editorconfig",
    ".clang-format",
    "stylua.toml",
    ".stylelintrc",
    ".stylelintrc.json",
];

// 5. Static assets — binary or non-code, never need LLM
const ASSET_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "avif",
    "woff", "woff2", "ttf", "eot", "otf",
    "mp3", "mp4", "wav", "ogg", "webm",
    "pdf", "zip", "tar", "gz",
];

// 4. Documentation extensions
const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "txt", "rst", "adoc"];

const DOC_FILES: &[&str] = &[
    "LICENSE", "LICENSE.md", "LICENSE.txt",
    "CHANGELOG", "CHANGELOG.md",
    "CONTRIBUTING", "CONTRIBUTING.md",
    "CODE_OF_CONDUCT.md",
    "SECURITY.md",
];

fn make(
    path: &str,
    relevance: Relevance,
    category: Category,
    risk: Risk,
    summary: &str,
) -> FileClassification {
    FileClassification {
        path: path.to_string(),
        relevance,
        category,
        risk,
        summary: summary.to_string(),
        source: ClassificationSource::Heuristic,
        additions: 0,
        deletions: 0,
    }
}

fn is_generated(path: &str, filename: &str) -> bool {
    path.contains("__generated__")
        || path.contains("/generated/")
        || path.contains("/dist/")
        || path.contains("/build/")
        || path.contains("node_modules/")
        || filename.ends_with(".pb.go")
        || filename.ends_with(".pb.rs")
        || filename.ends_with(".g.dart")
        || filename.ends_with(".gen.ts")
        || filename.ends_with(".generated.ts")
        || filename.ends_with(".d.ts")
        || filename.ends_with(".min.js")
        || filename.ends_with(".min.css")
        || filename == "schema.graphql"
}

fn is_migration(path: &str, ext: &str) -> bool {
    if ext != "sql" {
        return false;
    }
    path.contains("/migrations/")
        || path.contains("/migration/")
        || path.starts_with("migrations/")
        || path.starts_with("migration/")
}

fn is_ci_file(path: &str) -> bool {
    CI_PATTERNS.iter().any(|p| path.contains(p))
}

fn is_doc_file(path: &str, filename: &str, ext: &str) -> bool {
    DOC_FILES.iter().any(|&f| filename == f)
        || DOC_EXTENSIONS.contains(&ext)
}

fn is_asset(ext: &str) -> bool {
    ASSET_EXTENSIONS.contains(&ext)
}

/// Classify a file by path/stats alone. Returns `None` if the file needs LLM.
///
/// Design: only intercept files that are UNAMBIGUOUSLY classifiable.
/// Tests are intentionally left for the LLM — a test covering the main
/// feature change is medium, not low. Only the LLM sees the full context.
pub fn heuristic_classify(
    path: &str,
    additions: u32,
    deletions: u32,
) -> Option<FileClassification> {
    let filename = Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ext = ext.as_str();

    // 1. Lock files — always low, never interesting
    if LOCK_FILES.iter().any(|&lf| filename == lf) {
        return Some(make(path, Relevance::Low, Category::Boilerplate, Risk::Cosmetic,
            "Lock file updated"));
    }

    // 2. Generated/vendored code — machine output
    if is_generated(path, filename) {
        return Some(make(path, Relevance::Low, Category::Boilerplate, Risk::Cosmetic,
            "Generated file updated"));
    }

    // 3. CI/CD pipeline config
    if is_ci_file(path) {
        return Some(make(path, Relevance::Low, Category::Config, Risk::Cosmetic,
            "CI/CD pipeline change"));
    }

    // 4. Documentation and legal
    if is_doc_file(path, filename, ext) {
        return Some(make(path, Relevance::Low, Category::Style, Risk::Cosmetic,
            "Documentation updated"));
    }

    // 5. Static assets (images, fonts, media)
    if is_asset(ext) {
        return Some(make(path, Relevance::Low, Category::Style, Risk::Cosmetic,
            "Static asset updated"));
    }

    // 6. SQL migrations — ALWAYS high, schema changes need review
    if is_migration(path, ext) {
        return Some(make(path, Relevance::High, Category::Schema, Risk::BehavioralChange,
            "Database migration"));
    }

    // 7. Config files with minor edits (≤5 lines)
    if CONFIG_FILES.iter().any(|&cf| filename == cf) && additions + deletions <= 5 {
        return Some(make(path, Relevance::Low, Category::Config, Risk::Cosmetic,
            "Minor config change"));
    }

    // 8. Formatting/linting config files
    if FORMAT_CONFIG_FILES.iter().any(|&f| filename == f) {
        return Some(make(path, Relevance::Low, Category::Style, Risk::Cosmetic,
            "Formatting config updated"));
    }

    // Not classifiable by heuristic — send to LLM for context-aware analysis
    None
}

// ---------------------------------------------------------------------------
// LLM classification
// ---------------------------------------------------------------------------

const MAX_LINES_PER_FILE: usize = 300;
const MAX_FILES_TO_LLM: usize = 30;
const LLM_TIMEOUT: Duration = Duration::from_secs(60);

const TRIAGE_SYSTEM_PROMPT: &str = "\
You are a senior code reviewer triaging a changeset for a developer. \
Analyze ALL files together as a coherent changeset — understand how they relate. \
\n\n\
Return JSON with exactly this shape:\n\
{\"summary\": \"2-3 sentence overview of what this changeset does and why it matters\", \
\"files\": [{\"path\": \"...\", \"relevance\": \"high|medium|low\", \
\"category\": \"business-logic|api-surface|schema|config|test|boilerplate|style\", \
\"risk\": \"breaking-change|behavioral-change|cosmetic\", \
\"summary\": \"one sentence explaining THIS file's role in the changeset\"}]}\n\n\
Rules:\n\
- \"summary\" is the changeset overview — what a reviewer needs to know before diving in\n\
- Relevance is about review priority: high=must review carefully, medium=worth a look, low=can skip\n\
- A test file that covers the main change is medium, not low — context matters\n\
- Omit truly trivial files (lock files, formatting-only) from the files array entirely\n\
- Summaries must relate files to each other: \"Adds the handler that X calls\" not just \"Adds handler\"";

pub(crate) fn build_prompt(files: &[(String, String, u32, u32)]) -> String {
    let mut prompt = String::from("Triage this changeset:\n\n");
    for (path, diff_text, additions, deletions) in files {
        prompt.push_str(&format!(
            "<file path=\"{path}\" +{additions} -{deletions}>\n"
        ));
        let truncated: String = diff_text
            .lines()
            .take(MAX_LINES_PER_FILE)
            .collect::<Vec<_>>()
            .join("\n");
        prompt.push_str(&truncated);
        if diff_text.lines().count() > MAX_LINES_PER_FILE {
            prompt.push_str("\n[... truncated]");
        }
        prompt.push_str("\n</file>\n\n");
    }
    prompt
}

#[derive(Deserialize)]
struct LlmTriageResponse {
    summary: Option<String>,
    files: Vec<LlmFileResult>,
}

#[derive(Deserialize)]
struct LlmFileResult {
    path: String,
    relevance: Relevance,
    category: Category,
    risk: Risk,
    summary: String,
}

struct LlmParsed {
    summary: Option<String>,
    files: Vec<FileClassification>,
}

fn parse_llm_response(text: &str) -> Option<LlmParsed> {
    let parsed: LlmTriageResponse = serde_json::from_str(text).ok()?;
    Some(LlmParsed {
        summary: parsed.summary,
        files: parsed
            .files
            .into_iter()
            .map(|f| FileClassification {
                path: f.path,
                relevance: f.relevance,
                category: f.category,
                risk: f.risk,
                summary: f.summary,
                source: ClassificationSource::Llm,
                additions: 0,
                deletions: 0,
            })
            .collect(),
    })
}

fn fallback_classification(path: &str) -> FileClassification {
    FileClassification {
        path: path.to_string(),
        relevance: Relevance::Medium,
        category: Category::BusinessLogic,
        risk: Risk::BehavioralChange,
        summary: String::new(),
        source: ClassificationSource::Heuristic,
        additions: 0,
        deletions: 0,
    }
}

async fn classify_holistic(
    client: &genai::Client,
    model: &str,
    files: &[(String, String, u32, u32)],
) -> LlmParsed {
    use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatResponseFormat};

    let prompt = build_prompt(files);
    let chat_req = ChatRequest::default()
        .with_system(TRIAGE_SYSTEM_PROMPT)
        .append_message(ChatMessage::user(prompt));
    let opts = ChatOptions::default()
        .with_response_format(ChatResponseFormat::JsonMode);

    let result = tokio::time::timeout(
        LLM_TIMEOUT,
        client.exec_chat(model, chat_req, Some(&opts)),
    )
    .await;

    match result {
        Ok(Ok(resp)) => {
            let text = resp.first_text().unwrap_or_default();
            parse_llm_response(text.trim()).unwrap_or_else(|| LlmParsed {
                summary: None,
                files: files
                    .iter()
                    .map(|(path, _, _, _)| fallback_classification(path))
                    .collect(),
            })
        }
        _ => LlmParsed {
            summary: None,
            files: files
                .iter()
                .map(|(path, _, _, _)| fallback_classification(path))
                .collect(),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
struct TriageProgress {
    repo_path: String,
    summary: Option<String>,
    files: Vec<FileClassification>,
    phase: &'static str,
    done: bool,
    llm_used: bool,
    llm_model: Option<String>,
}

fn emit_progress(
    app: &tauri::AppHandle,
    repo_path: &str,
    summary: Option<&str>,
    files: &[FileClassification],
    phase: &'static str,
    done: bool,
    llm_used: bool,
    llm_model: Option<&str>,
) {
    let _ = app.emit(
        "triage-progress",
        TriageProgress {
            repo_path: repo_path.to_string(),
            summary: summary.map(String::from),
            files: files.to_vec(),
            phase,
            done,
            llm_used,
            llm_model: llm_model.map(String::from),
        },
    );
}

#[tauri::command]
pub(crate) async fn run_diff_triage(
    app: tauri::AppHandle,
    repo_path: String,
) -> Result<TriageResult, String> {
    let changed_files = crate::git::get_changed_files(repo_path.clone(), None).await?;
    if changed_files.is_empty() {
        if let Ok(mut cache) = triage_cache().lock() {
            cache.remove(&repo_path);
        }
        emit_progress(&app, &repo_path, None, &[], "done", true, false, None);
        return Ok(TriageResult {
            summary: None,
            files: vec![],
            llm_used: false,
            llm_model: None,
        });
    }

    let mut heuristic: Vec<FileClassification> = Vec::new();
    let mut needs_llm: Vec<(String, u32, u32, bool)> = Vec::new();

    for f in &changed_files {
        if let Some(c) = heuristic_classify(&f.path, f.additions, f.deletions) {
            heuristic.push(c);
        } else {
            let is_untracked = f.status == "?";
            needs_llm.push((f.path.clone(), f.additions, f.deletions, is_untracked));
        }
    }

    let stats: HashMap<&str, (u32, u32)> = changed_files
        .iter()
        .map(|f| (f.path.as_str(), (f.additions, f.deletions)))
        .collect();
    for c in &mut heuristic {
        if let Some(&(a, d)) = stats.get(c.path.as_str()) {
            c.additions = a;
            c.deletions = d;
        }
    }

    // Emit heuristic results immediately so UI is responsive
    if !heuristic.is_empty() {
        emit_progress(
            &app, &repo_path, None, &heuristic,
            if needs_llm.is_empty() { "done" } else { "heuristic" },
            needs_llm.is_empty(), false, None,
        );
    }

    if needs_llm.is_empty() {
        heuristic.sort_by(|a, b| a.relevance.cmp(&b.relevance));
        return Ok(TriageResult {
            summary: None,
            files: heuristic,
            llm_used: false,
            llm_model: None,
        });
    }

    // Fetch all diffs, check cache for individual files
    let llm_candidates: Vec<_> = needs_llm.iter().take(MAX_FILES_TO_LLM).collect();
    let mut cache_hits: Vec<FileClassification> = Vec::new();
    let mut uncached: Vec<(String, String, u32, u32)> = Vec::new();

    for (path, additions, deletions, is_untracked) in &llm_candidates {
        let diff_text = crate::git::get_file_diff(
            repo_path.clone(),
            path.clone(),
            None,
            Some(*is_untracked),
        )
        .await
        .unwrap_or_default();

        let h = hash_diff(&diff_text);
        let hit = triage_cache()
            .lock()
            .ok()
            .and_then(|cache| {
                cache
                    .get(&repo_path)
                    .and_then(|rc| rc.get(path))
                    .filter(|e| e.diff_hash == h)
                    .map(|e| {
                        let mut c = e.classification.clone();
                        c.additions = *additions;
                        c.deletions = *deletions;
                        c
                    })
            });

        if let Some(cached) = hit {
            cache_hits.push(cached);
        } else {
            uncached.push((path.clone(), diff_text, *additions, *deletions));
        }
    }

    if !cache_hits.is_empty() {
        emit_progress(
            &app, &repo_path, None, &cache_hits,
            if uncached.is_empty() { "done" } else { "cached" },
            uncached.is_empty(), true, None,
        );
    }

    let had_cache_hits = !cache_hits.is_empty();
    let mut all_classified = heuristic;
    all_classified.extend(cache_hits);

    let mut llm_used = false;
    let mut llm_model: Option<String> = None;
    let mut changeset_summary: Option<String> = None;

    if !uncached.is_empty() {
        let registry = crate::provider_registry::load_registry();
        match crate::provider_registry::resolve_slot(
            &registry,
            crate::provider_registry::SlotName::Enrichment,
        ) {
            Ok(resolved) => {
                let client =
                    crate::llm_api::build_client(&resolved.config, &resolved.api_key);
                let model_name = resolved.config.model.clone();

                let diff_hashes: HashMap<String, u64> = uncached
                    .iter()
                    .map(|(p, d, _, _)| (p.clone(), hash_diff(d)))
                    .collect();

                // Single holistic LLM call — all files together for context
                let parsed = classify_holistic(&client, &model_name, &uncached).await;
                changeset_summary = parsed.summary;

                // Store results in cache
                if let Ok(mut cache) = triage_cache().lock() {
                    let rc = cache.entry(repo_path.clone()).or_default();
                    for r in &parsed.files {
                        if let Some(&h) = diff_hashes.get(&r.path) {
                            rc.insert(
                                r.path.clone(),
                                CachedEntry {
                                    diff_hash: h,
                                    classification: r.clone(),
                                },
                            );
                        }
                    }
                }

                emit_progress(
                    &app, &repo_path, changeset_summary.as_deref(),
                    &parsed.files, "done", true, true, Some(&model_name),
                );
                all_classified.extend(parsed.files);

                llm_used = true;
                llm_model = Some(model_name);
            }
            Err(_) => {
                let mut fallbacks = Vec::new();
                for (path, _, additions, deletions) in &uncached {
                    let mut fc = fallback_classification(path);
                    fc.additions = *additions;
                    fc.deletions = *deletions;
                    fallbacks.push(fc);
                }
                emit_progress(&app, &repo_path, None, &fallbacks, "done", true, false, None);
                all_classified.extend(fallbacks);
            }
        }
    }

    for (path, _, _, _) in needs_llm.iter().skip(MAX_FILES_TO_LLM) {
        all_classified.push(fallback_classification(path));
    }

    for c in &mut all_classified {
        if let Some(&(a, d)) = stats.get(c.path.as_str()) {
            c.additions = a;
            c.deletions = d;
        }
    }

    if let Ok(mut cache) = triage_cache().lock() {
        if let Some(rc) = cache.get_mut(&repo_path) {
            let current: std::collections::HashSet<&str> =
                changed_files.iter().map(|f| f.path.as_str()).collect();
            rc.retain(|k, _| current.contains(k.as_str()));
        }
    }

    all_classified.sort_by(|a, b| a.relevance.cmp(&b.relevance));
    Ok(TriageResult {
        summary: changeset_summary,
        files: all_classified,
        llm_used: llm_used || had_cache_hits,
        llm_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(path: &str) -> Option<FileClassification> {
        heuristic_classify(path, 10, 5)
    }

    #[test]
    fn lock_files_are_boilerplate() {
        for path in &[
            "Cargo.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "some/nested/Cargo.lock",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Boilerplate, "{path}");
            assert_eq!(c.risk, Risk::Cosmetic, "{path}");
        }
    }

    #[test]
    fn test_files_go_to_llm() {
        for path in &[
            "src/__tests__/foo.test.ts",
            "src/components/Terminal.test.tsx",
            "src-tauri/src/pty_test.rs",
            "tests/integration_test.rs",
            "spec/models/user_spec.rb",
        ] {
            assert!(classify(path).is_none(), "{path} should go to LLM for context-aware classification");
        }
    }

    #[test]
    fn ci_files_are_low_config() {
        for path in &[
            ".github/workflows/ci.yml",
            ".github/actions/setup/action.yml",
            ".circleci/config.yml",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Config, "{path}");
        }
    }

    #[test]
    fn doc_files_are_low_style() {
        for path in &[
            "README.md",
            "docs/architecture.md",
            "CHANGELOG.md",
            "LICENSE",
            "CONTRIBUTING.md",
            "notes.txt",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Style, "{path}");
        }
    }

    #[test]
    fn asset_files_are_low_style() {
        for path in &[
            "src/assets/logo.png",
            "public/favicon.ico",
            "fonts/Inter.woff2",
            "docs/screenshot.jpg",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Style, "{path}");
        }
    }

    #[test]
    fn format_config_files_are_low() {
        for path in &[
            "rustfmt.toml",
            ".prettierrc",
            ".prettierignore",
            ".editorconfig",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Style, "{path}");
        }
    }

    #[test]
    fn migrations_are_high_relevance() {
        for path in &[
            "migrations/001_create_users.sql",
            "db/migrations/20260428_add_column.sql",
            "migration/schema.sql",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::High, "{path}");
            assert_eq!(c.category, Category::Schema, "{path}");
            assert_eq!(c.risk, Risk::BehavioralChange, "{path}");
        }
    }

    #[test]
    fn generated_files_are_boilerplate() {
        for path in &[
            "proto/__generated__/api.ts",
            "src/generated/types.ts",
            "api/service.pb.go",
            "src/bindings.pb.rs",
            "lib/models.g.dart",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Boilerplate, "{path}");
        }
    }

    #[test]
    fn small_config_changes_are_low_relevance() {
        let c = heuristic_classify("Cargo.toml", 2, 1).expect("should classify");
        assert_eq!(c.relevance, Relevance::Low);
        assert_eq!(c.category, Category::Config);
        assert_eq!(c.risk, Risk::Cosmetic);

        let c = heuristic_classify("package.json", 1, 1).expect("should classify");
        assert_eq!(c.category, Category::Config);
    }

    #[test]
    fn large_config_changes_need_llm() {
        assert!(
            heuristic_classify("Cargo.toml", 20, 10).is_none(),
            "large config change should need LLM"
        );
        assert!(
            heuristic_classify("package.json", 50, 0).is_none(),
            "large package.json change should need LLM"
        );
    }

    #[test]
    fn source_files_go_to_llm() {
        assert!(classify("src/main.rs").is_none());
        assert!(classify("src/components/App.tsx").is_none());
        assert!(classify("src-tauri/src/git.rs").is_none());
        assert!(classify("lib/utils/parser.go").is_none());
    }

    #[test]
    fn all_heuristic_results_have_heuristic_source() {
        let paths = &[
            "Cargo.lock",
            "migrations/001.sql",
            "proto/__generated__/api.ts",
            "README.md",
            ".github/workflows/ci.yml",
        ];
        for path in paths {
            let c = classify(path).unwrap();
            assert_eq!(c.source, ClassificationSource::Heuristic, "{path}");
        }
    }

    #[test]
    fn non_sql_migrations_need_llm() {
        assert!(
            classify("migrations/001_create_users.py").is_none(),
            "non-SQL migration should need LLM"
        );
    }

    #[test]
    fn path_is_preserved_in_classification() {
        let c = classify("deep/nested/path/Cargo.lock").unwrap();
        assert_eq!(c.path, "deep/nested/path/Cargo.lock");
    }

    #[test]
    fn build_prompt_xml_format() {
        let files = vec![(
            "src/main.rs".to_string(),
            "+fn hello() {}\n-fn old() {}".to_string(),
            1u32,
            1u32,
        )];
        let prompt = build_prompt(&files);
        assert!(prompt.contains("<file path=\"src/main.rs\" +1 -1>"));
        assert!(prompt.contains("</file>"));
        assert!(prompt.contains("+fn hello()"));
    }

    #[test]
    fn build_prompt_truncates_long_diffs() {
        let long_diff = (0..500).map(|i| format!("+line {i}")).collect::<Vec<_>>().join("\n");
        let files = vec![("big.rs".to_string(), long_diff, 500, 0)];
        let prompt = build_prompt(&files);
        assert!(prompt.contains("[... truncated]"));
        let line_count = prompt.lines().filter(|l| l.starts_with("+line")).count();
        assert_eq!(line_count, MAX_LINES_PER_FILE);
    }

    #[test]
    fn parse_llm_response_valid() {
        let json = r#"{"summary": "Refactored config API", "files": [
            {"path": "src/config.rs", "relevance": "high", "category": "api-surface",
             "risk": "breaking-change", "summary": "Changed public API"}
        ]}"#;
        let parsed = parse_llm_response(json).unwrap();
        assert_eq!(parsed.summary.as_deref(), Some("Refactored config API"));
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].path, "src/config.rs");
        assert_eq!(parsed.files[0].relevance, Relevance::High);
        assert_eq!(parsed.files[0].category, Category::ApiSurface);
        assert_eq!(parsed.files[0].risk, Risk::BreakingChange);
        assert_eq!(parsed.files[0].source, ClassificationSource::Llm);
    }

    #[test]
    fn parse_llm_response_without_summary() {
        let json = r#"{"files": [
            {"path": "a.rs", "relevance": "low", "category": "style",
             "risk": "cosmetic", "summary": "Formatting"}
        ]}"#;
        let parsed = parse_llm_response(json).unwrap();
        assert!(parsed.summary.is_none());
        assert_eq!(parsed.files.len(), 1);
    }

    #[test]
    fn parse_llm_response_malformed_returns_none() {
        assert!(parse_llm_response("not json").is_none());
        assert!(parse_llm_response("{\"files\": \"bad\"}").is_none());
    }

    #[test]
    fn fallback_classification_is_medium() {
        let c = fallback_classification("unknown.rs");
        assert_eq!(c.relevance, Relevance::Medium);
        assert_eq!(c.category, Category::BusinessLogic);
        assert!(c.summary.is_empty());
    }

    #[test]
    fn serialization_roundtrip() {
        let c = classify("Cargo.lock").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"relevance\":\"low\""));
        assert!(json.contains("\"category\":\"boilerplate\""));
        assert!(json.contains("\"risk\":\"cosmetic\""));
        assert!(json.contains("\"source\":\"heuristic\""));

        let c = classify("migrations/001.sql").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"risk\":\"behavioral-change\""));
        assert!(json.contains("\"category\":\"schema\""));
    }
}
