//! Runtime proof of the player-state primitives the clause layer lowers:
//! the Ring's temptation (CR 701.54), the monarch (CR 724), skipped turn
//! parts (CR 500.11), the `can't cast spells this turn` prohibition
//! (CR 101.2), phasing out (CR 702.26) and the `assigns no combat damage`
//! marker (CR 510.1).
use super::*;

fn controller_stack_object(card: &CardInstance, controller: &str) -> StackObject {
    StackObject {
        id: "stack:player-designation".to_string(),
        controller: controller.to_string(),
        card: card.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn run(engine: &mut GameEngine, effect: &Value, stack_object: &StackObject) {
    assert!(effect_supported(effect), "{effect}");
    engine
        .execute_effect(
            effect,
            None,
            stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the primitive resolves");
}

#[test]
fn each_ring_temptation_chooses_a_bearer_and_raises_the_ring_level() {
    let bearer = test_instance(
        "ring-bearer",
        test_definition("ring-bearer", "Creature - Halfling"),
        "player-0",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![bearer.clone()];
    let stack_object = controller_stack_object(&bearer, "player-0");
    let temptation = json!({ "kind": "temptWithRing", "player": { "kind": "controllerOf", "object": { "kind": "self" } } });
    run(&mut engine, &temptation, &stack_object);
    assert_eq!(
        engine.ring_bearer_level(&engine.state.players[0].battlefield[0]),
        1
    );
    run(&mut engine, &temptation, &stack_object);
    assert_eq!(
        engine.ring_bearer_level(&engine.state.players[0].battlefield[0]),
        2
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "ringTempted")
            .count(),
        2
    );
    // Without a creature the Ring still tempts: the level rises, no bearer.
    let mut empty = test_engine(2);
    run(&mut empty, &temptation, &stack_object);
    assert!(empty.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("ringEmblem")
            && modifier["level"].as_i64() == Some(1)
            && modifier["bearerId"].is_null()
    }));
}

#[test]
fn becoming_the_monarch_moves_the_designation() {
    let source = test_instance(
        "monarch-source",
        test_definition("monarch-source", "Creature - Human"),
        "player-0",
    );
    let mut engine = test_engine(2);
    let become_monarch = json!({ "kind": "becomeMonarch", "player": { "kind": "controllerOf", "object": { "kind": "self" } } });
    run(
        &mut engine,
        &become_monarch,
        &controller_stack_object(&source, "player-0"),
    );
    run(
        &mut engine,
        &become_monarch,
        &controller_stack_object(&source, "player-1"),
    );
    let monarchs = engine
        .state
        .rule_modifiers
        .iter()
        .filter(|modifier| value_kind(modifier) == Some("monarch"))
        .filter_map(|modifier| modifier["playerId"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(monarchs, ["player-1"]);
}

#[test]
fn a_skipped_turn_passes_to_the_next_player_once() {
    let source = test_instance(
        "skip-source",
        test_definition("skip-source", "Creature - Human"),
        "player-0",
    );
    let mut engine = test_engine(3);
    engine.state.active_player = 0;
    run(
        &mut engine,
        &json!({
            "kind": "skipTurnPart",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "part": "turn",
            "count": 1,
            "thisTurn": false,
        }),
        &controller_stack_object(&source, "player-1"),
    );
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 2);
    // The skip is consumed: player 1 takes their following turn.
    engine.state.turn_number += 1;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 0);
    engine.state.turn_number += 1;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 1);
}

#[test]
fn skipped_steps_and_phases_are_consumed_in_order() {
    let mut engine = test_engine(2);
    engine.schedule_skipped_turn_part("player-0", "combatPhase", 1, true);
    engine.schedule_skipped_turn_part("player-0", "untapStep", 2, false);
    assert!(!engine.consume_skipped_turn_part("player-1", "untapStep"));
    assert!(engine.consume_skipped_turn_part("player-0", "untapStep"));
    assert!(engine.consume_skipped_turn_part("player-0", "untapStep"));
    assert!(!engine.consume_skipped_turn_part("player-0", "untapStep"));
    // A `this turn` skip expires with its turn.
    engine.state.turn_number += 1;
    assert!(!engine.consume_skipped_turn_part("player-0", "combatPhase"));
    // A turn skip never carries `this turn`.
    assert!(!effect_supported(&json!({
        "kind": "skipTurnPart",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "part": "turn",
        "count": 1,
        "thisTurn": true,
    })));
}

#[test]
fn a_cast_prohibition_binds_the_acting_player_for_the_turn() {
    let source = test_instance(
        "chant-source",
        test_definition("chant-source", "Instant"),
        "player-0",
    );
    let mut engine = test_engine(2);
    run(
        &mut engine,
        &json!({
            "kind": "restrictPlayerActions",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "castSpells": true,
            "activateAbilities": false,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }),
        // The performer runs the primitive as its own instruction.
        &controller_stack_object(&source, "player-1"),
    );
    assert!(engine.spell_casting_prohibited("player-1"));
    assert!(!engine.spell_casting_prohibited("player-0"));
    assert!(!engine.player_activations_prohibited("player-1"));
}

