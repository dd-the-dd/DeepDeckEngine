//! Runtime proof of the program lowered for `each player|opponent|other player
//! sacrifices N QUERY [of their choice]`: every player of the population
//! chooses in APNAP order (CR 101.4), then the selections are sacrificed as
//! one batch. The IR below is exactly what the Oracle compiler emits.
use super::*;

#[derive(Default)]
struct FirstChoice {
    players: Vec<String>,
    battlefield_total: Vec<usize>,
}
impl DecisionProvider for FirstChoice {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.players.push(request.player_id.clone());
        self.battlefield_total
            .push(state.players.iter().map(|p| p.battlefield.len()).sum());
        Ok(0)
    }
}

fn program(players: Value, count: i64, predicate: Value) -> Value {
    let player = json!({"kind":"boundValue","id":"effect0:quantifiedPlayer"});
    json!([
        {"kind":"forEachPlayer","players":players,"bind":"effect0:quantifiedPlayer",
         "effects":[{"kind":"chooseCards","id":"effect0:quantifiedSacrifice","player":player,
            "from":{"kind":"eachPermanent","player":player,"where":predicate},
            "count":{"kind":"integer","value":count}}],
         "collect":{"bind":"effect0:quantifiedSacrificed",
            "objects":{"kind":"decisionResult","decisionId":"effect0:quantifiedSacrifice"}}},
        {"kind":"sacrificePermanents","player":players,
         "objects":{"kind":"boundObjects","binding":"effect0:quantifiedSacrificed"}}
    ])
}

fn creature() -> Value {
    json!({"kind":"cardTypeContains","value":"Creature"})
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
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
        rules: Vec::new(),
    }
}

fn position(players: usize) -> (GameEngine, StackObject) {
    let engine = GameEngine::new(
        GameSetup {
            players: (0..players)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        2024,
    )
    .unwrap();
    let card = card_instance("player-0", &definition("source", "Creature"), 9999);
    let stack = StackObject {
        id: "quantified-sacrifice".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn add(engine: &mut GameEngine, player: usize, name: &str, type_line: &str) {
    let index = 1000 + engine.state.players[player].battlefield.len() + player * 100;
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(&format!("{name}-{player}"), type_line),
        index,
    );
    card.controller = format!("player-{player}");
    engine.state.players[player].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture admission");
}

fn run(engine: &mut GameEngine, stack: &StackObject, effects: &Value) -> FirstChoice {
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    let mut provider = FirstChoice::default();
    engine
        .execute_effects(
            effects,
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("the lowered program executes");
    provider
}

fn counts(engine: &GameEngine) -> Vec<(usize, usize)> {
    engine
        .state
        .players
        .iter()
        .map(|p| (p.battlefield.len(), p.graveyard.len()))
        .collect()
}

#[test]
fn each_player_chooses_in_apnap_order_then_all_sacrifice_together() {
    let (mut engine, stack) = position(3);
    engine.state.active_player = 2;
    for player in 0..3 {
        add(&mut engine, player, "bear", "Creature");
        add(&mut engine, player, "ogre", "Creature");
        add(&mut engine, player, "relic", "Artifact");
    }
    // Player 1 has a third creature but is asked for exactly one.
    add(&mut engine, 1, "extra", "Creature");
    let before: usize = engine
        .state
        .players
        .iter()
        .map(|p| p.battlefield.len())
        .sum();
    let provider = run(
        &mut engine,
        &stack,
        &program(json!({"kind":"eachPlayer"}), 1, creature()),
    );
    // Active player first, then seating order; no sacrifice before the last choice.
    assert_eq!(
        provider.players,
        ["player-2", "player-0", "player-1"].map(String::from)
    );
    assert!(
        provider
            .battlefield_total
            .iter()
            .all(|total| *total == before)
    );
    assert_eq!(counts(&engine), [(2, 1), (3, 1), (2, 1)]);
    // Only creatures were eligible; every artifact survived.
    for player in 0..3 {
        assert!(
            engine.state.players[player]
                .battlefield
                .iter()
                .any(|card| card.definition.id == format!("relic-{player}"))
        );
    }
}

#[test]
fn opponent_scope_spares_the_controller_and_short_supply_is_not_an_error() {
    let (mut engine, stack) = position(3);
    add(&mut engine, 0, "mine", "Creature");
    add(&mut engine, 0, "mine-too", "Creature");
    add(&mut engine, 1, "only", "Creature");
    // Player 2 controls no creature at all and sacrifices nothing.
    add(&mut engine, 2, "relic", "Artifact");
    let opponents = json!({"kind":"opponentsOf","player":{"kind":"abilityController"}});
    // Two requested, one available for player 1: sacrifices what exists
    // without asking (a forced selection), and never touches the controller.
    run(&mut engine, &stack, &program(opponents, 2, creature()));
    assert_eq!(counts(&engine), [(2, 0), (0, 1), (1, 0)]);
}
