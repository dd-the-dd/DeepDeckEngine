//! Suspend's keyword-expanded exile-zone triggers (CR 702.62a/b).
use super::*;

fn exiled_with_time_counters(
    engine: &mut GameEngine,
    owner_index: usize,
    printed_suspend: bool,
    time_counters: i32,
) -> String {
    let owner = engine.state.players[owner_index].id.clone();
    let id = format!("suspended-{owner_index}");
    let mut definition = test_definition(&id, "Sorcery");
    definition.mana_cost = "{3}{U}".to_string();
    if printed_suspend {
        definition.rules = vec![json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": {
                "kind": "suspend",
                "timeCounters": 3,
                "cost": { "kind": "payMana", "manaCost": "{U}" },
            },
        })];
    }
    let mut card = test_instance(&id, definition, &owner);
    card.counters.insert("time".to_string(), time_counters);
    engine.state.players[owner_index].exile.push(card);
    id
}

/// The upkeep trigger belongs to the owner's upkeep only, and requires the
/// card to be suspended: in exile, with suspend, and with a time counter.
#[test]
fn suspend_upkeep_trigger_requires_the_owners_upkeep_and_a_suspended_card() {
    let mut engine = test_engine(2);
    exiled_with_time_counters(&mut engine, 0, true, 2);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.state.active_player = 1;
    engine.enqueue_step_triggers("upkeep");
    assert!(engine.state.stack.is_empty(), "not the owner's upkeep");

    let mut engine = test_engine(2);
    exiled_with_time_counters(&mut engine, 0, true, 0);
    exiled_with_time_counters(&mut engine, 1, false, 2);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.state.active_player = 0;
    engine.enqueue_step_triggers("upkeep");
    assert!(
        engine.state.stack.is_empty(),
        "no time counter, or no suspend, means the card is not suspended"
    );
}

/// A suspend granted by an effect (the `suspended` designation, without a
/// printed keyword) uses the same expansion. A removal that leaves counters
/// does not trigger the last-counter ability.
#[test]
fn granted_suspend_counts_down_without_casting_before_the_last_counter() {
    let mut engine = test_engine(2);
    let id = exiled_with_time_counters(&mut engine, 0, false, 2);
    engine.state.players[0].exile[0]
        .flags
        .insert("suspended".to_string(), true);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.state.active_player = 0;
    engine.enqueue_step_triggers("upkeep");
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut CastSpellDecisionProvider)
        .expect("one time counter is removed");
    let card = &engine.state.players[0].exile[0];
    assert_eq!(card.instance_id, id);
    assert_eq!(card.counters["time"], 1);
    assert!(engine.state.stack.is_empty(), "a counter remains");
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "counterRemoved" && event.card_instance_id.as_deref() == Some(id.as_str())
    }));
}
