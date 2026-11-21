use crate::oracle::model::RecognizedEntity;
use serde_json::Value;

pub(crate) fn is_class_type_line(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("Class"))
}

pub(crate) fn class_level_header_from_tokens(tokens: &[RecognizedEntity]) -> Option<i64> {
    let mut position = 0;
    while tokens
        .get(position)
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        position += 1;
    }
    if position == 0 {
        return None;
    }

    let colon = tokens.get(position)?;
    if colon.token_type != "separator" || colon.raw != ":" {
        return None;
    }
    position += 1;

    if !tokens
        .get(position)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("level"))
    {
        return None;
    }
    position += 1;

    let level_token = tokens.get(position)?;
    if level_token.token_type != "quantity" {
        return None;
    }
    let level = level_token.raw.parse::<i64>().ok()?;
    if level < 2 {
        return None;
    }
    position += 1;

    (position == tokens.len()).then_some(level)
}

pub(crate) fn apply_class_level_requirement(rule: &mut Value, minimum_level: i64) {
    if minimum_level <= 1 {
        return;
    }
    let existing = rule["minimumClassLevel"].as_i64().unwrap_or(1);
    rule["minimumClassLevel"] = Value::from(existing.max(minimum_level));
}

#[cfg(test)]
#[path = "class_tests.rs"]
mod tests;
