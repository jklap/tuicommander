use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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
    pub files: Vec<FileClassification>,
    pub llm_used: bool,
    pub llm_model: Option<String>,
}

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
];

const CONFIG_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "tsconfig.json",
    "tauri.conf.json",
    ".env.example",
    "Makefile",
    "Dockerfile",
    "docker-compose.yml",
    "biome.json",
    ".eslintrc.json",
    ".prettierrc",
    ".gitignore",
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

fn is_test_path(path: &str, filename: &str) -> bool {
    path.contains("/__tests__/")
        || path.contains("/test/")
        || path.contains("/tests/")
        || path.contains("/spec/")
        || path.contains("/specs/")
        || filename.ends_with("_test.rs")
        || filename.ends_with("_test.go")
        || filename.ends_with(".test.ts")
        || filename.ends_with(".test.tsx")
        || filename.ends_with(".test.js")
        || filename.ends_with(".test.jsx")
        || filename.ends_with(".spec.ts")
        || filename.ends_with(".spec.tsx")
        || filename.ends_with(".spec.js")
        || filename.ends_with("_spec.rb")
}

fn is_generated(path: &str, filename: &str) -> bool {
    path.contains("__generated__")
        || path.contains("/generated/")
        || filename.ends_with(".pb.go")
        || filename.ends_with(".pb.rs")
        || filename.ends_with(".g.dart")
        || filename.ends_with(".gen.ts")
        || filename.ends_with(".generated.ts")
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
        .unwrap_or("");

    if LOCK_FILES.iter().any(|&lf| filename == lf) {
        return Some(make(
            path,
            Relevance::Low,
            Category::Boilerplate,
            Risk::Cosmetic,
            "Lock file updated",
        ));
    }

    if is_generated(path, filename) {
        return Some(make(
            path,
            Relevance::Low,
            Category::Boilerplate,
            Risk::Cosmetic,
            "Generated file updated",
        ));
    }

    if is_migration(path, ext) {
        return Some(make(
            path,
            Relevance::High,
            Category::Schema,
            Risk::BehavioralChange,
            "Database migration",
        ));
    }

    if is_test_path(path, filename) {
        return Some(make(
            path,
            Relevance::Low,
            Category::Test,
            Risk::BehavioralChange,
            "Test file updated",
        ));
    }

    if CONFIG_FILES.iter().any(|&cf| filename == cf) && additions + deletions <= 5 {
        return Some(make(
            path,
            Relevance::Low,
            Category::Config,
            Risk::Cosmetic,
            "Minor config change",
        ));
    }

    None
}

// ---------------------------------------------------------------------------
// LLM classification
// ---------------------------------------------------------------------------

const MAX_LINES_PER_FILE: usize = 300;
const MAX_FILES_TO_LLM: usize = 20;
const FILES_PER_BATCH: usize = 3;
const LLM_TIMEOUT: Duration = Duration::from_secs(20);

const TRIAGE_SYSTEM_PROMPT: &str = "\
You classify code changes for developer review triage. \
For each file return a JSON object with exactly these fields: \
path (string), relevance (high|medium|low), \
category (business-logic|api-surface|schema|config|test|boilerplate|style), \
risk (breaking-change|behavioral-change|cosmetic), \
summary (one sentence, max 80 chars). \
Return {\"files\": [...]} wrapping all results. \
Classify only what the diff shows. Do not guess intent.";

