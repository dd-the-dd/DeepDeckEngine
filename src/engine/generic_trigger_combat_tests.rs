//! Focused event-matching proof, independent of Oracle compilation.
//! Root replaces the adjacent IR fixture with the actual emitted rule before
//! claiming parser-to-runtime coverage. Creature statistics are synthetic.

use super::*;

struct NoChoices;

impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "unexpected decision in deterministic combat proof: {}",
            request.id
        )))
    }

    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        Err(EngineError::new(format!(
            "unexpected numeric decision in deterministic combat proof: {}",
            request.id
        )))
    }
}

fn test_definition(id: &str, type_line: &str) -> CardDefinition {
    let creature = type_line.split_whitespace().any(|word| word == "Creature");
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": "{0}",
        "power": creature.then_some("2"),
        "toughness": creature.then_some("2"),
        "isCommander": false,
        "isGamePiece": false,
        "isSideboard": false,
        "isToken": false,
        "rules": [],
    }))
    .expect("synthetic runtime definition deserializes")
}

fn test_instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.to_string();
    instance
}

fn test_engine(player_count: usize) -> GameEngine {
    let filler = test_definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            players: (0..player_count)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 4],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        73,
    )
    .expect("synthetic runtime setup is valid")
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn generic_source_combat_trigger_draws_only_for_source_hitting_player() {
    let rule: Value = serde_json::from_str(include_str!("generic_trigger_combat_rule.json"))
        .expect("the generic combat-trigger rule fixture deserializes");
    assert!(
        rule_is_executable(&rule),
        "event source must use the runtime source contract"
    );

    for scenario in [
        "source-combat-player",
        "source-noncombat-player",
        "other-combat-player",
        "source-combat-creature",
    ] {
        let mut engine = test_engine(2);
        let mut source_definition =
            test_definition("generic-combat-observer", "Creature - Elemental");
        source_definition.rules = vec![rule.clone()];
        engine.state.players[0].battlefield = vec![
            test_instance("observer", source_definition, "player-0"),
            test_instance(
                "other",
                test_definition("other", "Creature - Test"),
                "player-0",
            ),
        ];
        engine.state.players[0].hand.clear();
        engine.state.players[0].library = vec![
            test_instance("draw-a", test_definition("draw-a", "Instant"), "player-0"),
            test_instance("draw-b", test_definition("draw-b", "Sorcery"), "player-0"),
        ];
        engine.state.players[1].battlefield.clear();
        engine.state.events.clear();
        engine.state.stack.clear();

        if scenario == "source-noncombat-player" {
            engine
                .admit_explicit_test_fixture_objects()
                .expect("fixture construction precedes references");
            engine
                .deal_damage(
                    TargetRef::Player {
                        player_id: "player-1".to_string(),
                    },
                    2,
                    "observer",
                )
                .expect("real noncombat damage is dealt");
        } else {
            let attacker = if scenario == "other-combat-player" {
                "other"
            } else {
                "observer"
            };
            let attacks = BTreeMap::from([(
                attacker.to_string(),
                AttackAssignment {
                    defender: TargetRef::Player {
                        player_id: "player-1".to_string(),
                    },
                    defending_player_id: "player-1".to_string(),
                },
            )]);
            let mut blocks = BTreeMap::new();
            let mut assignments = BTreeMap::new();
            if scenario == "source-combat-creature" {
                let mut blocker_definition = test_definition("blocker", "Creature - Test");
                blocker_definition.power = Some("0".to_string());
                blocker_definition.toughness = Some("5".to_string());
                engine.state.players[1].battlefield =
                    vec![test_instance("blocker", blocker_definition, "player-1")];
                blocks.insert(attacker.to_string(), vec!["blocker".to_string()]);
                assignments.insert(
                    attacker.to_string(),
                    BTreeMap::from([("blocker".to_string(), 2)]),
                );
            }
            engine
                .admit_explicit_test_fixture_objects()
                .expect("fixture construction precedes references");
            engine
                .resolve_combat_damage(&attacks, &blocks, &assignments)
                .unwrap_or_else(|error| {
                    panic!("{scenario}: actual combat damage resolves: {error}")
                });
        }

        let expected_triggers = usize::from(scenario == "source-combat-player");
        assert_eq!(
            engine.state.stack.len(),
            expected_triggers,
            "{scenario}: stack creation"
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|event| event.kind == "triggeredAbilityPutOnStack")
                .count(),
            expected_triggers,
            "{scenario}: actual trigger event count"
        );
        assert_eq!(
            engine.state.players[0].hand.len(),
            0,
            "{scenario}: no premature draw"
        );
        assert_eq!(
            engine.state.players[0].library.len(),
            2,
            "{scenario}: library before resolution"
        );
        assert_eq!(
            engine.state.players[1].life,
            if scenario == "source-combat-creature" {
                20
            } else {
                18
            },
            "{scenario}: damage really happened"
        );
        if scenario == "source-combat-creature" {
            assert_eq!(
                engine.state.players[1].battlefield[0].damage_marked, 2,
                "creature near miss must actually receive damage"
            );
        }

        if expected_triggers == 1 {
            assert_eq!(engine.state.stack[0].card.instance_id, "observer");
            assert_eq!(
                engine.state.stack[0].decisions["damageAmount"], 2,
                "runtime retains event-local numeric binding"
            );
            engine
                .resolve_top_stack(&mut NoChoices)
                .expect("the real combat-created trigger resolves");
        }
        assert!(
            engine.state.stack.is_empty(),
            "{scenario}: stack after resolution"
        );
        let expected_draws = expected_triggers * 2;
        assert_eq!(
            engine.state.players[0].hand.len(),
            expected_draws,
            "{scenario}: final hand"
        );
        assert_eq!(
            engine.state.players[0].library.len(),
            2 - expected_draws,
            "{scenario}: final library"
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|event| event.kind == "cardDrawn")
                .count(),
            expected_draws,
            "{scenario}: draw events"
        );
    }
}