#[test]
fn a_phased_out_permanent_phases_in_at_its_controllers_untap() {
    let creature = test_instance(
        "phasing-creature",
        test_definition("phasing-creature", "Creature - Djinn"),
        "player-0",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature.clone()];
    run(
        &mut engine,
        &json!({ "kind": "phaseOutPermanent", "permanent": { "kind": "self" } }),
        &controller_stack_object(&creature, "player-0"),
    );
    let phased = |engine: &GameEngine| {
        engine.state.players[0].battlefield[0]
            .flags
            .get("phasedOut")
            .copied()
            .unwrap_or(false)
    };
    assert!(phased(&engine));
    engine.state.active_player = 0;
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the untap step resolves");
    assert!(!phased(&engine));
}

#[test]
fn a_skipped_untap_step_keeps_every_permanent_tapped() {
    let mut creature = test_instance(
        "tapped-creature",
        test_definition("tapped-creature", "Creature - Bear"),
        "player-0",
    );
    creature.tapped = true;
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature];
    engine.state.active_player = 0;
    engine.schedule_skipped_turn_part("player-0", "untapStep", 1, false);
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the skipped untap step resolves");
    assert!(engine.state.players[0].battlefield[0].tapped);
    engine
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the next untap step resolves");
    assert!(!engine.state.players[0].battlefield[0].tapped);
}

#[test]
fn a_no_combat_damage_marker_stops_combat_damage_assignment() {
    let attacker = test_instance(
        "quiet-attacker",
        test_definition("quiet-attacker", "Creature - Snake"),
        "player-0",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    assert!(engine.deals_combat_damage_in_step(&attacker, CombatDamageStep::All));
    run(
        &mut engine,
        &json!({
            "kind": "grantKeyword",
            "object": { "kind": "self" },
            "keyword": "assignsNoCombatDamage",
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }),
        &controller_stack_object(&attacker, "player-0"),
    );
    let attacker = engine.state.players[0].battlefield[0].clone();
    assert!(!engine.deals_combat_damage_in_step(&attacker, CombatDamageStep::All));
}

#[test]
fn divided_counters_follow_the_announced_division() {
    let first = test_instance(
        "first-creature",
        test_definition("first-creature", "Creature - Elf"),
        "player-0",
    );
    let second = test_instance(
        "second-creature",
        test_definition("second-creature", "Creature - Elf"),
        "player-1",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![first.clone()];
    engine.state.players[1].battlefield = vec![second.clone()];
    let effect = json!({
        "kind": "putDividedCounters",
        "targetsDecisionId": "targetCreature",
        "divisionDecisionId": "targetCreature:division",
        "counter": "+1/+1",
    });
    assert!(effect_supported(&effect));
    let mut decisions = BTreeMap::from([
        (
            "targetCreature".to_string(),
            json!([
                { "permanent": { "instanceId": "first-creature" } },
                { "permanent": { "instanceId": "second-creature" } },
            ]),
        ),
        ("targetCreature:division".to_string(), json!([2, 1])),
    ]);
    engine
        .execute_effect(
            &effect,
            None,
            &controller_stack_object(&first, "player-0"),
            &mut BTreeMap::new(),
            &mut decisions,
            &mut EmeritusDecisionProvider,
        )
        .expect("the divided counters resolve");
    let counters = |engine: &GameEngine, player: usize| {
        engine.state.players[player].battlefield[0]
            .counters
            .get("+1/+1")
            .copied()
            .unwrap_or(0)
    };
    assert_eq!(counters(&engine, 0), 2);
    assert_eq!(counters(&engine, 1), 1);
}

#[test]
fn a_creature_removed_from_combat_leaves_the_attacks_and_blocks() {
    let attacker = test_instance(
        "removed-attacker",
        test_definition("removed-attacker", "Creature - Bird"),
        "player-0",
    );
    let mut engine = test_engine(2);
    let mut attacking = attacker.clone();
    attacking.flags.insert("attacking".to_string(), true);
    engine.state.players[0].battlefield = vec![attacking];
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "removed-attacker".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
    run(
        &mut engine,
        &json!({ "kind": "removeFromCombat", "permanent": { "kind": "self" } }),
        &controller_stack_object(&attacker, "player-0"),
    );
    assert!(engine.state.combat.attackers.is_empty());
    assert!(
        !engine.state.players[0].battlefield[0]
            .flags
            .get("attacking")
            .copied()
            .unwrap_or(false)
    );
    let mut attacks = BTreeMap::from([(
        "removed-attacker".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    let mut blocks = BTreeMap::from([(
        "other-attacker".to_string(),
        vec!["removed-attacker".to_string()],
    )]);
    engine.prune_removed_from_combat(&mut attacks, &mut blocks);
    assert!(attacks.is_empty());
    // The attacker it blocked stays blocked, with no blocker left.
    assert_eq!(blocks["other-attacker"], Vec::<String>::new());
}
