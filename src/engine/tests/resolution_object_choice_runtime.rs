//! Runtime proof of the programs the Oracle compiler emits for permanents and
//! numbers chosen on resolution (CR 608.2d): every chooser chooses before any
//! unchosen permanent is sacrificed (CR 608.2c), each player sacrifices only
//! its own unchosen permanents, a single choice is read through its binding,
//! and a chosen number is bound for `that many`. The IR below is exactly
//! what `effects::resolution_object_choice` lowers.
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

fn controller() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn nonland() -> Value {
    json!({"kind": "not", "operand": {"kind": "cardTypeContains", "value": "Land"}})
}

fn creature() -> Value {
    json!({"kind": "cardTypeContains", "value": "Creature"})
}

/// `Each player chooses a nonland permanent they control, then sacrifices
/// the rest.`
fn each_player_keeps_one_program() -> Value {
    let performer = json!({"kind": "boundValue", "id": "performer0"});
    json!([
        {"kind": "forEachPlayer", "bind": "performer0", "players": {"kind": "eachPlayer"},
         "effects": [
            {"kind": "choosePermanents", "id": "effect0:choosePermanents", "player": controller(),
             "candidates": {"kind": "permanents", "where": nonland(), "controller": controller()},
             "minimum": {"kind": "integer", "value": 1}, "maximum": {"kind": "integer", "value": 1},
             "performer": performer},
            {"kind": "bind", "id": "effect0:unchosenObjects",
             "value": {"kind": "setDifference",
                "left": {"kind": "eachPermanent", "where": nonland(), "player": controller()},
                "right": {"kind": "decisionResult", "decisionId": "effect0:choosePermanents"}},
             "performer": performer}],
         "collect": {"bind": "effect0:unchosenObjects",
            "objects": {"kind": "boundObjects", "binding": "effect0:unchosenObjects"}}},
        {"kind": "sacrificePermanents", "player": {"kind": "eachPlayer"},
         "objects": {"kind": "boundObjects", "binding": "effect0:unchosenObjects"}}
    ])
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
    let card = card_instance("player-0", &definition("source", "Sorcery"), 9999);
    let stack = StackObject {
        id: "resolution-object-choice".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
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

fn run(
    engine: &mut GameEngine,
    stack: &StackObject,
    effects: &Value,
    bindings: &mut BTreeMap<String, RuntimeBinding>,
) -> FirstChoice {
    assert!(
        effects.as_array().unwrap().iter().all(effect_supported),
        "{effects:#?}"
    );
    let mut provider = FirstChoice::default();
    engine
        .execute_effects(
            effects,
            stack,
            bindings,
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("the lowered program executes");
    provider
}

#[test]
fn each_player_chooses_then_every_player_sacrifices_its_own_rest() {
    let (mut engine, stack) = position(3);
    engine.state.active_player = 1;
    for player in 0..3 {
        add(&mut engine, player, "bear", "Creature");
        add(&mut engine, player, "relic", "Artifact");
        add(&mut engine, player, "forest", "Basic Land — Forest");
    }
    add(&mut engine, 2, "ogre", "Creature");
    let before: usize = engine
        .state
        .players
        .iter()
        .map(|p| p.battlefield.len())
        .sum();
    let provider = run(
        &mut engine,
        &stack,
        &each_player_keeps_one_program(),
        &mut BTreeMap::new(),
    );
    // Every player chose, in turn order from the active player, before any
    // permanent left the battlefield.
    assert_eq!(provider.players, ["player-1", "player-2", "player-0"]);
    assert!(
        provider
            .battlefield_total
            .iter()
            .all(|total| *total == before)
    );
    for player in &engine.state.players {
        // One chosen nonland permanent and the untouched land remain.
        assert_eq!(player.battlefield.len(), 2, "{}", player.id);
        assert!(
            player
                .battlefield
                .iter()
                .any(|card| card.definition.type_line.contains("Land")),
            "{}",
            player.id
        );
        // Every sacrificed permanent went to its owner's graveyard.
        assert!(player.graveyard.iter().all(|card| card.owner == player.id));
    }
    assert_eq!(engine.state.players[2].graveyard.len(), 2);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
}

#[test]
fn a_single_choice_is_read_through_its_binding() {
    let (mut engine, stack) = position(2);
    add(&mut engine, 0, "bear", "Creature");
    add(&mut engine, 0, "ogre", "Creature");
    add(&mut engine, 1, "wolf", "Creature");
    // `Choose a creature you control. Destroy that creature.`
    let program = json!([
        {"kind": "choosePermanents", "id": "effect0:choosePermanents", "player": controller(),
         "candidates": {"kind": "permanents", "where": creature(), "controller": controller()},
         "minimum": {"kind": "integer", "value": 1}, "maximum": {"kind": "integer", "value": 1}},
        {"kind": "bind", "id": "effect0:chosenObjects",
         "value": {"kind": "decisionResult", "decisionId": "effect0:choosePermanents"}},
        {"kind": "destroyPermanent",
         "permanent": {"kind": "singleBoundObject", "binding": "effect0:chosenObjects"}}
    ]);
    let provider = run(&mut engine, &stack, &program, &mut BTreeMap::new());
    assert_eq!(provider.players, ["player-0"]);
    assert_eq!(engine.state.players[0].battlefield.len(), 1);
    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    // The other player's creature was never a candidate.
    assert_eq!(engine.state.players[1].battlefield.len(), 1);
}

#[test]
fn a_chosen_number_is_bound_for_that_many() {
    let (mut engine, stack) = position(2);
    // `Choose a number between 2 and 5.` with the binding `that many` installs.
    let program = json!([
        {"kind": "chooseNumber", "id": "effect0:chosenNumber", "player": controller(),
         "minimum": {"kind": "integer", "value": 2}, "maximum": {"kind": "integer", "value": 5},
         "bind": "actionResult0"}
    ]);
    let mut bindings = BTreeMap::new();
    run(&mut engine, &stack, &program, &mut bindings);
    // The default provider answers the minimum.
    assert!(matches!(
        bindings.get("actionResult0"),
        Some(RuntimeBinding::Integer(2))
    ));
}
