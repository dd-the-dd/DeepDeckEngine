use super::*;

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn combat_supports_multiple_blockers_and_current_damage_assignment() {
    let mut engine = test_engine(2);
    let mut attacker_definition = test_definition("attacker", "Creature - Test");
    attacker_definition.power = Some("4".to_string());
    attacker_definition.toughness = Some("5".to_string());
    let mut blocker_a_definition = test_definition("blocker-a", "Creature - Test");
    blocker_a_definition.power = Some("2".to_string());
    blocker_a_definition.toughness = Some("3".to_string());
    let mut blocker_b_definition = test_definition("blocker-b", "Creature - Test");
    blocker_b_definition.power = Some("2".to_string());
    blocker_b_definition.toughness = Some("1".to_string());
    engine.state.players[0].battlefield =
        vec![test_instance("attacker", attacker_definition, "player-0")];
    engine.state.players[1].battlefield = vec![
        test_instance("blocker-a", blocker_a_definition, "player-1"),
        test_instance("blocker-b", blocker_b_definition, "player-1"),
    ];
    let mut provider = MultipleBlockersDecisionProvider::default();

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .run_combat(&mut provider)
        .expect("multiple blockers and their damage assignment resolve");

    assert!(provider.saw_attack_assignment);
    assert!(provider.saw_block_assignment);
    let attacker = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "attacker")
        .expect("the 4/5 attacker survives four combat damage");
    assert_eq!(attacker.damage_marked, 4);
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "blocker-a")
    );
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "blocker-b")
    );
    let serialized = serde_json::to_value(&engine.state).expect("state serializes");
    assert_eq!(serialized["combat"]["attackers"], json!([]));
    assert_eq!(serialized["combat"]["blockers"], json!([]));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn trample_assigns_lethal_damage_in_order_then_hits_the_defending_player() {
    let mut engine = test_engine(2);
    let mut attacker_definition = test_definition("tramper", "Creature - Beast");
    attacker_definition.power = Some("7".to_string());
    attacker_definition.toughness = Some("7".to_string());
    attacker_definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "trample" },
    }));
    let mut blocker_a_definition = test_definition("blocker-a", "Creature - Test");
    blocker_a_definition.power = Some("2".to_string());
    blocker_a_definition.toughness = Some("2".to_string());
    let mut blocker_b_definition = test_definition("blocker-b", "Creature - Test");
    blocker_b_definition.power = Some("2".to_string());
    blocker_b_definition.toughness = Some("3".to_string());
    engine.state.players[0].battlefield =
        vec![test_instance("tramper", attacker_definition, "player-0")];
    engine.state.players[1].battlefield = vec![
        test_instance("blocker-a", blocker_a_definition, "player-1"),
        test_instance("blocker-b", blocker_b_definition, "player-1"),
    ];
    let attacks = BTreeMap::from([(
        "tramper".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    let blocks = BTreeMap::from([(
        "tramper".to_string(),
        vec!["blocker-a".to_string(), "blocker-b".to_string()],
    )]);

    let assignments = engine
        .choose_combat_damage_assignments(
            &mut RandomAi::seeded(1),
            &attacks,
            &blocks,
            CombatDamageStep::All,
        )
        .expect("trample damage assignment succeeds");
    assert_eq!(assignments["tramper"]["blocker-a"], 2);
    assert_eq!(assignments["tramper"]["blocker-b"], 3);

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .resolve_combat_damage(&attacks, &blocks, &assignments)
        .expect("trample damage resolves");

    assert_eq!(engine.state.players[1].life, 18);
    assert_eq!(engine.state.players[1].graveyard.len(), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn blocked_trampler_hits_its_defender_after_all_blockers_are_gone() {
    let mut engine = test_engine(2);
    let mut attacker_definition = test_definition("tramper", "Creature - Beast");
    attacker_definition.power = Some("5".to_string());
    attacker_definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "trample" },
    }));
    engine.state.players[0].battlefield =
        vec![test_instance("tramper", attacker_definition, "player-0")];
    let attacks = BTreeMap::from([(
        "tramper".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    let blocks = BTreeMap::from([("tramper".to_string(), vec!["removed-blocker".to_string()])]);

    engine
        .resolve_combat_damage(&attacks, &blocks, &BTreeMap::new())
        .expect("unopposed trample damage resolves");

    assert_eq!(engine.state.players[1].life, 15);
}

/// Feature: Unblocked combat damage is assigned to the chosen permanent defender.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn combat_damage_hits_the_selected_permanent_instead_of_its_controller() {
    let mut engine = test_engine(2);
    let attacker = test_instance(
        "attacker",
        test_definition("attacker", "Creature - Test"),
        "player-0",
    );
    let mut planeswalker = test_instance(
        "planeswalker",
        test_definition("planeswalker", "Legendary Planeswalker - Test"),
        "player-1",
    );
    planeswalker.counters.insert("loyalty".to_string(), 3);
    engine.state.players[0].battlefield = vec![attacker];
    engine.state.players[1].battlefield = vec![planeswalker];
    let attacks = BTreeMap::from([(
        "attacker".to_string(),
        AttackAssignment {
            defender: TargetRef::Permanent {
                instance_id: "planeswalker".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);

    engine
        .resolve_combat_damage(&attacks, &BTreeMap::new(), &BTreeMap::new())
        .expect("combat damage resolves");

    assert_eq!(engine.state.players[1].life, 20);
    assert_eq!(engine.state.players[1].battlefield[0].damage_marked, 2);
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "combatDamage"
            && event.card_instance_id.as_deref() == Some("planeswalker")
            && event.detail["permanentDamage"] == 2
    }));
}

/// Feature: A permanent declaration includes an opponent land and returns that exact object.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn menace_requires_two_legal_blockers_before_blocking_can_finish() {
    let mut engine = test_engine(2);
    let mut menace_definition = test_definition("menace-attacker", "Creature - Test");
    menace_definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "menace" },
    })];
    engine.state.players[0].battlefield = vec![test_instance(
        "menace-attacker",
        menace_definition,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![test_instance(
        "blocker-a",
        test_definition("blocker-a", "Creature - Test"),
        "player-1",
    )];
    let mut attackers = vec!["menace-attacker".to_string()];
    let mut used_blockers = BTreeSet::new();
    let mut blocks = BTreeMap::new();

    let one_blocker_actions = engine.legal_blocker_actions(1, &attackers, &used_blockers, &blocks);
    assert!(
        one_blocker_actions
            .iter()
            .all(|action| action.kind != ActionKind::DeclareBlocker)
    );

    engine.state.players[1].battlefield.push(test_instance(
        "blocker-b",
        test_definition("blocker-b", "Creature - Test"),
        "player-1",
    ));
    let first_block_actions = engine.legal_blocker_actions(1, &attackers, &used_blockers, &blocks);
    assert!(
        first_block_actions
            .iter()
            .any(|action| action.kind == ActionKind::DeclareBlocker)
    );
    used_blockers.insert("blocker-a".to_string());
    blocks.insert("menace-attacker".to_string(), vec!["blocker-a".to_string()]);
    engine.state.players[0].battlefield.push(test_instance(
        "normal-attacker",
        test_definition("normal-attacker", "Creature - Test"),
        "player-0",
    ));
    attackers.push("normal-attacker".to_string());
    let second_block_actions = engine.legal_blocker_actions(1, &attackers, &used_blockers, &blocks);
    assert!(
        second_block_actions
            .iter()
            .all(|action| action.kind != ActionKind::FinishBlockers)
    );
    assert!(second_block_actions.iter().any(|action| {
        action.kind == ActionKind::DeclareBlocker
            && action.blocker_id.as_deref() == Some("blocker-b")
    }));
    assert!(
        second_block_actions
            .iter()
            .all(|action| { action.attacker_id.as_deref() == Some("menace-attacker") })
    );

    used_blockers.insert("blocker-b".to_string());
    blocks
        .get_mut("menace-attacker")
        .expect("menace block assignment")
        .push("blocker-b".to_string());
    let complete_actions = engine.legal_blocker_actions(1, &attackers, &used_blockers, &blocks);
    assert!(
        complete_actions
            .iter()
            .any(|action| action.kind == ActionKind::FinishBlockers)
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn first_and_double_strike_use_distinct_combat_damage_steps() {
    let mut first_strike_engine = test_engine(2);
    let mut first_striker_definition = test_definition("first-striker", "Creature - Test");
    first_striker_definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "firstStrike" },
    })];
    first_strike_engine.state.players[0].battlefield = vec![test_instance(
        "first-striker",
        first_striker_definition,
        "player-0",
    )];
    first_strike_engine.state.players[1].battlefield = vec![test_instance(
        "normal-blocker",
        test_definition("normal-blocker", "Creature - Test"),
        "player-1",
    )];
    let attacks = BTreeMap::from([(
        "first-striker".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    let blocks = BTreeMap::from([(
        "first-striker".to_string(),
        vec!["normal-blocker".to_string()],
    )]);
    let assignments = BTreeMap::from([(
        "first-striker".to_string(),
        BTreeMap::from([("normal-blocker".to_string(), 2)]),
    )]);
    first_strike_engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    first_strike_engine
        .resolve_combat_damage_step(
            &attacks,
            &blocks,
            &assignments,
            CombatDamageStep::FirstStrike,
        )
        .expect("resolve first-strike damage");
    assert!(first_strike_engine.state.players[1].battlefield.is_empty());
    assert_eq!(
        first_strike_engine.state.players[0].battlefield[0].damage_marked,
        0
    );

    let mut double_strike_engine = test_engine(2);
    let mut double_striker_definition = test_definition("double-striker", "Creature - Test");
    double_striker_definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "doubleStrike" },
    })];
    double_strike_engine.state.players[0].battlefield = vec![test_instance(
        "double-striker",
        double_striker_definition,
        "player-0",
    )];
    let double_strike_attacks = BTreeMap::from([(
        "double-striker".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    double_strike_engine
        .resolve_combat_damage_step(
            &double_strike_attacks,
            &BTreeMap::new(),
            &BTreeMap::new(),
            CombatDamageStep::FirstStrike,
        )
        .expect("resolve double-strike first damage");
    double_strike_engine
        .resolve_combat_damage_step(
            &double_strike_attacks,
            &BTreeMap::new(),
            &BTreeMap::new(),
            CombatDamageStep::Normal,
        )
        .expect("resolve double-strike normal damage");
    assert_eq!(double_strike_engine.state.players[1].life, 16);
}
