//! Rules a resolving effect creates (CR 603.7, 614.1a): delayed triggers
//! over captured objects, event-delayed triggers with a `this turn`
//! lifetime, and a one-shot death replacement.
use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn creature(id: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, "Creature"), owner)
}

fn ability_stack(rule_effects: Value, target: Option<&str>) -> StackObject {
    StackObject {
        id: "stack:created-rule".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "created-rule-source",
            test_definition("created-rule-source", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": rule_effects,
        })),
        decisions: BTreeMap::new(),
        targets: target
            .map(|instance_id| {
                BTreeMap::from([(
                    "targetCreature".to_string(),
                    TargetRef::Permanent {
                        instance_id: instance_id.to_string(),
                    },
                )])
            })
            .unwrap_or_default(),
    }
}

fn resolve(engine: &mut GameEngine, effects: &Value, stack: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        effects,
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut RandomAi::seeded(7),
    )
    .expect("the creating effect resolves");
}

/// `Destroy that creature at the beginning of the next end step.` as the
/// lowering writes it: the declared target is captured, the delayed action
/// reads the capture.
fn delayed_destroy() -> Value {
    json!([{
        "kind": "installDelayedStepTrigger",
        "step": "endStep",
        "controller": controller(),
        "effects": [{
            "kind": "destroyPermanent",
            "permanent": { "kind": "boundValue", "id": "delayedCapture1_0" },
        }],
        "captureReferences": [{
            "binding": "delayedCapture1_0",
            "reference": { "kind": "chosenTarget", "id": "targetCreature" },
        }],
    }])
}

fn delayed_destroy_engine() -> (GameEngine, StackObject) {
    let mut engine = test_engine(2);
    engine.state.players[0]
        .battlefield
        .push(creature("giant-target", "player-0"));
    let effects = delayed_destroy();
    assert!(rule_is_executable(&json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": effects,
    })));
    let stack = ability_stack(effects, Some("giant-target"));
    (engine, stack)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_captured_target_is_destroyed_at_the_next_end_step() {
    let (mut engine, stack) = delayed_destroy_engine();
    resolve(&mut engine, &delayed_destroy(), &stack);
    assert!(engine.permanent_position("giant-target").is_some());

    assert_eq!(engine.fire_delayed_step_triggers("endStep"), 1);
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the delayed destroy resolves");
    assert!(engine.permanent_position("giant-target").is_none());
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "giant-target")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_captured_target_that_became_a_new_object_is_ignored() {
    let (mut engine, stack) = delayed_destroy_engine();
    resolve(&mut engine, &delayed_destroy(), &stack);
    // The creature blinked: the same card returns as a new object
    // (CR 400.7) with a new incarnation.
    let (player, index) = engine
        .permanent_position("giant-target")
        .expect("the target is on the battlefield");
    let mut returned = engine.state.players[player].battlefield.remove(index);
    GameEngine::clear_tracked_zone_identity(&mut returned);
    engine.state.players[player].battlefield.push(returned);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("the returned object gets a new incarnation");

    assert_eq!(engine.fire_delayed_step_triggers("endStep"), 1);
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the delayed trigger resolves and finds nothing");
    assert!(
        engine.permanent_position("giant-target").is_some(),
        "the new object is not the captured one"
    );
}

/// `When that creature dies this turn, you gain 3 life.`
fn departure_gain_life() -> Value {
    json!([{
        "kind": "installDelayedZoneChangeTrigger",
        "controller": controller(),
        "trackedObject": { "kind": "chosenTarget", "id": "targetCreature" },
        "destinations": ["graveyard"],
        "duration": { "kind": "untilEndOfCurrentTurn" },
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": 3,
        }],
    }])
}

