use serde_json::Value;

use super::parse_numeric_expression_text;

pub(super) fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then(|| &value[prefix.len()..])
}

pub(super) fn strip_suffix_ascii_case<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
    let start = value.len().checked_sub(suffix.len())?;
    value
        .get(start..)
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(suffix))
        .then(|| &value[..start])
}

pub(super) fn split_once_ascii_case<'a>(
    value: &'a str,
    separator: &str,
) -> Option<(&'a str, &'a str)> {
    let index = value
        .to_ascii_lowercase()
        .find(&separator.to_ascii_lowercase())?;
    Some((&value[..index], &value[index + separator.len()..]))
}

pub(super) fn parse_quantity_prefix(value: &str) -> Option<(Value, &str)> {
    let value = value.trim();
    let mut boundaries = value
        .char_indices()
        .filter_map(|(index, character)| character.is_whitespace().then_some(index))
        .collect::<Vec<_>>();
    boundaries.reverse();
    boundaries.into_iter().find_map(|boundary| {
        let quantity = value[..boundary].trim();
        let description = value[boundary..].trim();
        (!description.is_empty())
            .then(|| Some((parse_numeric_expression_text(quantity)?, description)))?
    })
}
