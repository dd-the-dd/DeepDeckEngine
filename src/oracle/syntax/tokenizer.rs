use regex::Regex;

use super::super::model::RecognizedEntity;

fn is_quantity_token(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase().replace('-', " ");
    if normalized == "x" || normalized.parse::<i64>().is_ok() {
        return true;
    }
    let mut words = normalized.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let first_is_quantity = matches!(
        first,
        "a" | "an"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
    );
    if !first_is_quantity {
        return false;
    }
    let Some(second) = words.next() else {
        return true;
    };
    words.next().is_none()
        && matches!(
            second,
            "one" | "two" | "three" | "four" | "five" | "six" | "seven" | "eight" | "nine"
        )
}

fn token_type(raw: &str) -> (&'static str, &'static str) {
    let normalized = raw.to_ascii_lowercase();
    if raw.starts_with('{') {
        if normalized == "{t}" || normalized == "{q}" {
            return ("cost", "tapSymbol");
        }
        return ("cost", "manaSymbol");
    }
    if ["when", "whenever", "at"].contains(&normalized.as_str()) {
        return ("trigger", "triggerWord");
    }
    if ["enter", "enters", "entered", "attacks"].contains(&normalized.as_str()) {
        return ("trigger", "eventWord");
    }
    if ["if", "unless", "and", "or", "not", "then", "instead"].contains(&normalized.as_str()) {
        return ("logic", "logicWord");
    }
    if ["this", "that", "it", "its", "you", "your", "they", "their"].contains(&normalized.as_str())
    {
        return ("entityReference", "referenceWord");
    }
    if normalized == "target" {
        return ("entityReference", "targetMarker");
    }
    if [
        "artifact",
        "battle",
        "card",
        "creature",
        "enchantment",
        "instant",
        "land",
        "permanent",
        "planeswalker",
        "player",
        "sorcery",
        "spell",
    ]
    .contains(&normalized.as_str())
    {
        return ("entityReference", "typeWord");
    }
    if [
        "add", "cast", "copy", "counter", "create", "deal", "deals", "discard", "draw", "exile",
        "gain", "look", "lose", "mill", "play", "put", "return", "reveal", "search", "shuffle",
    ]
    .contains(&normalized.as_str())
    {
        return ("magicVocabulary", "actionWord");
    }
    if raw == ":" || raw == "," || raw == "." || raw == "\u{2022}" || raw == "+" {
        return ("boundary", "separator");
    }
    if is_quantity_token(raw) {
        return ("logic", "quantity");
    }
    ("vocabulary", "word")
}

pub(crate) fn recognize_entities(text: &str) -> Vec<RecognizedEntity> {
    let token_re =
        Regex::new(r"\{[^}]+\}|\u{2022}|\+|[.,:;]|[+-]?\d+/[+-]?\d+|[A-Za-z0-9][A-Za-z0-9'/-]*")
            .expect("entity token regex compiles");
    token_re
        .find_iter(text)
        .enumerate()
        .map(|(index, matched)| {
            let raw = matched.as_str();
            let (category, entity_type) = token_type(raw);
            RecognizedEntity {
                category: category.to_string(),
                index,
                raw: raw.to_string(),
                token_type: entity_type.to_string(),
            }
        })
        .collect()
}
