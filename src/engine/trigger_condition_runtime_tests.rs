//! Runtime proofs of the trigger condition language (CR 603.2): a list of
//! trigger conditions triggers once per occurrence, attack declarations
//! count their attackers, event-time conditions are checked as the event
//! occurs, and the new observers (turned face up, became untapped, tapped for
//! mana, attacked recipient) publish real triggers. Rules are the IR the
//! Oracle lowering emits; creature statistics are synthetic.
use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    let creature = type_line.contains("Creature");
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
        "rules": rules,
    }))
    .expect("synthetic runtime definition deserializes")
}

fn instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.to_string();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", Vec::new());
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            players: (0..2)
                .map(|index| PlayerDeck {
                    cards: vec![filler.clone(); 4],
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                })
                .collect(),
            starting_player: 0,
        },
        97,
    )
    .expect("synthetic runtime setup is valid");
    for player in &mut engine.state.players {
        player.battlefield.clear();
        player.hand.clear();
    }
    engine.state.events.clear();
    engine.state.stack.clear();
    engine
}

fn controller() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn draw_rule(event: Value) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": {"kind": "self"},
        "event": event,
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": {"kind": "integer", "value": 1},
        }],
    })
}

fn creature(id: &str, owner: &str) -> CardInstance {
    instance(id, definition(id, "Creature - Soldier", Vec::new()), owner)
}

fn attack(engine: &mut GameEngine, attackers: &[&str], defender: TargetRef, defending: &str) {
    engine.state.combat.attackers = attackers
        .iter()
        .map(|attacker| DeclaredAttacker {
            attacker_id: attacker.to_string(),
            defender: defender.clone(),
            defending_player_id: defending.to_string(),
        })
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    let ids = attackers
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    engine.enqueue_declared_attacker_triggers(&ids);
}

