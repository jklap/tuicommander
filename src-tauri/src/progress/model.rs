use serde::{Deserialize, Serialize};

pub const MAX_TEXT_CHARS: usize = 500;
pub const MAX_STEP_CHARS: usize = 80;

/// Newest entries returned by one `list`. There is no cursor and no paging: a
/// multi-hour session yields tens of entries, so the cap exists to bound a
/// pathological history, not to be paged through.
pub const LIST_LIMIT: usize = 500;

/// What a journal entry is.
///
/// Two kinds are reported by an agent and one is written by the host. The
/// distinction is not cosmetic: `Intent` is derived from the `intent:` marker
/// the agent already emits, so accepting it from the reporting tool would file
/// one announced task twice.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressKind {
    Done,
    Blocked,
    Intent,
}

impl ProgressKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Intent => "intent",
        }
    }

    /// Parse a kind read back from the database. Accepts `intent`, which the
    /// host writes; use [`ProgressKind::parse_reportable`] for caller input.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "done" => Ok(Self::Done),
            "blocked" => Ok(Self::Blocked),
            "intent" => Ok(Self::Intent),
            other => Err(format!("unknown progress type '{other}'")),
        }
    }

    /// Parse a kind an agent may report. `intent` is refused here rather than
    /// in the store, so the agent reads why instead of seeing its entry
    /// silently filed under a kind it did not ask for.
    pub(crate) fn parse_reportable(value: &str) -> Result<Self, String> {
        match Self::parse(value)? {
            Self::Intent => Err(INTENT_IS_NOT_REPORTABLE.to_string()),
            reportable => Ok(reportable),
        }
    }
}

pub(crate) const INTENT_IS_NOT_REPORTABLE: &str =
    "type must be 'done' or 'blocked' — 'intent' is recorded by TUIC from the intent: marker";

/// An entry on its way into the store. `project` and `created_at_ms` are added
/// by the store; nothing else is inferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewProgressEntry {
    pub kind: ProgressKind,
    pub text: String,
    pub step: Option<String>,
    pub agent_name: Option<String>,
}

impl NewProgressEntry {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("text", &self.text, MAX_TEXT_CHARS)?;
        if let Some(step) = &self.step {
            validate_text("step", step, MAX_STEP_CHARS)?;
        }
        Ok(())
    }

    pub(crate) fn trimmed_text(&self) -> String {
        self.text.trim().to_string()
    }

    /// An all-whitespace step is no step. Trimming it away here keeps the
    /// column's only contract — a crumb a human reads — out of the renderer.
    pub(crate) fn trimmed_step(&self) -> Option<String> {
        self.step
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn trimmed_agent_name(&self) -> Option<String> {
        self.agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }
}

pub(crate) fn validate_text(field: &str, value: &str, max: usize) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    let count = trimmed.chars().count();
    if count > max {
        return Err(format!(
            "{field} must be at most {max} characters (got {count})"
        ));
    }
    Ok(())
}

/// A stored entry. The rowid is identity and order in one.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEntry {
    pub id: i64,
    pub project: String,
    pub created_at_ms: u64,
    #[serde(rename = "type")]
    pub kind: ProgressKind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Everything the dialog renders in one response: the list and the divider.
