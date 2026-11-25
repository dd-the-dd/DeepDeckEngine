use super::super::*;
use super::aggregate_objects_supported;

fn definition(id: &str, type_line: &str, mana_cost: &str, power: Option<i32>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: mana_cost.to_string(),
        power: power.map(|power| power.to_string()),
        toughness: power.map(|_| "1".to_string()),
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", "", None);
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
            opening_hand_size: 7,
            starting_player: 0,
        },
        17,
    )
    .expect("aggregate test setup is valid")
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn aggregate(function: &str, property: &str, objects: Value) -> Value {
    json!({
        "kind": "aggregateObjects",
        "function": function,
        "property": property,
        "objects": objects,
    })
}

fn put(
    engine: &mut GameEngine,
    player: usize,
    zone: &str,
    cards: &[(&str, &str, &str, Option<i32>)],
) {
    let owner = engine.state.players[player].id.clone();
    for (index, (id, type_line, mana_cost, power)) in cards.iter().enumerate() {
        let mut card = card_instance(
            &owner,
            &definition(id, type_line, mana_cost, *power),
            700 + index,
        );
        card.instance_id = (*id).to_string();
        match zone {
            "battlefield" => engine.state.players[player].battlefield.push(card),
            _ => engine.state.players[player].graveyard.push(card),
        }
    }
}

#[test]
fn the_aggregate_shape_is_validated_by_its_own_grammar() {
    let permanents = json!({ "kind": "eachPermanent", "player": controller() });
    for (function, property) in [
        ("greatest", "power"),
        ("least", "toughness"),
        ("total", "manaValue"),
        ("distinct", "power"),
        ("distinct", "colors"),
        ("distinct", "cardTypes"),
        ("distinct", "creatureTypes"),
        ("distinct", "counterKinds"),
    ] {
        let expression = aggregate(function, property, permanents.clone());
        assert_eq!(aggregate_objects_supported(&expression), Some(true));
        assert!(numeric_expression_supported(&expression));
    }
    // A set property is no number a greatest/least/total reads; an unknown
    // property, two operands or a missing operand are refused; another kind
    // is left to its owner.
    assert_eq!(
        aggregate_objects_supported(&aggregate("total", "colors", permanents.clone())),
        Some(false)
    );
    assert_eq!(
        aggregate_objects_supported(&aggregate("distinct", "flavor", permanents.clone())),
        Some(false)
    );
    let mut both = aggregate("greatest", "power", permanents.clone());
    both["object"] = json!({ "kind": "self" });
    assert_eq!(aggregate_objects_supported(&both), Some(false));
    assert_eq!(
        aggregate_objects_supported(&json!({
            "kind": "aggregateObjects", "function": "greatest", "property": "power",
        })),
        Some(false)
    );
    assert_eq!(
        aggregate_objects_supported(&json!({ "kind": "countCards" })),
        None
    );
}

#[test]
fn extremums_and_totals_read_the_set_and_are_zero_or_undefined_when_empty() {
    let mut engine = engine();
    put(
        &mut engine,
        1,
        "battlefield",
        &[
            ("bear-a", "Creature - Bear", "{1}{G}", Some(3)),
            ("bear-b", "Creature - Bear", "{2}{G}", Some(3)),
            ("elf", "Creature - Elf", "{G}", Some(1)),
        ],
    );
    let opponents = json!({
        "kind": "eachPermanent",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
        "player": { "kind": "opponentsOf", "player": controller() },
    });
    let read = |engine: &GameEngine, function: &str, property: &str| {
        engine.numeric_expression(
            &aggregate(function, property, opponents.clone()),
            "player-0",
        )
    };
    // A tie for the greatest power is that power once.
    assert_eq!(read(&engine, "greatest", "power"), Some(3));
    assert_eq!(read(&engine, "least", "power"), Some(1));
    assert_eq!(read(&engine, "total", "power"), Some(7));
    assert_eq!(read(&engine, "total", "manaValue"), Some(6));
    assert_eq!(read(&engine, "distinct", "power"), Some(2));
    // The controller has no creature: zero, or undefined when required.
    let own = json!({ "kind": "eachPermanent", "player": controller() });
    assert_eq!(
        engine.numeric_expression(&aggregate("greatest", "power", own.clone()), "player-0"),
        Some(0)
    );
    let mut required = aggregate("greatest", "power", own);
    required["requireMembers"] = json!(true);
    assert_eq!(engine.numeric_expression(&required, "player-0"), None);
}

#[test]
fn distinct_card_types_count_card_types_not_subtypes() {
    let mut engine = engine();
    put(
        &mut engine,
        0,
        "graveyard",
        &[
            ("golem", "Artifact Creature - Golem", "{3}", Some(3)),
            ("zombie", "Creature - Zombie Golem", "{1}{B}", Some(2)),
            ("bolt", "Instant", "{R}", None),
            ("forest", "Land - Forest", "", None),
        ],
    );
    let cards = json!({
        "kind": "cardsInZone",
        "zone": { "kind": "graveyard", "player": controller() },
    });
    let read = |property: &str| {
        engine.numeric_expression(&aggregate("distinct", property, cards.clone()), "player-0")
    };
    assert_eq!(read("cardTypes"), Some(4));
    assert_eq!(read("creatureTypes"), Some(2));
    assert_eq!(read("names"), Some(4));
    // A card without a printed power is no member of a power aggregate.
    assert_eq!(read("power"), Some(2));
}

#[test]
fn distinct_counter_kinds_and_colors_read_each_member() {
    let mut engine = engine();
    put(
        &mut engine,
        0,
        "battlefield",
        &[
            ("gold", "Creature - Bear", "{W}{U}", Some(2)),
            ("red", "Creature - Bear", "{R}", Some(2)),
            ("relic", "Artifact", "{2}", None),
        ],
    );
    engine.state.players[0].battlefield[0]
        .counters
        .insert("+1/+1".to_string(), 2);
    engine.state.players[0].battlefield[2]
        .counters
        .insert("charge".to_string(), 1);
    engine.state.players[0].battlefield[2]
        .counters
        .insert("+1/+1".to_string(), 1);
    let own = json!({ "kind": "eachPermanent", "player": controller() });
    let read = |property: &str| {
        engine.numeric_expression(&aggregate("distinct", property, own.clone()), "player-0")
    };
    assert_eq!(read("counterKinds"), Some(2));
    // White, blue and red; the colorless artifact adds no color.
    assert_eq!(read("colors"), Some(3));
}
