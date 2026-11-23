//! The `another target` relation is resolved against the declaration: only
//! earlier slots of the same mode whose domain can overlap make it a
//! distinct-from relation; otherwise the slot keeps its source exclusion.
use super::compose_other_target_relation;
use serde_json::{Value, json};

fn slot(id: &str, kind: &str, condition: Option<&str>, other: bool) -> Value {
    let mut decision = json!({
        "kind": "chooseTargets",
        "id": id,
        "minimum": 1,
        "maximum": 1,
        "candidates": { "kind": kind },
    });
    if let Some(mode) = condition {
        decision["condition"] = json!({ "kind": "modeChosen", "mode": mode });
    }
    if other {
        decision["candidates"]["excludeSource"] = json!(true);
        decision["candidates"]["otherThanEarlierTargets"] = json!(true);
    }
    decision
}

fn rule(decisions: Vec<Value>) -> Value {
    json!({
        "kind": "spellAbility",
        "declaration": { "kind": "castingDeclaration", "decisions": decisions },
        "effects": [],
    })
}

#[test]
fn another_after_a_same_mode_overlapping_slot_is_distinct_from_it() {
    let mut composed = rule(vec![
        slot("first", "permanents", None, false),
        slot("second", "anyTarget", None, true),
    ]);
    compose_other_target_relation(&mut composed);
    let second = &composed["declaration"]["decisions"][1];
    assert_eq!(second["distinctFrom"], json!(["first"]));
    assert!(second["candidates"].get("excludeSource").is_none());
    assert!(!composed.to_string().contains("otherThanEarlierTargets"));
}

#[test]
fn another_in_another_mode_or_after_a_player_slot_names_only_the_source() {
    // Near misses: a mode-2 `another` when mode 1 declared a creature, and a
    // permanent `another` after a player slot (Stiltzkin).
    for decisions in [
        vec![
            slot("mode1:target", "permanents", Some("mode1"), false),
            slot("mode2:target", "permanents", Some("mode2"), true),
        ],
        vec![
            slot("targetOpponent", "players", None, false),
            slot("targetPermanent", "permanents", None, true),
        ],
    ] {
        let mut composed = rule(decisions);
        compose_other_target_relation(&mut composed);
        let later = &composed["declaration"]["decisions"][1];
        assert!(later.get("distinctFrom").is_none(), "{later}");
        assert_eq!(later["candidates"]["excludeSource"], true, "{later}");
        assert!(!composed.to_string().contains("otherThanEarlierTargets"));
    }
}

#[test]
fn a_nested_declaration_is_resolved_and_a_rule_without_one_gains_none() {
    let mut composed = json!({
        "kind": "triggeredAbility",
        "effects": [{
            "kind": "createReflexiveTrigger",
            "ability": rule(vec![
                slot("first", "permanents", None, false),
                slot("second", "permanents", None, true),
            ]),
        }],
    });
    compose_other_target_relation(&mut composed);
    assert!(composed.get("declaration").is_none());
    let nested = &composed["effects"][0]["ability"]["declaration"]["decisions"][1];
    assert_eq!(nested["distinctFrom"], json!(["first"]));
    assert!(!composed.to_string().contains("otherThanEarlierTargets"));
}
