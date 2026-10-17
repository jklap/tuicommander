//! Decode ego's native question surfaces; selectors require their exact footer.
use crate::output_parser::{ChoiceOption, ChoiceSelectionMode, ParsedEvent};

const FOOTER: &str = "Up/Down or number selects; Enter answers; Esc cancels.";
const MULTIPLE_FOOTER: &str = "Up/Down moves; Space or number toggles; Enter answers; Esc cancels.";

fn footer_end(rows: &[String], expected: &str) -> Option<usize> {
    // A terminal can wrap in the middle of a word. Compare the exact non-space
    // footer instead of inserting a word boundary at every physical row.
    let expected: String = expected.chars().filter(|ch| !ch.is_whitespace()).collect();
    (0..rows.len()).rev().find(|&start| {
        let mut footer = String::new();
        for (offset, row) in rows.iter().skip(start).take(4).enumerate() {
            footer.extend(row.chars().filter(|ch| !ch.is_whitespace()));
            if footer == expected {
                // The selector footer closes the active surface. Ego can leave
                // answered selectors above a newer composer after scrolling;
                // those historical footers must not re-arm awaiting on Enter.
                return rows[start + offset + 1..]
                    .iter()
                    .all(|row| row.trim().is_empty());
            }
            if !expected.starts_with(&footer) {
                return false;
            }
        }
        false
    })
}

fn option_text(row: &str, multiple: bool) -> Option<(&str, &str)> {
    let text = row
        .trim()
        .strip_prefix('>')
        .unwrap_or(row.trim())
        .trim_start();
    let text = if multiple {
        text.strip_prefix("[ ] ")
            .or_else(|| text.strip_prefix("[x] "))?
    } else {
        text
    };
    text.split_once(". ")
        .filter(|(key, _)| !key.is_empty() && key.bytes().all(|ch| ch.is_ascii_digit()))
}

