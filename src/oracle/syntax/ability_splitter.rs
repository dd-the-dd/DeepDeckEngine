use super::super::model::{AbilitySource, OracleCardParseRequest};
use super::normalize::normalize_oracle_encoding;

pub(crate) struct OracleDocument<'a> {
    pub face_id: Option<&'a str>,
    pub face_name: &'a str,
    pub face_type_line: &'a str,
    oracle_text: &'a str,
}

pub(crate) fn documents(request: &OracleCardParseRequest) -> Vec<OracleDocument<'_>> {
    if request.faces.is_empty() {
        return request
            .oracle_text
            .as_deref()
            .map(|oracle_text| {
                vec![OracleDocument {
                    face_id: None,
                    face_name: &request.card_name,
                    face_type_line: &request.type_line,
                    oracle_text,
                }]
            })
            .unwrap_or_default();
    }

    request
        .faces
        .iter()
        .map(|face| OracleDocument {
            face_id: Some(face.id.as_str()),
            face_name: &face.name,
            face_type_line: &face.type_line,
            oracle_text: &face.oracle_text,
        })
        .collect()
}

fn is_continuation_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('\u{2022}') || trimmed.starts_with("+ ")
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
