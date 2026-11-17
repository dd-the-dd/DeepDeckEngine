use super::*;

fn dismember_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Dismember".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B/P}{B/P}".to_string()),
        oracle_text: Some(
            "({B/P} can be paid with either {B} or 2 life.)\nTarget creature gets -5/-5 until end of turn."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    let rule = parsed
        .abilities
        .iter()
        .find_map(|ability| {
            ability
                .rule
                .as_ref()
                .filter(|rule| rule["kind"] == "spellAbility")
        })
        .cloned()
        .expect("Dismember emits a spell rule");
    assert!(rule_is_executable(&rule));
    rule
}

fn creature(id: &str, power: i64, toughness: i64) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    test_instance(id, definition, "player-1")
}

fn dismember_stack(rule: &Value, target_id: &str) -> StackObject {
    StackObject {
        id: "stack:dismember".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "dismember",
            test_definition("dismember", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: target_id.to_string(),
            },
        )]),
    }
}

#[test]
fn parsed_dismember_executes_the_generic_temporary_modifier_and_state_based_death() {
    let rule = dismember_rule();

    let mut survivor_engine = test_engine(2);
    survivor_engine.state.players[1]
        .battlefield
        .push(creature("six-six", 6, 6));
    let stack = dismember_stack(&rule, "six-six");
    execute_effects_at_resolution_start(
        &mut survivor_engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("Dismember resolves on the surviving creature");
    let survivor = &survivor_engine.state.players[1].battlefield[0];
    assert_eq!(survivor_engine.current_power(survivor), 1);
    assert_eq!(survivor_engine.current_toughness(survivor), 1);
    survivor_engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup expires the temporary modifier");
    let survivor = &survivor_engine.state.players[1].battlefield[0];
    assert_eq!(survivor_engine.current_power(survivor), 6);
    assert_eq!(survivor_engine.current_toughness(survivor), 6);

    let mut lethal_engine = test_engine(2);
    lethal_engine.state.players[1]
        .battlefield
        .push(creature("four-four", 4, 4));
    let stack = dismember_stack(&rule, "four-four");
    execute_effects_at_resolution_start(
        &mut lethal_engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("Dismember resolves on the lethal target");
    lethal_engine.check_state_based_actions();
    assert!(lethal_engine.state.players[1].battlefield.is_empty());
    assert!(
        lethal_engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "four-four")
    );
}
