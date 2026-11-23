//! Real Oracle text -> primitive IR -> engine behavior for effect sentences
//! with an explicit subject (docs/oracle-clause-layer.md, phase 6): the
//! player the subject names is the one the action changes.
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Subject Probe".to_string(),
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
    }
}

fn counters(engine: &GameEngine, player: usize, counter: &str) -> i64 {
    engine.state.players[player]
        .counters
        .get(counter)
        .copied()
        .map_or(0, i64::from)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn player_counters_go_to_the_player_the_subject_names() {
    // Unused recombinations of Pit Scorpion and Aethertorch Renegade: the
    // event's player gets the poison counter, the controller the energy.
    let mut rules = parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, that player gets a poison counter.",
    );
    rules.extend(parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, you get {E}{E} (two energy counters).",
    ));
    let mut definition = test_definition("subject", "Enchantment");
    definition.rules = rules;
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(test_instance("subject", definition, "player-0"));
    engine.draw_cards("player-2", 1).expect("player-2 draws");
    resolve_stack(&mut engine);
    assert_eq!(counters(&engine, 2, "poison"), 1, "the drawing opponent");
    assert_eq!(counters(&engine, 1, "poison"), 0);
    assert_eq!(counters(&engine, 0, "energy"), 2, "the controller");
    assert_eq!(counters(&engine, 2, "energy"), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn they_is_the_player_the_event_records() {
    // Sheoldred, the Apocalypse's opponent clause: the drawing opponent loses
    // the life, not the controller nor the other opponent.
    let rules = parsed_rules(
        "Creature \u{2014} Phyrexian Praetor",
        "Whenever an opponent draws a card, they lose 2 life.",
    );
    let mut definition = test_definition("sheoldred", "Creature");
    definition.rules = rules;
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(test_instance("sheoldred", definition, "player-0"));
    engine.draw_cards("player-1", 1).expect("player-1 draws");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[1].life, 18, "the drawing opponent");
    assert_eq!(engine.state.players[2].life, 20);
    assert_eq!(engine.state.players[0].life, 20);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_object_subject_that_does_not_untap_skips_its_next_untap_step() {
    // Chill of the Grave's restriction after a tap: the tapped target gets
    // the engine's next-untap-step restriction (CR 502.3).
    let rule = parsed_rules(
        "Instant",
        "Tap target creature. It doesn't untap during its controller's next untap step.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-1",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:chill".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("chill", test_definition("chill", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "bear".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    assert!(engine.state.players[1].battlefield[0].tapped, "tapped");
    assert!(
        engine.state.rule_modifiers.iter().any(|modifier| {
            modifier["kind"] == "skipNextUntap" && modifier["cardInstanceId"] == "bear"
        }),
        "{:#?}",
        engine.state.rule_modifiers
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_returned_card_is_the_permanent_the_next_action_changes() {
    // CR 400.7: the card returned to the battlefield is a new object the
    // returning effect finds; the counter goes on that permanent.
    let rule = parsed_rules(
        "Sorcery",
        "Return target creature card from your graveyard to the battlefield. Put a +1/+1 counter on it.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:revival".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("revival", test_definition("revival", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Card {
                instance_id: "bear".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    let permanent = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.definition.id == "bear")
        .expect("the returned creature");
    assert_eq!(
        permanent.counters.get("+1/+1").copied(),
        Some(1),
        "{rule:#}"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_performer_creates_the_token_it_controls() {
    // Goblin Spymaster's recombination with a draw event: the drawing
    // opponent performs the creation, so the token is theirs (CR 111.2).
    let rules = parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, that player creates a 1/1 red Goblin creature token.",
    );
    let mut definition = test_definition("spymaster", "Enchantment");
    definition.rules = rules;
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(test_instance("spymaster", definition, "player-0"));
    engine.draw_cards("player-2", 1).expect("player-2 draws");
    resolve_stack(&mut engine);
    assert_eq!(
        engine.state.players[2].battlefield.len(),
        1,
        "the drawing opponent's token"
    );
    assert_eq!(
        engine.state.players[0].battlefield.len(),
        1,
        "only the enchantment"
    );
    assert!(engine.state.players[1].battlefield.is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn each_player_of_a_population_performs_the_action_with_their_own_cards() {
    // Exhume: each player puts a creature card from their own graveyard onto
    // the battlefield; the choice and the card are that player's (CR 101.4).
    let rule = parsed_rules(
        "Sorcery",
        "Each player puts a creature card from their graveyard onto the battlefield.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    for (index, id) in [(0, "bear-0"), (1, "bear-1")] {
        let owner = format!("player-{index}");
        engine.state.players[index].graveyard.push(test_instance(
            id,
            test_definition(id, "Creature - Bear"),
            &owner,
        ));
    }
    let stack = StackObject {
        id: "stack:exhume".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("exhume", test_definition("exhume", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    for (index, id) in [(0, "bear-0"), (1, "bear-1")] {
        assert!(
            engine.state.players[index]
                .battlefield
                .iter()
                .any(|permanent| permanent.instance_id == id),
            "{id} on its owner's battlefield: {rule:#}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_grant_without_a_duration_is_not_a_temporary_one() {
    // CR 611.2a: `Target creature gains flying.` lasts indefinitely; the
    // keyword is granted without the until-end-of-turn expiry.
    let rule = parsed_rules("Sorcery", "Target creature gains flying.").remove(0);
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-1",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:grant".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("grant", test_definition("grant", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "bear".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    let granted = engine.state.players[1].battlefield[0]
        .definition
        .rules
        .iter()
        .find(|rule| rule["kind"] == "keywordAbility" && rule["ability"]["kind"] == "flying")
        .cloned()
        .expect("flying granted");
    assert!(granted.get("temporaryUntilTurn").is_none(), "{granted:#}");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_returned_card_is_exiled_by_the_delayed_action_that_captured_it() {
    // Goryo's Vengeance: the delayed trigger captures the returned
    // permanent's identity when it is created (CR 603.7c) and exiles that
    // object at the next end step.
    let rule = parsed_rules(
        "Instant",
        "Return target legendary creature card from your graveyard to the battlefield. That creature gains haste. Exile it at the beginning of the next end step.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        "emrakul",
        test_definition("emrakul", "Legendary Creature - Eldrazi"),
        "player-0",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:goryo".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("goryo", test_definition("goryo", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Card {
                instance_id: "emrakul".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    assert!(engine.permanent_position("emrakul").is_some(), "returned");
    let modifiers = format!("{:#?}", engine.state.rule_modifiers);
    engine.enqueue_step_triggers("endStep");
    let stack = engine.state.stack.len();
    resolve_stack(&mut engine);
    assert!(
        engine.permanent_position("emrakul").is_none(),
        "stack {stack}: {modifiers}"
    );
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "emrakul"),
        "exiled at the end step"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_deployed_creature_has_haste_and_is_sacrificed_at_the_end_step() {
    // Sneak Attack's ability as a spell body: the deployed card keeps haste
    // without a duration (CR 611.2a), and the delayed sacrifice finds the
    // permanent it captured (CR 603.7c).
    let rule = parsed_rules(
        "Sorcery",
        "You may put a creature card from your hand onto the battlefield. That creature gains haste. Sacrifice the creature at the beginning of the next end step.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].hand.push(test_instance(
        "giant",
        test_definition("giant", "Creature - Giant"),
        "player-0",
    ));
    let stack = StackObject {
        id: "stack:sneak".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("sneak", test_definition("sneak", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    let (player, index) = engine
        .permanent_position("giant")
        .unwrap_or_else(|| panic!("deployed: {rule:#}"));
    assert!(
        engine.state.players[player].battlefield[index]
            .definition
            .rules
            .iter()
            .any(|rule| rule["kind"] == "keywordAbility" && rule["ability"]["kind"] == "haste"),
        "haste"
    );
    engine.enqueue_step_triggers("endStep");
    resolve_stack(&mut engine);
    assert!(engine.permanent_position("giant").is_none(), "sacrificed");
}

/// The permanent leaves and returns as a new object (CR 400.7).
fn new_incarnation(engine: &mut GameEngine, id: &str) {
    let (player_index, card_index) = engine.permanent_position(id).expect("permanent");
    let mut card = engine.state.players[player_index].battlefield[card_index].clone();
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("a new incarnation");
    engine.state.players[player_index].battlefield[card_index] = card;
}

fn returned_to_battlefield(oracle: &str, id: &str) -> (GameEngine, Value) {
    let rule = parsed_rules("Instant", oracle).remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        id,
        test_definition(id, "Legendary Creature - Eldrazi"),
        "player-0",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:return".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("spell", test_definition("spell", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Card {
                instance_id: id.to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
    (engine, rule)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_flickered_creature_is_a_new_object_the_delayed_exile_does_not_find() {
    // Goryo's Vengeance + a flicker before the end step: the delayed trigger
    // captured the old object; the new one stays (CR 400.7, 603.7c).
    let (mut engine, _) = returned_to_battlefield(
        "Return target legendary creature card from your graveyard to the battlefield. That creature gains haste. Exile it at the beginning of the next end step.",
        "emrakul",
    );
    new_incarnation(&mut engine, "emrakul");
    engine.enqueue_step_triggers("endStep");
    resolve_stack(&mut engine);
    assert!(
        engine.permanent_position("emrakul").is_some(),
        "a new object"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_returned_creature_another_player_controls_is_not_sacrificed() {
    // CR 701.21a: only its controller sacrifices a permanent; after a control
    // change the delayed sacrifice of the returned creature does nothing.
    let (mut engine, _) = returned_to_battlefield(
        "Return target creature card from your graveyard to the battlefield. Sacrifice it at the beginning of the next end step.",
        "bear",
    );
    let (player, index) = engine.permanent_position("bear").expect("returned");
    engine.state.players[player].battlefield[index].controller = "player-1".to_string();
    engine.enqueue_step_triggers("endStep");
    resolve_stack(&mut engine);
    assert!(
        engine.permanent_position("bear").is_some(),
        "not the thief's sacrifice"
    );
}

fn resolve_spell(engine: &mut GameEngine, rule: &Value, targets: BTreeMap<String, TargetRef>) {
    let stack = StackObject {
        id: "stack:spell".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("spell", test_definition("spell", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    };
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_empty_deployment_binds_nothing_and_its_delayed_sacrifice_does_nothing() {
    // Sneak Attack with an empty hand: nothing is deployed, the haste grant
    // and the delayed sacrifice read an empty capture and do nothing.
    let rule = parsed_rules(
        "Sorcery",
        "You may put a creature card from your hand onto the battlefield. That creature gains haste. Sacrifice the creature at the beginning of the next end step.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(test_instance(
        "bystander",
        test_definition("bystander", "Creature - Bear"),
        "player-0",
    ));
    resolve_spell(&mut engine, &rule, BTreeMap::new());
    engine.enqueue_step_triggers("endStep");
    resolve_stack(&mut engine);
    assert!(
        engine.permanent_position("bystander").is_some(),
        "untouched"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_restriction_without_a_duration_still_applies() {
    // `Target creature can't block.` (CR 611.2a: no stated duration).
    let rule = parsed_rules("Instant", "Target creature can't block.").remove(0);
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(test_instance(
        "wall",
        test_definition("wall", "Creature - Wall"),
        "player-1",
    ));
    engine.state.players[0].battlefield.push(test_instance(
        "bear",
        test_definition("bear", "Creature - Bear"),
        "player-0",
    ));
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    resolve_spell(
        &mut engine,
        &rule,
        BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "wall".to_string(),
            },
        )]),
    );
    // The legal-block enumeration excludes a creature with `cantBlock`.
    let wall = engine.state.players[1].battlefield[0].clone();
    assert!(engine.permanent_has_keyword(&wall, "cantBlock"), "{rule:#}");
}
