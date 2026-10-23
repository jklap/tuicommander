use super::*;
use std::cell::Cell;

fn intake() -> EventIntake {
    EventIntake {
        repositories: vec!["sstraus/tuic-test-sandbox".into()],
        label: "tuic-run".into(),
        definition_id: "pilot".into(),
    }
}

fn request<'a>(repository: &'a str, head: &'a str) -> DraftPr<'a> {
    DraftPr {
        repository,
        head,
        base: "main",
        issue_number: 3,
    }
}

// Catches a publisher that reaches gh for a repository outside the allowlist.
#[test]
fn a_repository_outside_the_allowlist_never_reaches_gh() {
    let called = Cell::new(false);
    for repository in ["sstraus/other", "evil/tuic-test-sandbox"] {
        let result = publish_draft_pr(&intake(), &request(repository, "fix-3"), |_| {
            called.set(true);
            Ok(String::new())
        });
        assert!(result.is_err());
    }
    assert!(!called.get());
    // An empty allowlist (the default) disables publishing entirely.
    let result = publish_draft_pr(
        &EventIntake::default(),
        &request("sstraus/tuic-test-sandbox", "fix-3"),
        |_| {
            called.set(true);
            Ok(String::new())
        },
    );
    assert!(result.is_err() && !called.get());
}

// Catches a PR that is not a draft, targets another repo, or lets a branch name act as a gh option.
#[test]
fn the_pr_is_always_a_draft_on_the_allowlisted_repository() {
    let seen = std::cell::RefCell::new(Vec::new());
    let url = publish_draft_pr(
        &intake(),
        &request("SStraus/TUIC-test-sandbox", "fix-3"),
        |args| {
            seen.replace(args.to_vec());
            Ok("https://github.com/sstraus/tuic-test-sandbox/pull/9".into())
        },
    )
    .unwrap();
    assert!(url.ends_with("/pull/9"));
    let args = seen.take();
    assert_eq!(&args[..3], ["pr", "create", "--draft"]);
    let position = args.iter().position(|a| a == "--repo").unwrap();
    assert_eq!(args[position + 1], "SStraus/TUIC-test-sandbox");
    assert!(
        publish_draft_pr(
            &intake(),
            &request("sstraus/tuic-test-sandbox", "--web"),
            |_| Ok(String::new())
        )
        .is_err()
    );
}
