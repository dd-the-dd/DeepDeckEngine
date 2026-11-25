//! Runtime proofs of the resolution records and totals the Oracle numeric
//! language composes (`NumericQuantity::AbilityHistory`, `PropertyTotal`,
//! `PlayersComparedWithController`, `PlayersAtExtremum`).
use super::super::*;
use super::{ability_digest, resolution_record_supported};

fn definition(id: &str, type_line: &str, power: Option<&str>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{2}".to_string(),
        power: power.map(str::to_string),
        toughness: power.map(str::to_string),
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", None);
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
        97,
    )
    .expect("resolution record test setup is valid")
}

fn put_on_battlefield(engine: &mut GameEngine, player: usize, card: &CardDefinition) -> String {
    let owner = engine.state.players[player].id.clone();
    let index = engine.state.players[player].battlefield.len() + 700;
    let instance = card_instance(&owner, card, index);
    let id = instance.instance_id.clone();
    engine.state.players[player].battlefield.push(instance);
    id
}

fn ability(source: &CardInstance, kind: &str, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:records".to_string(),
        controller: source.controller.clone(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(kind.to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn record(engine: &mut GameEngine, kind: &str, source: &str, rule: &Value) {
    engine.record_event(
        kind,
        None,
        Some(source.to_string()),
        json!({ "abilityDigest": ability_digest(rule) }),
    );
}

fn read(engine: &GameEngine, expression: Value, stack: &StackObject) -> Option<Option<i32>> {
    engine.resolution_record_numeric(&expression, stack, &BTreeMap::new(), &BTreeMap::new())
}

#[test]
fn record_shapes_are_owned_and_validated() {
    for expression in [
        json!({ "kind": "abilityResolutionsThisTurn" }),
        json!({ "kind": "abilityActivationsThisTurn" }),
        json!({ "kind": "sumBoundObjectProperty", "binding": "actionResult:0", "property": "power" }),
    ] {
        assert_eq!(resolution_record_supported(&expression), Some(true));
        assert!(numeric_expression_supported(&expression));
    }
    // A record with an unexpected field, an unknown property and a kind of
    // another owner are refused or left to their owners.
    assert_eq!(
        resolution_record_supported(&json!({ "kind": "abilityResolutionsThisTurn", "player": {} })),
        Some(false)
    );
    assert_eq!(
        resolution_record_supported(&json!({
            "kind": "sumBoundObjectProperty", "binding": "b", "property": "loyalty",
        })),
        Some(false)
    );
    assert_eq!(
        resolution_record_supported(&json!({ "kind": "countPermanents" })),
        None
    );
}

#[test]
fn the_digest_names_one_rule_deterministically() {
    let first = json!({ "kind": "activatedAbility", "effects": [{ "kind": "drawCards" }] });
    let second = json!({ "kind": "activatedAbility", "effects": [{ "kind": "gainLife" }] });
    assert_eq!(ability_digest(&first), ability_digest(&first.clone()));
    assert_ne!(ability_digest(&first), ability_digest(&second));
}

#[test]
fn resolutions_count_this_turn_for_the_same_ability_of_the_same_object() {
    let mut engine = engine();
    let source_id = put_on_battlefield(
        &mut engine,
        0,
        &definition("igniter", "Creature", Some("2")),
    );
    let source = engine.state.players[0].battlefield.last().unwrap().clone();
    let rule = json!({ "kind": "activatedAbility", "effects": [{ "kind": "modifyStats" }] });
    let other_rule = json!({ "kind": "triggeredAbility", "effects": [{ "kind": "gainLife" }] });
    record(
        &mut engine,
        "stackObjectStartedResolving",
        &source_id,
        &rule,
    );
    record(
        &mut engine,
        "stackObjectStartedResolving",
        &source_id,
        &other_rule,
    );
    record(
        &mut engine,
        "stackObjectStartedResolving",
        "another-object",
        &rule,
    );
    record(
        &mut engine,
        "stackObjectStartedResolving",
        &source_id,
        &rule,
    );
    let stack = ability(&source, "activatedAbility", &rule);
    let resolutions = json!({ "kind": "abilityResolutionsThisTurn" });
    // The current resolution is already recorded: this is the second one.
    assert_eq!(read(&engine, resolutions.clone(), &stack), Some(Some(2)));
    // A new turn has no resolution of the ability yet.
    engine.state.turn_number += 1;
    assert_eq!(read(&engine, resolutions.clone(), &stack), Some(Some(0)));
    record(
        &mut engine,
        "stackObjectStartedResolving",
        &source_id,
        &rule,
    );
    assert_eq!(read(&engine, resolutions.clone(), &stack), Some(Some(1)));
    // A permanent that left and returned is a new object (CR 400.7).
    engine.record_event(
        "permanentEnteredBattlefield",
        None,
        Some(source_id.clone()),
        json!({}),
    );
    assert_eq!(read(&engine, resolutions.clone(), &stack), Some(Some(0)));
    // A spell has no ability record: no value, never zero.
    let mut spell = stack.clone();
    spell.ability_kind = None;
    spell.ability_rule = None;
    assert_eq!(read(&engine, resolutions, &spell), Some(None));
}

#[test]
fn activations_count_only_for_an_activated_ability() {
    let mut engine = engine();
    let source_id = put_on_battlefield(&mut engine, 0, &definition("whelp", "Creature", Some("2")));
    let source = engine.state.players[0].battlefield.last().unwrap().clone();
    let rule = json!({ "kind": "activatedAbility", "effects": [{ "kind": "modifyStats" }] });
    for _ in 0..4 {
        record(&mut engine, "activatedAbilityPutOnStack", &source_id, &rule);
    }
    record(
        &mut engine,
        "stackObjectStartedResolving",
        &source_id,
        &rule,
    );
    let activations = json!({ "kind": "abilityActivationsThisTurn" });
    let stack = ability(&source, "activatedAbility", &rule);
    assert_eq!(read(&engine, activations.clone(), &stack), Some(Some(4)));
    let triggered = ability(&source, "triggeredAbility", &rule);
    assert_eq!(read(&engine, activations, &triggered), Some(None));
}

#[test]
fn a_bound_total_reads_departed_objects_as_last_known_information() {
    let mut engine = engine();
    let live = put_on_battlefield(&mut engine, 1, &definition("live", "Creature", Some("3")));
    let source = engine.state.players[0]
        .battlefield
        .first()
        .cloned()
        .unwrap_or_else(|| card_instance("player-0", &definition("source", "Sorcery", None), 1));
    let stack = ability(
        &source,
        "activatedAbility",
        &json!({ "kind": "activatedAbility" }),
    );
    let binding = "actionResult:0";
    let bindings = BTreeMap::from([(
        binding.to_string(),
        RuntimeBinding::Objects(vec!["departed".to_string(), live.clone()]),
    )]);
    let decisions = BTreeMap::from([(
        effect_domains::bound_object_lki_key(binding),
        json!({ "departed": { "power": 5, "toughness": 1, "manaValue": 4 } }),
    )]);
    let total = |property: &str| json!({ "kind": "sumBoundObjectProperty", "binding": binding, "property": property });
    // The departed object's recorded power plus the live permanent's.
    assert_eq!(
        engine.resolution_record_numeric(&total("power"), &stack, &bindings, &decisions),
        Some(Some(8))
    );
    assert_eq!(
        engine.resolution_record_numeric(&total("manaValue"), &stack, &bindings, &decisions),
        Some(Some(6))
    );
    // Nothing affected: the total of an empty result is zero.
    let empty = BTreeMap::from([(binding.to_string(), RuntimeBinding::Objects(Vec::new()))]);
    assert_eq!(
        engine.resolution_record_numeric(&total("power"), &stack, &empty, &decisions),
        Some(Some(0))
    );
    // An unbound producer has no value.
    assert_eq!(
        engine.resolution_record_numeric(&total("power"), &stack, &BTreeMap::new(), &decisions),
        Some(None)
    );
    // A filter over a departed object without a departure record has no
    // recorded answer.
    let mut filtered = total("power");
    filtered["where"] = json!({ "kind": "cardTypeContains", "value": "Creature" });
    assert_eq!(
        engine.resolution_record_numeric(&filtered, &stack, &bindings, &decisions),
        Some(None)
    );
}

#[test]
fn players_compare_their_measure_with_the_controller_and_the_extremum() {
    let mut engine = engine();
    let land = definition("land", "Land", None);
    for _ in 0..2 {
        put_on_battlefield(&mut engine, 0, &land);
    }
    for _ in 0..3 {
        put_on_battlefield(&mut engine, 1, &land);
    }
    put_on_battlefield(&mut engine, 2, &land);
    let lands = json!({
        "kind": "countPermanents",
        "player": { "kind": "quantifiedPlayer" },
        "where": { "kind": "cardTypeContains", "value": "Land" },
    });
    let compared = |operator: &str| {
        json!({
            "kind": "countPlayersComparedWithController",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "players": { "kind": "opponentsOf", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "operator": operator,
            "value": lands,
        })
    };
    assert_eq!(resolution_record_supported(&compared(">")), Some(true));
    assert_eq!(
        engine.controller_relative_record(&compared(">"), "player-0"),
        Some(Some(1))
    );
    assert_eq!(
        engine.controller_relative_record(&compared("<"), "player-0"),
        Some(Some(1))
    );
    let extremum =
        json!({ "kind": "countPlayersAtExtremum", "extremum": "greatest", "value": lands });
    assert_eq!(
        engine.controller_relative_record(&extremum, "player-0"),
        Some(Some(1))
    );
    put_on_battlefield(&mut engine, 2, &land);
    put_on_battlefield(&mut engine, 2, &land);
    // A tie for the greatest: no player has more than each other player.
    assert_eq!(
        engine.controller_relative_record(&extremum, "player-0"),
        Some(Some(2))
    );
    // A measure naming another player than the member is refused.
    let mut foreign = extremum.clone();
    foreign["value"]["player"] = json!({ "kind": "triggeringPlayer" });
    assert_eq!(resolution_record_supported(&foreign), Some(false));
}

#[test]
fn a_restated_removal_is_tested_on_its_departure_record() {
    let mut engine = engine();
    let source = card_instance("player-0", &definition("smash", "Sorcery", None), 1);
    let stack = ability(
        &source,
        "activatedAbility",
        &json!({ "kind": "activatedAbility" }),
    );
    for (id, type_line) in [
        ("golem", "Artifact Creature - Golem"),
        ("plains", "Basic Land - Plains"),
    ] {
        engine.record_event(
            "permanentLeftBattlefield",
            None,
            Some(id.to_string()),
            json!({ "typeLine": type_line, "lastKnownState": { "token": false } }),
        );
    }
    let destroyed = |ids: &[&str]| {
        BTreeMap::from([(
            "actionResult:0".to_string(),
            RuntimeBinding::Objects(ids.iter().map(|id| id.to_string()).collect()),
        )])
    };
    let artifacts = json!({
        "kind": "countBoundObjectsLastKnown",
        "binding": "actionResult:0",
        "where": { "kind": "cardTypeContains", "value": "Artifact" },
    });
    assert_eq!(resolution_record_supported(&artifacts), Some(true));
    let read = |bindings: &BTreeMap<String, RuntimeBinding>| {
        engine.resolution_record_numeric(&artifacts, &stack, bindings, &BTreeMap::new())
    };
    assert_eq!(read(&destroyed(&["golem", "plains"])), Some(Some(1)));
    // Nothing was destroyed: the restated condition counts zero.
    assert_eq!(read(&destroyed(&[])), Some(Some(0)));
    assert_eq!(read(&destroyed(&["plains"])), Some(Some(0)));
    // An object without a departure record has no last known type line.
    assert_eq!(read(&destroyed(&["unrecorded"])), Some(None));
    // A characteristic the departure record does not keep is refused.
    let mut colored = artifacts.clone();
    colored["where"] = json!({ "kind": "colorContains", "value": "R" });
    assert_eq!(resolution_record_supported(&colored), Some(false));
}
