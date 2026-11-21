use super::super::model::RecognizedEntity;
use super::NamedCardIndex;

const MAX_NAMED_TOKEN_TOKENS: usize = 20;

/// Resolves exact catalog names before `token(s)` and as reminder subjects.
///
/// The bounded context prevents ordinary unknown words from being promoted to
/// named tokens. When names overlap, the longest exact span wins.
pub(crate) fn resolve_named_token_spans(
    tokens: &mut [RecognizedEntity],
    index: &impl NamedCardIndex,
) {
    for noun_index in 0..tokens.len() {
        if !matches_token_noun(&tokens[noun_index].raw) {
            continue;
        }
        let first = noun_index.saturating_sub(MAX_NAMED_TOKEN_TOKENS);
        let best = (first..noun_index)
            .filter(|start| {
                index
                    .exact_name_ends(tokens, *start, noun_index)
                    .contains(&noun_index)
            })
            .min();
        let Some(start) = best else {
            continue;
        };
        for token in &mut tokens[start..noun_index] {
            token.category = "magicVocabulary".to_string();
            token.token_type = "namedTokenWord".to_string();
        }
    }
    resolve_reminder_subjects(tokens, index);
}

#[derive(Clone, Copy)]
enum ReminderSubjectState {
    Outside,
    Subject,
    Description,
    QuotedDescription,
}

/// A bare catalog name is meaningful only at a reminder description boundary,
/// followed by `is`. Embedded quoted rules and ordinary Oracle remain untouched.
/// This pass links identity only; the reminder grammar owns semantic validation.
fn resolve_reminder_subjects(tokens: &mut [RecognizedEntity], index: &impl NamedCardIndex) {
    let mut state = ReminderSubjectState::Outside;
    let mut depth = 0_usize;
    for position in 0..tokens.len() {
        match tokens[position].raw.as_str() {
            "\"" => {
                state = match state {
                    ReminderSubjectState::QuotedDescription if depth == 0 => {
                        ReminderSubjectState::Outside
                    }
                    ReminderSubjectState::QuotedDescription => ReminderSubjectState::Subject,
                    _ => ReminderSubjectState::QuotedDescription,
                };
                continue;
            }
            _ if matches!(state, ReminderSubjectState::QuotedDescription) => continue,
            "(" => {
                depth += 1;
                state = ReminderSubjectState::Subject;
                continue;
            }
            ")" => {
                depth = depth.saturating_sub(1);
                state = if depth == 0 {
                    ReminderSubjectState::Outside
                } else {
                    ReminderSubjectState::Description
                };
                continue;
            }
            "." if depth > 0 => {
                state = ReminderSubjectState::Subject;
                continue;
            }
            _ => {}
        }
        if !matches!(state, ReminderSubjectState::Subject) {
            continue;
        }
        let maximum_end = (position + MAX_NAMED_TOKEN_TOKENS).min(tokens.len());
        let end = index
            .exact_name_ends(tokens, position, maximum_end)
            .into_iter()
            .filter(|end| {
                tokens
                    .get(*end)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case("is"))
            })
            .max();
        if let Some(end) = end {
            for token in &mut tokens[position..end] {
                token.category = "magicVocabulary".to_string();
                token.token_type = "namedTokenWord".to_string();
            }
            state = ReminderSubjectState::Description;
        } else if !tokens[position].raw.eq_ignore_ascii_case("a")
            && !tokens[position].raw.eq_ignore_ascii_case("an")
        {
            state = ReminderSubjectState::Description;
        }
    }
}

fn matches_token_noun(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("token") || raw.eq_ignore_ascii_case("tokens")
}
