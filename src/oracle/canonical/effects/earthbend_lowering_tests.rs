use super::{earthbend_target_id, lower_earthbend_action};
use crate::oracle::canonical::{EarthbendActionNode, NumericAtom};
use serde_json::json;

#[test]
fn effect_index_allocates_distinct_target_bindings() {
    let action = EarthbendActionNode {
        quantity: NumericAtom::Integer(2),
    };
    let first_id = earthbend_target_id(0);
    let second_id = earthbend_target_id(1);
    let first = lower_earthbend_action(&action, &first_id, json!({ "kind": "permanents" }), 1, 1);
    let second = lower_earthbend_action(&action, &second_id, json!({ "kind": "permanents" }), 1, 1);

    assert_ne!(first.decision["id"], second.decision["id"]);
    assert_eq!(first.effects[0]["object"]["id"], first.decision["id"]);
    assert_eq!(second.effects[0]["object"]["id"], second.decision["id"]);
}
