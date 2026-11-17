use super::*;

fn definition(id: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":"Land","manaCost":"","rules":[]}))
        .unwrap()
}
fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition("filler"); 4],
                })
                .collect(),
        },
        51253,
    )
    .unwrap()
}
fn tap_rule() -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentTapped","object":{"kind":"self"}},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},
            "amount":{"kind":"integer","value":1}}]})
}

#[test]
fn tapped_observer_uses_the_shared_activity_and_presence_admission() {
    for mode in [
        "active",
        "locked-door",
        "under-level",
        "phased",
        "lost-abilities",
    ] {
        let mut e = engine();
        let mut source = card_instance("p0", &definition("observer"), 100);
        let mut rule = tap_rule();
        rule["roomDoorIndex"] = json!(0);
        rule["minimumClassLevel"] = json!(2);
        source
            .flags
            .insert("roomDoor0Unlocked".to_owned(), mode != "locked-door");
        source
            .flags
            .insert("phasedOut".to_owned(), mode == "phased");
        source.counters.insert(
            "classLevel".to_owned(),
            if mode == "under-level" { 1 } else { 2 },
        );
        source.definition.rules = vec![rule];
        source.tapped = true;
        e.state.players[0].battlefield.push(source.clone());
        if mode == "lost-abilities" {
            e.state
                .rule_modifiers
                .push(json!({"kind":"loseAllAbilities","cardInstanceId":source.instance_id}));
        }
        // Invoke the observer's capture boundary with the exact transition snapshot.
        // This does not claim that gameplay may tap a phased-out permanent.
        e.enqueue_permanent_tapped_triggers(&source);
        assert_eq!(e.state.stack.len(), usize::from(mode == "active"), "{mode}");
    }
}

#[test]
fn tapped_transition_queues_only_its_exact_instance_and_actual_controller_once() {
    let mut e = engine();
    let mut shared = definition("same-printed-source");
    shared.rules = vec![tap_rule()];
    let mut first = card_instance("p1", &shared, 100);
    first.controller = "p1".to_owned();
    let second = card_instance("p0", &shared, 101);
    let first_id = first.instance_id.clone();
    let second_id = second.instance_id.clone();
    // Deliberately mismatch battlefield holder and actual controller.
    e.state.players[0].battlefield.push(first);
    e.state.players[1].battlefield.push(second);
    let snapshot = e
        .tap_permanent(&first_id)
        .unwrap()
        .expect("first untapped transition");
    e.enqueue_permanent_tapped_triggers(&snapshot);
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(e.state.stack[0].card.instance_id, first_id);
    assert_ne!(e.state.stack[0].card.instance_id, second_id);
    assert_eq!(e.state.stack[0].controller, "p1");
    assert!(
        e.tap_permanent(&first_id).unwrap().is_none(),
        "already tapped is not another transition"
    );
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|event| event.kind == "permanentTapped")
            .count(),
        1
    );
}
