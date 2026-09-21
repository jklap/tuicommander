//! An AI review of one pull request, run by ego.
//!
//! What came back from story 784 and what deliberately did not.
//!
//! **Back:** the findings, the confidence gate, and the posting flow. A finding
//! is still `{path, line, severity, message, confidence}`, still filtered by
//! [`finding_confidence_threshold`] before anything renders, and still posted
//! through `github::post_pr_review` by the popover. Those three are what
//! criterion 1 of story 795-320b names, and they are the contract.
//!
//! **Not back:** the multi-turn engine. The deleted version held a session per
//! repo for 300 seconds, spent one LLM turn on an overview and one per file, and
//! let the model call `read_file` between them. That was the embedded agent, and
//! it is the thing 784 deleted. This is one unattended ego turn over the whole
//! diff (`acp::oneshot`), which is why there is no session cache, no per-file
//! phase and no tool dispatch here.
//!
//! **Also not back:** `llm_used` and `llm_model`. Which model ran is ego's
//! configuration, not a property of this call, and TUICommander is not told. A
//! field that could only ever be reported as `null` is worse than no field.
//!
//! The diff goes in whole rather than being fetched by the model: the turn has
//! no host tools, so what ego sees is what this module sends, which is the same
//! reason `improvement_scan` assembles its own snapshot.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::acp::oneshot;

/// The most files whose diffs are sent. A pull request larger than this is
/// reviewed on its first `MAX_FILES` files rather than not at all.
const MAX_FILES: usize = 30;
/// The most diff lines sent per file, matching the deleted version's budget.
const MAX_LINES_PER_FILE: usize = 300;

/// Below this, a finding is the model speculating and is not shown.
const DEFAULT_FINDING_CONFIDENCE_THRESHOLD: f32 = 0.7;

const PR_REVIEW_SYSTEM_PROMPT: &str = "\
You are a senior code reviewer reviewing a pull request. You are shown the diff \
of each changed file. Relate the files to each other; a change that is only a \
problem in combination with another file is exactly what a reviewer is for.

Respond with ONLY a single JSON object, no prose and no code fences, of the form:
{\"summary\":\"2-3 sentence overview of the changeset\",\"files\":[{\"path\":\"...\",\
\"summary\":\"one sentence\",\"findings\":[{\"path\":\"...\",\"line\":123,\
\"hunk\":\"optional surrounding context\",\"severity\":\"bug|risk|nit\",\
\"message\":\"actionable review finding\",\"confidence\":0.0}]}]}

Rules: findings are line-level and actionable. Use severity=bug for a likely \
defect, risk for a plausible regression, nit for minor cleanup. `line` is a line \
number in the file after the change. Use confidence 0.0-1.0 and do not report \
speculation — anything you would not defend gets a confidence below 0.7. A file \
with nothing worth saying gets an empty findings array, not an invented nit.";

/// How sure the model says it is, and what it found.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Finding {
    pub path: String,
    pub line: Option<u32>,
    pub hunk: Option<String>,
    pub severity: Severity,
    pub message: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Bug,
    Risk,
    Nit,
}

/// One file's verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ReviewedFile {
    pub path: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

/// What a review produced, as the popover renders it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct PrReviewResult {
    pub repo_path: String,
    pub pr_number: i64,
    /// Identity of the diff this review was run against.
    ///
    /// A hash of the diff text and not a git sha: it exists so the frontend can
    /// tell "the review you are looking at" from "the review of a PR that has
    /// been pushed to since", and the diff is the thing that has to match.
    pub head_sha: String,
    pub summary: Option<String>,
    pub files: Vec<ReviewedFile>,
}

/// The envelope ego is asked for.
#[derive(Debug, Deserialize)]
struct ReviewEnvelope {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<ReviewedFile>,
}

/// One file's slice of a unified diff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct UnifiedDiffFile {
    pub path: String,
    pub diff: String,
    pub additions: u32,
    pub deletions: u32,
}

fn hash_diff(diff: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    diff.hash(&mut hasher);
    hasher.finish()
}

