use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn single_rule(name: &str, type_line: &str, text: &str) -> Result<Value, String> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let [ability] = result.abilities.as_slice() else {
        panic!("{name}: expected one ability");
    };
    ability.rule.clone().ok_or_else(|| {
        ability.diagnostics[0]
            .state
            .clone()
            .unwrap_or_else(|| ability.diagnostics[0].code.clone())
    })
}

fn attached() -> Value {
    json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}})
}

fn your_turn() -> Value {
    json!({"kind": "not", "operand": {"kind": "notYourTurn"}})
}

/// Real cards for each new predicate and the turn condition (CR 611.3a,
/// 508.1c, 509.1b, 502.3, 613.1f).
#[test]
fn new_static_predicates_lower_to_engine_modifiers() {
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Pacifism",
            "Enchantment — Aura",
            "Enchanted creature can't attack or block.",
            json!([
                {"kind": "grantKeyword", "objects": attached(), "keyword": "cantAttack"},
                {"kind": "grantKeyword", "objects": attached(), "keyword": "cantBlock"},
            ]),
        ),
        (
            "Dehydration",
            "Enchantment — Aura",
            "Enchanted creature doesn't untap during its controller's untap step.",
            json!([{"kind": "doesNotUntap", "objects": attached()}]),
        ),
        (
            "Brass Man",
            "Artifact Creature — Construct",
            "This creature doesn't untap during your untap step.",
            json!([{"kind": "doesNotUntap", "objects": {"kind": "self"}}]),
        ),
        (
            "Ancient Lumberknot",
            "Creature — Treefolk",
            "Each creature you control assigns combat damage equal to its toughness rather than its power.",
            json!([{"kind": "assignCombatDamageUsingToughness",
                "objects": {"kind": "permanents", "where": {"kind": "cardTypeContains", "value": "Creature"},
                    "controller": {"kind": "controllerOf", "object": {"kind": "self"}}},
                "onlyIfToughnessGreater": false}]),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        let rule =
            single_rule(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
        assert_eq!(&rule["modifiers"], expected, "{name}: {rule}");
        assert!(rule.get("condition").is_none(), "{name}");
    }

    let lance = single_rule(
        "Jousting Lance",
        "Artifact — Equipment",
        "During your turn, equipped creature has first strike.",
    )
    .expect("parses");
    assert_eq!(lance["condition"], your_turn());
    assert_eq!(
        lance["modifiers"],
        json!([{"kind": "grantKeyword", "objects": attached(), "keyword": "firstStrike"}])
    );
}

/// Unused combinations: a collection subject with a combat clause, an
/// opposite turn condition with a stat delta, and two new predicates joined.
#[test]
fn new_predicates_compose_with_existing_subjects_and_conditions() {
    let restricted = single_rule(
        "Generic Curfew",
        "Enchantment",
        "Creatures your opponents control can't block.",
    )
    .expect("parses");
    assert_eq!(restricted["modifiers"][0]["keyword"], "cantBlock");
    assert_eq!(
        restricted["modifiers"][0]["objects"]["controller"]["kind"],
        "opponentsOf"
    );

    let night = single_rule(
        "Generic Night Watch",
        "Creature — Human Soldier",
        "During turns other than yours, this creature gets +0/+2 and doesn't untap during your untap step.",
    )
    .expect("parses");
    assert_eq!(night["condition"], json!({"kind": "notYourTurn"}));
    assert_eq!(night["modifiers"][0]["kind"], "modifyPowerToughness");
    assert_eq!(night["modifiers"][1]["kind"], "doesNotUntap");

    let defender = single_rule(
        "Generic Wall Breaker",
        "Creature — Wall",
        "This creature loses all abilities and can attack as though it didn't have defender.",
    )
    .expect("parses");
    assert_eq!(defender["modifiers"][0]["kind"], "loseAllAbilities");
    assert_eq!(defender["modifiers"][1]["kind"], "allowAttackWithDefender");
}

/// Real cards: an Aura's control change (CR 613.1b), collection entry
/// modifications (CR 614.1c-d) and a block requirement (CR 509.1c).
#[test]
fn control_entry_and_block_requirement_predicates() {
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Control Magic",
            "Enchantment — Aura",
            "You control enchanted creature.",
            json!([{"kind": "controlAttachedPermanent", "attachment": {"kind": "self"}}]),
        ),
        (
            "Gaea's Protector",
            "Creature — Elemental Warrior",
            "This creature must be blocked if able.",
            json!([{"kind": "grantKeyword", "objects": {"kind": "self"}, "keyword": "mustBeBlockedIfAble"}]),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        let rule =
            single_rule(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
        assert_eq!(&rule["modifiers"], expected, "{name}: {rule}");
    }

    let tapped = single_rule(
        "Imposing Sovereign",
        "Creature — Human",
        "Creatures your opponents control enter tapped.",
    )
    .expect("parses");
    assert_eq!(tapped["modifiers"][0]["kind"], "permanentsEnterTapped");
    assert_eq!(
        tapped["modifiers"][0]["objects"]["controller"]["kind"],
        "opponentsOf"
    );

    let counters = single_rule(
        "Generic Mentor",
        "Creature — Human",
        "Other creatures you control enter with an additional +1/+1 counter on them.",
    )
    .expect("parses");
    let modifier = &counters["modifiers"][0];
    assert_eq!(modifier["kind"], "addEnteringCounters");
    assert_eq!(modifier["counter"], "+1/+1");
    assert_eq!(modifier["count"], json!({"kind": "integer", "value": 1}));
    assert_eq!(modifier["objects"]["excludeSource"], true);
}

#[test]
fn static_predicate_near_misses_fail_inside_the_frame() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "Enchantment — Aura",
            "Enchanted creature doesn't untap during your upkeep.",
            "static.continuous.untap-exclusion.step",
        ),
        (
            "Creature — Bird",
            "During your upkeep, this creature has flying.",
            "static.continuous.turn-condition",
        ),
        // The source's own entry is a replacement frame, not a collection.
        (
            "Enchantment — Aura",
            "Enchanted creature enters tapped.",
            "static.continuous.entry-modification",
        ),
        // The engine evaluates an attack requirement only on the source.
        (
            "Enchantment",
            "Each creature you control attacks each combat if able.",
            "static.combat-restriction.requirement",
        ),
    ];
    for (type_line, text, state) in cases {
        let actual = single_rule("Generic Static", type_line, text).expect_err(text);
        assert_eq!(actual, *state, "{text}");
    }
    // Only the enchanted permanent changes control statically; another
    // object is never approximated as the attached one.
    single_rule(
        "Generic Static",
        "Enchantment",
        "You control this enchantment.",
    )
    .expect_err("a non-attached control object");
}
