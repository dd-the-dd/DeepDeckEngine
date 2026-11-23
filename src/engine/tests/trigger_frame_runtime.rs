//! Real Oracle text -> trigger-frame IR -> engine behavior for events claimed
//! by the shared trigger envelope: source event unions (blocks or becomes
//! blocked), turn-owner step scopes with their event player, and cycling the
//! source from a nonpermanent card.
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn parsed_instance(instance_id: &str, owner: &str, card: (&str, &str, &str, &str)) -> CardInstance {
    let (name, type_line, mana_cost, oracle) = card;
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.mana_cost = mana_cost.to_string();
    if type_line.contains("Creature") {
        definition.power = Some("3".to_string());
        definition.toughness = Some("3".to_string());
    }
    definition.rules = parsed_rules(name, type_line, mana_cost, oracle);
    test_instance(instance_id, definition, owner)
}

const RAGING_GORILLA: (&str, &str, &str, &str) = (
    "Raging Gorilla",
    "Creature — Ape",
    "{2}{R}",
    "Whenever this creature blocks or becomes blocked, it gets +2/-2 until end of turn.",
);

/// Declares exactly one attack and one block, then passes priority.
struct OneBlockProvider {
    attacker: &'static str,
    blocker: &'static str,
}

impl DecisionProvider for OneBlockProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let position = |kind: ActionKind, matches: &dyn Fn(&LegalAction) -> bool| {
            request
                .options
                .iter()
                .position(|action| action.kind == kind && matches(action))
        };
        match request.kind {
            DecisionKind::Attackers => position(ActionKind::DeclareAttacker, &|action| {
                action.attacker_id.as_deref() == Some(self.attacker)
            })
            .or_else(|| position(ActionKind::FinishAttackers, &|_| true)),
            DecisionKind::Blockers => position(ActionKind::DeclareBlocker, &|action| {
                action.attacker_id.as_deref() == Some(self.attacker)
                    && action.blocker_id.as_deref() == Some(self.blocker)
            })
            .or_else(|| position(ActionKind::FinishBlockers, &|_| true)),
            DecisionKind::Priority => position(ActionKind::PassPriority, &|_| true),
            _ => Some(0),
        }
        .ok_or_else(|| EngineError::new("scripted combat action is unavailable"))
    }
}

