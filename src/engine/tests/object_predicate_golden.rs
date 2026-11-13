use super::*;
use crate::engine::object_predicate::{DefinitionView, matches_object_predicate};

fn keyword(kind: &str) -> Value {
    json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": kind },
    })
}

fn candidate(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    power: Option<&str>,
    toughness: Option<&str>,
) -> CardDefinition {
    let mut definition = test_definition(name, type_line);
    definition.name = name.to_string();
    definition.mana_cost = mana_cost.to_string();
    definition.power = power.map(ToOwned::to_owned);
    definition.toughness = toughness.map(ToOwned::to_owned);
    definition
}

fn candidate_property(kind: &str) -> Value {
    json!({
        "kind": kind,
        "object": { "kind": "candidate" },
    })
}

fn comparison(left: &str, operator: &str, right: Value) -> Value {
    json!({
        "kind": "compare",
        "left": candidate_property(left),
        "operator": operator,
        "right": right,
    })
}

#[test]
fn definition_view_preserves_atomic_and_boolean_predicate_golden_semantics() {
    let mut card = candidate(
        "Mirror Entity",
        "Legendary Artifact Creature - Shapeshifter",
        "{2}{W}",
        Some("1"),
        Some("1"),
    );
    card.rules = vec![
        keyword("changeling"),
        keyword("flying"),
        json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [],
            "effects": [],
        }),
    ];

    let positives = [
        json!({ "kind": "cardTypeContains", "value": "Creature" }),
        json!({ "kind": "subtypeContains", "value": "Rogue" }),
        json!({ "kind": "typeLineContains", "value": "artifact creature" }),
        json!({ "kind": "nameEquals", "value": "mirror entity" }),
        json!({ "kind": "nameStartsWith", "value": "MIRROR" }),
        json!({ "kind": "isLegendary" }),
        json!({ "kind": "isPermanentCard" }),
        json!({ "kind": "totalPowerAndToughnessAtMost", "value": 2 }),
        json!({ "kind": "hasActivatedAbility" }),
        json!({ "kind": "hasKeyword", "value": "flying" }),
        json!({ "kind": "manaCostEquals", "value": "{2}{w}" }),
        json!({ "kind": "historic" }),
        json!({ "kind": "colorContains", "value": "white" }),
        comparison(
            "manaValueOf",
            "==",
            json!({ "kind": "integer", "value": 3 }),
        ),
        comparison("colorCountOf", ">=", json!(1)),
        comparison("powerOf", "<=", candidate_property("toughnessOf")),
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Creature" },
                {
                    "kind": "or",
                    "operands": [
                        { "kind": "subtypeContains", "value": "Rogue" },
                        { "kind": "cardTypeContains", "value": "Land" }
                    ]
                },
                { "kind": "not", "operand": { "kind": "isToken" } }
            ]
        }),
        Value::Null,
    ];
    for predicate in positives {
        assert!(
            matches_object_predicate(DefinitionView::new(&card), &predicate),
            "expected predicate to match: {predicate}"
        );
    }

    let negatives = [
        json!({ "kind": "cardTypeContains", "value": "Land" }),
        json!({ "kind": "typeLineContains", "value": "Planeswalker" }),
        json!({ "kind": "nameEquals", "value": "Other Entity" }),
        json!({ "kind": "isToken" }),
        json!({ "kind": "isBasic" }),
        json!({ "kind": "manaCostContainsX" }),
        json!({ "kind": "colorContains", "value": "blue" }),
        comparison("manaValueOf", "<", json!(3)),
        comparison("powerOf", ">", candidate_property("toughnessOf")),
        json!({ "kind": "or", "operands": [] }),
        json!({ "kind": "unsupportedPredicate" }),
    ];
    for predicate in negatives {
        assert!(
            !matches_object_predicate(DefinitionView::new(&card), &predicate),
            "expected predicate not to match: {predicate}"
        );
    }
}

