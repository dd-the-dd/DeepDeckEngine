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
        if is_continuation_line(line)
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
