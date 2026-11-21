#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn each_opponent_independently_pays_or_grants_the_controller_effect() {
    struct MixedOpponentProvider;
    impl DecisionProvider for MixedOpponentProvider {
        fn choose(
            &mut self,
            _state: &GameState,
            request: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            if request.player_id == "player-1" {
                Ok(0)
            } else {
                request
                    .options
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| EngineError::new("test decision has no option"))
            }
        }
    }

    let mut engine = test_engine(3);
    let first_creature = test_instance(
        "first-opponent-creature",
        test_definition("first-opponent-creature", "Creature"),
        "player-1",
    );
    let second_creature = test_instance(
        "second-opponent-creature",
        test_definition("second-opponent-creature", "Creature"),
        "player-2",
    );
    engine.state.players[1]
        .battlefield
        .push(first_creature.clone());
    engine.state.players[2]
        .battlefield
        .push(second_creature.clone());
    let source = test_instance(
        "opponent-choice-source",
        test_definition("opponent-choice-source", "Creature"),
        "player-0",
    );
    let stack_object = StackObject {
        id: "stack:opponent-choice".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &json!([{
                "kind": "forEachOpponentPaysCostOrControllerEffect",
                "cost": {
                    "kind": "sacrificePermanent",
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
                "otherwise": [{
                    "kind": "createTokens",
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": {
                        "name": "Zombie Token",
                        "colors": ["black"],
                        "types": ["Creature"],
                        "subtypes": ["Zombie"],
                        "power": 2,
                        "toughness": 2,
                    },
                }],
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut MixedOpponentProvider,
        )
        .expect("each opponent makes an independent payment choice");

    assert!(
        engine
            .permanent_position(&first_creature.instance_id)
            .is_some()
    );
    assert!(
        engine
            .permanent_position(&second_creature.instance_id)
            .is_none()
    );
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| {
                card.definition.is_token && has_card_type(&card.definition, "Creature")
            })
            .count(),
        1
    );
}
