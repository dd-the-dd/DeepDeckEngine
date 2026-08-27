use serde_json::{Value, json};

use super::CanonicalRuleDraft;

pub(super) fn draft(rule: Value, operations: &[&str]) -> CanonicalRuleDraft {
    CanonicalRuleDraft {
        operations: operations.iter().map(|value| value.to_string()).collect(),
        rule,
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

pub(super) fn selection(decision_id: &str, value: &str) -> Value {
    json!({
        "kind": "selectionContains",
        "selection": { "kind": "decisionResult", "decisionId": decision_id },
        "value": value,
    })
}

pub(super) fn selection_count_at_least(decision_id: &str, value: &str, minimum: i64) -> Value {
    json!({
        "kind": "selectionCountAtLeast",
        "selection": { "kind": "decisionResult", "decisionId": decision_id },
        "value": value,
        "minimum": integer(minimum),
    })
}
