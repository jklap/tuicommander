//! Test-only recorded oracles. Normal tests never accept a new baseline.
use serde_json::Value;
use std::path::Path;

pub(crate) fn first_difference(expected: &[Value], actual: &[Value]) -> Option<String> {
    (0..expected.len().max(actual.len())).find_map(|i| {
        (expected.get(i) != actual.get(i)).then(|| {
            format!(
                "first differing event {i}:\nexpected: {}\nactual: {}",
                expected
                    .get(i)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<end>".into()),
                actual
                    .get(i)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<end>".into())
            )
        })
    })
}

pub(crate) fn assert_golden(relative: &Path, actual: &[Value]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/fixtures/replay_oracle")
        .join(relative);
    if std::env::var("TUIC_REGENERATE_ORACLES").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create golden directory");
        let lines = actual
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&path, lines).expect("write explicitly regenerated golden");
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; run scripts/replay-oracle.sh regenerate explicitly",
            path.display()
        )
    });
    let expected: Vec<Value> = recorded
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid golden event"))
        .collect();
    if let Some(diff) = first_difference(&expected, actual) {
        panic!(
            "{}: {diff}; review drift before scripts/replay-oracle.sh regenerate",
            relative.display()
        );
    }
}

// Catches: zip-based comparisons silently accept truncated or appended traces.
#[test]
fn replay_oracle_reports_first_changed_missing_or_extra_event() {
    let baseline = vec![
        serde_json::json!({"state":"busy"}),
        serde_json::json!({"state":"idle"}),
    ];
    assert!(first_difference(&baseline, &baseline).is_none());
    let changed = vec![baseline[0].clone(), serde_json::json!({"state":"awaiting"})];
    for actual in [
        changed,
        baseline[..1].to_vec(),
        vec![baseline[0].clone(), baseline[1].clone(), Value::Null],
    ] {
        let diff = first_difference(&baseline, &actual).unwrap();
        assert!(
            diff.contains(if actual.len() == 3 {
                "event 2:"
            } else {
                "event 1:"
            }),
            "{diff}"
        );
    }
}
