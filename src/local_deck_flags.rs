//! Interpret both the section-based export and legacy Boolean deck storage.
use serde_json::Value;

fn flag(card: &Value, selected: &Value, key: &str) -> bool {
    card.get(key).and_then(Value::as_bool).unwrap_or(false)
        || selected.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn is_sideboard(card: &Value, selected: &Value) -> bool {
    let section = card
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(section.as_str(), "sideboard" | "companion")
        || flag(card, selected, "isSideboard")
        || flag(card, selected, "isCompanion")
}

pub(crate) fn is_auxiliary(card: &Value, selected: &Value) -> bool {
    let type_line = selected
        .get("typeLine")
        .or_else(|| selected.get("type_line"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let layout = selected
        .get("layout")
        .and_then(Value::as_str)
        .unwrap_or_default();
    flag(card, selected, "isGamePiece")
        || flag(card, selected, "isToken")
        || matches!(
            layout,
            "token" | "double_faced_token" | "emblem" | "art_series"
        )
        || matches!(
            type_line.as_str(),
            "card" | "counter" | "marker" | "checklist" | "helper"
        )
        || ["token", "emblem", "dungeon", "sticker", "art card"]
            .iter()
            .any(|prefix| type_line == *prefix || type_line.starts_with(&format!("{prefix} ")))
}
