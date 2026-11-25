use super::super::*;
use super::{cost_mana_value_binding, counter_total, numeric_quantity_supported};

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: mana_cost.to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", "");
    GameEngine::new(
        GameSetup {
            players: (0..3)
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
        91,
    )
    .expect("numeric quantity test setup is valid")
}

fn controller() -> Value {
    json!({ "kind": "controller" })
}

fn put_in_graveyard(engine: &mut GameEngine, player: usize, cards: &[(&str, &str, &str)]) {
    for (index, (id, type_line, mana_cost)) in cards.iter().enumerate() {
        let owner = engine.state.players[player].id.clone();
        let card = card_instance(&owner, &definition(id, type_line, mana_cost), 900 + index);
        engine.state.players[player].graveyard.push(card);
    }
}

#[test]
fn quantity_shapes_are_owned_and_validated_by_one_grammar() {
    for expression in [
        json!({ "kind": "commanderCastsFromCommandZone", "player": controller() }),
        json!({ "kind": "countOpponentsAttackedThisTurn", "player": controller() }),
        json!({
            "kind": "countDistinctManaValues",
            "zone": { "kind": "graveyard", "player": controller() },
            "where": null,
        }),
        json!({
            "kind": "countManaSymbolsAmongCards",
            "zone": { "kind": "graveyard", "player": controller() },
            "where": null,
            "color": "B",
        }),
    ] {
        assert_eq!(numeric_quantity_supported(&expression), Some(true));
        assert!(numeric_expression_supported(&expression));
    }
    // An unknown color, an exile zone without a linked source and a kind
    // this module does not own are refused or left to their owners.
    assert_eq!(
        numeric_quantity_supported(&json!({
            "kind": "countManaSymbolsAmongCards",
            "zone": { "kind": "graveyard", "player": controller() },
            "where": null,
            "color": "Black",
        })),
        Some(false)
    );
    assert_eq!(
        numeric_quantity_supported(&json!({
            "kind": "countDistinctManaValues",
            "zone": { "kind": "exile", "player": controller() },
            "where": null,
        })),
        Some(false)
    );
    assert_eq!(
        numeric_quantity_supported(&json!({ "kind": "countCards" })),
        None
    );
}

#[test]
fn commander_record_counts_only_the_players_own_commander_casts() {
    let mut engine = engine();
    engine.state.commanders = vec![
        CommanderState {
            card_instance_id: "commander-a".to_string(),
            name: "Commander A".to_string(),
            owner_id: "player-0".to_string(),
            casts_from_command_zone: 2,
        },
        CommanderState {
            card_instance_id: "commander-b".to_string(),
            name: "Commander B".to_string(),
            owner_id: "player-0".to_string(),
            casts_from_command_zone: 1,
        },
        CommanderState {
            card_instance_id: "commander-c".to_string(),
            name: "Commander C".to_string(),
            owner_id: "player-1".to_string(),
            casts_from_command_zone: 5,
        },
    ];
    let expression = json!({ "kind": "commanderCastsFromCommandZone", "player": controller() });
    // Commander's Insignia: the partner pair was cast three times in all.
    assert_eq!(engine.numeric_expression(&expression, "player-0"), Some(3));
    assert_eq!(engine.numeric_expression(&expression, "player-2"), Some(0));
}

#[test]
fn opponents_attacked_this_turn_reads_every_attack_declaration_of_the_turn() {
    let mut engine = engine();
    let turn = engine.state.turn_number;
    let attack = |defender: TargetRef| {
        json!({
            "defender": serde_json::to_value(defender).expect("a serializable defender"),
        })
    };
    // Two attacks at player-1 (two combats), one at player-2's planeswalker.
    for defender in [
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        TargetRef::Permanent {
            instance_id: "planeswalker-of-player-2".to_string(),
        },
    ] {
        engine.record_event(
            "attackerDeclared",
            Some("player-0".to_string()),
            Some("attacker".to_string()),
            attack(defender),
        );
    }
    let expression = json!({ "kind": "countOpponentsAttackedThisTurn", "player": controller() });
    assert_eq!(engine.numeric_expression(&expression, "player-0"), Some(1));
    // An attack of an earlier turn is not this turn's.
    engine.state.turn_number = turn + 1;
    assert_eq!(engine.numeric_expression(&expression, "player-0"), Some(0));
}

