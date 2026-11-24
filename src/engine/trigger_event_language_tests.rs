//! Runtime proofs of the trigger-event language attributes: the player
//! milestones (`roomFullyUnlocked` CR 709.5, `manaExpended` CR 701.58), the
//! land-play observer of any related player (CR 305.1), designation-gained
//! observers (CR 701.37b, 702.112b), the attached object's look-back
//! departure (CR 603.10a), `during combat` on a death (CR 506.1) and the
//! event shapes the compiler now emits (attacked recipients, unions).
use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        709,
    )
    .expect("trigger event language test setup is valid")
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn draw_rule(event: Value) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": event,
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

fn add(
    engine: &mut GameEngine,
    player: usize,
    id: &str,
    type_line: &str,
    rules: Vec<Value>,
) -> CardInstance {
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(id, type_line, rules),
        0,
    );
    card.instance_id = id.to_string();
    card.printed_definition = Some(card.definition.clone());
    engine.state.players[player].battlefield.push(card.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    card
}

fn triggered_sources(engine: &GameEngine) -> Vec<String> {
    engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
        .map(|object| object.card.instance_id.clone())
        .collect()
}

#[test]
fn emitted_event_shapes_are_executable_and_their_near_misses_are_not() {
    let self_ref = json!({ "kind": "self" });
    let opponent = json!({ "kind": "opponentsOf", "player": controller() });
    for (event, executable) in [
        (
            json!({"kind": "roomFullyUnlocked", "player": controller()}),
            true,
        ),
        (
            json!({"kind": "roomFullyUnlocked", "player": controller(), "door": 1}),
            false,
        ),
        (
            json!({"kind": "manaExpended", "player": controller(), "threshold": 4}),
            true,
        ),
        (
            json!({"kind": "manaExpended", "player": controller(), "threshold": 0}),
            false,
        ),
        (
            json!({"kind": "designationGained", "object": self_ref, "designation": "monstrous"}),
            true,
        ),
        (
            json!({"kind": "designationGained", "object": self_ref, "designation": "suspected"}),
            false,
        ),
        (
            json!({"kind": "landPlayed", "player": {"kind": "eachPlayer"}}),
            true,
        ),
        (json!({"kind": "landPlayed", "player": opponent}), true),
        (json!({"kind": "doorUnlocked", "object": self_ref}), true),
        // An attacked recipient of the source's own attack (CR 506.2).
        (
            json!({
                "kind": "eventPattern", "lemma": "attack",
                "subject": {"role": "attacker", "object": self_ref},
                "attacked": {"anyOf": [{"kind": "player", "player": opponent}]},
            }),
            true,
        ),
        // Only an attack record has an attacked object.
        (
            json!({
                "kind": "eventPattern", "lemma": "block",
                "subject": {"role": "blocker", "object": self_ref},
                "attacked": {"anyOf": [{"kind": "player", "player": opponent}]},
            }),
            false,
        ),
        // The attached object's departure, read on the observer's attachment.
        (
            json!({
                "kind": "eventPattern", "lemma": "move",
                "subject": {"object": {"kind": "attachedPermanent", "attachment": self_ref}},
                "from": "battlefield",
            }),
            true,
        ),
        (
            json!({
                "kind": "eventPattern", "lemma": "move",
                "subject": {"object": {"kind": "attachedPermanent", "attachment": {"kind": "target"}}},
                "from": "battlefield",
            }),
            false,
        ),
        (
            json!({"kind": "permanentDied", "object": self_ref, "duringCombat": true}),
            true,
        ),
        (
            json!({"kind": "enterBattlefield", "object": self_ref, "duringCombat": true}),
            true,
        ),
        // A union is one rule whose every alternative is executable.
        (
            json!({"kind": "oneOf", "events": [
                {"kind": "enterBattlefield", "object": self_ref},
                {"kind": "designationGained", "object": self_ref, "designation": "renowned"},
            ]}),
            true,
        ),
    ] {
        assert_eq!(
            triggered_rule_supported(&draw_rule(event.clone())),
            executable,
            "{event}"
        );
    }
}

#[test]
fn expend_triggers_once_when_the_turn_total_crosses_its_threshold() {
    let mut engine = engine();
    engine.state.step = GameStep::PrecombatMain;
    add(
        &mut engine,
        0,
        "expender",
        "Creature — Raccoon",
        vec![draw_rule(json!({
            "kind": "manaExpended", "player": controller(), "threshold": 4,
        }))],
    );
    let cast = |engine: &mut GameEngine, caster: &str, mana: usize| {
        let decisions = BTreeMap::from([(
            "manaPoolPayment".to_string(),
            Value::Array((0..mana).map(Value::from).collect()),
        )]);
        engine.record_event(
            "spellCast",
            Some(caster.to_string()),
            None,
            json!({ "decisions": decisions }),
        );
        engine.enqueue_mana_expended_triggers(caster, &decisions);
        triggered_sources(engine).len()
    };
    // Three mana: the fourth has not been spent yet.
    assert_eq!(cast(&mut engine, "player-0", 3), 0);
    // An opponent's spending is not the observer's controller expending.
    assert_eq!(cast(&mut engine, "player-1", 5), 0);
    // Two more: the fourth total mana is spent now, once.
    assert_eq!(cast(&mut engine, "player-0", 2), 1);
    // Later spending crosses no other threshold of this observer.
    assert_eq!(cast(&mut engine, "player-0", 2), 1);
}

