//! Real Oracle text -> player-action trigger IR -> engine behavior: the
//! acting player of a discard (CR 701.9), a cycling activation that discards
//! its card as a cost (CR 702.29a), and a completed scry (CR 701.22d) are
//! event attributes matched against each observer's player reference.
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
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

fn observer(instance_id: &str, owner: &str, name: &str, oracle: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, "Enchantment");
    definition.name = name.to_string();
    definition.rules = parsed_rules(name, "Enchantment", oracle);
    test_instance(instance_id, definition, owner)
}

fn hand_card(instance_id: &str, owner: &str, type_line: &str) -> CardInstance {
    test_instance(instance_id, test_definition(instance_id, type_line), owner)
}

fn triggers_from(engine: &GameEngine, instance_id: &str) -> usize {
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
fn an_opponent_discard_binds_the_discarding_player_and_filters_the_card() {
    // Liliana's Caress, plus an unused combination: any player, a creature card.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        observer(
            "caress",
            "player-0",
            "Liliana's Caress",
            "Whenever an opponent discards a card, that player loses 2 life.",
        ),
        observer(
            "creature-discard",
            "player-0",
            "Creature Discard Probe",
            "Whenever a player discards a creature card, you gain 3 life.",
        ),
    ];
    engine.state.players[0].hand = vec![hand_card("own-bear", "player-0", "Creature — Bear")];
    engine.state.players[1].hand = vec![
        hand_card("their-bolt", "player-1", "Instant"),
        hand_card("their-bear", "player-1", "Creature — Bear"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    engine
        .discard_card_as_cost("player-0", "own-bear")
        .expect("the controller discards");
    assert_eq!(triggers_from(&engine, "caress"), 0, "not an opponent");
    assert_eq!(triggers_from(&engine, "creature-discard"), 1);

    engine
        .discard_card_as_cost("player-1", "their-bolt")
        .expect("the opponent discards an instant");
    assert_eq!(triggers_from(&engine, "caress"), 1);
    assert_eq!(
        triggers_from(&engine, "creature-discard"),
        1,
        "not a creature card"
    );

    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("discard triggers resolve");
    }
    assert_eq!(engine.state.players[1].life, 18, "that player loses 2");
    assert_eq!(
        engine.state.players[0].life, 23,
        "one creature card was discarded"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn cycling_discards_its_card_and_triggers_cycle_or_discard_once() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    let mut cycler = test_definition("cycler", "Sorcery");
    cycler.name = "Cycler".to_string();
    cycler.rules = parsed_rules(
        "Cycler",
        "Sorcery",
        "Draw a card.\nCycling {1} ({1}, Discard this card: Draw a card.)",
    );
    engine.state.players[0].hand = vec![test_instance("cycler", cycler, "player-0")];
    engine.state.players[0].library = vec![hand_card("next-card", "player-0", "Instant")];
    engine.state.players[0].battlefield = vec![
        mana_source("plains", "Basic Land - Plains", "{W}", None),
        // Archfiend of Ifnir's event with an unused body.
        observer(
            "cycle-or-discard",
            "player-0",
            "Cycle Or Discard Probe",
            "Whenever you cycle or discard another card, you gain 1 life.",
        ),
        observer(
            "cycle-only",
            "player-0",
            "Cycle Probe",
            "Whenever you cycle a card, you gain 2 life.",
        ),
        // The opponent's observer of the controller's actions does not fire.
        observer(
            "opponent-cycles",
            "player-0",
            "Opponent Cycle Probe",
            "Whenever an opponent cycles a card, you gain 5 life.",
        ),
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
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardDiscarded" && event.card_instance_id.as_deref() == Some("cycler")
    }));
    assert_eq!(
        triggers_from(&engine, "cycle-or-discard"),
        1,
        "one event, not two"
    );
    assert_eq!(triggers_from(&engine, "cycle-only"), 1);
    assert_eq!(triggers_from(&engine, "opponent-cycles"), 0);
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("triggers and cycling resolve");
    }
    assert_eq!(engine.state.players[0].life, 23);
    assert_eq!(engine.state.players[0].hand.len(), 1, "cycling drew");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_completed_scry_triggers_observers_of_the_scrying_player() {
    for (scrying, own_triggers, opponent_triggers) in [(0, 1_i32, 0_i32), (1, 0, 1)] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = vec![
            // Chance-Met Elves' event with its once-each-turn limit.
            observer(
                "elves",
                "player-0",
                "Scry Probe",
                "Whenever you scry, you gain 1 life. This ability triggers only once each turn.",
            ),
            // Unused combination: an opponent actor and `that player`.
            observer(
                "opponent-scries",
                "player-0",
                "Opponent Scry Probe",
                "Whenever an opponent scries or surveils, that player loses 2 life.",
            ),
        ];
        let scryer = format!("player-{scrying}");
        engine.state.players[scrying].library = (0..3)
            .map(|index| hand_card(&format!("library-{index}"), &scryer, "Instant"))
            .collect();
        let source = StackObject {
            id: "stack:scry".to_string(),
            controller: scryer.clone(),
            card: hand_card("scry-source", &scryer, "Instant"),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("spellAbility".to_string()),
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        };
        let scry = parsed_rules("Scry Source", "Instant", "Scry 2.")
            .into_iter()
            .next()
            .expect("Scry 2 compiles");
        for _ in 0..2 {
            execute_effects_at_resolution_start(
                &mut engine,
                &scry["effects"],
                &source,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut EmeritusDecisionProvider,
            )
            .expect("scry resolves");
        }
        assert_eq!(
            triggers_from(&engine, "elves"),
            own_triggers as usize,
            "once each turn"
        );
        assert_eq!(
            triggers_from(&engine, "opponent-scries"),
            2 * opponent_triggers as usize
        );
        while !engine.state.stack.is_empty() {
            engine
                .resolve_top_stack(&mut EmeritusDecisionProvider)
                .expect("scry triggers resolve");
        }
        assert_eq!(engine.state.players[0].life, 20 + own_triggers);
        assert_eq!(engine.state.players[1].life, 20 - 4 * opponent_triggers);
    }
}
