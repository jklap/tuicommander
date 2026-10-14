use super::get_pr_line_threads_impl;
use serde_json::{Value, json};

// Catches: live GraphQL bot authors lose bot=true, or outdated coordinates disappear.
// Expectations were recorded from GitHub on 2026-10-14, not derived from the mapper.
#[tokio::test]
#[ignore = "requires network and GH_TOKEN for read-only public GitHub review threads"]
async fn live_copilot_is_bot_and_outdated_thread_keeps_original_line() {
    let token = std::env::var("GH_TOKEN").expect("GH_TOKEN is required");
    assert!(!token.is_empty(), "GH_TOKEN is required");
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(scratch.path().join("config"));
    let state = crate::state::tests_support::make_test_app_state();
    *state.github.token.write() = Some(token);
    let mut evidence = Vec::new();
    for (name, remote, number) in [
        ("vscode", "https://github.com/microsoft/vscode.git", 341276),
        (
            "verifiers",
            "https://github.com/PrimeIntellect-ai/verifiers.git",
            2833,
        ),
    ] {
        let path = scratch.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec!["remote", "add", "origin", remote],
        ] {
            let output = std::process::Command::new("git")
                .current_dir(&path)
                .env_remove("GH_TOKEN")
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "local repository setup failed");
        }
        let page = get_pr_line_threads_impl(path.to_str().unwrap(), number, None, &state)
            .await
            .expect("read-only live review threads request failed");
        assert!(!page.has_next_page, "live oracle requires a complete page");
        if name == "vscode" {
            let author = page
                .threads
                .iter()
                .flat_map(|t| t["comments"]["nodes"].as_array().unwrap().iter())
                .map(|c| &c["author"])
                .find(|a| a["login"] == "copilot-pull-request-reviewer")
                .expect("recorded Copilot thread must still exist");
            assert_eq!(author["bot"], true);
        } else {
            let outdated = page
                .threads
                .iter()
                .find(|t| t["id"] == "PRRT_kwDONt9L486rlu-F")
                .expect("recorded outdated deployment.py thread must still exist");
            assert_eq!(outdated["isOutdated"], true);
            assert!(outdated["line"].is_null());
            assert_eq!(outdated["originalLine"], 148);
            let human = page
                .threads
                .iter()
                .flat_map(|t| t["comments"]["nodes"].as_array().unwrap().iter())
                .map(|c| &c["author"])
                .find(|a| a["login"] == "mikasenghaas")
                .expect("recorded human thread must still exist");
            assert_eq!(human["bot"], false);
        }
        let threads: Vec<Value> = page
            .threads
            .iter()
            .map(|t| {
                let authors: Vec<&Value> = t["comments"]["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| &c["author"])
                    .collect();
                json!({"id":t["id"], "path":t["path"], "line":t["line"],
                "originalLine":t["originalLine"], "isOutdated":t["isOutdated"], "authors":authors})
            })
            .collect();
        evidence.push(
            json!({"repository":remote,"pr":number,"headSha":page.head_sha,"threads":threads}),
        );
    }
    if let Ok(path) = std::env::var("TUIC_LIVE_REVIEW_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
