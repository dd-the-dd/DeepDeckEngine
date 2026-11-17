use super::super::*;

fn definition() -> CardDefinition {
    CardDefinition {
        id: "identity-filler".into(),
        name: "Identity Filler".into(),
        type_line: "Creature — Bear".into(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{2}".into(),
        power: Some("2".into()),
        toughness: Some("2".into()),
        rules: Vec::new(),
    }
}
fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        829,
    )
    .unwrap()
}
struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "unexpected identity-clock choice {}",
            request.id
        )))
    }
}
fn upkeep(engine: &mut GameEngine) {
    engine.enqueue_step_triggers("upkeep");
    while !engine.state.stack.is_empty() {
        engine.resolve_top_stack(&mut NoChoices).unwrap();
    }
}

#[test]
fn incarnation_allocation_preserves_copy_sequence_and_does_not_publish_events() {
    let mut engine = engine();
    let initial_clock = engine.state.battlefield_incarnation_sequence;
    engine.copy_sequence = 7;
    let mut card = card_instance("player-0", &definition(), 0);
    let before_events = engine.state.events.len();
    engine.prepare_battlefield_entry_identity(&mut card);
    let first = GameEngine::tracked_zone_identity(&card).unwrap().to_owned();
    engine.prepare_battlefield_entry_identity(&mut card);
    assert_ne!(
        GameEngine::tracked_zone_identity(&card),
        Some(first.as_str())
    );
    assert_eq!(engine.copy_sequence, 7);
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        initial_clock + 2
    );
    assert_eq!(engine.state.events.len(), before_events);
}

#[test]
fn incarnation_state_is_backward_compatible_and_serializes_allocator_progress() {
    let mut engine = engine();
    let initial_clock = engine.state.battlefield_incarnation_sequence;
    // Construct the historical DTO explicitly; modern initial admission now
    // serializes a nonzero shared clock. This is not a live-state import.
    let mut legacy = serde_json::to_value(&engine.state).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("battlefieldIncarnationSequence");
    for player in legacy["players"].as_array_mut().unwrap() {
        for zone in [
            "library",
            "hand",
            "battlefield",
            "graveyard",
            "exile",
            "commandZone",
        ] {
            if let Some(cards) = player.get_mut(zone).and_then(Value::as_array_mut) {
                for card in cards {
                    if let Some(flags) = card.get_mut("flags").and_then(Value::as_object_mut) {
                        flags.retain(|key, _| !key.starts_with("trackedZoneObject:"));
                    }
                }
            }
        }
    }
    assert!(legacy.get("battlefieldIncarnationSequence").is_none());
    let decoded: GameState = serde_json::from_value(legacy).unwrap();
    assert_eq!(decoded.battlefield_incarnation_sequence, 0);
    let mut card = card_instance("player-0", &definition(), 0);
    GameEngine::assign_tracked_zone_identity(&mut card, "battlefield:17:4:legacy-object");
    engine.prepare_battlefield_entry_identity(&mut card);
    let old_identity = GameEngine::tracked_zone_identity(&card).unwrap().to_owned();
    assert!(old_identity.starts_with("object:incarnation:"));
    engine.state.players[0].battlefield.push(card);
    let encoded = serde_json::to_value(&engine.state).unwrap();
    assert_eq!(encoded["battlefieldIncarnationSequence"], initial_clock + 1);
    let restored_state: GameState = serde_json::from_value(encoded).unwrap();
    let mut restored = engine.clone();
    restored.state = restored_state;
    let mut returned = restored.state.players[0].battlefield.remove(0);
    restored.prepare_battlefield_entry_identity(&mut returned);
    assert_ne!(
        GameEngine::tracked_zone_identity(&returned),
        Some(old_identity.as_str())
    );
    assert_eq!(
        restored.state.battlefield_incarnation_sequence,
        initial_clock + 2
    );
    assert_eq!(restored.copy_sequence, 0);
}

#[test]
fn incarnation_allocator_is_cloned_and_restored_with_state_snapshots() {
    let mut engine = engine();
    let initial_clock = engine.state.battlefield_incarnation_sequence;
    let mut card = card_instance("player-0", &definition(), 0);
    engine.prepare_battlefield_entry_identity(&mut card);
    let saved = engine.state.clone();
    let mut branch = engine.clone();
    branch.prepare_battlefield_entry_identity(&mut card);
    assert_eq!(
        branch.state.battlefield_incarnation_sequence,
        initial_clock + 2
    );
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        initial_clock + 1
    );
    engine.prepare_battlefield_entry_identity(&mut card);
    engine.state = saved;
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        initial_clock + 1
    );
    assert_eq!(engine.copy_sequence, 0);
}

#[test]
fn countered_training_supply_does_not_advance_copy_rotation_after_clock_setup() {
    let mut engine = engine();
    let initial_clock = engine.state.battlefield_incarnation_sequence;
    engine.configure_punching_bag("player-0").unwrap();
    assert_eq!(engine.copy_sequence, 0);
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        initial_clock + 2
    );
    engine.enqueue_step_triggers("upkeep");
    let supply = engine.state.stack.last().unwrap().clone();
    assert_eq!(supply.card.definition.id, "punching-bag-supply-clock");
    engine
        .counter_stack_object(&supply.id, "counter-source", "player-1", false)
        .unwrap();
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.copy_sequence, 0);
    assert!(
        !engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "punchingBagSupplyAdvanced")
    );
}

#[test]
fn first_training_supply_keeps_historical_generated_creature_rotation() {
    let mut engine = engine();
    let initial_clock = engine.state.battlefield_incarnation_sequence;
    engine.configure_punching_bag("player-0").unwrap();
    let initial_library = engine.state.players[0].library.len();
    for _ in 0..7 {
        upkeep(&mut engine);
    }
    assert_eq!(engine.copy_sequence, 1);
    assert_eq!(
        engine.state.battlefield_incarnation_sequence,
        initial_clock + 2
    );
    assert_eq!(engine.state.players[0].library.len(), initial_library + 1);
    assert_eq!(
        engine.state.players[0]
            .library
            .last()
            .unwrap()
            .definition
            .id,
        "punching-bag-generated-creature"
    );
}
