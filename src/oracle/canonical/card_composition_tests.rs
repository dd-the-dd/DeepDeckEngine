//! The `another target` relation is resolved against the declaration: only
//! earlier slots of the same mode whose domain and type head can overlap make it a
//! distinct-from relation; otherwise the slot keeps its source exclusion.
use super::{compose_other_target_relation, compose_other_target_relation_for_source};
use crate::oracle::canonical::automata::type_line_type_heads;
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

fn triggered(decisions: Vec<Value>) -> Value {
    let mut composed = rule(decisions);
    composed["kind"] = json!("triggeredAbility");
    composed
}

fn typed(mut decision: Value, where_: Value) -> Value {
    decision["candidates"]["where"] = where_;
    decision
}

fn card_type(value: &str) -> Value {
    json!({ "kind": "cardTypeContains", "value": value })
}

#[test]
fn another_contrasts_with_an_earlier_slot_only_through_a_shared_type_head() {
    let attacking_creature = json!({ "kind": "and", "operands": [
        card_type("Creature"), { "kind": "isAttacking" },
    ] });
    // Amy Rose: `attach up to one target Equipment to her. Then up to one
    // other target attacking creature ...`; Rhino/Jaheira/Kinetic Ooze: an
    // earlier artifact, enchantment or land slot. Neither head is a kind of
    // creature, so `other` then names only the source.
    for earlier in [
        json!({ "kind": "subtypeContains", "value": "Equipment" }),
        json!({ "kind": "or", "operands": [card_type("Artifact"), card_type("Enchantment")] }),
    ] {
        let first = typed(slot("first", "permanents", None, false), earlier);
        let decisions = vec![
            first,
            typed(
                slot("second", "permanents", None, true),
                attacking_creature.clone(),
            ),
        ];
        let mut composed = triggered(decisions.clone());
        compose_other_target_relation(&mut composed);
        let later = &composed["declaration"]["decisions"][1];
        assert!(later.get("distinctFrom").is_none(), "{later}");
        assert_eq!(later["candidates"]["excludeSource"], true, "{later}");
        // A spell is no permanent target: its `another` contrasts with the
        // earlier slot whatever its type heads.
        let mut composed = rule(decisions);
        compose_other_target_relation(&mut composed);
        let later = &composed["declaration"]["decisions"][1];
        assert_eq!(later["distinctFrom"], json!(["first"]), "{later}");
    }
    // An unused combination: an earlier `creature or planeswalker` slot
    // shares the creature head, so `another target creature` contrasts with it.
    let mut composed = triggered(vec![
        typed(
            slot("first", "permanents", None, false),
            json!({ "kind": "or", "operands": [card_type("Creature"), card_type("Planeswalker")] }),
        ),
        typed(
            slot("second", "permanents", None, true),
            card_type("Creature"),
        ),
    ]);
    compose_other_target_relation(&mut composed);
    let later = &composed["declaration"]["decisions"][1];
    assert_eq!(later["distinctFrom"], json!(["first"]), "{later}");
    assert!(
        later["candidates"].get("excludeSource").is_none(),
        "{later}"
    );
}

fn subtype(value: &str) -> Value {
    json!({ "kind": "subtypeContains", "value": value })
}

/// The later slot of a composed triggered declaration `first`, `second`.
fn later_after(earlier: Value, later: Value, source_type_line: Option<&str>) -> Value {
    let mut composed = triggered(vec![
        typed(slot("first", "permanents", None, false), earlier),
        typed(slot("second", "permanents", None, true), later),
    ]);
    let heads = source_type_line.map(type_line_type_heads);
    compose_other_target_relation_for_source(&mut composed, heads.as_deref());
    composed["declaration"]["decisions"][1].clone()
}