/// The confidence a finding must reach to be shown.
///
/// Overridable by environment because it is a reviewer's taste, not a constant:
/// a value outside 0.0..=1.0, or one that does not parse, falls back rather than
/// silently turning the gate off.
pub(crate) fn finding_confidence_threshold() -> f32 {
    std::env::var("TUIC_REVIEW_CONFIDENCE_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| (0.0..=1.0).contains(v))
        .unwrap_or(DEFAULT_FINDING_CONFIDENCE_THRESHOLD)
}

/// Drop everything the model was not sure enough about.
///
/// `is_finite` is load-bearing: `NaN` fails every comparison, so a model that
/// emits one would otherwise produce a finding that passes no gate and is shown
/// anyway if the test is written the other way round.
pub(crate) fn filter_findings_by_confidence(findings: &[Finding], threshold: f32) -> Vec<Finding> {
    findings
        .iter()
        .filter(|f| f.confidence.is_finite() && f.confidence >= threshold && f.confidence <= 1.0)
        .cloned()
        .collect()
}

/// Split a unified diff into per-file slices, counting changed lines.
pub(crate) fn split_unified_diff(diff: &str) -> Vec<UnifiedDiffFile> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut additions = 0u32;
    let mut deletions = 0u32;

    let flush = |files: &mut Vec<UnifiedDiffFile>,
                 current_path: &mut Option<String>,
                 current_lines: &mut Vec<String>,
                 additions: &mut u32,
                 deletions: &mut u32| {
        if let Some(path) = current_path.take() {
            files.push(UnifiedDiffFile {
                path,
                diff: current_lines.join("\n"),
                additions: *additions,
                deletions: *deletions,
            });
        }
        current_lines.clear();
        *additions = 0;
        *deletions = 0;
    };

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush(
                &mut files,
                &mut current_path,
                &mut current_lines,
                &mut additions,
                &mut deletions,
            );
            let path = rest
                .split_whitespace()
                .nth(1)
                .or_else(|| rest.split_whitespace().next())
                .unwrap_or("")
                .trim_start_matches("b/")
                .trim_start_matches("a/")
                .to_string();
            current_path = (!path.is_empty()).then_some(path);
        }
        if current_path.is_some() {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions = additions.saturating_add(1);
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions = deletions.saturating_add(1);
            }
            current_lines.push(line.to_string());
        }
    }
    flush(
        &mut files,
        &mut current_path,
        &mut current_lines,
        &mut additions,
        &mut deletions,
    );
    files
}

/// Assemble the prompt body: every file, bounded.
///
/// Truncation is announced in the text rather than done silently, so the model
/// does not report "this function has no early return" about a function whose
/// early return was cut off.
pub(crate) fn build_review_prompt(files: &[UnifiedDiffFile]) -> String {
    let shown = files.len().min(MAX_FILES);
    let mut out = format!("Pull request diff, {} changed file", files.len());
    if files.len() != 1 {
        out.push('s');
    }
    if files.len() > MAX_FILES {
        out.push_str(&format!(" (only the first {MAX_FILES} are shown)"));
    }
    out.push_str(":\n");

    for file in files.iter().take(shown) {
        out.push_str(&format!(
            "\n--- {} (+{} -{})\n",
            file.path, file.additions, file.deletions
        ));
        let lines: Vec<&str> = file.diff.lines().collect();
        out.push_str(&lines[..lines.len().min(MAX_LINES_PER_FILE)].join("\n"));
        out.push('\n');
        if lines.len() > MAX_LINES_PER_FILE {
            out.push_str(&format!(
                "[... {} more lines of this diff not shown ...]\n",
                lines.len() - MAX_LINES_PER_FILE
            ));
        }
    }
    out
}

/// Read ego's answer, then apply the gate.
///
/// The gate runs here and not in the UI on purpose: a finding below the
/// threshold must never reach a client, because the posting flow posts what it
/// was given.
pub(crate) fn parse_review_output(raw: &str, threshold: f32) -> Result<ReviewEnvelopeOut, String> {
    let candidate = oneshot::extract_json(raw);
    let envelope: ReviewEnvelope = serde_json::from_str(candidate)
        .map_err(|e| oneshot::unparseable("review", e, candidate))?;

    let files = envelope
        .files
        .into_iter()
        .map(|file| ReviewedFile {
            findings: filter_findings_by_confidence(&file.findings, threshold),
            ..file
        })
        .collect();

    Ok(ReviewEnvelopeOut {
        summary: envelope.summary.filter(|s| !s.trim().is_empty()),
        files,
    })
}

/// A parsed, gated review, before it is given its PR identity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReviewEnvelopeOut {
    pub summary: Option<String>,
    pub files: Vec<ReviewedFile>,
}