#[test]
fn a_room_is_fully_unlocked_only_when_its_last_door_unlocks() {
    let mut engine = engine();
    let watcher = add(
        &mut engine,
        0,
        "watcher",
        "Creature — Human Scout",
        vec![draw_rule(
            json!({"kind": "roomFullyUnlocked", "player": controller()}),
        )],
    );
    add(
        &mut engine,
        1,
        "opposing-watcher",
        "Creature — Human Scout",
        vec![draw_rule(
            json!({"kind": "roomFullyUnlocked", "player": controller()}),
        )],
    );
    let mut room = add(
        &mut engine,
        0,
        "room",
        "Enchantment — Room // Enchantment — Room",
        vec![],
    );
    room.flags.insert("roomDoor0Unlocked".to_string(), true);
    engine.enqueue_room_fully_unlocked_triggers(&room);
    assert!(triggered_sources(&engine).is_empty());
    room.flags.insert("roomDoor1Unlocked".to_string(), true);
    engine.enqueue_room_fully_unlocked_triggers(&room);
    assert_eq!(triggered_sources(&engine), vec![watcher.instance_id]);
}

#[test]
fn land_play_observers_match_the_playing_player_and_bind_them() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "any-player",
        "Artifact",
        vec![draw_rule(
            json!({"kind": "landPlayed", "player": {"kind": "eachPlayer"}}),
        )],
    );
    add(
        &mut engine,
        0,
        "opponent",
        "Artifact",
        vec![draw_rule(json!({
            "kind": "landPlayed",
            "player": {"kind": "opponentsOf", "player": controller()},
        }))],
    );
    add(
        &mut engine,
        0,
        "controller-only",
        "Artifact",
        vec![draw_rule(
            json!({"kind": "landPlayed", "player": controller()}),
        )],
    );
    engine.enqueue_player_event_triggers("landPlayed", "player-1", None);
    let mut sources = triggered_sources(&engine);
    sources.sort();
    assert_eq!(
        sources,
        vec!["any-player".to_string(), "opponent".to_string()]
    );
    assert!(engine.state.stack.iter().all(|object| {
        object
            .decisions
            .get("triggeringPlayerId")
            .and_then(Value::as_str)
            == Some("player-1")
    }));
}

#[test]
fn designation_gained_observers_trigger_for_their_own_designation() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "monster",
        "Creature — Dinosaur",
        vec![draw_rule(json!({
            "kind": "designationGained",
            "object": {"kind": "self"},
            "designation": "monstrous",
        }))],
    );
    engine.publish_designation_gained_observers("monster", "renowned");
    assert!(triggered_sources(&engine).is_empty());
    engine.publish_designation_gained_observers("monster", "monstrous");
    assert_eq!(triggered_sources(&engine), vec!["monster".to_string()]);
}

#[test]
fn attached_departure_and_combat_death_are_read_on_the_move_record() {
    let mut engine = engine();
    let creature = add(&mut engine, 0, "bearer", "Creature — Bear", vec![]);
    let mut aura = add(&mut engine, 0, "aura", "Enchantment — Aura", vec![]);
    aura.attached_to = Some(creature.instance_id.clone());
    let unattached = add(&mut engine, 0, "loose-aura", "Enchantment — Aura", vec![]);
    let departure = json!({
        "kind": "eventPattern", "lemma": "move",
        "subject": {"object": {"kind": "attachedPermanent", "attachment": {"kind": "self"}}},
        "from": "battlefield",
    });
    let pattern = zone_change_events::zone_change_pattern(&departure)
        .expect("a zone-change pattern")
        .expect("a supported attached departure");
    assert_eq!(
        pattern.group,
        zone_change_events::ZoneChangeGroup::Departure
    );
    let record = zone_change_events::ZoneChangeRecord {
        object: &creature,
        from: Some("battlefield"),
        to: "exile",
        reason: None,
    };
    assert!(engine.zone_change_matches(&pattern, &record, &aura));
    assert!(!engine.zone_change_matches(&pattern, &record, &unattached));

    let death = json!({"kind": "permanentDied", "object": {"kind": "self"}, "duringCombat": true});
    let pattern = zone_change_events::zone_change_pattern(&death)
        .expect("a zone-change pattern")
        .expect("a supported combat death");
    let record = zone_change_events::ZoneChangeRecord {
        object: &creature,
        from: Some("battlefield"),
        to: "graveyard",
        reason: None,
    };
    engine.state.step = GameStep::PrecombatMain;
    assert!(!engine.zone_change_matches(&pattern, &record, &creature));
    engine.state.step = GameStep::CombatDamage;
    assert!(engine.zone_change_matches(&pattern, &record, &creature));
}