#[test]
fn definition_view_keeps_unrelated_legacy_edge_semantics_unchanged() {
    let basic_land = candidate("Island", "Basic Land - Island", "", None, None);
    assert!(matches_object_predicate(
        DefinitionView::new(&basic_land),
        &json!({ "kind": "isBasic" })
    ));
    assert!(matches_object_predicate(
        DefinitionView::new(&basic_land),
        &json!({ "kind": "isPermanentCard" })
    ));

    let mut devoid = candidate(
        "Devoid Candidate",
        "Creature - Eldrazi",
        "{2}{U}",
        Some("*"),
        Some("3"),
    );
    devoid.rules.push(keyword("devoid"));
    assert!(matches_object_predicate(
        DefinitionView::new(&devoid),
        &json!({ "kind": "colorContains", "value": "blue" })
    ));
    assert!(matches_object_predicate(
        DefinitionView::new(&devoid),
        &json!({ "kind": "colorDoesNotContain", "value": "blue" })
    ));
    assert!(matches_object_predicate(
        DefinitionView::new(&devoid),
        &comparison("powerOf", "==", json!(0))
    ));

    assert!(matches_object_predicate(
        DefinitionView::new(&devoid),
        &json!({ "kind": "and", "operands": [] })
    ));
    assert!(matches_object_predicate(
        DefinitionView::new(&devoid),
        &json!({
            "kind": "not",
            "operand": { "kind": "unsupportedPredicate" }
        })
    ));
}

#[test]
fn permanent_card_predicate_accepts_exactly_the_permanent_card_types() {
    for type_line in [
        "Artifact",
        "Battle - Siege",
        "Creature - Bear",
        "Enchantment",
        "Land",
        "Planeswalker - Jace",
    ] {
        let definition = candidate("Permanent candidate", type_line, "", None, None);
        assert!(
            matches_object_predicate(
                DefinitionView::new(&definition),
                &json!({ "kind": "isPermanentCard" })
            ),
            "{type_line} should be a permanent card"
        );
    }

    for type_line in ["Instant", "Sorcery", "Mystery"] {
        let definition = candidate("Nonpermanent candidate", type_line, "", None, None);
        assert!(
            !matches_object_predicate(
                DefinitionView::new(&definition),
                &json!({ "kind": "isPermanentCard" })
            ),
            "{type_line} should not be a permanent card"
        );
    }
}

#[test]
fn compatibility_adapter_matches_the_definition_view_across_a_predicate_matrix() {
    let mut creature = candidate(
        "Adaptive Automaton",
        "Artifact Creature - Construct",
        "{3}",
        Some("2"),
        Some("2"),
    );
    creature.rules.push(keyword("changeling"));
    let mut token = candidate(
        "Goblin Token",
        "Token Creature - Goblin",
        "",
        Some("1"),
        Some("1"),
    );
    token.is_token = true;
    let definitions = [
        creature,
        token,
        candidate("Urza's Saga", "Enchantment Land - Saga", "", None, None),
        candidate("Opt", "Instant", "{U}", None, None),
    ];
    let predicates = [
        json!({ "kind": "cardTypeContains", "value": "Creature" }),
        json!({ "kind": "subtypeContains", "value": "Elf" }),
        json!({ "kind": "typeLineContains", "value": "Land" }),
        json!({ "kind": "nameStartsWith", "value": "Urza" }),
        json!({ "kind": "isToken" }),
        json!({ "kind": "isLegendary" }),
        json!({ "kind": "isPermanentCard" }),
        json!({ "kind": "historic" }),
        json!({ "kind": "colorDoesNotContain", "value": "red" }),
        comparison("manaValueOf", ">=", json!(1)),
        json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": { "kind": "isToken" } },
                {
                    "kind": "or",
                    "operands": [
                        { "kind": "cardTypeContains", "value": "Land" },
                        { "kind": "cardTypeContains", "value": "Instant" }
                    ]
                }
            ]
        }),
        Value::Null,
    ];

    for definition in &definitions {
        for predicate in &predicates {
            assert_eq!(
                matches_card_filter(definition, predicate),
                matches_object_predicate(DefinitionView::new(definition), predicate),
                "adapter diverged for {} with {predicate}",
                definition.name
            );
        }
    }
}
