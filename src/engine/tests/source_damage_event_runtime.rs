//! Real Oracle text -> dealt-damage trigger IR -> engine behavior: the damage
//! source (the source, an attached object, a controlled permanent), the damage
//! kind and the recipient are independent attributes of each damage event
//! (CR 120.1, 120.2), published by every damage path, the player batch included.
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

fn parsed_permanent(instance_id: &str, owner: &str, type_line: &str, oracle: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = instance_id.to_string();
    if type_line.contains("Creature") {
        definition.power = Some("2".to_string());
        definition.toughness = Some("4".to_string());
    }
    definition.rules = parsed_rules(instance_id, type_line, oracle);
    test_instance(instance_id, definition, owner)
}

fn bear(instance_id: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, "Creature — Bear");
    definition.power = Some("2".to_string());
    definition.toughness = Some("4".to_string());
    test_instance(instance_id, definition, owner)
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

fn deal(engine: &mut GameEngine, source: &str, target: TargetRef, combat: bool) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            target,
            2,
            source,
            combat,
            QuantityOrigin::RuleAction,
            None,
        )
        .expect("damage is dealt");
}

fn player(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn resolve_all(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("triggers resolve");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn the_source_observes_its_damage_by_recipient_and_kind() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        // Pit Scorpion's event with the opponent recipient of Hypnotic Specter.
        parsed_permanent(
            "specter",
            "player-0",
            "Creature — Specter",
            "Whenever this creature deals damage to an opponent, that player loses 1 life.",
        ),
        // Unused combination: combat damage to a creature recipient.
        parsed_permanent(
            "duelist",
            "player-0",
            "Creature — Human",
            "Whenever this creature deals combat damage to a creature, you gain 3 life.",
        ),
    ];
    engine.state.players[1].battlefield = vec![bear("their-bear", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    deal(&mut engine, "specter", player("player-0"), false);
    assert_eq!(triggers_from(&engine, "specter"), 0, "not an opponent");
    deal(&mut engine, "specter", player("player-1"), false);
    assert_eq!(triggers_from(&engine, "specter"), 1);
    deal(&mut engine, "duelist", permanent("their-bear"), false);
    assert_eq!(triggers_from(&engine, "duelist"), 0, "noncombat damage");
    deal(&mut engine, "duelist", permanent("their-bear"), true);
    assert_eq!(triggers_from(&engine, "duelist"), 1);
    deal(&mut engine, "duelist", player("player-1"), true);
    assert_eq!(
        triggers_from(&engine, "duelist"),
        1,
        "a player is not a creature"
    );
    resolve_all(&mut engine);
    assert_eq!(
        engine.state.players[1].life,
        20 - 2 - 1 - 2,
        "that player lost 1"
    );
    assert_eq!(engine.state.players[0].life, 20 - 2 + 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn attached_and_controlled_damage_sources_are_matched_when_they_deal_damage() {
    let mut engine = test_engine(2);
    let mut skewer = parsed_permanent(
        "skewer",
        "player-0",
        "Artifact — Equipment",
        "Whenever equipped creature deals combat damage to a creature, you gain 1 life.",
    );
    skewer.attached_to = Some("own-bear".to_string());
    engine.state.players[0].battlefield = vec![
        bear("own-bear", "player-0"),
        bear("other-bear", "player-0"),
        skewer,
        // Reconnaissance Mission's event, one trigger per damage source.
        parsed_permanent(
            "mission",
            "player-0",
            "Enchantment",
            "Whenever a creature you control deals combat damage to a player, you gain 2 life.",
        ),
    ];
    engine.state.players[1].battlefield = vec![bear("their-bear", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    deal(&mut engine, "other-bear", permanent("their-bear"), true);
    assert_eq!(
        triggers_from(&engine, "skewer"),
        0,
        "not the equipped creature"
    );
    deal(&mut engine, "own-bear", permanent("their-bear"), true);
    assert_eq!(triggers_from(&engine, "skewer"), 1);
    deal(&mut engine, "own-bear", player("player-1"), true);
    deal(&mut engine, "other-bear", player("player-1"), true);
    assert_eq!(triggers_from(&engine, "mission"), 2, "one per creature");
    deal(&mut engine, "their-bear", player("player-0"), true);
    assert_eq!(
        triggers_from(&engine, "mission"),
        2,
        "not a creature you control"
    );
    resolve_all(&mut engine);
    assert_eq!(engine.state.players[0].life, 20 - 2 + 1 + 2 * 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_player_damage_batch_publishes_one_event_per_damaged_player() {
    let mut engine = test_engine(3);
    let pinger = parsed_permanent(
        "pinger",
        "player-0",
        "Creature — Goblin",
        "Whenever this creature deals damage to an opponent, you gain 1 life.\n{T}: This creature deals 1 damage to each opponent.",
    );
    let ping = pinger
        .definition
        .rules
        .iter()
        .find(|rule| rule["kind"] == "activatedAbility")
        .expect("the ping ability")
        .clone();
    engine.state.players[0].battlefield = vec![pinger.clone()];
    let source = StackObject {
        id: "stack:ping".to_string(),
        controller: "player-0".to_string(),
        card: pinger,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(ping.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    execute_effects_at_resolution_start(
        &mut engine,
        &ping["effects"],
        &source,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the batch is not rejected by the source observer");
    assert_eq!(
        triggers_from(&engine, "pinger"),
        2,
        "one per damaged opponent"
    );
    resolve_all(&mut engine);
    assert_eq!(engine.state.players[0].life, 22);
    assert_eq!(engine.state.players[1].life, 19);
    assert_eq!(engine.state.players[2].life, 19);
}