#[test]
fn another_contrasts_with_an_earlier_slot_whose_head_is_a_kind_of_its_own() {
    // The shared type hierarchy (CR 205.3d, 110.4): a subtype is a kind of
    // its card type and every permanent type a kind of permanent.
    // Itzquinth: `target Dinosaur you control ... another target creature`;
    // `target Aura ... another target enchantment`; `target Equipment ...
    // another target artifact`; a bare earlier `target permanent`.
    for (earlier, later, source) in [
        (
            subtype("Dinosaur"),
            card_type("Creature"),
            "Legendary Creature \u{2014} Dinosaur",
        ),
        (
            subtype("Aura"),
            card_type("Enchantment"),
            "Enchantment \u{2014} Aura",
        ),
        (
            subtype("Equipment"),
            card_type("Artifact"),
            "Artifact Creature \u{2014} Golem",
        ),
        (Value::Null, card_type("Creature"), "Creature \u{2014} Elf"),
        // The reverse direction: an earlier creature, a later Dinosaur.
        (
            card_type("Creature"),
            subtype("Dinosaur"),
            "Creature \u{2014} Dinosaur",
        ),
    ] {
        let later = later_after(earlier.clone(), later.clone(), Some(source));
        assert_eq!(
            later["distinctFrom"],
            json!(["first"]),
            "{earlier} / {later}"
        );
        assert!(
            later["candidates"].get("excludeSource").is_none(),
            "{later}"
        );
    }
}

#[test]
fn a_source_the_later_slot_cannot_name_leaves_only_the_earlier_slots() {
    // The Curse of Fenric III on a Saga: `another target creature` cannot
    // name the Saga, so it contrasts with the earlier `target Mutant` (and
    // with an unrelated earlier head too, `target artifact`).
    for earlier in [subtype("Mutant"), card_type("Artifact")] {
        let later = later_after(
            earlier.clone(),
            card_type("Creature"),
            Some("Enchantment \u{2014} Saga"),
        );
        assert_eq!(
            later["distinctFrom"],
            json!(["first"]),
            "{earlier} / {later}"
        );
    }
    // Near miss: a creature source that the later slot can name keeps its
    // source contrast after an unrelated earlier head (Kinetic Ooze).
    let later = later_after(
        card_type("Artifact"),
        card_type("Creature"),
        Some("Creature \u{2014} Ooze"),
    );
    assert!(later.get("distinctFrom").is_none(), "{later}");
    assert_eq!(later["candidates"]["excludeSource"], true, "{later}");
}

#[test]
fn type_heads_are_read_from_the_candidate_itself_not_its_relations() {
    // A negated type and a comparison operand name no head of the
    // candidate: `nonartifact creature` is not an artifact head, and
    // `power less than the number of Wizards you control` names no Wizard.
    let earlier = json!({ "kind": "and", "operands": [
        { "kind": "not", "operand": card_type("Artifact") },
        card_type("Enchantment"),
        { "kind": "compare", "operator": "<", "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
          "right": { "kind": "countPermanents", "where": card_type("Artifact") } },
    ] });
    let later = later_after(
        earlier,
        card_type("Artifact"),
        Some("Artifact Creature \u{2014} Golem"),
    );
    assert!(later.get("distinctFrom").is_none(), "{later}");
    assert_eq!(later["candidates"]["excludeSource"], true, "{later}");
}

#[test]
fn a_kicked_another_is_distinct_from_the_slot_every_cast_declares() {
    // Jilt: `Return target creature ... If this spell was kicked, it deals 2
    // damage to another target creature` (CR 115.3): the unconditional slot
    // is declared whenever the kicked one is.
    let kicked = json!({ "kind": "wasKicked", "spell": { "kind": "self" } });
    let mut gated = slot("second", "permanents", None, true);
    gated["condition"] = kicked.clone();
    let mut composed = rule(vec![slot("first", "permanents", None, false), gated]);
    compose_other_target_relation(&mut composed);
    let later = &composed["declaration"]["decisions"][1];
    assert_eq!(later["distinctFrom"], json!(["first"]), "{later}");
    assert!(
        later["candidates"].get("excludeSource").is_none(),
        "{later}"
    );

    // Near miss: a gated earlier slot is not declared on every cast, so an
    // unconditional `another` keeps only its source exclusion.
    let mut earlier = slot("first", "permanents", None, false);
    earlier["condition"] = kicked;
    let mut composed = rule(vec![earlier, slot("second", "permanents", None, true)]);
    compose_other_target_relation(&mut composed);
    let later = &composed["declaration"]["decisions"][1];
    assert!(later.get("distinctFrom").is_none(), "{later}");
    assert_eq!(later["candidates"]["excludeSource"], true, "{later}");
}
