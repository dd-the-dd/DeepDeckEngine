use serde_json::{Value, json};

use super::{controller, not, parse_numeric_expression_text};

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

pub(super) fn parse_graveyard_object_filter_prefix(value: &str) -> Option<(Value, &str)> {
    let value = strip_prefix_ascii_case(value, "a ")?;
    for (phrase, filter) in [
        ("card or token", Value::Null),
        ("card", not(json!({ "kind": "isToken" }))),
        ("token", json!({ "kind": "isToken" })),
    ] {
        if let Some(remainder) = strip_prefix_ascii_case(value, phrase) {
            return Some((filter, remainder));
        }
    }
    None
}

pub(super) fn parse_graveyard_owner_scope_prefix(value: &str) -> Option<(Value, &str)> {
    for (phrase, owners) in [
        (
            "an opponent's graveyard",
            json!({
                "kind": "opponentsOf",
                "player": controller(),
            }),
        ),
        ("your graveyard", controller()),
        ("a graveyard", json!({ "kind": "eachPlayer" })),
    ] {
        if let Some(remainder) = strip_prefix_ascii_case(value, phrase) {
            return Some((owners, remainder));
        }
    }
    None
}

pub(super) fn parse_zone_origin_prefix(value: &str) -> Option<(Value, &str)> {
    strip_prefix_ascii_case(value, "from anywhere")
        .map(|remainder| (json!({ "kind": "anyZone" }), remainder))
}

pub(super) fn parse_zone_replacement_destination_prefix(value: &str) -> Option<(Value, &str)> {
    strip_prefix_ascii_case(value, "exile it instead")
        .map(|remainder| (json!({ "kind": "exile" }), remainder))
}

pub(super) fn normalize_ability_class(label: &str) -> Option<String> {
    let mut words = label.split('-').filter(|word| !word.is_empty());
    let first = words.next()?.to_ascii_lowercase();
    Some(words.fold(first, |mut class, word| {
        let mut characters = word.chars();
        if let Some(first) = characters.next() {
            class.extend(first.to_uppercase());
            class.extend(characters.flat_map(char::to_lowercase));
        }
        class
    }))
}
