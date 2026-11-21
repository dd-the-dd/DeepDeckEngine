use super::*;

const SOURCE_ID: &str = "quest-source";
const TARGET_ID: &str = "quest-target";
const PLACEMENT_BINDING: &str = "questCounterRecipients";

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn quest_threshold_condition() -> Value {
    json!({
        "kind": "compare",
        "left": {
            "kind": "countCounters",
            "object": { "kind": "self" },
            "counter": "quest",
        },
        "operator": ">=",
        "right": { "kind": "integer", "value": 4 },
    })
}

fn reflexive_reward() -> Value {
    json!({
        "kind": "createReflexiveTrigger",
        "source": { "kind": "self" },
        "controller": controller(),
        "ability": {
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": { "kind": "reflexiveTriggerCreated", "object": { "kind": "self" } },
            "condition": quest_threshold_condition(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetCreature",
                    "kind": "chooseTargets",
                    "candidates": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                    "minimum": 1,
                    "maximum": 1,
                }],
            },
            "effects": [
                {
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTarget", "id": "targetCreature" },
                    "counter": "+1/+1",
                    "count": { "kind": "integer", "value": 1 },
                },
                {
                    "kind": "grantKeyword",
                    "object": { "kind": "chosenTarget", "id": "targetCreature" },
                    "keyword": "trample",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                },
            ],
        },
    })
}

fn counter_then_reflexive_effects() -> Value {
    json!([
        {
            "kind": "putCounters",
            "permanent": { "kind": "self" },
            "counter": "quest",
            "count": { "kind": "integer", "value": 1 },
            "bind": PLACEMENT_BINDING,
        },
        {
            "kind": "conditionalEffect",
            "condition": {
                "kind": "compare",
                "left": { "kind": "boundValue", "id": PLACEMENT_BINDING },
                "operator": ">",
                "right": { "kind": "integer", "value": 0 },
            },
            "then": [reflexive_reward()],
            "else": [],
        },
    ])
}

fn setup(quest_counters: i32, source_on_battlefield: bool) -> (GameEngine, StackObject) {
    let mut engine = test_engine(2);
    let mut source = test_instance(
        SOURCE_ID,
        test_definition("quest-enchantment", "Enchantment"),
        "player-0",
    );
    source.counters.insert("quest".to_string(), quest_counters);
    // The trigger's source copy carries the incarnation of the permanent that
    // triggered, whether or not that permanent is still on the battlefield.
    engine
        .assign_new_object_incarnation(&mut source)
        .expect("the source has an incarnation");
    if source_on_battlefield {
        engine.state.players[0].battlefield.push(source.clone());
    }
    engine.state.players[0].battlefield.push(test_instance(
        TARGET_ID,
        test_definition("quest-creature", "Creature - Ally"),
        "player-0",
    ));
    let stack = StackObject {
        id: "stack:quest-landfall".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn execute_counter_sequence(engine: &mut GameEngine, stack: &StackObject) {
    let effects = counter_then_reflexive_effects();
    assert!(
        effects
            .as_array()
            .is_some_and(|effects| effects.iter().all(effect_supported))
    );
    execute_effects_at_resolution_start(
        engine,
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("counter placement and reflexive trigger resolve");
}

#[test]
fn live_source_counter_count_creates_and_resolves_the_intervening_if_trigger() {
    let (mut engine, stack) = setup(3, true);
    execute_counter_sequence(&mut engine, &stack);

    let reflexive = engine.state.stack.last().expect("reflexive trigger");
    let ability = reflexive.ability_rule.as_ref().expect("reflexive rule");
    assert_eq!(ability["condition"]["left"]["kind"], "countCounters");
    assert_eq!(
        reflexive.targets.get("targetCreature"),
        Some(&TargetRef::Permanent {
            instance_id: TARGET_ID.to_string(),
        })
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("reflexive reward resolves");
    let (player_index, card_index) = engine
        .permanent_position(TARGET_ID)
        .expect("target remains on the battlefield");
    let target = &engine.state.players[player_index].battlefield[card_index];
    assert_eq!(target.counters.get("+1/+1"), Some(&1));
    assert!(engine.permanent_has_keyword(target, "trample"));
}

#[test]
fn intervening_if_is_checked_again_when_the_reflexive_trigger_resolves() {
    let (mut engine, stack) = setup(3, true);
    execute_counter_sequence(&mut engine, &stack);
    assert_eq!(engine.state.stack.len(), 1);

    let (source_player, source_index) = engine
        .permanent_position(SOURCE_ID)
        .expect("source remains on the battlefield");
    engine.state.players[source_player].battlefield[source_index]
        .counters
        .insert("quest".to_string(), 3);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("failed intervening-if trigger resolves without effects");

    let (player_index, card_index) = engine
        .permanent_position(TARGET_ID)
        .expect("target remains on the battlefield");
    let target = &engine.state.players[player_index].battlefield[card_index];
    assert_eq!(target.counters.get("+1/+1"), None);
    assert!(!engine.permanent_has_keyword(target, "trample"));
}

#[test]
fn reflexive_trigger_is_not_created_when_no_counter_was_actually_placed() {
    let (mut engine, stack) = setup(4, false);
    execute_counter_sequence(&mut engine, &stack);

    assert!(engine.state.stack.is_empty());
    let (player_index, card_index) = engine
        .permanent_position(TARGET_ID)
        .expect("target remains on the battlefield");
    assert_eq!(
        engine.state.players[player_index].battlefield[card_index]
            .counters
            .get("+1/+1"),
        None
    );
}

#[test]
fn source_counter_expressions_prefer_live_state_and_fall_back_to_last_known_state() {
    let (mut engine, mut stack) = setup(3, true);
    let (source_player, source_index) = engine
        .permanent_position(SOURCE_ID)
        .expect("source on battlefield");
    engine.state.players[source_player].battlefield[source_index]
        .counters
        .insert("quest".to_string(), 5);

    for expression in [
        json!({
            "kind": "countCounters",
            "object": { "kind": "self" },
            "counter": "quest",
        }),
        json!({
            "kind": "counterCountOf",
            "object": { "kind": "abilitySource" },
            "counter": "quest",
        }),
    ] {
        assert_eq!(
            engine.runtime_numeric_expression(
                &expression,
                &stack,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            Some(5)
        );
    }

    stack.card.counters.insert("quest".to_string(), 3);
    engine.state.players[source_player]
        .battlefield
        .remove(source_index);
    for expression in [
        json!({
            "kind": "countCounters",
            "object": { "kind": "self" },
            "counter": "quest",
        }),
        json!({
            "kind": "counterCountOf",
            "object": { "kind": "abilitySource" },
            "counter": "quest",
        }),
    ] {
        assert_eq!(
            engine.runtime_numeric_expression(
                &expression,
                &stack,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            Some(3)
        );
    }
}
