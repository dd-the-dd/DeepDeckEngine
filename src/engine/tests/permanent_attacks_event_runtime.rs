use super::*;

#[test]
fn generic_permanent_attacks_event_matches_player_and_controlled_planeswalker_union() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Scholar".to_string(),
        type_line: "Legendary Planeswalker — Test".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(
            "+2: Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -1/-0 until end of turn."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("generic loyalty ability parses");
    assert_eq!(rule["effects"][0]["event"]["kind"], "permanentAttacks");
    assert_ne!(
        rule["effects"][0]["event"]["kind"],
        "permanentAttacksProtectedPlayerOrPlaneswalker"
    );
    assert!(rule_is_executable(&rule));

    let source = test_instance(
        "test-scholar",
        test_definition("test-scholar", "Legendary Planeswalker — Test"),
        "player-0",
    );
    let protected_planeswalker = test_instance(
        "protected-planeswalker",
        test_definition("protected-planeswalker", "Legendary Planeswalker — Test"),
        "player-0",
    );
    let attacker = |id: &str| {
        let mut definition = test_definition(id, "Creature — Soldier");
        definition.power = Some("2".to_string());
        definition.toughness = Some("2".to_string());
        test_instance(id, definition, "player-1")
    };
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone(), protected_planeswalker];
    engine.state.players[1].battlefield = vec![
        attacker("attacks-player"),
        attacker("attacks-planeswalker"),
        attacker("near-miss-attacker"),
    ];
    let stack_object = StackObject {
        id: "stack:test-scholar".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("temporary attack trigger installs");

    engine.state.combat.attackers = vec![
        DeclaredAttacker {
            attacker_id: "attacks-player".to_string(),
            defender: TargetRef::Player {
                player_id: "player-0".to_string(),
            },
            defending_player_id: "player-0".to_string(),
        },
        DeclaredAttacker {
            attacker_id: "attacks-planeswalker".to_string(),
            defender: TargetRef::Permanent {
                instance_id: "protected-planeswalker".to_string(),
            },
            defending_player_id: "player-0".to_string(),
        },
        DeclaredAttacker {
            attacker_id: "near-miss-attacker".to_string(),
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    ];
    engine.enqueue_declared_attacker_triggers(&[
        "attacks-player".to_string(),
        "attacks-planeswalker".to_string(),
        "near-miss-attacker".to_string(),
    ]);
    assert_eq!(engine.state.stack.len(), 2);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("temporary trigger resolves");
    }

    let power = |engine: &GameEngine, id: &str| {
        let (player_index, card_index) = engine.permanent_position(id).expect("attacker exists");
        engine.current_power(&engine.state.players[player_index].battlefield[card_index])
    };
    assert_eq!(power(&engine, "attacks-player"), 1);
    assert_eq!(power(&engine, "attacks-planeswalker"), 1);
    assert_eq!(power(&engine, "near-miss-attacker"), 2);
}
