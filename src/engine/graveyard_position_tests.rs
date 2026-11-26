use super::super::*;
use super::graveyard_end_cards_supported;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{1}".to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        404,
    )
    .expect("graveyard position test setup is valid")
}

fn stack_object() -> StackObject {
    StackObject {
        id: "stack:graveyard-position".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &definition("position-source", "Sorcery"), 0),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// The graveyard, bottom first: the engine appends each card put there.
fn engine_with_graveyard() -> (GameEngine, Vec<String>) {
    let mut engine = engine();
    let cards = [
        ("old-bear", "Creature - Bear"),
        ("old-land", "Land"),
        ("new-elf", "Creature - Elf"),
        ("newest-land", "Land"),
    ]
    .iter()
    .enumerate()
    .map(|(index, (id, type_line))| {
        card_instance("player-0", &definition(id, type_line), index + 1)
    })
    .collect::<Vec<_>>();
    let ids = cards
        .iter()
        .map(|card| card.instance_id.clone())
        .collect::<Vec<_>>();
    let index = engine.player_index("player-0").expect("player-0 exists");
    engine.state.players[index].graveyard = cards;
    (engine, ids)
}

fn positioned(end: &str, count: i64, filter: Option<Value>) -> Value {
    let mut expression = json!({
        "kind": "graveyardEndCards",
        "zone": {"kind": "graveyard", "player": {"kind": "abilityController"}},
        "end": end,
        "count": {"kind": "integer", "value": count},
    });
    if let Some(filter) = filter {
        expression["where"] = filter;
    }
    expression
}

fn resolve(engine: &GameEngine, expression: &Value) -> Vec<String> {
    engine
        .resolve_runtime_object_ids_checked(
            expression,
            &stack_object(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("a supported graveyard position resolves")
}

/// The top of a graveyard is the card most recently put into it; a filter
/// counts only matching cards from that end (`the top creature card`), and
/// the bottom reads from the oldest card.
#[test]
fn graveyard_end_reads_matching_cards_from_the_named_end() {
    let (engine, ids) = engine_with_graveyard();
    let creature = json!({"kind": "cardTypeContains", "value": "Creature"});
    assert_eq!(
        resolve(&engine, &positioned("top", 1, None)),
        vec![ids[3].clone()]
    );
    assert_eq!(
        resolve(&engine, &positioned("top", 1, Some(creature.clone()))),
        vec![ids[2].clone()]
    );
    assert_eq!(
        resolve(&engine, &positioned("top", 2, Some(creature.clone()))),
        vec![ids[2].clone(), ids[0].clone()]
    );
    assert_eq!(
        resolve(&engine, &positioned("bottom", 1, None)),
        vec![ids[0].clone()]
    );
    assert_eq!(
        resolve(&engine, &positioned("bottom", 3, None)),
        vec![ids[0].clone(), ids[1].clone(), ids[2].clone()]
    );
    // More cards than exist: every matching card, never an invented one.
    assert_eq!(
        resolve(&engine, &positioned("top", 5, Some(creature))).len(),
        2
    );
}

/// A card put into the graveyard later becomes its new top card.
#[test]
fn graveyard_top_follows_the_order_cards_were_put_there() {
    let (mut engine, ids) = engine_with_graveyard();
    let index = engine.player_index("player-0").expect("player-0 exists");
    let newest = card_instance("player-0", &definition("newest-bear", "Creature - Bear"), 9);
    let newest_id = newest.instance_id.clone();
    engine.state.players[index].graveyard.push(newest);
    let creature = json!({"kind": "cardTypeContains", "value": "Creature"});
    assert_eq!(
        resolve(&engine, &positioned("top", 1, Some(creature))),
        vec![newest_id]
    );
    assert_eq!(
        resolve(&engine, &positioned("bottom", 1, None)),
        vec![ids[0].clone()]
    );
}

#[test]
fn graveyard_end_validation_requires_one_holder_an_end_and_a_count() {
    assert!(graveyard_end_cards_supported(&positioned("top", 1, None)));
    assert!(graveyard_end_cards_supported(&positioned(
        "bottom",
        2,
        Some(json!({"kind": "cardTypeContains", "value": "Creature"}))
    )));
    let mut middle = positioned("top", 1, None);
    middle["end"] = json!("middle");
    assert!(!graveyard_end_cards_supported(&middle));
    let mut library = positioned("top", 1, None);
    library["zone"]["kind"] = json!("library");
    assert!(!graveyard_end_cards_supported(&library));
    let mut population = positioned("top", 1, None);
    population["zone"]["players"] = json!({"kind": "eachPlayer"});
    assert!(!graveyard_end_cards_supported(&population));
    let mut uncounted = positioned("top", 1, None);
    uncounted["count"] = json!({"kind": "unknownCount"});
    assert!(!graveyard_end_cards_supported(&uncounted));
    assert!(effect_supported(&json!({
        "kind": "bind",
        "id": "graveyardPositionCards",
        "value": positioned("top", 1, None),
    })));
}
