use super::*;
use rand::RngCore;

fn count_condition(zone: &str, count: i32) -> Value {
    json!({
        "kind":"compare", "operator":"==", "right":count,
        "left":{"kind":"countCards", "zone":{"kind":zone,
            "player":{"kind":"controllerOf","object":{"kind":"self"}}}, "where":null}
    })
}

#[test]
fn policy_is_read_only_and_frozen_until_the_next_movement() {
    let mut engine = engine();
    engine.state.players[0].battlefield[0].definition.rules[0]["condition"] =
        count_condition("graveyard", 1);
    for serial in 1..=2 {
        engine.state.players[0].graveyard.push(card_instance(
            "p0",
            &definition(&format!("entrant-{serial}"), "Creature", vec![]),
            serial,
        ));
    }
    let before = serde_json::to_value(&engine.state).unwrap();
    let mut before_rng = engine.rng.clone();
    let frozen = engine.capture_zone_entry_policy();
    assert_eq!(before, serde_json::to_value(&engine.state).unwrap());
    let mut after_rng = engine.rng.clone();
    assert_eq!(before_rng.next_u64(), after_rng.next_u64());
    engine.state.players[0].graveyard.pop();
    let creature = &engine.state.players[0].graveyard[0].definition;
    assert_eq!(
        frozen.admission(creature, "graveyard"),
        BattlefieldEntryAdmission::Allowed
    );
    assert_eq!(
        engine
            .capture_zone_entry_policy()
            .admission(creature, "graveyard"),
        BattlefieldEntryAdmission::RemainInSourceZone
    );
}

#[test]
fn destination_policy_does_not_reactivate_after_source_removal() {
    let mut engine = engine();
    let rule = &mut engine.state.players[0].battlefield[0].definition.rules[0];
    rule["condition"] = count_condition("graveyard", 1);
    rule["modifiers"] =
        json!([{"kind":"prohibitUncastBattlefieldEntry","where":null,"destination":"exile"}]);
    for serial in 1..=2 {
        engine.state.players[0].graveyard.push(card_instance(
            "p0",
            &definition("entrant", "Creature", vec![]),
            serial,
        ));
    }
    let frozen = engine.capture_zone_entry_policy();
    let removed = engine.state.players[0].graveyard.pop().unwrap();
    assert_eq!(frozen.destination_replacement(&removed.definition), None);
    assert_eq!(
        engine
            .capture_zone_entry_policy()
            .destination_replacement(&removed.definition),
        Some("exile")
    );
}

#[test]
fn reveal_uses_pre_removal_library_condition_in_both_directions() {
    for active_count in [0, 2] {
        let mut engine = engine();
        engine.state.players[0].battlefield[0].definition.rules[0]["condition"] =
            count_condition("library", active_count);
        let a = card_instance("p0", &definition("entrant-a", "Creature", vec![]), 20);
        let b = card_instance("p0", &definition("entrant-b", "Creature", vec![]), 21);
        engine.state.players[0].library = vec![a.clone(), b.clone()];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture construction precedes references");
        let before_identity = engine.state.battlefield_incarnation_sequence;
        engine
            .reveal_creatures_to_battlefield("p0", 2, "source", &mut NoChoices)
            .unwrap();
        let denied = active_count == 2;
        for entrant in [&a, &b] {
            assert_eq!(
                engine.state.players[0]
                    .battlefield
                    .iter()
                    .any(|c| c.instance_id == entrant.instance_id),
                !denied
            );
            assert_eq!(
                engine.state.players[0]
                    .library
                    .iter()
                    .any(|c| c.instance_id == entrant.instance_id),
                denied
            );
        }
        if denied {
            assert_eq!(
                engine.state.battlefield_incarnation_sequence,
                before_identity
            );
        }
    }
}

#[test]
fn denied_reveal_retains_pop_order_and_the_existing_shuffle_rng() {
    let mut engine = engine();
    let other = card_instance("p0", &definition("other", "Artifact", vec![]), 10);
    let a = card_instance("p0", &definition("a", "Creature", vec![]), 11);
    let b = card_instance("p0", &definition("b", "Creature", vec![]), 12);
    engine.state.players[0].library = vec![a.clone(), other.clone(), b.clone()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    // Existing reveal order: popped noncreatures, followed by denied creatures.
    let mut expected = vec![other, b, a];
    let mut expected_rng = engine.rng.clone();
    expected.shuffle(&mut expected_rng);
    engine
        .reveal_creatures_to_battlefield("p0", 2, "source", &mut NoChoices)
        .unwrap();
    let ids = |cards: &[CardInstance]| {
        cards
            .iter()
            .map(|c| c.instance_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&engine.state.players[0].library), ids(&expected));
    assert_eq!(engine.rng.next_u64(), expected_rng.next_u64());
}
