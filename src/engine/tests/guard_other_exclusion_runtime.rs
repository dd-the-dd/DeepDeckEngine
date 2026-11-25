//! `When this creature enters, if you control another Human, draw a card.`
//! (Resistance Squad, CR 603.4): the guard's count excludes the ability's
//! source by its current incarnation, when the trigger is put on the stack
//! and again on resolution. The source alone never satisfies `another`.
use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

/// The lowered guard of `if you control another Human`.
fn another_human() -> Value {
    json!({
        "kind": "compare",
        "operator": ">=",
        "left": {
            "kind": "countPermanents",
            "player": controller(),
            "where": { "kind": "subtypeContains", "value": "Human" },
            "excludeSource": true,
        },
        "right": { "kind": "integer", "value": 1 },
    })
}

fn squad(engine: &mut GameEngine) -> CardInstance {
    let mut definition = test_definition("resistance-squad", "Creature - Human Soldier");
    definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "condition": another_human(),
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": 1 },
        }],
    })];
    engine.state.players[0].battlefield.push(test_instance(
        "resistance-squad",
        definition,
        "player-0",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture source is admitted");
    let (player, index) = engine
        .permanent_position("resistance-squad")
        .expect("source is on the battlefield");
    engine.state.players[player].battlefield[index].clone()
}

fn add_human(engine: &mut GameEngine, id: &str) {
    engine.state.players[0].battlefield.push(test_instance(
        id,
        test_definition(id, "Creature - Human Peasant"),
        "player-0",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture Human is admitted");
}

#[test]
fn the_source_alone_is_not_another_human() {
    let mut engine = test_engine(2);
    let source = squad(&mut engine);
    let rule = source.definition.rules[0].clone();
    engine.put_triggered_ability_on_stack(&source, rule);
    assert!(
        engine.state.stack.is_empty(),
        "the Squad itself is excluded: the guard is false and nothing triggers"
    );
}

#[test]
fn another_human_is_counted_and_rechecked_on_resolution() {
    for (human_still_there, draws) in [(true, 1), (false, 0)] {
        let mut engine = test_engine(2);
        let source = squad(&mut engine);
        add_human(&mut engine, "villager");
        let hand_before = engine.state.players[0].hand.len();
        let rule = source.definition.rules[0].clone();
        engine.put_triggered_ability_on_stack(&source, rule);
        assert_eq!(
            engine.state.stack.len(),
            1,
            "another Human: the guard holds"
        );
        if !human_still_there {
            engine.state.players[0]
                .battlefield
                .retain(|card| card.instance_id != "villager");
        }
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
        assert_eq!(
            engine.state.players[0].hand.len(),
            hand_before + draws,
            "the guard is rechecked on resolution (other Human present: {human_still_there})"
        );
    }
}
