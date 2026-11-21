use super::*;

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, controller)
}

fn resolving_spell_effects(text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Granting Spell".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the granting spell parses canonically");
    rule["effects"].as_array().cloned().expect("spell effects")
}

fn spell_stack_object() -> StackObject {
    StackObject {
        id: "stack:test-granting-spell".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "test-granting-spell",
            test_definition("test-granting-spell", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn engine_with_two_controlled_creatures_and_one_opposing() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("mine-a", "player-0"),
        creature("mine-b", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![creature("theirs", "player-1")];
    engine
}

fn granted_rules<'a>(engine: &'a GameEngine, id: &str) -> Vec<&'a Value> {
    let (player, index) = engine
        .permanent_position(id)
        .expect("permanent on battlefield");
    engine.state.players[player].battlefield[index]
        .definition
        .rules
        .iter()
        .filter(|rule| rule.get("temporaryUntilTurn").is_some())
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn quoted_triggered_ability_is_granted_to_each_controlled_creature_until_cleanup() {
    let effects = resolving_spell_effects(
        "Until end of turn, creatures you control gain \"Whenever this creature attacks, you gain 1 life.\"",
    );
    assert_eq!(effects.len(), 1);
    assert_eq!(value_kind(&effects[0]), Some("grantAbility"));
    assert!(effect_supported(&effects[0]));

    let mut engine = engine_with_two_controlled_creatures_and_one_opposing();
    let life_before = engine.state.players[0].life;
    engine
        .execute_effects(
            &Value::Array(effects.clone()),
            &spell_stack_object(),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the grant resolves");
    for id in ["mine-a", "mine-b"] {
        let rules = granted_rules(&engine, id);
        assert_eq!(rules.len(), 1, "{id} gains exactly one rule");
        assert_eq!(value_kind(rules[0]), Some("triggeredAbility"));
    }
    assert!(granted_rules(&engine, "theirs").is_empty());

    // The engine reads the granted rule through its ordinary event publisher.
    engine.enqueue_declared_attacker_triggers(&["mine-a".to_string()]);
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the granted trigger resolves");
    assert_eq!(engine.state.players[0].life, life_before + 1);

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("end-of-turn cleanup succeeds");
    for id in ["mine-a", "mine-b"] {
        assert!(granted_rules(&engine, id).is_empty(), "{id} loses the rule");
    }
    engine.enqueue_declared_attacker_triggers(&["mine-a".to_string()]);
    assert!(engine.state.stack.is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn quoted_mana_ability_is_granted_alongside_a_keyword_to_the_same_collection() {
    let effects = resolving_spell_effects(
        "Until end of turn, creatures you control gain flying and \"{T}: Add {G}.\"",
    );
    assert_eq!(
        effects.iter().map(value_kind).collect::<Vec<_>>(),
        [Some("grantKeyword"), Some("grantAbility")]
    );
    assert!(effects.iter().all(effect_supported));

    let mut engine = engine_with_two_controlled_creatures_and_one_opposing();
    engine
        .execute_effects(
            &Value::Array(effects.clone()),
            &spell_stack_object(),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the grant resolves");
    for id in ["mine-a", "mine-b"] {
        assert!(
            granted_rules(&engine, id)
                .iter()
                .any(|rule| value_kind(rule) == Some("manaAbility"))
        );
    }
    assert!(granted_rules(&engine, "theirs").is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn permanent_collection_grant_is_not_an_accepted_engine_shape() {
    let mut effect = resolving_spell_effects(
        "Until end of turn, creatures you control gain \"Whenever this creature attacks, you gain 1 life.\"",
    )
    .remove(0);
    effect["duration"] = json!({ "kind": "permanent" });
    assert!(!effect_supported(&effect));
}
