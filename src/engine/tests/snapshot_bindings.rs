use super::*;

fn test_stack_object(rule: Value) -> StackObject {
    StackObject {
        id: "stack:snapshot-test".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "snapshot-test",
            test_definition("snapshot-test", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn swords_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Swords to Plowshares".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(
            "Exile target creature. Its controller gains life equal to its power.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.abilities[0].recognizer,
        crate::oracle::RecognizerProvenance::Native
    );
    parsed.abilities[0]
        .rule
        .clone()
        .expect("Swords should produce primitive IR")
}

fn resolve_swords(power_modifier: i32) -> GameEngine {
    let rule = swords_rule();
    assert!(rule_is_executable(&rule));

    let mut creature = test_instance(
        "borrowed-creature",
        test_definition("borrowed-creature", "Creature"),
        "player-0",
    );
    creature.controller = "player-1".to_string();
    creature.power_modifier = power_modifier;

    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(creature);
    let mut stack_object = test_stack_object(rule.clone());
    stack_object.id = "stack:swords".to_string();
    stack_object.targets = BTreeMap::from([(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "borrowed-creature".to_string(),
        },
    )]);
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the clamped snapshot sequence should resolve");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn maximum_numeric_expression_is_validated_and_selects_the_greatest_operand() {
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": {
                "kind": "maximum",
                "operands": [
                    { "kind": "integer", "value": -3 },
                    { "kind": "integer", "value": 2 },
                    { "kind": "integer", "value": 1 },
                ],
            },
        }],
    });
    assert!(rule_is_executable(&rule));

    let invalid = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "maximum", "operands": [] },
        }],
    });
    assert!(!rule_is_executable(&invalid));

    let mut engine = test_engine(2);
    let stack_object = test_stack_object(rule.clone());
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("maximum should evaluate inside a primitive life effect");
    assert_eq!(engine.state.players[0].life, 22);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn exile_then_life_uses_pre_exile_power_and_controller_snapshots() {
    let engine = resolve_swords(3);

    assert!(engine.state.players[1].battlefield.is_empty());
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "borrowed-creature")
    );
    assert_eq!(engine.state.players[0].life, 20);
    assert_eq!(
        engine.state.players[1].life, 25,
        "the current controller gains the creature's modified power"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn exile_then_life_clamps_negative_power_to_zero_life_gain() {
    let engine = resolve_swords(-5);

    assert!(engine.state.players[1].battlefield.is_empty());
    assert_eq!(
        engine.state.players[1].life, 20,
        "negative power must produce zero life gain, never life loss"
    );
}
