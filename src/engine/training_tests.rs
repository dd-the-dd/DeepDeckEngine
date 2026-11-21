use super::super::*;
use super::{clock_kind, clock_trigger};

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "unexpected training choice {}",
            request.id
        )))
    }
}

fn engine() -> GameEngine {
    let filler = CardDefinition {
        id: "filler".to_string(),
        name: "Filler".to_string(),
        type_line: "Basic Land ? Island".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    };
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        91,
    )
    .expect("training setup")
}

fn upkeep(engine: &mut GameEngine) {
    engine.enqueue_step_triggers("upkeep");
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut NoChoices)
            .expect("training clock resolves");
    }
}

#[test]
fn clocks_keep_periods_real_resolution_and_the_shared_copy_sequence() {
    let mut engine = engine();
    engine
        .configure_punching_bag("player-0")
        .expect("install clocks");
    let initial_library = engine.state.players[0].library.len();
    for _ in 0..6 {
        upkeep(&mut engine);
    }
    assert_eq!(engine.state.players[0].life, 22);
    assert_eq!(engine.state.players[0].library.len(), initial_library);
    engine.copy_sequence = 1; // Other game copies intentionally participate in the historical rotation.
    upkeep(&mut engine);
    assert_eq!(engine.copy_sequence, 2);
    let generated = engine.state.players[0].library.last().unwrap();
    assert_eq!(
        generated.definition.id,
        "punching-bag-generated-enchantment"
    );
    assert!(!generated.definition.is_token);
    assert_eq!(generated.owner, "player-0");
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "punchingBagSupplyAdvanced"
                && event.detail["createdCardInstanceId"] == generated.instance_id)
    );
    assert!(engine.state.events.iter().any(
        |event| event.kind == "punchingBagVitalityAdvanced" && event.detail["lifeGained"] == 1
    ));
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .find(|card| card.definition.id == "punching-bag-vitality-clock")
            .unwrap()
            .counters["clock"],
        1
    );
}

#[test]
fn countered_clock_does_not_advance_and_ordinary_cards_cannot_request_training_actions() {
    let mut engine = engine();
    engine.configure_punching_bag("player-0").unwrap();
    engine.enqueue_step_triggers("upkeep");
    assert_eq!(engine.state.stack.len(), 2);
    let supply = engine.state.stack.last().unwrap().clone();
    assert_eq!(supply.card.definition.id, "punching-bag-supply-clock");
    // The same backend used by counterStackObject removes the trigger before resolution.
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
    assert!(!triggered_rule_supported(&clock_trigger()));
    let mut ordinary = supply.card;
    ordinary.definition.is_game_piece = false;
    assert!(clock_kind(&ordinary).is_none());
    assert_eq!(ordinary.counters.get("clock").copied().unwrap_or(0), 0);
}
