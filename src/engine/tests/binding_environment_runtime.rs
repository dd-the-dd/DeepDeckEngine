//! Real Oracle text -> primitive IR -> engine behavior for references the
//! shared binding environment resolves (docs/oracle-binding-environment.md):
//! the player an event records and the player related to a declared target,
//! read as last known information once it changed zones (CR 608.2h).
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Binding Probe".to_string(),
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

fn permanent(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.rules = rules;
    test_instance(id, definition, owner)
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn that_player_is_the_player_the_draw_event_records() {
    // Underworld Dreams: the drawing opponent is dealt the damage; a draw by
    // the controller does not trigger.
    let rules = parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, this enchantment deals 1 damage to that player.",
    );
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(permanent("dreams", "Enchantment", rules, "player-0"));
    engine.draw_cards("player-2", 1).expect("player-2 draws");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[2].life, 19, "the drawing opponent");
    assert_eq!(engine.state.players[1].life, 20);
    engine
        .draw_cards("player-0", 1)
        .expect("the controller draws");
    assert!(engine.state.stack.is_empty(), "no trigger on its own draw");
    assert_eq!(engine.state.players[0].life, 20);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn possessive_controller_of_a_destroyed_target_is_its_last_known_controller() {
    // Consign to the Pit: the creature owned by player-1 and controlled by
    // player-2 is destroyed first; its controller when it left is dealt the
    // damage, not its owner.
    let rule = parsed_rules(
        "Sorcery",
        "Destroy target creature. This spell deals 2 damage to that creature's controller.",
    )
    .remove(0);
    let mut engine = test_engine(3);
    let mut bear = test_instance(
        "borrowed-bear",
        test_definition("borrowed-bear", "Creature - Bear"),
        "player-1",
    );
    bear.controller = "player-2".to_string();
    engine.state.players[2].battlefield.push(bear);
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:consign".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("consign", test_definition("consign", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "borrowed-bear".to_string(),
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
    assert!(engine.permanent_position("borrowed-bear").is_none());
    assert_eq!(engine.state.players[2].life, 18, "the last controller");
    assert_eq!(engine.state.players[1].life, 20, "not the owner");
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn event_object_controller_is_its_last_known_controller() {
    // Dingus Staff: the creature owned by player-1 and controlled by
    // player-2 dies; its controller as it last existed is dealt the damage.
    let rules = parsed_rules(
        "Artifact",
        "Whenever a creature dies, this artifact deals 2 damage to that creature's controller.",
    );
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(permanent("staff", "Artifact", rules, "player-0"));
    let mut bear = permanent("borrowed-bear", "Creature - Bear", Vec::new(), "player-1");
    bear.controller = "player-2".to_string();
    engine.state.players[2].battlefield.push(bear);
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("borrowed-bear", "destroyed")
        .expect("the creature dies");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[2].life, 18, "the last controller");
    assert_eq!(engine.state.players[1].life, 20, "not the owner");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn event_object_is_only_the_incarnation_the_event_saw() {
    // Stinkweed Imp: `that creature` is the damaged creature, and only while
    // it is the same object (CR 400.7).
    let rule = parsed_rules(
        "Creature - Imp",
        "Whenever this creature deals combat damage to a creature, destroy that creature.",
    )
    .remove(0);
    for blinked in [false, true] {
        let mut engine = test_engine(2);
        let imp = permanent("imp", "Creature - Imp", vec![rule.clone()], "player-0");
        engine.state.players[0].battlefield.push(imp.clone());
        engine.state.players[1].battlefield.push(permanent(
            "victim",
            "Creature - Bear",
            Vec::new(),
            "player-1",
        ));
        admit(&mut engine);
        engine.put_triggered_ability_on_stack_with_decisions(
            &imp,
            rule.clone(),
            BTreeMap::from([(
                "triggeringPermanentId".to_string(),
                Value::String("victim".to_string()),
            )]),
        );
        if blinked {
            let (player, index) = engine.permanent_position("victim").expect("victim");
            GameEngine::assign_tracked_zone_identity(
                &mut engine.state.players[player].battlefield[index],
                "a-new-object",
            );
        }
        resolve_stack(&mut engine);
        assert_eq!(
            engine.permanent_position("victim").is_some(),
            blinked,
            "blinked: {blinked}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn it_is_the_attacking_creature_the_event_records() {
    // Fervent Charge: the attacker the record binds gets +2/+2.
    let rule = parsed_rules(
        "Enchantment",
        "Whenever a creature you control attacks, it gets +2/+2 until end of turn.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let charge = permanent("charge", "Enchantment", vec![rule.clone()], "player-0");
    engine.state.players[0].battlefield.push(charge.clone());
    engine.state.players[0].battlefield.push(permanent(
        "attacker",
        "Creature - Bear",
        Vec::new(),
        "player-0",
    ));
    engine.state.players[0].battlefield.push(permanent(
        "bystander",
        "Creature - Bear",
        Vec::new(),
        "player-0",
    ));
    admit(&mut engine);
    engine.put_triggered_ability_on_stack_with_decisions(
        &charge,
        rule,
        BTreeMap::from([(
            "triggeringPermanentId".to_string(),
            Value::String("attacker".to_string()),
        )]),
    );
    resolve_stack(&mut engine);
    let power = |engine: &GameEngine, id: &str| {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.current_power(&engine.state.players[player].battlefield[index])
    };
    let base = power(&engine, "bystander");
    assert_eq!(
        power(&engine, "attacker"),
        base + 2,
        "the recorded attacker"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn genitive_power_of_a_dead_creature_is_its_last_known_power() {
    // Kresh-like: the dying creature's power is read as it last existed on
    // the battlefield (CR 608.2h), with its pump still applied.
    let rules = parsed_rules(
        "Enchantment",
        "Whenever another creature dies, you gain life equal to that creature's power.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(permanent(
        "watcher",
        "Enchantment",
        rules,
        "player-0",
    ));
    let mut bear = permanent("pumped-bear", "Creature - Bear", Vec::new(), "player-1");
    bear.power_modifier = 3;
    let expected = 20 + engine.current_power(&bear);
    engine.state.players[1].battlefield.push(bear);
    admit(&mut engine);
    engine
        .move_permanent_to_graveyard("pumped-bear", "destroyed")
        .expect("the creature dies");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, expected);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn it_is_the_card_the_death_put_into_the_graveyard() {
    // Endless Cockroaches: the card returns from the graveyard to its
    // owner's hand, but only while it is still the card the death made
    // (CR 400.7): moved elsewhere first, it is a new object.
    let rules = parsed_rules(
        "Creature - Insect",
        "When this creature dies, return it to its owner's hand.",
    );
    for moved_first in [false, true] {
        let mut engine = test_engine(2);
        engine.state.players[1].battlefield.push(permanent(
            "roach",
            "Creature - Insect",
            rules.clone(),
            "player-1",
        ));
        admit(&mut engine);
        engine
            .move_permanent_to_graveyard("roach", "destroyed")
            .expect("the creature dies");
        if moved_first {
            let card = engine.state.players[1].graveyard.pop().expect("dead card");
            engine.state.players[1].exile.push(card);
        }
        resolve_stack(&mut engine);
        let in_hand = engine.state.players[1]
            .hand
            .iter()
            .any(|card| card.instance_id == "roach");
        assert_eq!(in_hand, !moved_first, "moved first: {moved_first}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_exiled_card_returns_through_the_linked_ability() {
    // Journey to Nowhere's shape: the entry exile records the link; leaving
    // returns that card under its owner's control (CR 607.2a).
    let rules = parsed_rules(
        "Enchantment",
        "When this enchantment enters, exile target creature.\nWhen this enchantment leaves the battlefield, return the exiled card to the battlefield under its owner's control.",
    );
    let mut engine = test_engine(2);
    let journey = permanent("journey", "Enchantment", rules.clone(), "player-0");
    engine.state.players[0].battlefield.push(journey.clone());
    engine.state.players[1].battlefield.push(permanent(
        "captive",
        "Creature - Bear",
        Vec::new(),
        "player-1",
    ));
    admit(&mut engine);
    let entry = &rules[0];
    let target_id = entry["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let (player, index) = engine.permanent_position("journey").expect("journey");
    let live = engine.state.players[player].battlefield[index].clone();
    let stack = StackObject {
        id: "stack:journey-entry".to_string(),
        controller: "player-0".to_string(),
        card: live,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(entry.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "captive".to_string(),
            },
        )]),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &entry["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the exile resolves");
    assert!(engine.card_zone_owner_id("captive", "exile").is_some());
    engine
        .move_permanent_to_graveyard("journey", "destroyed")
        .expect("the enchantment leaves");
    resolve_stack(&mut engine);
    let (player, _) = engine
        .permanent_position("captive")
        .expect("the linked card returned");
    assert_eq!(
        engine.state.players[player].id, "player-1",
        "owner's control"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn where_x_reads_the_destroyed_targets_last_known_mana_value() {
    // Death Mutation's shape: X is read after the target was destroyed.
    let rule = parsed_rules(
        "Sorcery",
        "Destroy target creature. You gain X life, where X is that creature's mana value.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let mut definition = test_definition("ogre", "Creature - Ogre");
    definition.mana_cost = "{3}{R}".to_string();
    engine.state.players[1]
        .battlefield
        .push(test_instance("ogre", definition, "player-1"));
    admit(&mut engine);
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    let stack = StackObject {
        id: "stack:mutation".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "mutation",
            test_definition("mutation", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            target_id,
            TargetRef::Permanent {
                instance_id: "ogre".to_string(),
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
    assert!(engine.permanent_position("ogre").is_none());
    assert_eq!(
        engine.state.players[0].life, 24,
        "mana value 4 as last known"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn from_among_them_reads_the_milled_cards() {
    // Vastlands Scavenger's shape: the milled creature card is put into the
    // hand; a creature card already in the graveyard is not a candidate.
    let rule = parsed_rules(
        "Sorcery",
        "Mill two cards, then put a creature card from among them into your hand.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        "old-bear",
        test_definition("old-bear", "Creature - Bear"),
        "player-0",
    ));
    engine.state.players[0].library.push(test_instance(
        "milled-bear",
        test_definition("milled-bear", "Creature - Bear"),
        "player-0",
    ));
    admit(&mut engine);
    let stack = StackObject {
        id: "stack:scavenger".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "scavenger",
            test_definition("scavenger", "Sorcery"),
            "player-0",
        ),
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
    let in_hand = |id: &str| {
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == id)
    };
    assert!(in_hand("milled-bear"), "the milled creature card");
    assert!(!in_hand("old-bear"), "not a card milled by this spell");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_cast_spells_controller_is_its_caster_once_it_left_the_stack() {
    // The spell already resolved: its controller is the caster the cast
    // record binds (CR 110.2).
    let rule = parsed_rules(
        "Enchantment",
        "Whenever an opponent casts a spell, this enchantment deals 1 damage to that spell's controller.",
    )
    .remove(0);
    let mut engine = test_engine(2);
    let source = permanent("watcher", "Enchantment", vec![rule.clone()], "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    admit(&mut engine);
    engine.put_triggered_ability_on_stack_with_decisions(
        &source,
        rule,
        BTreeMap::from([
            (
                "triggeringPlayerId".to_string(),
                Value::String("player-1".to_string()),
            ),
            (
                "triggeringStackId".to_string(),
                Value::String("stack:resolved-spell".to_string()),
            ),
        ]),
    );
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[1].life, 19, "the caster");
    assert_eq!(engine.state.players[0].life, 20);
}
