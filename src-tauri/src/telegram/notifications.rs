use crate::state::AppEvent;

/// Only the bound terminal contributes authored progress, never another peer.
pub(super) fn notice(event: &AppEvent, pty: &str) -> Option<String> {
    let AppEvent::ProgressRecorded { payload, .. } = event else {
        return None;
    };
    let entry = &payload["entry"];
    if entry["ptyId"].as_str()? != pty {
        return None;
    }
    if !matches!(entry["type"].as_str()?, "done" | "blocked") {
        return None;
    }
    let text = entry["text"].as_str()?;
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Catches: another peer's outcome leaks to the phone, intent becomes a
    // completed reply, or authored language/whitespace is rewritten.
    #[test]
    fn notices_keep_bound_authored_language_and_ignore_other_progress() {
        for (pty, kind, expected) in [
            ("bound", "done", Some(" Finito.\n")),
            ("bound", "blocked", Some(" Finito.\n")),
            ("other", "done", None),
            ("bound", "intent", None),
        ] {
            let event = AppEvent::ProgressRecorded {
                repo_path: "repo".into(),
                payload: json!({"entry":{"id":7,"ptyId":pty,"type":kind,"text":" Finito.\n"}}),
            };
            assert_eq!(notice(&event, "bound").as_deref(), expected);
        }
    }
}
