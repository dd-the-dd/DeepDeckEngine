use serde_json::{Value, json};

use super::self_ref;

pub(in crate::oracle::canonical) fn active_while_battlefield() -> Value {
    json!({
        "kind": "inZone",
        "object": self_ref(),
        "zone": { "kind": "battlefield" },
    })
}

pub(in crate::oracle::canonical) fn replace_decision_ids(
    value: &mut Value,
    replacements: &[(String, String)],
) {
    match value {
        Value::Array(values) => {
            for value in values {
                replace_decision_ids(value, replacements);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                replace_decision_ids(value, replacements);
            }
        }
        Value::String(value) => {
            if let Some((_, replacement)) =
                replacements.iter().find(|(original, _)| original == value)
            {
                *value = replacement.clone();
            }
        }
        _ => {}
    }
}

pub(in crate::oracle::canonical) fn minimum(operands: Vec<Value>) -> Value {
    json!({ "kind": "minimum", "operands": operands })
}

pub(in crate::oracle::canonical) fn hand(player: Value) -> Value {
    json!({ "kind": "hand", "player": player })
}

pub(in crate::oracle::canonical) fn chosen_creature_type() -> Value {
    json!({
        "kind": "chosenCreatureType",
        "decisionId": "chosenCreatureType",
    })
}

pub(super) fn object_choice(id: &str, candidates: Value, quantity: Value) -> Value {
    json!({
        "id": id,
        "kind": "chooseObjects",
        "quantity": quantity,
        "candidates": candidates,
    })
}
