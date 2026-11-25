//! Real Oracle text -> primitive IR -> engine behavior for the shared
//! aggregate over an object set (`aggregateObjects`, CR 107.1): an extremum
//! with ties, the distinct kinds of counters, the distinct card types of a
//! set (subtypes are not card types, CR 205.2a), the superlative holder
//! condition, and half of a player's hand chosen by that player.
use super::*;

fn parsed_rule(type_line: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Aggregate Probe".to_string(),
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
        parsed.diagnostics
    );
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn spell(rule: &Value, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:aggregate-probe".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "aggregate-probe",
            test_definition("aggregate-probe", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, targets: BTreeMap<String, TargetRef>) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &spell(rule, targets),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
}

fn permanent(id: &str, type_line: &str, power: i32, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = "{2}".to_string();
    definition.power = Some(power.to_string());
    definition.toughness = Some("4".to_string());
    test_instance(id, definition, controller)
}

fn counters(mut card: CardInstance, kinds: &[(&str, i32)]) -> CardInstance {
    for (kind, count) in kinds {
        card.counters.insert((*kind).to_string(), *count);
    }
    card
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_greatest_power_among_another_players_creatures_counts_a_tie_once() {
    // The opponent's two 3-power creatures tie for the greatest power: the
    // amount is 3, never the sum and never the controller's own 5-power one.
    let rule = parsed_rule(
        "Sorcery",
        "Draw cards equal to the greatest power among creatures your opponents control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![permanent(
        "own-giant",
        "Creature \u{2014} Giant",
        5,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        permanent("their-bear-a", "Creature \u{2014} Bear", 3, "player-1"),
        permanent("their-bear-b", "Creature \u{2014} Bear", 3, "player-1"),
        permanent("their-elf", "Creature \u{2014} Elf", 1, "player-1"),
    ];
    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, &rule, BTreeMap::new());
    assert_eq!(engine.state.players[0].hand.len(), hand + 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn each_kind_of_counter_among_your_permanents_is_counted_once() {
    // Two permanents share +1/+1 counters, one also has a charge counter and
    // the opponent's oil counter is not among the controller's permanents.
    let rule = parsed_rule(
        "Sorcery",
        "Draw a card for each kind of counter among permanents you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        counters(
            permanent("pumped", "Creature \u{2014} Bear", 2, "player-0"),
            &[("+1/+1", 2)],
        ),
        counters(
            permanent("battery", "Artifact Creature \u{2014} Golem", 1, "player-0"),
            &[("+1/+1", 1), ("charge", 3)],
        ),
        permanent("plain", "Creature \u{2014} Elf", 1, "player-0"),
    ];
    engine.state.players[1].battlefield = vec![counters(
        permanent("oiled", "Artifact Creature \u{2014} Golem", 1, "player-1"),
        &[("oil", 3)],
    )];
    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, &rule, BTreeMap::new());
    assert_eq!(engine.state.players[0].hand.len(), hand + 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn distinct_card_types_of_a_permanent_set_exclude_subtypes() {
    // Artifact, Creature and Land: `Golem` and `Forest` are subtypes and the
    // second creature adds no new card type (CR 205.2a).
    let rule = parsed_rule(
        "Sorcery",
        "Draw cards equal to the number of card types among permanents you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("golem", "Artifact Creature \u{2014} Golem", 2, "player-0"),
        permanent("bear", "Creature \u{2014} Bear", 2, "player-0"),
        permanent("forest", "Land \u{2014} Forest", 0, "player-0"),
    ];
    engine.state.players[1].battlefield = vec![permanent(
        "their-aura",
        "Enchantment \u{2014} Aura",
        0,
        "player-1",
    )];
    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, &rule, BTreeMap::new());
    assert_eq!(engine.state.players[0].hand.len(), hand + 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_superlative_holder_condition_holds_on_a_tie_and_needs_a_creature() {
    let rule = parsed_rule(
        "Sorcery",
        "Draw a card if you control the creature with the greatest power or tied for the greatest power.",
    );
    // (controller's creature power, opponent's creature power, drawn)
    for (own, theirs, drawn) in [
        (Some(3), 3, 1),
        (Some(2), 3, 0),
        (Some(4), 3, 1),
        (None, 3, 0),
    ] {
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = own
            .map(|power| {
                vec![permanent(
                    "own",
                    "Creature \u{2014} Bear",
                    power,
                    "player-0",
                )]
            })
            .unwrap_or_default();
        engine.state.players[1].battlefield = vec![permanent(
            "theirs",
            "Creature \u{2014} Bear",
            theirs,
            "player-1",
        )];
        let hand = engine.state.players[0].hand.len();
        resolve(&mut engine, &rule, BTreeMap::new());
        assert_eq!(
            engine.state.players[0].hand.len(),
            hand + drawn,
            "own {own:?}, theirs {theirs}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn half_of_their_hand_rounded_down_is_discarded_by_that_player() {
    // Five cards: half rounded down is two, chosen and discarded by the
    // declared opponent from their own hand.
    let rule = parsed_rule(
        "Sorcery",
        "Target opponent discards half the cards in their hand, rounded down.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].hand = (0..5)
        .map(|index| {
            permanent(
                &format!("their-card-{index}"),
                "Creature \u{2014} Bear",
                2,
                "player-1",
            )
        })
        .collect();
    let graveyard = engine.state.players[1].graveyard.len();
    let controller_hand = engine.state.players[0].hand.len();
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared opponent")
        .to_string();
    resolve(
        &mut engine,
        &rule,
        BTreeMap::from([(
            target_id,
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    );
    assert_eq!(engine.state.players[1].hand.len(), 3);
    assert_eq!(engine.state.players[1].graveyard.len(), graveyard + 2);
    assert_eq!(engine.state.players[0].hand.len(), controller_hand);
}