/// Dual-emit, in the shape `remote_runtime::publish` uses: the window emit is
/// the desktop's half and is gated, the bus send is everyone's and is not.
///
/// `POST /repo/pr-review` is mounted unconditionally, so the headless build
/// answers it as well — and its clients are browsers reading `/events`. Gating
/// the whole function left them with a review column stuck on "running".
fn emit_review_progress(state: &crate::AppState, repo_path: &str, payload: serde_json::Value) {
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().clone() {
        use tauri::Emitter;
        let _ = app.emit(
            "review-progress",
            serde_json::json!({ "repo_path": repo_path, "payload": payload }),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::ReviewProgress {
            repo_path: repo_path.to_string(),
            payload,
        });
}

pub(crate) async fn run_pr_review_impl(
    repo_path: String,
    pr_number: i64,
    state: &std::sync::Arc<crate::AppState>,
) -> Result<PrReviewResult, String> {
    let diff = crate::github::get_pr_diff_impl(&repo_path, pr_number, state).await?;
    let head_sha = format!("{:016x}", hash_diff(&diff));
    let files = split_unified_diff(&diff);

    // An empty diff is an answer, and it is not one ego has to be started for.
    if files.is_empty() {
        return Ok(PrReviewResult {
            repo_path,
            pr_number,
            head_sha,
            summary: Some("This pull request changes no files.".to_string()),
            files: Vec::new(),
        });
    }

    emit_review_progress(
        state,
        &repo_path,
        serde_json::json!({ "pr_number": pr_number, "done": false, "findings_count": 0 }),
    );

    let answer = oneshot::ask(
        state,
        std::path::PathBuf::from(&repo_path),
        PR_REVIEW_SYSTEM_PROMPT,
        build_review_prompt(&files),
    )
    .await;

    let parsed =
        match answer.and_then(|raw| parse_review_output(&raw, finding_confidence_threshold())) {
            Ok(parsed) => parsed,
            Err(error) => {
                // The column must not be left saying "running" forever because the
                // turn failed; the reason travels with the end of the review.
                emit_review_progress(
                    state,
                    &repo_path,
                    serde_json::json!({ "pr_number": pr_number, "done": true, "error": error }),
                );
                return Err(error);
            }
        };

    let findings_count: usize = parsed.files.iter().map(|f| f.findings.len()).sum();
    emit_review_progress(
        state,
        &repo_path,
        serde_json::json!({
            "pr_number": pr_number,
            "done": true,
            "findings_count": findings_count,
        }),
    );

    Ok(PrReviewResult {
        repo_path,
        pr_number,
        head_sha,
        summary: parsed.summary,
        files: parsed.files,
    })
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn run_pr_review(
    repo_path: String,
    pr_number: i64,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<PrReviewResult, String> {
    let state = state.inner().clone();
    run_pr_review_impl(repo_path, pr_number, &state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `POST /repo/pr-review` is mounted unconditionally, so the headless build
    /// answers it too — and every client of that build is a browser reading
    /// `/events`. The window emit is the desktop's half of the dual emit; the
    /// bus send is everybody's. Gating the bus send along with it left a review
    /// started from a browser with a column stuck on "running" and no way to
    /// learn it had finished.
    #[test]
    fn review_progress_reaches_the_bus_on_every_build() {
        let state = crate::state::tests_support::make_test_app_state();
        let mut events = state.event_bus.subscribe();

        emit_review_progress(
            &state,
            "/repo",
            serde_json::json!({ "pr_number": 7, "done": false }),
        );

        match events
            .try_recv()
            .expect("the review's progress reaches the event bus")
        {
            crate::state::AppEvent::ReviewProgress { repo_path, payload } => {
                assert_eq!(repo_path, "/repo");
                assert_eq!(payload["pr_number"], 7);
            }
            other => panic!("expected ReviewProgress, got {other:?}"),
        }
    }

    fn finding(confidence: f32, severity: Severity) -> Finding {
        Finding {
            path: "src/lib.rs".into(),
            line: Some(12),
            hunk: None,
            severity,
            message: "m".into(),
            confidence,
        }
    }

    #[test]
    fn split_unified_diff_splits_files_and_counts_changed_lines() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,3 @@
 keep
+added
-removed
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1 +1 @@
+only
";
        let files = split_unified_diff(diff);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        // `+++`/`---` headers are not changed lines.
        assert_eq!((files[0].additions, files[0].deletions), (1, 1));
        assert_eq!(files[1].path, "src/b.rs");
        assert_eq!((files[1].additions, files[1].deletions), (1, 0));
    }

    #[test]
    fn a_diff_with_no_files_splits_to_nothing() {
        assert!(split_unified_diff("").is_empty());
        assert!(split_unified_diff("not a diff at all\n").is_empty());
    }

    #[test]
    fn the_gate_drops_what_the_model_was_not_sure_about() {
        let findings = vec![
            finding(0.9, Severity::Bug),
            finding(0.69, Severity::Risk),
            finding(0.7, Severity::Nit),
        ];
        let kept = filter_findings_by_confidence(&findings, 0.7);

        // Exactly at the threshold is kept; just under is not.
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|f| f.confidence >= 0.7));
    }

    // NaN fails every comparison, so a gate written as `!(c < threshold)` would
    // let it through. It must be excluded by being non-finite, not by luck.
    #[test]
    fn a_nonsense_confidence_never_passes_the_gate() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.5, -0.2] {
            let kept = filter_findings_by_confidence(&[finding(bad, Severity::Bug)], 0.7);
            assert!(kept.is_empty(), "confidence {bad} should not pass");
        }
    }

    #[test]
    fn a_threshold_of_zero_still_rejects_an_impossible_confidence() {
        // Turning the gate all the way down is a reviewer's choice; accepting
        // a confidence of 2.0 is never one.
        assert!(filter_findings_by_confidence(&[finding(2.0, Severity::Nit)], 0.0).is_empty());
        assert_eq!(
            filter_findings_by_confidence(&[finding(0.0, Severity::Nit)], 0.0).len(),
            1
        );
    }

    #[test]
    fn the_gate_runs_before_anything_leaves_this_module() {
        let raw = r#"{"summary":"s","files":[{"path":"src/a.rs","summary":"f",
            "findings":[
              {"path":"src/a.rs","line":1,"severity":"bug","message":"real","confidence":0.95},
              {"path":"src/a.rs","line":2,"severity":"nit","message":"guess","confidence":0.2}
            ]}]}"#;
        let parsed = parse_review_output(raw, 0.7).unwrap();

        assert_eq!(parsed.files[0].findings.len(), 1);
        assert_eq!(parsed.files[0].findings[0].message, "real");
    }

    #[test]
    fn a_review_that_found_nothing_is_a_result_and_not_a_failure() {
        let parsed = parse_review_output(r#"{"summary":"all fine","files":[]}"#, 0.7).unwrap();

        assert_eq!(parsed.summary.as_deref(), Some("all fine"));
        assert!(parsed.files.is_empty());
    }

    #[test]
    fn a_blank_summary_is_no_summary() {
        let parsed = parse_review_output(r#"{"summary":"   ","files":[]}"#, 0.7).unwrap();
        assert!(parsed.summary.is_none());
    }

    #[test]
    fn an_unparseable_review_says_what_ego_actually_said() {
        let err = parse_review_output("I am not going to do that", 0.7).unwrap_err();

        assert!(err.contains("review"), "{err}");
        assert!(err.contains("I am not going to do that"), "{err}");
    }

    #[test]
    fn a_fenced_answer_is_read_like_any_other() {
        let parsed =
            parse_review_output("```json\n{\"summary\":\"s\",\"files\":[]}\n```", 0.7).unwrap();
        assert_eq!(parsed.summary.as_deref(), Some("s"));
    }

    #[test]
    fn the_prompt_carries_every_file_and_says_when_it_does_not() {
        let files: Vec<UnifiedDiffFile> = (0..MAX_FILES + 5)
            .map(|i| UnifiedDiffFile {
                path: format!("src/f{i}.rs"),
                diff: "+x".into(),
                additions: 1,
                deletions: 0,
            })
            .collect();
        let prompt = build_review_prompt(&files);

        assert!(prompt.contains(&format!("{} changed files", MAX_FILES + 5)));
        assert!(prompt.contains("only the first 30 are shown"));
        assert!(prompt.contains("src/f29.rs"));
        assert!(!prompt.contains("src/f30.rs"));
    }

    // Silent truncation makes the model confidently wrong about the part it was
    // not shown, so the cut has to be visible in the prompt.
    #[test]
    fn a_truncated_file_diff_says_so_in_the_prompt() {
        let long = (0..MAX_LINES_PER_FILE + 40)
            .map(|i| format!("+line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = build_review_prompt(&[UnifiedDiffFile {
            path: "src/big.rs".into(),
            diff: long,
            additions: 340,
            deletions: 0,
        }]);

        assert!(prompt.contains("40 more lines of this diff not shown"));
        assert!(prompt.contains("+line 299"));
        assert!(!prompt.contains("+line 300"));
    }

    #[test]
    fn one_file_is_not_pluralised() {
        let prompt = build_review_prompt(&[UnifiedDiffFile {
            path: "src/a.rs".into(),
            diff: "+x".into(),
            additions: 1,
            deletions: 0,
        }]);
        assert!(prompt.contains("1 changed file:"), "{prompt}");
    }

    #[test]
    fn the_same_diff_always_identifies_the_same_review() {
        assert_eq!(hash_diff("a diff"), hash_diff("a diff"));
        assert_ne!(hash_diff("a diff"), hash_diff("a diff with one more line"));
    }
}