///
/// `last_viewed_ms` travels with the entries rather than in a second call
/// because the divider is drawn across this exact list; two calls could only
/// disagree about where the line goes.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressList {
    pub project: String,
    pub entries: Vec<ProgressEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_viewed_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressListInput {
    /// The dialog's single filter.
    #[serde(default)]
    pub blocked_only: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressDeleteInput {
    pub ids: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDeleteReceipt {
    pub deleted: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressViewedReceipt {
    pub last_viewed_ms: u64,
}

/// Caller-supplied report fields. The host adds the timestamp, the project and
/// the agent name.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressReportInput {
    #[serde(rename = "type")]
    pub kind: ProgressKind,
    pub text: String,
    #[serde(default)]
    pub step: Option<String>,
}

impl ProgressReportInput {
    /// Refuse `intent` on the way in, wherever the input came from. The MCP
    /// path rejects it while parsing the string; a typed transport (HTTP, IPC)
    /// deserialises the enum first and lands here, and both must answer with
    /// the same sentence.
    pub fn into_entry(self, agent_name: Option<String>) -> Result<NewProgressEntry, String> {
        if self.kind == ProgressKind::Intent {
            return Err(INTENT_IS_NOT_REPORTABLE.to_string());
        }
        Ok(NewProgressEntry {
            kind: self.kind,
            text: self.text,
            step: self.step,
            agent_name,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReceipt {
    pub id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deserialise a valid body, then the same body with one extra field, and
    /// require the second to fail with an "unknown field" error.
    ///
    /// The valid case is asserted first on purpose. A rejection alone proves
    /// nothing — a fixture that is malformed for an unrelated reason is
    /// rejected too, and the test would then pass while `deny_unknown_fields`
    /// was gone.
    macro_rules! assert_rejects_unknown_field {
        ($ty:ty, $json:tt) => {{
            let name = stringify!($ty);
            let mut value = serde_json::json!($json);
            serde_json::from_value::<$ty>(value.clone())
                .unwrap_or_else(|e| panic!("{name} must accept its own valid body: {e}"));

            value
                .as_object_mut()
                .expect("fixture is a JSON object")
                .insert("nopeNotAField".to_string(), serde_json::json!(1));
            let err = serde_json::from_value::<$ty>(value).expect_err(&format!(
                "{name} accepted an unknown field — its deny_unknown_fields is gone"
            ));
            assert!(
                err.to_string().contains("unknown field"),
                "{name} rejected the body for the wrong reason: {err}"
            );
        }};
    }

    #[test]
    fn every_progress_input_type_refuses_an_unknown_field() {
        assert_rejects_unknown_field!(ProgressListInput, {"blockedOnly": true});
        assert_rejects_unknown_field!(ProgressDeleteInput, {"ids": [1, 2]});
        assert_rejects_unknown_field!(ProgressReportInput, {"type": "done", "text": "shipped"});
    }

    #[test]
    fn the_wire_names_the_kind_field_type() {
        let input: ProgressReportInput =
            serde_json::from_value(serde_json::json!({"type": "blocked", "text": "needs a key"}))
                .unwrap();
        assert_eq!(input.kind, ProgressKind::Blocked);
        assert_eq!(input.step, None);
    }

    #[test]
    fn intent_is_refused_from_both_input_paths() {
        assert_eq!(
            ProgressKind::parse_reportable("intent").unwrap_err(),
            INTENT_IS_NOT_REPORTABLE
        );
        let typed = ProgressReportInput {
            kind: ProgressKind::Intent,
            text: "wiring auth middleware".to_string(),
            step: None,
        };
        assert_eq!(
            typed.into_entry(None).unwrap_err(),
            INTENT_IS_NOT_REPORTABLE
        );
    }

    #[test]
    fn text_and_step_are_capped_and_never_empty() {
        let entry = |text: &str, step: Option<&str>| NewProgressEntry {
            kind: ProgressKind::Done,
            text: text.to_string(),
            step: step.map(str::to_string),
            agent_name: None,
        };
        assert!(entry("   ", None).validate().unwrap_err().contains("text"));
        assert!(
            entry(&"x".repeat(MAX_TEXT_CHARS + 1), None)
                .validate()
                .unwrap_err()
                .contains("at most 500")
        );
        assert!(
            entry("ok", Some(&"s".repeat(MAX_STEP_CHARS + 1)))
                .validate()
                .unwrap_err()
                .contains("at most 80")
        );
        assert!(entry("ok", Some("Step 3")).validate().is_ok());
    }

    /// A step that is only whitespace is not a step. It reaches the store as
    /// `None`, so the dialog never renders an empty crumb beside an entry.
    #[test]
    fn a_blank_step_and_a_blank_agent_name_become_absent() {
        let entry = NewProgressEntry {
            kind: ProgressKind::Done,
            text: "  shipped the parser  ".to_string(),
            step: Some("   ".to_string()),
            agent_name: Some("  ".to_string()),
        };
        assert_eq!(entry.trimmed_text(), "shipped the parser");
        assert_eq!(entry.trimmed_step(), None);
        assert_eq!(entry.trimmed_agent_name(), None);
    }
}
