use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    let creature = type_line.contains("Creature");
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: id.to_string(),
        power: creature.then(|| "2".to_string()),
        rules,
        toughness: creature.then(|| "2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn instance(id: &str, type_line: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let definition = definition(id, type_line, rules);
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.to_string();
    instance
}

/// The rules the Oracle compiler produces for a printed landwalk line.
fn compiled_rules(oracle: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Walker".to_string(),
        type_line: "Creature - Zombie".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("canonical ability has a rule"))
        .collect()
}

fn attacking(engine: &mut GameEngine, attacker: &CardInstance) {
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker.instance_id.clone(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
}

fn defender_controls(engine: &mut GameEngine, id: &str, type_line: &str) {
    engine.state.players[1]
        .battlefield
        .push(instance(id, type_line, Vec::new(), "player-1"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn printed_desertwalk_with_reminder_makes_the_attacker_unblockable_by_a_desert_controller() {
    let mut engine = test_engine(2);
    let rules = compiled_rules(
        "Desertwalk (This creature can't be blocked as long as defending player controls a Desert.)",
    );
    let attacker = instance("desertwalker", "Creature - Human", rules, "player-0");
    let blocker = instance("blocker", "Creature - Soldier", Vec::new(), "player-1");
    attacking(&mut engine, &attacker);

    assert!(engine.can_block(&blocker, &attacker));
    defender_controls(&mut engine, "other-land", "Basic Land - Swamp");
    assert!(
        engine.can_block(&blocker, &attacker),
        "an unrelated land type does not enable the walk"
    );
    defender_controls(&mut engine, "the-desert", "Land - Desert");
    assert!(!engine.can_block(&blocker, &attacker));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_land_subtype_with_no_printed_walker_uses_the_same_restriction() {
    let mut engine = test_engine(2);
    let rules = compiled_rules(
        "Gatewalk (This creature can't be blocked as long as defending player controls a Gate.)",
    );
    let attacker = instance("gatewalker", "Creature - Human", rules, "player-0");
    let blocker = instance("blocker", "Creature - Soldier", Vec::new(), "player-1");
    attacking(&mut engine, &attacker);

    defender_controls(&mut engine, "desert", "Land - Desert");
    assert!(engine.can_block(&blocker, &attacker));
    defender_controls(&mut engine, "gate", "Land - Gate");
    assert!(!engine.can_block(&blocker, &attacker));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn landwalk_checks_the_defending_player_and_the_walkers_own_type_only() {
    let mut engine = test_engine(2);
    let rules = compiled_rules(
        "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp.)",
    );
    let attacker = instance("swampwalker", "Creature - Zombie", rules, "player-0");
    let blocker = instance("blocker", "Creature - Soldier", Vec::new(), "player-1");
    attacking(&mut engine, &attacker);

    // The attacking player's own Swamp is irrelevant (CR 702.14c).
    engine.state.players[0].battlefield.push(instance(
        "own-swamp",
        "Basic Land - Swamp",
        Vec::new(),
        "player-0",
    ));
    assert!(engine.can_block(&blocker, &attacker));
    defender_controls(&mut engine, "island", "Basic Land - Island");
    assert!(engine.can_block(&blocker, &attacker));
    defender_controls(&mut engine, "swamp", "Basic Land - Swamp");
    assert!(!engine.can_block(&blocker, &attacker));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn landwalk_keyword_support_follows_the_shared_land_subtype_vocabulary() {
    for kind in ["swampwalk", "desertwalk", "townwalk", "gatewalk"] {
        assert!(keyword_kind_supported(&json!({ "kind": kind })), "{kind}");
    }
    for kind in [
        "landwalk",
        "planeswalk",
        "urza'swalk",
        "walk",
        "creaturewalk",
    ] {
        assert!(!keyword_kind_supported(&json!({ "kind": kind })), "{kind}");
    }
}
