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
