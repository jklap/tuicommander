use super::Schedule;
use chrono::{DateTime, Utc};

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

// Catches: a stepped DOM wildcard becoming OR and scheduling unrequested agent runs.
// Vixie entry.c sets DOM_STAR when the field starts with '*'; cron.c then ANDs DOM/DOW.
#[test]
fn stepped_dom_wildcard_does_not_run_on_non_mondays() {
    let schedule = Schedule::parse("0 9 */2 * MON", "UTC").unwrap();
    assert_eq!(
        schedule.next_after(utc("2025-01-01T09:00:00Z")).unwrap(),
        utc("2025-01-13T09:00:00Z"),
        "stepped wildcard ran on an unrequested day"
    );
    assert_eq!(
        schedule
            .latest_due(utc("2025-01-14T09:00:00Z"), None)
            .unwrap(),
        Some(utc("2025-01-13T09:00:00Z"))
    );
}