fn triggers_put_on_stack(engine: &GameEngine) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "triggeredAbilityPutOnStack")
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_trigger_condition_list_triggers_once_per_occurrence() {
    // Two members that one occurrence satisfies at once: the ability still
    // triggers once (CR 603.2c); a second occurrence triggers again.
    let rule = draw_rule(json!({
        "kind": "oneOf",
        "events": [
            {"kind": "turnedFaceUp", "object": {"kind": "self"}},
            {"kind": "turnedFaceUp", "object": {"kind": "permanents", "controller": controller()}},
            {"kind": "permanentUntapped", "object": {"kind": "self"}},
        ],
    }));
    assert!(rule_is_executable(&rule), "the lowered list is executable");
    let mut engine = engine();
    let observer = instance(
        "observer",
        definition("observer", "Creature - Shapeshifter", vec![rule]),
        "player-0",
    );
    engine.state.players[0].battlefield = vec![observer.clone()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine.publish_turned_face_up_observers(&observer);
    assert_eq!(
        triggers_put_on_stack(&engine),
        1,
        "one occurrence, one trigger"
    );
    engine.state.players[0].battlefield[0].tapped = true;
    assert_eq!(
        engine.attempt_untap_permanent("observer"),
        untap_attempt::UntapAttemptOutcome::Untapped
    );
    assert_eq!(
        triggers_put_on_stack(&engine),
        2,
        "another member's occurrence"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_battalion_declaration_counts_the_source_and_its_companions() {
    let rule = draw_rule(json!({
        "kind": "controlledCreaturesAttacked",
        "player": controller(),
        "where": {"kind": "cardTypeContains", "value": "Creature"},
        "sourceMustAttack": true,
        "minimum": 3,
    }));
    assert!(rule_is_executable(&rule));
    for (attackers, expected) in [
        (vec!["source", "a", "b"], 1),
        (vec!["source", "a"], 0),
        (vec!["a", "b", "c"], 0),
    ] {
        let mut engine = engine();
        engine.state.players[0].battlefield = vec![
            instance(
                "source",
                definition("source", "Creature - Soldier", vec![rule.clone()]),
                "player-0",
            ),
            creature("a", "player-0"),
            creature("b", "player-0"),
            creature("c", "player-0"),
        ];
        attack(
            &mut engine,
            &attackers,
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            "player-1",
        );
        assert_eq!(triggers_put_on_stack(&engine), expected, "{attackers:?}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_member_event_condition_is_checked_by_its_matcher() {
    // `Whenever you attack with two or more creatures while you control an
    // artifact, and whenever a creature you control is turned face up`.
    let rule = draw_rule(json!({
        "kind": "oneOf",
        "events": [
            {
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "where": {"kind": "cardTypeContains", "value": "Creature"},
                "minimum": 2,
                "eventCondition": {
                    "kind": "controlsPermanent",
                    "where": {"kind": "cardTypeContains", "value": "Artifact"},
                },
            },
            {"kind": "turnedFaceUp", "object": {"kind": "permanents", "controller": controller()}},
        ],
    }));
    assert!(rule_is_executable(&rule));
    for (artifact, expected) in [(true, 1), (false, 0)] {
        let mut engine = engine();
        let mut battlefield = vec![
            instance(
                "observer",
                definition("observer", "Enchantment", vec![rule.clone()]),
                "player-0",
            ),
            creature("a", "player-0"),
            creature("b", "player-0"),
        ];
        if artifact {
            battlefield.push(instance(
                "relic",
                definition("relic", "Artifact", Vec::new()),
                "player-0",
            ));
        }
        engine.state.players[0].battlefield = battlefield;
        attack(
            &mut engine,
            &["a", "b"],
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            "player-1",
        );
        assert_eq!(
            triggers_put_on_stack(&engine),
            expected,
            "artifact: {artifact}"
        );
        // The unconditioned member is unaffected by the other's condition.
        let turned = engine.state.players[0].battlefield[1].clone();
        engine.publish_turned_face_up_observers(&turned);
        assert_eq!(
            triggers_put_on_stack(&engine),
            expected + 1,
            "artifact: {artifact}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_rule_event_condition_is_checked_when_the_event_occurs() {
    let mut rule = draw_rule(json!({"kind": "permanentUntapped", "object": {"kind": "self"}}));
    rule["eventCondition"] = json!({"kind": "duringControllerTurn", "player": controller()});
    assert!(rule_is_executable(&rule));
    for (active_player, expected) in [(0, 1), (1, 0)] {
        let mut engine = engine();
        let mut source = instance(
            "source",
            definition("source", "Creature - Sphinx", vec![rule.clone()]),
            "player-0",
        );
        source.tapped = true;
        engine.state.players[0].battlefield = vec![source];
        engine.state.active_player = active_player;
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture construction precedes references");
        engine.attempt_untap_permanent("source");
        assert_eq!(
            triggers_put_on_stack(&engine),
            expected,
            "active player {active_player}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_stun_counter_replacing_the_untap_is_no_became_untapped_event() {
    let rule = draw_rule(json!({"kind": "permanentUntapped", "object": {"kind": "self"}}));
    let mut engine = engine();
    let mut source = instance(
        "source",
        definition("source", "Creature - Sphinx", vec![rule]),
        "player-0",
    );
    source.tapped = true;
    source.counters.insert("stun".to_string(), 1);
    engine.state.players[0].battlefield = vec![source];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    assert_eq!(
        engine.attempt_untap_permanent("source"),
        untap_attempt::UntapAttemptOutcome::StunCounterRemoved
    );
    assert_eq!(triggers_put_on_stack(&engine), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_land_tapped_for_mana_triggers_observers_of_any_player() {
    // Manabarbs: `Whenever a player taps a land for mana`.
    let rule = draw_rule(json!({
        "kind": "controlledPermanentManaAbilityActivated",
        "anyPlayer": true,
        "where": {"kind": "cardTypeContains", "value": "Land"},
    }));
    assert!(rule_is_executable(&rule));
    for (tapped_type, expected) in [("Basic Land - Forest", 1), ("Artifact", 0)] {
        let mut engine = engine();
        engine.state.players[0].battlefield = vec![instance(
            "observer",
            definition("observer", "Enchantment", vec![rule.clone()]),
            "player-0",
        )];
        let tapped = instance(
            "mana-source",
            definition("mana-source", tapped_type, Vec::new()),
            "player-1",
        );
        engine.state.players[1].battlefield = vec![tapped.clone()];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture construction precedes references");
        engine.enqueue_mana_ability_activated_triggers(&tapped, &["G".to_string()]);
        assert_eq!(triggers_put_on_stack(&engine), expected, "{tapped_type}");
        if expected == 1 {
            assert_eq!(
                engine.state.stack[0].decisions["triggeringPlayerId"], "player-1",
                "the tapping player is the event player"
            );
        }
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_attacked_recipient_distinguishes_players_and_planeswalkers() {
    // `Whenever a creature attacks you or a planeswalker you control`.
    let rule = draw_rule(json!({
        "kind": "eventPattern",
        "lemma": "attack",
        "subject": {"role": "attacker", "object": {"kind": "permanents"}},
        "attacked": {"anyOf": [
            {"kind": "player", "player": controller()},
            {
                "kind": "permanents",
                "controller": controller(),
                "where": {"kind": "cardTypeContains", "value": "Planeswalker"},
            },
        ]},
    }));
    assert!(rule_is_executable(&rule));
    for (scenario, expected) in [("you", 1), ("your planeswalker", 1), ("another player", 0)] {
        let mut engine = engine();
        engine.state.players[0].battlefield = vec![
            instance(
                "observer",
                definition("observer", "Enchantment", vec![rule.clone()]),
                "player-0",
            ),
            instance(
                "walker",
                definition("walker", "Legendary Planeswalker - Test", Vec::new()),
                "player-0",
            ),
        ];
        engine.state.players[1].battlefield = vec![creature("attacker", "player-1")];
        let (defender, defending) = match scenario {
            "you" => (
                TargetRef::Player {
                    player_id: "player-0".to_string(),
                },
                "player-0",
            ),
            "your planeswalker" => (
                TargetRef::Permanent {
                    instance_id: "walker".to_string(),
                },
                "player-0",
            ),
            _ => (
                TargetRef::Player {
                    player_id: "player-1".to_string(),
                },
                "player-1",
            ),
        };
        attack(&mut engine, &["attacker"], defender, defending);
        assert_eq!(triggers_put_on_stack(&engine), expected, "{scenario}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn another_permanent_turned_face_up_binds_the_triggering_permanent() {
    let rule = draw_rule(json!({
        "kind": "turnedFaceUp",
        "object": {"kind": "permanents", "controller": controller(), "excludeSource": true},
    }));
    assert!(rule_is_executable(&rule));
    let mut engine = engine();
    let observer = instance(
        "observer",
        definition("observer", "Creature - Djinn", vec![rule]),
        "player-0",
    );
    let other = creature("other", "player-0");
    engine.state.players[0].battlefield = vec![observer.clone(), other.clone()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine.publish_turned_face_up_observers(&observer);
    assert_eq!(triggers_put_on_stack(&engine), 0, "the source is excluded");
    engine.publish_turned_face_up_observers(&other);
    assert_eq!(triggers_put_on_stack(&engine), 1);
    assert_eq!(
        engine.state.stack[0].decisions["triggeringPermanentId"],
        "other"
    );
}