pub fn parse_ego_question(rows: &[String]) -> Option<ParsedEvent> {
    let (end, multiple) = footer_end(rows, FOOTER)
        .map(|end| (end, false))
        .or_else(|| footer_end(rows, MULTIPLE_FOOTER).map(|end| (end, true)))?;
    let first = (0..end)
        .rev()
        .find(|&i| option_text(&rows[i], multiple).is_some_and(|(key, _)| key == "1"))?;
    let mut options: Vec<ChoiceOption> = Vec::new();
    for row in &rows[first..end] {
        let text = row.trim();
        if text.is_empty() {
            continue;
        }
        let highlighted = text.starts_with('>');
        let numbered = option_text(row, multiple);
        if let Some((key, label)) = numbered {
            if key.parse::<usize>().ok()? != options.len() + 1 || label.is_empty() {
                return None;
            }
            options.push(ChoiceOption {
                key: key.to_owned(),
                label: if multiple {
                    format!(
                        "{} {label}",
                        if text
                            .trim_start_matches('>')
                            .trim_start()
                            .starts_with("[x] ")
                        {
                            "[x]"
                        } else {
                            "[ ]"
                        }
                    )
                } else {
                    label.to_owned()
                },
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
        selection_mode: Some(if multiple {
            ChoiceSelectionMode::ToggleEnter
        } else {
            ChoiceSelectionMode::NavigateEnter
        }),
        dismiss_key: Some("cancel".to_owned()),
        amend_key: None,
    })
}

/// This text shape alone is not authoritative: call only during a confident ego
/// awaiting interval. A ready composer has the same empty `>` row.
pub fn parse_ego_free_text_question(rows: &[String]) -> Option<ParsedEvent> {
    let end = rows.iter().rposition(|row| row.trim() == ">")?;
    if rows[end + 1..].iter().any(|row| !row.trim().is_empty()) {
        return None;
    }
    let mut start = end;
    while start > 0 {
        let row = rows[start - 1].trim();
        if row.is_empty()
            || row.starts_with("εgo:")
            || row.starts_with("Ctrl+B backgrounds")
            || row.starts_with("Ctrl+Enter steers")
            || row.contains("Option+P models")
            || matches!(row, "/help" | "· /help")
            || (row.chars().count() >= 3
                && row.chars().all(|ch| matches!(ch, '─' | '━' | '═' | '-')))
        {
            break;
        }
        start -= 1;
    }
    let prompt_text = rows[start..end]
        .iter()
        .map(|row| row.trim())
        .collect::<Vec<_>>()
        .join(" ");
    if prompt_text.is_empty()
        || prompt_text.contains("Esc to interrupt")
        || prompt_text.contains("Esc cancels.")
    {
        return None;
    }
    Some(ParsedEvent::Question {
        prompt_text,
        confident: true,
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
    /// Catches: answered selector history reappearing as a question on composer repaint.
    #[test]
    fn ego_question_rejects_a_footer_above_newer_output() {
        for (options, footer) in [
            (["> 1. First", "  2. Second"], FOOTER),
            (["> [x] 1. First", "  [ ] 2. Second"], MULTIPLE_FOOTER),
        ] {
            for newer in [
                "◓ Working 1s · Esc to interrupt",
                "> next prompt",
                "gpt-6.1-sol · medium",
            ] {
                let mut screen = rows(&["Question?", options[0], options[1], footer, ""]);
                screen.push(newer.to_owned());
                screen.push(">".to_owned());
                assert!(
                    parse_ego_question(&screen).is_none(),
                    "historical {footer} above {newer}"
                );
            }
        }
    }

    /// Catches: active wrapped footers being rejected with historical ones.
    #[test]
    fn ego_question_keeps_an_active_wrapped_footer_with_blank_tail() {
        for (options, footer) in [
            (["> 1. First", "  2. Second"], FOOTER),
            (["> [x] 1. First", "  [ ] 2. Second"], MULTIPLE_FOOTER),
        ] {
            let (left, right) = footer.split_at(17);
            assert!(
                parse_ego_question(&rows(&[
                    "Question?",
                    options[0],
                    options[1],
                    left,
                    right,
                    "",
                    "   ",
                ]))
                .is_some()
            );
        }
    }

    /// Catches: checkbox state being dropped or interpreted as a single-answer selector.
    #[test]
    fn ego_multiple_question_preserves_checks_and_requires_explicit_submit() {
        let Some(ParsedEvent::ChoicePrompt {
            options,
            selection_mode,
            ..
        }) = parse_ego_question(&rows(&[
            "Which release checks should we run?",
            "> [x] 1. Parser regression",
            "  [ ] 2. Mobile answer controls",
            MULTIPLE_FOOTER,
        ]))
        else {
            panic!("multiple selector lost")
        };
        assert_eq!(selection_mode, Some(ChoiceSelectionMode::ToggleEnter));
        assert_eq!(options[0].label, "[x] Parser regression");
        assert_eq!(options[1].label, "[ ] Mobile answer controls");
    }

    /// Catches: contiguous startup/help or earlier output leaking into a wrapped modal question.
    #[test]
    fn ego_free_text_question_stops_at_the_nearest_screen_boundary() {
        for boundary in [
            "εgo: ego 0.1.0 · openrouter/openai/gpt-6.1-sol",
            "Ctrl+B backgrounds · Esc interrupts · Enter queues · Up recalls",
            "Ctrl+Enter steers · Ctrl+S draft · Option+P models · /help",
            "────────────────────",
        ] {
            let question = parse_ego_free_text_question(&rows(&[
                "Earlier output must not become part of the question",
                boundary,
                "Which release checks",
                "should we run?",
                ">",
            ]));
            assert!(
                matches!(question, Some(ParsedEvent::Question { prompt_text, confident: true })
                if prompt_text == "Which release checks should we run?")
            );
        }
    }

    /// Catches: awaiting arriving before the modal repaint exposing a busy spinner as a question.
    #[test]
    fn ego_free_text_question_rejects_busy_composer_and_nonempty_draft() {
        for screen in [
            rows(&["◓ Working 1s · Esc to interrupt", ">"]),
            rows(&["Question?", "> draft"]),
            rows(&[">"]),
        ] {
            assert!(parse_ego_free_text_question(&screen).is_none());
        }
    }
}
