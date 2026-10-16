//! Process-local, sanitized activity readback. Never persisted with Telegram setup/status.
use serde::Serialize;
use std::{collections::VecDeque, sync::LazyLock};

#[derive(Clone, Serialize)]
pub(crate) struct Event {
    id: u64,
    time: u64,
    direction: &'static str,
    kind: &'static str,
    outcome: String,
    preview: String,
}

#[derive(Default)]
pub(crate) struct Recent {
    sequence: u64,
    events: VecDeque<Event>,
}

static CREDENTIAL: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"[0-9]+:[A-Za-z0-9_-]+|[A-Fa-f0-9]{6}").unwrap());

impl Recent {
    pub(super) fn record(
        &mut self,
        direction: &'static str,
        kind: &'static str,
        outcome: impl Into<String>,
        text: Option<&str>,
    ) {
        // Redact before truncation: cutting a credential first could hide its signature.
        let preview = text
            .map(|text| {
                let safe = crate::redaction::redact_secrets(text);
                CREDENTIAL
                    .replace_all(&safe, "[redacted]")
                    .chars()
                    .take(60)
                    .collect()
            })
            .unwrap_or_else(|| "[redacted]".into());
        self.sequence += 1;
        if self.events.len() == 20 {
            self.events.pop_back();
        }
        self.events.push_front(Event {
            id: self.sequence,
            time: super::settings::now_ms(),
            direction,
            kind,
            outcome: outcome.into(),
            preview,
        });
    }

    pub(super) fn snapshot(&self) -> Vec<Event> {
        self.events.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: history grows beyond 20 or reads in oldest-first order.
    #[test]
    fn recent_bounds_order_and_restart_1956() {
        let mut recent = Recent::default();
        for n in 0..25 {
            recent.record("in", "message", "delivered", Some(&format!("message {n}")));
        }
        let value = serde_json::to_value(recent.snapshot()).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 20);
        assert_eq!(value[0]["preview"], "message 24");
        assert_eq!(value[19]["preview"], "message 5");
        assert!(Recent::default().snapshot().is_empty());
    }

    // Catches: truncation leaks partial secrets or unauthorized text, or splits UTF-8.
    #[test]
    fn recent_redacts_before_unicode_truncation_1956() {
        let mut recent = Recent::default();
        let long = "😀".repeat(70);
        for text in [
            Some("code A1B2C3 token 111:fake-one"),
            None,
            Some(long.as_str()),
        ] {
            recent.record("in", "message", "delivered", text);
        }
        let value = serde_json::to_value(recent.snapshot()).unwrap();
        assert_eq!(value[0]["preview"].as_str().unwrap().chars().count(), 60);
        assert_eq!(value[1]["preview"], "[redacted]");
        assert_eq!(value[2]["preview"], "code [redacted] token [redacted]");
        assert!(!value.to_string().contains("fake-one"));
        assert!(!value.to_string().contains("A1B2C3"));
    }
}
