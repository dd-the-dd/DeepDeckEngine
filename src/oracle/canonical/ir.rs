use serde_json::{Value, json};

use super::CanonicalRuleDraft;

pub(super) fn draft(rule: Value, operations: &[&str]) -> CanonicalRuleDraft {
    let mut rule = rule;
    // A distributive `they` / `that player` names the player slot of the
    // primitive whose operand it is (`effects::player_set`).
    super::bind_pending_acting_players(&mut rule);
    CanonicalRuleDraft {
        operations: operations.iter().map(|value| value.to_string()).collect(),
        rule,
        construction: Vec::new(),
    }
}

pub(super) fn self_ref() -> Value {
    json!({ "kind": "self" })
}

pub(super) fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

pub(super) fn integer(value: i64) -> Value {
    json!({ "kind": "integer", "value": value })
}

pub(super) fn x_value() -> Value {
    json!({
        "id": "xValue",
        "kind": "chooseNumber",
        "minimum": 0,
    })
}

pub(super) fn card_type(value: &str) -> Value {
    json!({ "kind": "cardTypeContains", "value": value })
}

pub(super) fn subtype(value: &str) -> Value {
    json!({ "kind": "subtypeContains", "value": value })
}

pub(super) fn compare(operator: &str, left: Value, right: Value) -> Value {
    json!({
        "kind": "compare",
        "operator": operator,
        "left": left,
        "right": right,
    })
}

pub(super) fn or(operands: Vec<Value>) -> Value {
    json!({ "kind": "or", "operands": operands })
}

pub(super) fn and(operands: Vec<Value>) -> Value {
    json!({ "kind": "and", "operands": operands })
}

pub(super) fn not(operand: Value) -> Value {
    json!({ "kind": "not", "operand": operand })
}

/// Logical negation that keeps one IR per proposition: `not not P` is `P`,
/// and the controller-turn proposition has a primitive for each polarity
/// (`notYourTurn` and `duringControllerTurn` of the ability's controller), so
/// `unless it's your turn` and `if it's not your turn` lower identically.
pub(super) fn negation(operand: Value) -> Value {
    match operand["kind"].as_str() {
        Some("not") => operand["operand"].clone(),
        Some("notYourTurn") => json!({ "kind": "duringControllerTurn", "player": controller() }),
        Some("duringControllerTurn") if operand["player"] == controller() => {
            json!({ "kind": "notYourTurn" })
        }
        _ => not(operand),
    }
}

pub(super) fn selection(decision_id: &str, value: &str) -> Value {
    json!({
        "kind": "selectionContains",
        "selection": { "kind": "decisionResult", "decisionId": decision_id },
        "value": value,
    })
}
