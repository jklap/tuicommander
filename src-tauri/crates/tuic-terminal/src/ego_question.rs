//! Decode ego's native single-answer selector, anchored by its exact footer.
use crate::output_parser::{ChoiceOption, ChoiceSelectionMode, ParsedEvent};

const FOOTER: &str = "Up/Down or number selects; Enter answers; Esc cancels.";

pub fn parse_ego_question(rows: &[String]) -> Option<ParsedEvent> {
    let end = (0..rows.len()).rev().find(|&start| {
        let mut footer = String::new();
        for row in rows.iter().skip(start).take(4) {
            if !footer.is_empty() {
                footer.push(' ');
            }
            footer.push_str(row.trim());
            if footer == FOOTER {
                return true;
            }
            if !FOOTER.starts_with(&footer) {
                return false;
            }
        }
        false
    })?;
    let first = (0..end).rev().find(|&i| {
        let text = rows[i].trim().trim_start_matches('>').trim_start();
        text.starts_with("1. ")
    })?;
    let mut options: Vec<ChoiceOption> = Vec::new();
    for row in &rows[first..end] {
        let text = row.trim();
        if text.is_empty() {
            continue;
        }
        let highlighted = text.starts_with('>');
        let text = text.trim_start_matches('>').trim_start();
        let numbered = text
            .split_once(". ")
            .filter(|(key, _)| !key.is_empty() && key.bytes().all(|ch| ch.is_ascii_digit()));
        if let Some((key, label)) = numbered {
            if key.parse::<usize>().ok()? != options.len() + 1 || label.is_empty() {
                return None;
            }
            options.push(ChoiceOption {
                key: key.to_owned(),
                label: label.to_owned(),
                highlighted,
                destructive: false,
                hint: None,
            });
        } else {
            let option = options.last_mut()?;
            option.label.push(' ');
            option.label.push_str(text);
        }
    }
    if options.is_empty() || options.iter().filter(|option| option.highlighted).count() != 1 {
        return None;
    }
    let mut start = first;
    while start > 0 && !rows[start - 1].trim().is_empty() {
        start -= 1;
    }
    let title = rows[start..first]
        .iter()
        .map(|row| row.trim())
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() {
        return None;
    }
    Some(ParsedEvent::ChoicePrompt {
        title,
        options,
        selection_mode: Some(ChoiceSelectionMode::NavigateEnter),
        dismiss_key: Some("cancel".to_owned()),
        amend_key: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn ego_question_keeps_wrapped_question_and_answer_text() {
        let screen = rows(&[
            "",
            "Does Boss authorize editing",
            "the orchestrator registry?",
            "> 1. Yes, authorize registry",
            "integration and keep the complete question visible",
            "  2. No, leave the registry unchanged",
            FOOTER,
        ]);
        let Some(ParsedEvent::ChoicePrompt {
            title,
            options,
            selection_mode,
            ..
        }) = parse_ego_question(&screen)
        else {
            panic!("wrapped selector was lost")
        };
        assert_eq!(
            title,
            "Does Boss authorize editing the orchestrator registry?"
        );
        assert_eq!(
            options[0].label,
            "Yes, authorize registry integration and keep the complete question visible"
        );
        assert_eq!(selection_mode, Some(ChoiceSelectionMode::NavigateEnter));
    }

    #[test]
    fn ego_question_does_not_turn_prose_or_multiple_choice_into_single_answer() {
        for footer in [
            "",
            "Up/Down moves; Space or number toggles; Enter answers; Esc cancels.",
        ] {
            assert!(
                parse_ego_question(&rows(&["Question?", "> 1. First", "2. Second", footer]))
                    .is_none()
            );
        }
        assert!(
            parse_ego_question(&rows(&["Question?", "1. First", "2. Second", FOOTER])).is_none()
        );
        assert!(
            parse_ego_question(&rows(&["Question?", "> 1. First", "3. Third", FOOTER])).is_none()
        );
    }
}
