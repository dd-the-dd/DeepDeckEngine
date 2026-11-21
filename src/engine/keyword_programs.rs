//! Keyword definitions compile to ordinary reusable runtime instructions.

use serde_json::{Value, json};

/// The source mana value is captured from the resulting spell at cast time,
/// including its chosen X. Each trigger owns its rule and transient bindings;
/// resolving or countering the source cannot change the captured threshold.
pub(super) fn cascade_trigger_rule(source_mana_value: i32) -> Value {
    let controller = json!({ "kind": "controllerOf", "object": { "kind": "self" } });
    let below_source = json!({
        "kind": "compare",
        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
        "operator": "<",
        "right": { "kind": "integer", "value": source_mana_value },
    });
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "effects": [
            {
                "kind": "exileFromTopUntil",
                "zone": { "kind": "library", "player": controller.clone() },
                "bind": "cascadeExiled",
                "stopCardBind": "cascadeHit",
                "faceDown": false,
                "stopWhere": {
                    "kind": "and",
                    "operands": [
                        { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } },
                        below_source.clone(),
                    ],
                },
                "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
            },
            {
                "kind": "castOneCard",
                "player": controller.clone(),
                "cards": { "kind": "boundObjects", "binding": "cascadeHit" },
                "sourceZone": "exile",
                "normalCostsApply": false,
                "optional": true,
                "spellWhere": below_source,
                "bind": "cascadeCast",
            },
            {
                "kind": "moveCards",
                "cards": { "kind": "boundObjects", "binding": "cascadeExiled" },
                "from": { "kind": "exile", "player": controller.clone() },
                "to": { "kind": "library", "player": controller, "position": "bottom" },
                "order": { "kind": "random" },
            },
        ],
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "keyword_programs_tests.rs"]
mod tests;
