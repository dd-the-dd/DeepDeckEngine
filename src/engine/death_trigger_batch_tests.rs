use super::rule_observes_permanent_death;
use serde_json::json;

#[test]
fn recognizes_nested_death_event_inside_one_of() {
    let rule = json!({
        "event": {
            "kind": "oneOf",
            "events": [
                { "kind": "cardDrawn" },
                {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "lifeGained" },
                        { "kind": "permanentDied" }
                    ]
                }
            ]
        }
    });

    assert!(rule_observes_permanent_death(&rule));
}

#[test]
fn ignores_rules_without_a_death_event() {
    let rule = json!({
        "event": {
            "kind": "oneOf",
            "events": [{ "kind": "cardDrawn" }, { "kind": "lifeGained" }]
        }
    });

    assert!(!rule_observes_permanent_death(&rule));
}