fn source_triggers(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| {
            event.kind == "triggeredAbilityPutOnStack"
                && event.card_instance_id.as_deref() == Some(instance_id)
        })
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_source_union_triggers_once_when_it_blocks_or_becomes_blocked() {
    for gorilla_attacks in [true, false] {
        let mut engine = test_engine(2);
        let (gorilla_owner, bear_owner) = if gorilla_attacks {
            ("player-0", "player-1")
        } else {
            ("player-1", "player-0")
        };
        let gorilla = parsed_instance("gorilla", gorilla_owner, RAGING_GORILLA);
        let bear = test_instance(
            "bear",
            test_definition("bear", "Creature — Bear"),
            bear_owner,
        );
        let (attacker, blocker) = if gorilla_attacks {
            ("gorilla", "bear")
        } else {
            ("bear", "gorilla")
        };
        let gorilla_index = usize::from(!gorilla_attacks);
        engine.state.players[gorilla_index].battlefield = vec![gorilla];
        engine.state.players[1 - gorilla_index].battlefield = vec![bear];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine
            .run_combat(&mut OneBlockProvider { attacker, blocker })
            .expect("combat completes");
        assert_eq!(
            source_triggers(&engine, "gorilla"),
            1,
            "one occurrence of the union when the gorilla attacks={gorilla_attacks}"
        );
        assert_eq!(source_triggers(&engine, "bear"), 0);
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_step_owned_by_another_player_binds_that_player_as_the_event_player() {
    // (card, cards drawn and life lost by the active player, whether the
    // step of player 0 and of player 1 triggers).
    for (card, drawn, life_lost, triggering_steps) in [
        (
            (
                "Seizan, Perverter of Truth",
                "Legendary Creature — Human Wizard",
                "{1}{B}{B}",
                "At the beginning of each player's upkeep, that player loses 2 life and draws two cards.",
            ),
            2,
            2,
            [true, true],
        ),
        // Unused combination: opponent owner with the same event player.
        (
            (
                "Opponent Upkeep Probe",
                "Enchantment",
                "{1}{U}",
                "At the beginning of each opponent's upkeep, that player draws a card.",
            ),
            1,
            0,
            [false, true],
        ),
    ] {
        for (active, triggers) in [0, 1].into_iter().zip(triggering_steps) {
            let mut engine = test_engine(2);
            engine.state.players[0].battlefield =
                vec![parsed_instance("observer", "player-0", card)];
            for player in 0..2 {
                engine.state.players[player].hand.clear();
                engine.state.players[player].library = (0..2)
                    .map(|index| {
                        let id = format!("library-{player}-{index}");
                        test_instance(
                            &id,
                            test_definition(&id, "Instant"),
                            &format!("player-{player}"),
                        )
                    })
                    .collect();
            }
            engine.state.active_player = active;
            engine
                .admit_explicit_test_fixture_objects()
                .expect("fixture objects are admitted");
            engine.enqueue_step_triggers("upkeep");
            assert_eq!(
                engine.state.stack.len(),
                usize::from(triggers),
                "{}",
                card.0
            );
            if triggers {
                engine
                    .resolve_top_stack(&mut EmeritusDecisionProvider)
                    .expect("the step trigger resolves");
            }
            for player in 0..2 {
                let affected = triggers && player == active;
                assert_eq!(
                    engine.state.players[player].hand.len(),
                    if affected { drawn } else { 0 },
                    "{}: active {active}, player {player}",
                    card.0
                );
                assert_eq!(
                    engine.state.players[player].life,
                    20 - if affected { life_lost } else { 0 },
                    "{}: active {active}, player {player}",
                    card.0
                );
            }
        }
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_sorcery_cycling_trigger_resolves_before_the_cycling_draw() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    let stir = parsed_instance(
        "stir",
        "player-0",
        (
            "Stir the Sands",
            "Sorcery",
            "{4}{B}{B}",
            "Create three 2/2 black Zombie creature tokens.\nCycling {3}{B} ({3}{B}, Discard this card: Draw a card.)\nWhen you cycle this card, create a 2/2 black Zombie creature token.",
        ),
    );
    engine.state.players[0].hand = vec![stir];
    engine.state.players[0].library = vec![test_instance(
        "next-card",
        test_definition("next-card", "Instant"),
        "player-0",
    )];
    engine.state.players[0].battlefield = vec![
        mana_source("swamp", "Basic Land - Swamp", "{B}", None),
        mana_source("plains-a", "Basic Land - Plains", "{W}", None),
        mana_source("plains-b", "Basic Land - Plains", "{W}", None),
        mana_source("plains-c", "Basic Land - Plains", "{W}", None),
    ];
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| action.decisions.get("cycling") == Some(&Value::Bool(true)))
        .expect("cycling is a legal hand activation");
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .apply_priority_action(&action, &mut EmeritusDecisionProvider)
        .expect("cycling pays and discards");
    assert_eq!(engine.state.stack.len(), 2, "cycling and its trigger");

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the cycling trigger resolves first");
    let zombies = |engine: &GameEngine| {
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|permanent| permanent.definition.is_token)
            .count()
    };
    assert_eq!(zombies(&engine), 1);
    assert!(engine.state.players[0].hand.is_empty());
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("cycling draws");
    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert_eq!(
        zombies(&engine),
        1,
        "the spell ability itself never resolved"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_second_main_phase_is_the_main_phase_with_ordinal_two() {
    // Defiant Survivor's frame (CR 505.1a) with an unconditional body: the
    // precombat main phase is the first; the postcombat one is the second.
    let card = (
        "Second Main Probe",
        "Enchantment",
        "{1}{G}",
        "At the beginning of your second main phase, draw a card.",
    );
    let rules = parsed_rules(card.0, card.1, card.2, card.3);
    assert_eq!(rules[0]["event"]["mainPhaseOrdinal"], 2, "{:#}", rules[0]);
    for (step, triggers) in [("precombatMain", 0), ("postcombatMain", 1)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![parsed_instance("observer", "player-0", card)];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine.enqueue_step_triggers(step);
        assert_eq!(engine.state.stack.len(), triggers, "{step}");
    }
    // Near miss: an ordinal no main step of the turn has is not executable.
    let mut third = rules[0].clone();
    third["event"]["mainPhaseOrdinal"] = json!(3);
    assert!(!rule_is_executable(&third), "{third:#}");
}