pub(crate) fn build_prompt(files: &[(String, String, u32, u32)]) -> String {
    let mut prompt = String::new();
    for (path, diff_text, additions, deletions) in files {
        prompt.push_str(&format!(
            "<file path=\"{path}\" additions={additions} deletions={deletions}>\n"
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
        prompt.push_str("\n</file>\n");
    }
    prompt
}

#[derive(Deserialize)]
struct LlmTriageResponse {
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

fn parse_llm_response(text: &str) -> Option<Vec<FileClassification>> {
    let parsed: LlmTriageResponse = serde_json::from_str(text).ok()?;
    Some(
        parsed
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
    )
}

fn fallback_classification(path: &str) -> FileClassification {
    FileClassification {
        path: path.to_string(),
        relevance: Relevance::Medium,
        category: Category::BusinessLogic,
        risk: Risk::BehavioralChange,
        summary: "Classification failed — review manually".to_string(),
        source: ClassificationSource::Heuristic,
        additions: 0,
        deletions: 0,
    }
}

async fn classify_batch(
    client: &genai::Client,
    model: &str,
    files: &[(String, String, u32, u32)],
) -> Vec<FileClassification> {
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
            parse_llm_response(text.trim()).unwrap_or_else(|| {
                files
                    .iter()
                    .map(|(path, _, _, _)| fallback_classification(path))
                    .collect()
            })
        }
        _ => files
            .iter()
            .map(|(path, _, _, _)| fallback_classification(path))
            .collect(),
    }
}

#[tauri::command]
pub(crate) async fn run_diff_triage(
    repo_path: String,
) -> Result<TriageResult, String> {
    let changed_files = crate::git::get_changed_files(repo_path.clone(), None).await?;
    if changed_files.is_empty() {
        if let Ok(mut cache) = triage_cache().lock() {
            cache.remove(&repo_path);
        }
        return Ok(TriageResult {
            files: vec![],
            llm_used: false,
            llm_model: None,
        });
    }

    let mut classified: Vec<FileClassification> = Vec::new();
    let mut needs_llm: Vec<(String, u32, u32, bool)> = Vec::new();

    for f in &changed_files {
        if let Some(c) = heuristic_classify(&f.path, f.additions, f.deletions) {
            classified.push(c);
        } else {
            let is_untracked = f.status == "?";
            needs_llm.push((f.path.clone(), f.additions, f.deletions, is_untracked));
        }
    }

    // Enrich heuristic results with line stats
    let stats: HashMap<&str, (u32, u32)> = changed_files
        .iter()
        .map(|f| (f.path.as_str(), (f.additions, f.deletions)))
        .collect();
    for c in &mut classified {
        if let Some(&(a, d)) = stats.get(c.path.as_str()) {
            c.additions = a;
            c.deletions = d;
        }
    }

    if needs_llm.is_empty() {
        classified.sort_by(|a, b| a.relevance.cmp(&b.relevance));
        return Ok(TriageResult {
            files: classified,
            llm_used: false,
            llm_model: None,
        });
    }

    // Fetch diffs and check cache — only LLM-classify files whose diff changed
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

    let had_cache_hits = !cache_hits.is_empty();
    classified.extend(cache_hits);

    let mut llm_used = false;
    let mut llm_model: Option<String> = None;

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

                // Build path→diff_hash lookup for cache storage
                let diff_hashes: HashMap<String, u64> = uncached
                    .iter()
                    .map(|(p, d, _, _)| (p.clone(), hash_diff(d)))
                    .collect();

                for batch in uncached.chunks(FILES_PER_BATCH) {
                    let results = classify_batch(&client, &model_name, batch).await;
                    // Store new LLM results in cache, matched by path
                    if let Ok(mut cache) = triage_cache().lock() {
                        let rc = cache.entry(repo_path.clone()).or_default();
                        for r in &results {
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
                    classified.extend(results);
                }

                llm_used = true;
                llm_model = Some(model_name);
            }
            Err(_) => {
                for (path, _, additions, deletions) in &uncached {
                    let mut fc = fallback_classification(path);
                    fc.additions = *additions;
                    fc.deletions = *deletions;
                    classified.push(fc);
                }
            }
        }
    }

    for (path, _, _, _) in needs_llm.iter().skip(MAX_FILES_TO_LLM) {
        classified.push(fallback_classification(path));
    }

    // Enrich LLM/fallback results with line stats
    for c in &mut classified {
        if let Some(&(a, d)) = stats.get(c.path.as_str()) {
            c.additions = a;
            c.deletions = d;
        }
    }

    // Prune stale cache entries for files no longer changed
    if let Ok(mut cache) = triage_cache().lock() {
        if let Some(rc) = cache.get_mut(&repo_path) {
            let current: std::collections::HashSet<&str> =
                changed_files.iter().map(|f| f.path.as_str()).collect();
            rc.retain(|k, _| current.contains(k.as_str()));
        }
    }

    classified.sort_by(|a, b| a.relevance.cmp(&b.relevance));
    Ok(TriageResult {
        files: classified,
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
    fn test_files_detected() {
        for path in &[
            "src/__tests__/foo.test.ts",
            "src/components/Terminal.test.tsx",
            "src-tauri/src/pty_test.rs",
            "tests/integration_test.rs",
            "spec/models/user_spec.rb",
        ] {
            let c = classify(path).unwrap_or_else(|| panic!("expected classification for {path}"));
            assert_eq!(c.relevance, Relevance::Low, "{path}");
            assert_eq!(c.category, Category::Test, "{path}");
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
    fn unknown_files_return_none() {
        assert!(classify("src/main.rs").is_none());
        assert!(classify("src/components/App.tsx").is_none());
        assert!(classify("README.md").is_none());
        assert!(classify("src-tauri/src/git.rs").is_none());
    }

    #[test]
    fn all_heuristic_results_have_heuristic_source() {
        let paths = &[
            "Cargo.lock",
            "src/__tests__/foo.test.ts",
            "migrations/001.sql",
            "proto/__generated__/api.ts",
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
        assert!(prompt.contains("<file path=\"src/main.rs\" additions=1 deletions=1>"));
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
        let json = r#"{"files": [
            {"path": "src/config.rs", "relevance": "high", "category": "api-surface",
             "risk": "breaking-change", "summary": "Changed public API"}
        ]}"#;
        let results = parse_llm_response(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "src/config.rs");
        assert_eq!(results[0].relevance, Relevance::High);
        assert_eq!(results[0].category, Category::ApiSurface);
        assert_eq!(results[0].risk, Risk::BreakingChange);
        assert_eq!(results[0].source, ClassificationSource::Llm);
    }

    #[test]
    fn parse_llm_response_malformed_returns_none() {
        assert!(parse_llm_response("not json").is_none());
        assert!(parse_llm_response("{}").is_none());
        assert!(parse_llm_response("{\"files\": \"bad\"}").is_none());
    }

    #[test]
    fn fallback_classification_is_medium() {
        let c = fallback_classification("unknown.rs");
        assert_eq!(c.relevance, Relevance::Medium);
        assert_eq!(c.category, Category::BusinessLogic);
        assert!(c.summary.contains("review manually"));
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