#[test]
fn distinct_mana_values_and_mana_symbols_read_the_graveyard_collection() {
    let mut engine = engine();
    put_in_graveyard(
        &mut engine,
        0,
        &[
            ("bear", "Creature - Bear", "{1}{G}"),
            ("divination", "Sorcery", "{2}{U}"),
            ("duress", "Sorcery", "{B}"),
            ("shock", "Instant", "{R}"),
            ("swamp", "Basic Land - Swamp", ""),
            ("hybrid", "Creature - Ooze", "{B/G}{B/G}"),
            ("phyrexian", "Instant", "{1}{B/P}"),
            ("twobrid", "Sorcery", "{2/B}{B}"),
        ],
    );
    let nonland =
        json!({ "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } });
    let distinct = json!({
        "kind": "countDistinctManaValues",
        "zone": { "kind": "graveyard", "player": controller() },
        "where": nonland,
    });
    // Sudden Insight: mana values 2, 3, 1, 1, 2, 2, 2 among nonland cards.
    let graveyard = &engine.state.players[0].graveyard;
    let expected = graveyard
        .iter()
        .filter(|card| !card.definition.type_line.contains("Land"))
        .map(|card| mana_value(&card.definition))
        .collect::<BTreeSet<_>>()
        .len() as i32;
    assert_eq!(
        engine.numeric_expression(&distinct, "player-0"),
        Some(expected)
    );
    assert_eq!(expected, 3);
    // Umbra Stalker: {B}, {B/G}{B/G}, {B/P} and {2/B}{B}: a hybrid or
    // Phyrexian symbol of black counts once (CR 202.2).
    let symbols = json!({
        "kind": "countManaSymbolsAmongCards",
        "zone": { "kind": "graveyard", "player": controller() },
        "where": null,
        "color": "B",
    });
    assert_eq!(engine.numeric_expression(&symbols, "player-0"), Some(6));
    // Another player's graveyard is empty: zero, not the controller's.
    assert_eq!(engine.numeric_expression(&symbols, "player-1"), Some(0));
}

#[test]
fn sacrifice_cost_records_the_permanents_mana_value_and_counter_totals_are_bounded() {
    let permanent = card_instance(
        "player-0",
        &definition("pod-fodder", "Creature - Elf", "{2}{G}"),
        0,
    );
    assert_eq!(
        cost_mana_value_binding(
            &json!({ "kind": "sacrificePermanent", "bindManaValueAs": "sacrificedPermanentManaValue" }),
            &permanent,
        ),
        Some(("sacrificedPermanentManaValue".to_string(), 3))
    );
    assert_eq!(
        cost_mana_value_binding(&json!({ "kind": "sacrificePermanent" }), &permanent),
        None
    );
    let counters = BTreeMap::from([
        ("+1/+1".to_string(), 2),
        ("charge".to_string(), 3),
        ("lore".to_string(), i32::MAX),
    ]);
    assert_eq!(counter_total(&counters, Some("charge")), 3);
    assert_eq!(counter_total(&counters, Some("time")), 0);
    // Every kind (Kyler): saturating, never wrapping.
    assert_eq!(counter_total(&counters, None), i32::MAX);
}

fn spell_targeting(player: &str) -> StackObject {
    StackObject {
        id: "stack:numeric-quantity".to_string(),
        controller: "player-0".to_string(),
        card: card_instance(
            "player-0",
            &definition("traumatize", "Sorcery", "{3}{U}{U}"),
            0,
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPlayer".to_string(),
            TargetRef::Player {
                player_id: player.to_string(),
            },
        )]),
    }
}

#[test]
fn half_the_acting_players_library_rounds_as_stated() {
    // The IR `mills half their library, rounded down` lowers to once the
    // acting player (the declared target) replaces the placeholder.
    let mut engine = engine();
    let filler = definition("library-card", "Sorcery", "{1}");
    engine.state.players[1].library = (0..41)
        .map(|index| card_instance("player-1", &filler, 500 + index))
        .collect();
    let half = |round: &str| {
        json!({
            "kind": "divide",
            "left": {
                "kind": "countCards",
                "zone": {
                    "kind": "library",
                    "player": { "kind": "chosenTarget", "id": "targetPlayer" },
                },
                "where": null,
            },
            "right": 2,
            "round": round,
        })
    };
    assert!(numeric_expression_supported(&half("down")));
    let stack_object = spell_targeting("player-1");
    // Traumatize on 41 cards mills 20; rounded up it would mill 21.
    assert_eq!(
        engine.runtime_numeric_expression(
            &half("down"),
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        Some(20)
    );
    assert_eq!(
        engine.runtime_numeric_expression(
            &half("up"),
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        Some(21)
    );
    // The unreplaced placeholder names no player: refused, never zero.
    assert!(!numeric_expression_supported(&json!({
        "kind": "countCards",
        "zone": { "kind": "library", "player": { "kind": "pendingActingPlayer" } },
        "where": null,
    })));
}

#[test]
fn every_counter_kind_on_the_source_is_read_by_static_and_resolution_paths() {
    let engine = engine();
    let mut source = card_instance(
        "player-0",
        &definition("kyler", "Legendary Creature - Human", "{W}{W}"),
        0,
    );
    source.counters = BTreeMap::from([("+1/+1".to_string(), 2), ("shield".to_string(), 1)]);
    let every = json!({ "kind": "countCounters", "object": { "kind": "self" } });
    assert!(numeric_expression_supported(&every));
    // Kyler, Sigardian Emissary: two +1/+1 counters and a shield counter.
    assert_eq!(
        engine.numeric_expression_with_source(&every, "player-0", Some(&source)),
        Some(3)
    );
    let mut stack_object = spell_targeting("player-1");
    stack_object.card = source;
    assert_eq!(
        engine.runtime_numeric_expression(
            &every,
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        Some(3)
    );
}
