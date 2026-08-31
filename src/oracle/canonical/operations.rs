use super::*;

/// An object selected by an operation is a choice, not necessarily a target.
pub(super) fn object_choice(id: &str, candidates: Value, quantity: Value) -> Value {
    json!({
        "id": id,
        "kind": "chooseObjects",
        "quantity": quantity,
        "candidates": candidates,
    })
}

pub(super) fn chosen_objects(id: &str) -> Value {
    json!({ "kind": "chosenObjects", "id": id })
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = text.get(..prefix.len())?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

fn rsplit_once_ignore_ascii_case<'a>(text: &'a str, separator: &str) -> Option<(&'a str, &'a str)> {
    let normalized = text.to_ascii_lowercase();
    let index = normalized.rfind(&separator.to_ascii_lowercase())?;
    Some((&text[..index], &text[index + separator.len()..]))
}

fn parse_controller_zone(text: &str) -> Option<Value> {
    match text.trim().to_ascii_lowercase().as_str() {
        "your graveyard" => Some(graveyard(controller())),
        "your hand" => Some(hand(controller())),
        "your library" => Some(library(controller())),
        "your exile" => Some(json!({ "kind": "exile", "player": controller() })),
        _ => None,
    }
}

fn parse_counted_card_description(text: &str, face_name: &str) -> Option<(Value, Value)> {
    let counted_cards_re = Regex::new(&format!(
        r"(?i)^({})(?: (.+?))? cards?$",
        count_word_pattern(),
    ))
    .expect("counted card-description regex compiles");
    let captures = counted_cards_re.captures(text.trim())?;
    let quantity = parse_exact_quantity(captures.get(1)?.as_str())?;
    let criteria = if let Some(criteria) = captures.get(2) {
        parse_permanent_criteria(criteria.as_str(), face_name)?
    } else {
        Value::Null
    };
    Some((quantity, criteria))
}

/// Parses the reusable operation tree behind
/// `Exile <quantity> <criteria> card(s) from <zone>`.
///
/// The caller decides whether this operation is a payment or a resolving
/// effect. That execution context is deliberately not encoded in `move`.
pub(super) fn parse_exile_cards_from_zone_operation(
    text: &str,
    face_name: &str,
    decision_id: &str,
) -> Option<(Value, Value)> {
    let selection_text = strip_prefix_ignore_ascii_case(text.trim(), "exile ")?;
    let (card_description, zone_description) =
        rsplit_once_ignore_ascii_case(selection_text, " from ")?;
    let from = parse_controller_zone(zone_description)?;
    let (quantity, where_filter) = parse_counted_card_description(card_description, face_name)?;
    let decision = object_choice(
        decision_id,
        json!({
            "kind": "cards",
            "zone": from.clone(),
            "where": where_filter,
        }),
        quantity,
    );
    let operation = json!({
        "kind": "move",
        "objects": chosen_objects(decision_id),
        "from": from,
        "to": { "kind": "exile" },
    });
    Some((operation, decision))
}
