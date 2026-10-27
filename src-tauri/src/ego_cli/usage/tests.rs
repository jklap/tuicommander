use super::*;

fn recorded() -> Value {
    serde_json::from_str(include_str!("../../fixtures/ego-usage-v3.json")).unwrap()
}

// Catches: subscription equivalents labelled as money spent, selecting another
// session's occupancy, and incomplete prices being rendered as zero.
#[test]
fn ego_usage_recorded_report_keeps_session_and_cost_semantics() {
    let report = recorded();
    assert_eq!(
        project(&report, "01a158f3-2878-7373-a912-83cdca86822d")
            .unwrap()
            .text
            .as_deref(),
        Some("ctx: 63151 · API eq: USD 1.261548")
    );
    assert_eq!(
        project(&report, "d2c92a03-27ea-574a-ad3b-f482ef41491a")
            .unwrap()
            .text
            .as_deref(),
        Some("ctx: 137126")
    );
    assert_eq!(project(&report, "not-this-session").unwrap().text, None);
}

// Catches: future report versions, absent observations, and invalid decimal
// amounts gaining a plausible-looking usage display.
#[test]
fn ego_usage_missing_or_invalid_fields_never_invent_usage() {
    let mut report = recorded();
    report["v"] = 4.into();
    assert!(project(&report, "not-this-session").is_err());
    report["v"] = 3.into();
    let id = report["sessions"][0]["session"]
        .as_str()
        .unwrap()
        .to_owned();
    report["sessions"][0]["context"] = Value::Null;
    report["sessions"][0]["lines"]["total"]["cost"] = Value::Null;
    assert_eq!(project(&report, &id).unwrap().text, None);
    for amount in ["NaN", "-1", "1.2.3", "", "1e1000"] {
        report["sessions"][0]["lines"]["total"]["cost"] = serde_json::json!({
            "unknown": [], "known": [{"currency": "USD", "amount": amount}],
            "subscription_equivalent": [], "subscription_unknown": []
        });
        assert_eq!(project(&report, &id).unwrap().text, None, "{amount}");
    }
}

// Catches: suppressing real exact monetary totals or treating known zero as missing.
#[test]
fn ego_usage_known_monetary_total_preserves_exact_decimal() {
    let mut report = recorded();
    let id = report["sessions"][0]["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for amount in ["1.261548", "0"] {
        report["sessions"][0]["context"] = Value::Null;
        report["sessions"][0]["lines"]["total"]["cost"] = serde_json::json!({
            "unknown": [], "known": [{"currency": "USD", "amount": amount}],
            "subscription_equivalent": [], "subscription_unknown": []
        });
        assert_eq!(
            project(&report, &id).unwrap().text,
            Some(format!("USD {amount}"))
        );
    }
}
