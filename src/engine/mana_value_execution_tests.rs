use super::super::mana_projection_contract::NoManaReason;
use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"rules":rules})).unwrap()
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Land", vec![]);
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        604,
    )
    .unwrap()
}
fn rule(value: Value) -> Value {
    json!({"kind":"manaAbility","source":{"kind":"self"},"costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":value}]})
}
fn instance(id: &str, value: Value) -> CardInstance {
    let mut p = card_instance("player-0", &definition(id, "Land", vec![rule(value)]), 0);
    p.instance_id = id.into();
    p
}
fn choices(result: ExecutableManaOutputs) -> Vec<Vec<String>> {
    match result {
        ExecutableManaOutputs::Choices(v) => v,
        other => panic!("expected executable choices, got {other:?}"),
    }
}

#[test]
fn checked_mana_value_preserves_actual_quantities_and_mixed_choices() {
    let e = engine();
    let source = instance("source", json!("{G}"));
    let fixed = e
        .try_mana_options_for_value(
            &json!({"kind":"fixedMana","symbol":"G","amount":{"kind":"integer","value":3}}),
            "player-0",
            Some(&source),
        )
        .unwrap();
    assert_eq!(choices(fixed), vec![vec!["G", "G", "G"]]);
    let mixed = choices(
        e.try_mana_options_for_value(
            &json!({"kind":"chooseColors","amount":2}),
            "player-0",
            Some(&source),
        )
        .unwrap(),
    );
    assert_eq!(mixed.len(), 25);
    assert!(mixed.contains(&vec!["W".into(), "G".into()]));
    assert!(mixed.contains(&vec!["G".into(), "W".into()]));
    assert!(mixed.iter().all(|v| v.len() == 2));
    let with_zero = choices(
        e.try_mana_options_for_value(
            &json!({"kind":"chooseOne","options":[
            {"kind":"fixedMana","symbol":"G","amount":{"kind":"integer","value":0}}, "{C}"]}),
            "player-0",
            Some(&source),
        )
        .unwrap(),
    );
    assert_eq!(with_zero, vec![Vec::<String>::new(), vec!["C".to_string()]]);
}

#[test]
fn checked_mana_value_keeps_no_mana_separate_from_unsupported() {
    let e = engine();
    let source = instance("source", json!("{G}"));
    let zero = e
        .try_mana_options_for_value(
            &json!({"kind":"fixedMana","symbol":"G","amount":{"kind":"integer","value":0}}),
            "player-0",
            Some(&source),
        )
        .unwrap();
    assert_eq!(
        zero,
        ExecutableManaOutputs::NoMana(NoManaReason::ZeroAmount)
    );
    let error=e.try_mana_options_for_value(&json!({"kind":"fixedMana","symbol":"G","amount":{"kind":"decisionResult","decisionId":"missing"}}),"player-0",Some(&source)).expect_err("missing binding is not zero");
    assert!(error.message.contains("mana.execution.numeric-binding"));
    let error = e
        .try_mana_options_for_value(
            &json!({"kind":"chooseColors","amount":6}),
            "player-0",
            Some(&source),
        )
        .expect_err("no silent min5 truncation");
    assert!(error.message.contains("mana.execution.choice-cardinality"));
}

#[test]
fn checked_mana_value_queries_exact_types_with_full_output_amounts() {
    let mut e = engine();
    let source = instance(
        "pool",
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
    );
    e.state.players[0].battlefield = vec![source.clone(), instance("anchor", json!("{C}"))];
    let output = e
        .try_mana_options_for_value(
            &json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":3}),
            "player-0",
            Some(&source),
        )
        .unwrap();
    assert_eq!(choices(output), vec![vec!["C", "C", "C"]]);
}

#[test]
fn checked_mana_value_fixed_output_does_not_certify_unrelated_land_programs() {
    let mut e = engine();
    let source = instance("source", json!("{G}"));
    let mut unrelated = instance("bad-land", json!("{U}"));
    unrelated.definition.rules[0]["effects"].as_array_mut().unwrap().insert(0,
        json!({"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":1}}));
    e.state.players[1].battlefield = vec![unrelated];
    let actual = e
        .try_mana_options_for_value(&json!("{G}"), "player-0", Some(&source))
        .unwrap();
    assert_eq!(choices(actual), vec![vec!["G"]]);
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .try_mana_options_for_value(
            &json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
            "player-0",
            Some(&source),
        )
        .expect_err("query construction must retain unsupported source error");
    assert!(error.message.contains("StatefulPrefix"));
    assert!(error.message.contains("bad-land"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
