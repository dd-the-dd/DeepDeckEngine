use super::super::card_structure::CardStructurePlan;
use super::super::model::{AbilitySource, OracleCardParseRequest};
use super::normalize::normalize_oracle_encoding;

pub(crate) struct OracleDocument<'a> {
    pub face_id: Option<&'a str>,
    pub face_name: &'a str,
    pub face_type_line: &'a str,
    oracle_text: &'a str,
}

pub(crate) fn documents<'a>(
    request: &'a OracleCardParseRequest,
    structure: &CardStructurePlan,
) -> Vec<OracleDocument<'a>> {
    structure
        .parts
        .iter()
        .filter_map(|part| match part.face_index {
            Some(index) => {
                let face = &request.faces[index];
                Some(OracleDocument {
                    face_id: Some(face.id.as_str()),
                    face_name: &face.name,
                    face_type_line: &face.type_line,
                    oracle_text: &face.oracle_text,
                })
            }
            None => request
                .oracle_text
                .as_deref()
                .map(|oracle_text| OracleDocument {
                    face_id: None,
                    face_name: &request.card_name,
                    face_type_line: &request.type_line,
                    oracle_text,
                }),
        })
        .collect()
}

fn is_continuation_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('\u{2022}') || trimmed.starts_with("+ ")
}

/// The ability text names a die (`d20`, `six-sided`): the rows that follow
/// it are its results table, not separate abilities. A station threshold
/// row (`12+ | ...`) follows no die and keeps its own ability.
fn names_a_die(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        let word =
            word.trim_matches(|character: char| !character.is_alphanumeric() && character != '-');
        word.to_ascii_lowercase().ends_with("-sided")
            || word.strip_prefix('d').is_some_and(|faces| {
                !faces.is_empty() && faces.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

/// CR 706.3: a die roll's results table prints one row per line (`1—9 | ...`,
/// `20 | ...`, `15+ | ...`, `9 or less | ...`, `6 — ...`). The rows belong to
/// the ability that rolls the die; the effect grammar reads each row as the
/// conditional instruction of its results.
fn is_die_result_row_line(line: &str) -> bool {
    fn after_digits(text: &str) -> Option<&str> {
        let count = text.bytes().take_while(u8::is_ascii_digit).count();
        (count > 0).then(|| &text[count..])
    }
    let Some(rest) = after_digits(line) else {
        return false;
    };
    let rest = if let Some(after) = rest
        .strip_prefix('\u{2014}')
        .or_else(|| rest.strip_prefix('-'))
        .and_then(after_digits)
    {
        after
    } else if let Some(after) = rest.strip_prefix('+') {
        after
    } else if let Some(after) = rest.strip_prefix(" or ") {
        let Some(after) = ["less", "lower", "more", "higher", "greater"]
            .iter()
            .find_map(|word| after.strip_prefix(word))
            .or_else(|| after_digits(after))
        else {
            return false;
        };
        after
    } else {
        rest
    };
    let rest = rest.trim_start();
    rest.starts_with('|') || rest.starts_with('\u{2014}')
}

/// CR 207.2: reminder text has no rules meaning. A line that is exactly one
/// balanced parenthetical therefore carries no ability of its own. Text that
/// follows real ability text is not covered: it stays with that ability.
pub(crate) fn is_reminder_only_line(text: &str) -> bool {
    let text = text.trim();
    if !text.starts_with('(') || !text.ends_with(')') {
        return false;
    }
    let mut depth = 0_usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return false,
                };
                if depth == 0 && index + ch.len_utf8() != text.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

pub(crate) fn split_abilities(document: &OracleDocument<'_>) -> Vec<AbilitySource> {
    let lines = normalize_oracle_encoding(document.oracle_text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut abilities: Vec<AbilitySource> = Vec::new();
    for (line_index, line) in lines.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if (is_continuation_line(line)
            || (is_die_result_row_line(line.trim_start())
                && abilities
                    .last()
                    .is_some_and(|previous| names_a_die(&previous.text))))
            && let Some(previous) = abilities.last_mut()
        {
            previous.text.push('\n');
            previous.text.push_str(line);
            previous.line_end = line_index;
            continue;
        }
        abilities.push(AbilitySource {
            face_id: document.face_id.map(ToOwned::to_owned),
            text: line.to_string(),
            line_start: line_index,
            line_end: line_index,
        });
    }
    abilities
}
