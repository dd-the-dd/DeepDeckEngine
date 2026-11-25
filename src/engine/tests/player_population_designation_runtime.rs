//! Runtime proof of the restricted player populations and seat-relative
//! players the Oracle compiler lowers for player subjects:
//! `each player who controls a creature draws a card`, `each player who
//! controls the most lands ...` (ties included), `each player other than
//! its controller ...` (CR 101.4, 102.1-102.3), and `the player to your
//! right gains control of this artifact` (seating order, CR 101.4). The IR
//! below is the shape `effects/player_set.rs` and the performer lowering
//! emit.
use super::*;

#[derive(Default)]
struct FirstChoice;

impl DecisionProvider for FirstChoice {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn controller() -> Value {
    json!({"kind": "controllerOf", "object": {"kind": "self"}})
}

fn member() -> Value {
    json!({"kind": "quantifiedPlayer"})
}

fn type_is(card_type: &str) -> Value {
    json!({"kind": "cardTypeContains", "value": card_type})
}

fn controlled_count(player: Value, card_type: &str) -> Value {
    json!({"kind": "countPermanents", "player": player, "where": type_is(card_type),
        "excludeSource": false})
}

fn at_least_one(card_type: &str) -> Value {
    json!({"kind": "compare", "operator": ">=",
        "left": controlled_count(member(), card_type),
        "right": {"kind": "integer", "value": 1}})
}

/// `forEachPlayer` over `players`, each member drawing one card as the
/// performer of the controller's `draw a card`.
fn each_draws(players: Value) -> Value {
    json!([{
        "kind": "forEachPlayer",
        "bind": "performer0",
        "players": players,
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": {"kind": "integer", "value": 1},
            "performer": {"kind": "boundValue", "id": "performer0"},
        }],
    }])
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
        2026,
    )
    .unwrap();
    let card = card_instance("player-0", &definition("source", "Artifact"), 9999);
    let stack = StackObject {
        id: "player-population".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
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

fn run(engine: &mut GameEngine, stack: &StackObject, effects: &Value) {
    assert!(effects.as_array().unwrap().iter().all(effect_supported));
    engine
        .execute_effects(
            effects,
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstChoice,
        )
        .expect("the lowered program executes");
}

fn hand_sizes(engine: &GameEngine) -> Vec<usize> {
    engine.state.players.iter().map(|p| p.hand.len()).collect()
}

#[test]
fn only_members_satisfying_the_relative_predicate_perform_the_action() {
    // Depopulate: `Each player who controls a multicolored creature draws a
    // card.` with the predicate reduced to `a creature`.
    let (mut engine, stack) = position(3);
    add(&mut engine, 1, "bear", "Creature");
    add(&mut engine, 2, "relic", "Artifact");
    let players = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"},
        "where": at_least_one("Creature")});
    run(&mut engine, &stack, &each_draws(players));
    assert_eq!(hand_sizes(&engine), [0, 1, 0]);
}

#[test]
fn a_negated_predicate_over_the_opponents_spares_the_controller() {
    // Thornbow Archer: `each opponent who doesn't control an Elf`.
    let (mut engine, stack) = position(3);
    add(&mut engine, 1, "elf", "Creature");
    let players = json!({"kind": "filterPlayers",
        "players": {"kind": "opponentsOf", "player": controller()},
        "where": {"kind": "not", "operand": at_least_one("Creature")}});
    run(&mut engine, &stack, &each_draws(players));
    assert_eq!(hand_sizes(&engine), [0, 0, 1]);
}

#[test]
fn a_superlative_selects_every_player_tied_for_the_extremum() {
    // Tectonic Hellion: `each player who controls the most lands`.
    let (mut engine, stack) = position(3);
    for player in [0, 2] {
        for land in 0..3 {
            add(&mut engine, player, &format!("land{land}"), "Land");
        }
    }
    add(&mut engine, 1, "land", "Land");
    let most = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"},
        "superlative": {"extremum": "greatest", "value": controlled_count(member(), "Land")}});
    run(&mut engine, &stack, &each_draws(most));
    assert_eq!(hand_sizes(&engine), [1, 0, 1]);
    // Gor Muldrak: `each player who controls the fewest ...`.
    let fewest = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"},
        "superlative": {"extremum": "least", "value": controlled_count(member(), "Land")}});
    run(&mut engine, &stack, &each_draws(fewest));
    assert_eq!(hand_sizes(&engine), [1, 1, 1]);
}

