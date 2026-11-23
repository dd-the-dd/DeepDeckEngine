use serde_json::{Value, json};

use super::super::{CardDefinition, card_instance};
use super::{band_gate_open, band_gate_supported};

fn leveler(level_counters: i32) -> super::super::CardInstance {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id": "leveler", "name": "Leveler", "typeLine": "Creature",
        "manaCost": "{1}{U}", "power": "0", "toughness": "3", "rules": [],
    }))
    .unwrap();
    let mut card = card_instance("player-0", &definition, 0);
    if level_counters > 0 {
        card.counters.insert("level".to_string(), level_counters);
    }
    card
}

fn gated(kind: &str, gate: Value) -> Value {
    json!({ "kind": kind, "source": { "kind": "self" }, "bandGate": gate })
}

fn level(minimum: i32, maximum: Option<i32>) -> Value {
    let mut gate = json!({ "kind": "counterRange", "counter": "level", "minimum": minimum });
    if let Some(maximum) = maximum {
        gate["maximum"] = json!(maximum);
    }
    gate
}

#[test]
fn a_level_range_opens_only_between_its_bounds() {
    // CR 711.2a: LEVEL 1-4 applies with at least one and at most four counters.
    let rule = gated("keywordAbility", level(1, Some(4)));
    assert!(!band_gate_open(&leveler(0), &rule));
    assert!(band_gate_open(&leveler(1), &rule));
    assert!(band_gate_open(&leveler(4), &rule));
    assert!(!band_gate_open(&leveler(5), &rule));
    // CR 711.2b: LEVEL 5+ has no upper bound.
    let open_ended = gated("staticAbility", level(5, None));
    assert!(!band_gate_open(&leveler(4), &open_ended));
    assert!(band_gate_open(&leveler(9), &open_ended));
    // An ungated rule is unaffected.
    assert!(band_gate_open(
        &leveler(0),
        &json!({ "kind": "staticAbility" })
    ));
}

#[test]
fn a_gate_is_supported_only_where_the_runtime_reads_it() {
    assert!(band_gate_supported(&gated(
        "staticAbility",
        level(1, Some(4))
    )));
    assert!(band_gate_supported(&gated(
        "triggeredAbility",
        level(5, None)
    )));
    // A spell ability never reads its source's band.
    assert!(!band_gate_supported(&gated("spellAbility", level(1, None))));
    // A hand activation does not consult battlefield metadata.
    let mut hand = gated("activatedAbility", level(1, None));
    hand["activationZone"] = json!("hand");
    assert!(!band_gate_supported(&hand));
    // Malformed bounds and the unproduced solved designation are refused.
    assert!(!band_gate_supported(&gated(
        "staticAbility",
        level(4, Some(1))
    )));
    assert!(!band_gate_supported(&gated(
        "staticAbility",
        json!({ "kind": "designation", "designation": "solved" }),
    )));
    assert!(!band_gate_open(
        &leveler(3),
        &gated(
            "staticAbility",
            json!({ "kind": "designation", "designation": "solved" })
        ),
    ));
}