fn departure_engine() -> (GameEngine, StackObject) {
    let mut engine = test_engine(2);
    engine.state.players[1]
        .battlefield
        .push(creature("fed-creature", "player-1"));
    let effects = departure_gain_life();
    assert!(rule_is_executable(&json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": effects,
    })));
    let stack = ability_stack(effects, Some("fed-creature"));
    (engine, stack)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_departure_trigger_fires_only_when_the_creature_dies_this_turn() {
    let (mut engine, stack) = departure_engine();
    resolve(&mut engine, &departure_gain_life(), &stack);
    let life = engine.state.players[0].life;
    engine
        .move_permanent_to_graveyard("fed-creature", "destroyed")
        .expect("the creature dies");
    assert_eq!(engine.state.stack.len(), 1, "the delayed trigger waits");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(7))
        .expect("the delayed trigger resolves");
    assert_eq!(engine.state.players[0].life, life + 3);
    assert!(
        engine
            .state
            .rule_modifiers
            .iter()
            .all(|modifier| value_kind(modifier) != Some("delayedZoneChangeTrigger")),
        "the trigger is consumed (CR 603.7c)"
    );

    // The same creature dying on a later turn triggers nothing.
    let (mut later, stack) = departure_engine();
    resolve(&mut later, &departure_gain_life(), &stack);
    later.state.turn_number += 1;
    let life = later.state.players[0].life;
    later
        .move_permanent_to_graveyard("fed-creature", "destroyed")
        .expect("the creature dies next turn");
    assert!(later.state.stack.is_empty());
    assert_eq!(later.state.players[0].life, life);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_next_spell_trigger_copies_only_the_next_matching_spell() {
    let mut engine = test_engine(2);
    let effects = json!([{
        "kind": "installDelayedSpellCastTrigger",
        "player": controller(),
        "where": {
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Instant" },
                { "kind": "cardTypeContains", "value": "Sorcery" },
            ],
        },
        "duration": { "kind": "untilEndOfCurrentTurn" },
        "effects": [{
            "kind": "copyStackItem",
            "object": { "kind": "triggeringStackObject" },
            "controller": controller(),
            "mayChooseNewTargets": true,
        }],
    }]);
    let stack = ability_stack(effects.clone(), None);
    resolve(&mut engine, &effects, &stack);

    let creature_spell = test_definition("next-creature", "Creature");
    engine.enqueue_spell_cast_triggers("player-0", &creature_spell, &BTreeMap::new());
    assert!(
        engine.state.stack.is_empty(),
        "a creature spell does not match"
    );

    let instant = test_definition("next-instant", "Instant");
    engine.enqueue_spell_cast_triggers("player-0", &instant, &BTreeMap::new());
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the first matching spell triggers"
    );
    engine.state.stack.clear();

    engine.enqueue_spell_cast_triggers("player-0", &instant, &BTreeMap::new());
    assert!(
        engine.state.stack.is_empty(),
        "only the next spell is copied"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_created_death_replacement_lasts_this_turn_only() {
    let effects = json!([{
        "kind": "installDeathExileReplacement",
        "object": { "kind": "chosenTarget", "id": "targetCreature" },
    }]);
    for (later_turn, zone) in [(false, "exile"), (true, "graveyard")] {
        let mut engine = test_engine(2);
        engine.state.players[1]
            .battlefield
            .push(creature("coiled", "player-1"));
        let stack = ability_stack(effects.clone(), Some("coiled"));
        resolve(&mut engine, &effects, &stack);
        if later_turn {
            engine.state.turn_number += 1;
        }
        engine
            .move_permanent_to_graveyard("coiled", "destroyed")
            .expect("the creature would die");
        let player = &engine.state.players[1];
        let found = match zone {
            "exile" => player.exile.iter().any(|card| card.instance_id == "coiled"),
            _ => player
                .graveyard
                .iter()
                .any(|card| card.instance_id == "coiled"),
        };
        assert!(found, "later turn: {later_turn}, expected {zone}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn parsed_created_rules_are_executable() {
    for (name, type_line, text) in [
        (
            "Time to Feed",
            "Sorcery",
            "Choose target creature an opponent controls. When that creature dies this turn, you gain 3 life. Target creature you control fights that creature. (Each deals damage equal to its power to the other.)",
        ),
        (
            "Lava Coil",
            "Sorcery",
            "Lava Coil deals 4 damage to target creature. If that creature would die this turn, exile it instead.",
        ),
        (
            "Stone Giant",
            "Creature \u{2014} Giant",
            "{T}: Target creature you control with toughness less than this creature's power gains flying until end of turn. Destroy that creature at the beginning of the next end step.",
        ),
        (
            "Flare",
            "Instant",
            "Flare deals 1 damage to any target.\nDraw a card at the beginning of the next turn's upkeep.",
        ),
    ] {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{2}".to_string()),
            oracle_text: Some(text.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        });
        assert_eq!(
            parsed.status, "canonical",
            "{name}: {:#?}",
            parsed.abilities
        );
        for ability in &parsed.abilities {
            let rule = ability.rule.as_ref().expect("a canonical rule");
            assert!(rule_is_executable(rule), "{name}: {rule:#}");
        }
    }
}

fn colored_creature(id: &str, mana_cost: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature");
    definition.mana_cost = mana_cost.to_string();
    test_instance(id, definition, "player-1")
}

fn life_of(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .find(|player| player.id == id)
        .expect("player")
        .life
}

fn damage_player(engine: &mut GameEngine, player_id: &str, source: &str) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            TargetRef::Player {
                player_id: player_id.to_string(),
            },
            2,
            source,
            false,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves");
}

/// Circle of Protection: White prevents the damage of the one white source
/// chosen on resolution, once (CR 615.7, 609.7a).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_next_time_shield_prevents_one_chosen_source_once() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Circle of Protection: White".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(
            "{1}: The next time a white source of your choice would deal damage to you this turn, prevent that damage."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("a canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");

    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        colored_creature("white-striker", "{W}"),
        colored_creature("red-striker", "{R}"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let stack = StackObject {
        id: "stack:circle".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "circle",
            test_definition("circle", "Enchantment"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the shield is created");
    let start = life_of(&engine, "player-0");

    damage_player(&mut engine, "player-0", "red-striker");
    assert_eq!(
        life_of(&engine, "player-0"),
        start - 2,
        "another source is not chosen"
    );
    damage_player(&mut engine, "player-0", "white-striker");
    assert_eq!(
        life_of(&engine, "player-0"),
        start - 2,
        "the chosen source is prevented"
    );
    damage_player(&mut engine, "player-0", "white-striker");
    assert_eq!(
        life_of(&engine, "player-0"),
        start - 4,
        "the shield applied once"
    );
}

/// The canonical rule of a one-ability card parsed from its real Oracle
/// text, proved executable before it runs.
fn parsed_rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    let rule = parsed.abilities[0].rule.clone().expect("a canonical rule");
    assert!(rule_is_executable(&rule), "{name}: {rule:#}");
    rule
}

/// The installing action of a parsed rule, run alone by its controller.
fn install_created(engine: &mut GameEngine, rule: &Value, index: usize, source: &str) {
    let effects = json!([rule["effects"][index].clone()]);
    let stack = StackObject {
        id: format!("stack:{source}"),
        controller: "player-0".to_string(),
        card: test_instance(source, test_definition(source, "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    resolve(engine, &effects, &stack);
}

/// Death Frenzy's created `whenever` trigger triggers for each death this
/// turn and ends with the turn (CR 603.7b, 611.2a).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_created_whenever_trigger_repeats_this_turn_and_expires() {
    let rule = parsed_rule(
        "Death Frenzy",
        "Sorcery",
        "All creatures get -2/-2 until end of turn. Whenever a creature dies this turn, you gain 1 life.",
    );
    assert_eq!(
        rule["effects"][1]["kind"], "installCreatedTrigger",
        "{rule:#}"
    );
    assert_eq!(rule["effects"][1]["oneShot"], false);
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        creature("first-victim", "player-1"),
        creature("second-victim", "player-1"),
        creature("next-turn-victim", "player-1"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    install_created(&mut engine, &rule, 1, "death-frenzy");
    for victim in ["first-victim", "second-victim"] {
        let life = life_of(&engine, "player-0");
        engine
            .move_permanent_to_graveyard(victim, "destroyed")
            .expect("the creature dies");
        assert_eq!(engine.state.stack.len(), 1, "{victim} triggers");
        engine
            .resolve_top_stack(&mut RandomAi::seeded(7))
            .expect("the created trigger resolves");
        assert_eq!(life_of(&engine, "player-0"), life + 1, "{victim}");
    }
    engine.state.turn_number += 1;
    engine
        .move_permanent_to_graveyard("next-turn-victim", "destroyed")
        .expect("the creature dies next turn");
    assert!(
        engine.state.stack.is_empty(),
        "the trigger ended with its turn"
    );
}

/// A created `when` trigger over the shared event triggers once (CR 603.7c).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_created_when_trigger_over_an_event_union_is_consumed_once() {
    // Unseen combination: a created trigger whose event is a subject union.
    let rule = parsed_rule(
        "Test Created Union",
        "Instant",
        "Draw a card. When a creature or planeswalker you control dies this turn, you gain 2 life.",
    );
    assert_eq!(
        rule["effects"][1]["kind"], "installCreatedTrigger",
        "{rule:#}"
    );
    assert_eq!(rule["effects"][1]["oneShot"], true);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        creature("mine-first", "player-0"),
        creature("mine-second", "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    install_created(&mut engine, &rule, 1, "created-union");
    engine
        .move_permanent_to_graveyard("mine-first", "destroyed")
        .expect("the creature dies");
    assert_eq!(engine.state.stack.len(), 1);
    engine.state.stack.clear();
    engine
        .move_permanent_to_graveyard("mine-second", "destroyed")
        .expect("the creature dies");
    assert!(
        engine.state.stack.is_empty(),
        "the created trigger was consumed"
    );
}

/// Skullcrack: `Players can't gain life this turn.` blocks a gain this turn
/// only (CR 611.2a, 119.8).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_temporary_gain_life_prohibition_lasts_this_turn() {
    let rule = parsed_rule(
        "Skullcrack",
        "Instant",
        "Players can't gain life this turn. Damage can't be prevented this turn. Skullcrack deals 3 damage to target player or planeswalker.",
    );
    assert_eq!(
        rule["effects"][0]["kind"], "installTemporaryStaticModifiers",
        "{rule:#}"
    );
    let mut engine = test_engine(2);
    install_created(&mut engine, &rule, 0, "skullcrack");
    let life = life_of(&engine, "player-1");
    engine
        .gain_life("player-1", 3)
        .expect("the gain is attempted");
    assert_eq!(life_of(&engine, "player-1"), life, "no player gains life");
    engine.state.turn_number += 1;
    engine.gain_life("player-1", 3).expect("the gain applies");
    assert_eq!(life_of(&engine, "player-1"), life + 3, "the rule ended");
}

/// Unused combination: the same temporary prohibition lasting through the
/// controller's next turn (CR 611.2a, 514.2), read by the shared window.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_temporary_prohibition_lasts_through_the_end_of_the_next_turn() {
    let rule = parsed_rule(
        "Test Next Turn Prohibition",
        "Sorcery",
        "Players can't gain life until the end of your next turn.",
    );
    assert_eq!(
        rule["effects"][0]["duration"]["kind"], "untilEndOfNextTurn",
        "{rule:#}"
    );
    let mut engine = test_engine(2);
    install_created(&mut engine, &rule, 0, "next-turn-prohibition");
    let life = life_of(&engine, "player-1");
    // This turn, the opponent's turn and the controller's next turn.
    for _ in 0..3 {
        engine
            .gain_life("player-1", 3)
            .expect("the gain is attempted");
        assert_eq!(life_of(&engine, "player-1"), life, "no player gains life");
        engine.state.turn_number += 1;
    }
    engine.gain_life("player-1", 3).expect("the gain applies");
    assert_eq!(life_of(&engine, "player-1"), life + 3, "the rule ended");
}

/// A `next N damage` shield keeps what is left across damage events
/// (CR 615.7).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_amount_shield_decrements_across_two_damage_events() {
    let rule = parsed_rule(
        "Test Amount Shield",
        "Instant",
        "Prevent the next 3 damage that would be dealt to you this turn.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![colored_creature("striker", "{R}")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    install_created(&mut engine, &rule, 0, "amount-shield");
    let start = life_of(&engine, "player-0");
    damage_player(&mut engine, "player-0", "striker");
    assert_eq!(life_of(&engine, "player-0"), start, "2 of 3 prevented");
    damage_player(&mut engine, "player-0", "striker");
    assert_eq!(life_of(&engine, "player-0"), start - 1, "1 left, 1 dealt");
}

/// `The next sorcery spell you cast this turn can be cast as though it had
/// flash.`: only the next matching spell, consumed by its cast.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_next_spell_permission_is_consumed_by_the_first_matching_spell() {
    let rule = parsed_rule(
        "Test Quicken",
        "Instant",
        "The next sorcery spell you cast this turn can be cast as though it had flash.\nDraw a card at the beginning of the next turn's upkeep.",
    );
    assert_eq!(
        rule["effects"][0]["kind"], "installOneShotModifier",
        "{rule:#}"
    );
    let mut engine = test_engine(2);
    install_created(&mut engine, &rule, 0, "quicken");
    let sorcery = test_definition("next-sorcery", "Sorcery");
    let creature_spell = test_definition("next-creature", "Creature");
    assert!(engine.one_shot_flash_permission("player-0", &sorcery));
    assert!(!engine.one_shot_flash_permission("player-0", &creature_spell));
    assert!(!engine.one_shot_flash_permission("player-1", &sorcery));
    assert!(
        engine
            .consume_one_shot_spell_cast_modifier("player-0", &creature_spell)
            .is_empty(),
        "another spell does not consume it"
    );
    assert!(
        !engine
            .consume_one_shot_spell_cast_modifier("player-0", &sorcery)
            .is_empty()
    );
    assert!(!engine.one_shot_flash_permission("player-0", &sorcery));
}

/// `Prevent all damage that would be dealt to and dealt by target
/// creature this turn.`: two filters over one declared creature.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_dealt_to_and_dealt_by_shield_prevents_both_roles() {
    let rule = parsed_rule(
        "Test Fog Shield",
        "Instant",
        "Prevent all damage that would be dealt to and dealt by target creature this turn.",
    );
    assert_eq!(rule["effects"][0]["recipientAlsoSource"], true, "{rule:#}");
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        colored_creature("shielded", "{G}"),
        colored_creature("other", "{R}"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let stack = StackObject {
        id: "stack:fog-shield".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("fog", test_definition("fog", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: rule["effects"][0]["recipient"]["id"]
            .as_str()
            .map(|target_id| {
                BTreeMap::from([(
                    target_id.to_string(),
                    TargetRef::Permanent {
                        instance_id: "shielded".to_string(),
                    },
                )])
            })
            .expect("a declared recipient"),
    };
    resolve(&mut engine, &rule["effects"], &stack);
    let shields = engine
        .state
        .rule_modifiers
        .iter()
        .filter(|modifier| value_kind(modifier) == Some("damagePreventionShield"))
        .collect::<Vec<_>>();
    assert_eq!(shields.len(), 2, "one shield per role");
    assert!(
        shields
            .iter()
            .any(|shield| shield["recipient"]["permanentId"] == "shielded")
    );
    assert!(
        shields
            .iter()
            .any(|shield| shield["sourcePermanentId"] == "shielded")
    );
    let start = life_of(&engine, "player-0");
    damage_player(&mut engine, "player-0", "shielded");
    assert_eq!(
        life_of(&engine, "player-0"),
        start,
        "its damage is prevented"
    );
    damage_player(&mut engine, "player-0", "other");
    assert_eq!(
        life_of(&engine, "player-0"),
        start - 2,
        "another source is not"
    );
}