#[test]
fn an_excluded_player_is_removed_and_an_unresolved_one_is_an_error() {
    // Fractured Identity: `Each player other than its controller ...`.
    let (mut engine, stack) = position(3);
    let players = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"},
        "except": {"kind": "abilityController"}});
    run(&mut engine, &stack, &each_draws(players));
    assert_eq!(hand_sizes(&engine), [0, 1, 1]);
    // An excluded player the program never bound is not an unrestricted
    // population.
    let unbound = each_draws(json!({"kind": "filterPlayers",
        "players": {"kind": "eachPlayer"},
        "except": {"kind": "boundValue", "id": "neverBound"}}));
    let result = engine.execute_effects(
        &unbound,
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut FirstChoice,
    );
    assert!(result.is_err());
    assert_eq!(hand_sizes(&engine), [0, 1, 1]);
}

#[test]
fn a_filter_naming_another_player_or_no_restriction_is_unsupported() {
    let players = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"},
        "where": {"kind": "compare", "operator": ">=",
            "left": controlled_count(controller(), "Land"),
            "right": {"kind": "integer", "value": 1}}});
    assert!(!effect_supported(&each_draws(players)[0]));
    let unrestricted = json!({"kind": "filterPlayers", "players": {"kind": "eachPlayer"}});
    assert!(!effect_supported(&each_draws(unrestricted)[0]));
    // A superlative is compared with every player, never a sub-population.
    let opponents = json!({"kind": "filterPlayers",
        "players": {"kind": "opponentsOf", "player": controller()},
        "superlative": {"extremum": "greatest", "value": controlled_count(member(), "Land")}});
    assert!(!effect_supported(&each_draws(opponents)[0]));
}

fn neighbor(direction: &str) -> Value {
    json!({"kind": "seatNeighbor", "player": controller(), "direction": direction})
}

#[test]
fn seat_neighbors_follow_turn_order_and_skip_players_who_left() {
    let (mut engine, stack) = position(3);
    let bindings = BTreeMap::new();
    // Turn order passes to the left (CR 101.4).
    assert_eq!(
        engine.resolve_player(&neighbor("left"), &stack, &bindings),
        Some("player-1".to_string())
    );
    assert_eq!(
        engine.resolve_player(&neighbor("right"), &stack, &bindings),
        Some("player-2".to_string())
    );
    engine.state.players[2].has_lost = true;
    assert_eq!(
        engine.resolve_player(&neighbor("right"), &stack, &bindings),
        Some("player-1".to_string())
    );
    // Two players: both neighbors are the opponent.
    let (two, stack) = position(2);
    for direction in ["left", "right"] {
        assert_eq!(
            two.resolve_player(&neighbor(direction), &stack, &bindings),
            Some("player-1".to_string())
        );
    }
}

#[test]
fn the_player_to_your_right_gains_control_of_the_source() {
    // Hithlain Rope: `The player to your right gains control of this
    // artifact.` performed by the seat neighbor.
    let (mut engine, mut stack) = position(3);
    add(&mut engine, 0, "rope", "Artifact");
    let rope = engine.state.players[0].battlefield[0].clone();
    stack.card = rope.clone();
    let program = json!([{
        "kind": "gainControlPermanent",
        "permanent": {"kind": "self"},
        "controller": controller(),
        "performer": neighbor("right"),
    }]);
    run(&mut engine, &stack, &program);
    let (player_index, card_index) = engine
        .permanent_position(&rope.instance_id)
        .expect("the rope stays on the battlefield");
    assert_eq!(
        engine.state.players[player_index].battlefield[card_index].controller,
        "player-2"
    );
}
