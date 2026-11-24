//! Runtime evaluation of the condition-atom language primitives: the open
//! event-history lexicon (descend, sacrifice, entry excluding the source,
//! cast-mode details), the mana spent to cast a spell, the extreme measure
//! of the opponents, and the last known type line of a departed target.
use super::*;

fn conditional_rule(condition: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "condition": condition,
        "modifiers": [
            { "kind": "grantKeyword", "objects": { "kind": "self" }, "keyword": "flying" },
        ],
    })
}

fn engine_with_source(condition: Value) -> GameEngine {
    let mut engine = test_engine(2);
    let mut definition = test_definition("conditional-source", "Creature - Test");
    definition.rules = vec![conditional_rule(condition)];
    engine.state.players[0].battlefield.push(test_instance(
        "conditional-source",
        definition,
        "player-0",
    ));
    engine
}

fn condition_holds(engine: &GameEngine) -> bool {
    let source = engine.state.players[0].battlefield[0].clone();
    engine.permanent_has_effective_keyword(&source, "flying")
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn at_least(left: Value, value: i64) -> Value {
    json!({
        "kind": "compare",
        "operator": ">=",
        "left": left,
        "right": { "kind": "integer", "value": value },
    })
}

#[test]
fn descending_counts_permanent_cards_put_into_a_graveyard_not_spells_or_tokens() {
    let mut engine = engine_with_source(at_least(
        json!({
            "kind": "countEventsThisTurn",
            "event": "cardsEnteredGraveyard",
            "player": controller(),
            "where": { "kind": "isPermanentCard" },
            "measure": "cards",
        }),
        1,
    ));
    let instant = test_instance(
        "arrived-instant",
        test_definition("arrived-instant", "Instant"),
        "player-0",
    );
    engine.record_event(
        "cardsEnteredGraveyard",
        Some("player-0".to_string()),
        None,
        event_history::graveyard_arrival_detail(&[instant]),
    );
    assert!(
        !condition_holds(&engine),
        "an instant card is not a permanent card"
    );

    let mut token_definition = test_definition("arrived-token", "Token Creature - Zombie");
    token_definition.is_token = true;
    let token = test_instance("arrived-token", token_definition, "player-0");
    engine.record_event(
        "cardsEnteredGraveyard",
        Some("player-0".to_string()),
        None,
        event_history::graveyard_arrival_detail(&[token]),
    );
    assert!(
        !condition_holds(&engine),
        "a token that ceased to exist was never a card"
    );

    let creature = test_instance(
        "arrived-creature",
        test_definition("arrived-creature", "Creature - Goblin"),
        "player-1",
    );
    engine.record_event(
        "cardsEnteredGraveyard",
        Some("player-1".to_string()),
        None,
        event_history::graveyard_arrival_detail(&[creature.clone()]),
    );
    assert!(
        !condition_holds(&engine),
        "another player's graveyard is not the controller's descent"
    );
    engine.record_event(
        "cardsEnteredGraveyard",
        Some("player-0".to_string()),
        None,
        event_history::graveyard_arrival_detail(&[creature]),
    );
    assert!(condition_holds(&engine));
}

#[test]
fn an_entry_history_with_another_excludes_the_source_itself() {
    let mut engine = engine_with_source(at_least(
        json!({
            "kind": "countEventsThisTurn",
            "event": "permanentEnteredBattlefield",
            "player": controller(),
            "where": { "kind": "cardTypeContains", "value": "Creature" },
            "excludeObject": { "kind": "self" },
        }),
        1,
    ));
    engine.record_event(
        "permanentEnteredBattlefield",
        Some("player-0".to_string()),
        Some("conditional-source".to_string()),
        json!({ "typeLine": "Creature - Test", "wasCreature": true }),
    );
    assert!(!condition_holds(&engine), "the source's own entry");
    engine.record_event(
        "permanentEnteredBattlefield",
        Some("player-0".to_string()),
        Some("other-creature".to_string()),
        json!({ "typeLine": "Creature - Elk", "wasCreature": true }),
    );
    assert!(condition_holds(&engine));
}

#[test]
fn a_sacrifice_history_reads_the_type_line_its_record_kept() {
    let mut engine = engine_with_source(at_least(
        json!({
            "kind": "countEventsThisTurn",
            "event": "permanentSacrificed",
            "player": controller(),
            "where": { "kind": "subtypeContains", "value": "Clue" },
        }),
        2,
    ));
    for (id, type_line) in [
        ("sacrificed-food", "Token Artifact - Food"),
        ("sacrificed-clue-a", "Token Artifact - Clue"),
    ] {
        engine.record_event(
            "permanentSacrificed",
            Some("player-0".to_string()),
            Some(id.to_string()),
            json!({ "typeLine": type_line, "wasCreature": false, "wasToken": true }),
        );
    }
    assert!(!condition_holds(&engine), "one Clue, not two");
    engine.record_event(
        "permanentSacrificed",
        Some("player-0".to_string()),
        Some("sacrificed-clue-b".to_string()),
        json!({ "typeLine": "Token Artifact - Clue", "wasCreature": false, "wasToken": true }),
    );
    assert!(condition_holds(&engine));
}

#[test]
fn a_cast_mode_detail_is_a_pointer_into_the_cast_record() {
    let mut engine = engine_with_source(at_least(
        json!({
            "kind": "countEventsThisTurn",
            "event": "spellCast",
            "where": null,
            "detail": { "/decisions/useWarp": true },
        }),
        1,
    ));
    engine.record_event(
        "spellCast",
        Some("player-1".to_string()),
        Some("plain-spell".to_string()),
        json!({ "decisions": { "useWarp": false } }),
    );
    assert!(!condition_holds(&engine));
    engine.record_event(
        "spellCast",
        Some("player-1".to_string()),
        Some("warped-spell".to_string()),
        json!({ "decisions": { "useWarp": true } }),
    );
    assert!(condition_holds(&engine));
}

#[test]
fn a_landfall_history_counts_lands_entering_under_the_controller() {
    let mut engine = engine_with_source(at_least(
        json!({
            "kind": "countEventsThisTurn",
            "event": "permanentEnteredBattlefield",
            "player": controller(),
            "where": { "kind": "cardTypeContains", "value": "Land" },
        }),
        1,
    ));
    engine.record_event(
        "permanentEnteredBattlefield",
        Some("player-1".to_string()),
        Some("opponent-land".to_string()),
        json!({ "typeLine": "Basic Land - Mountain", "wasCreature": false }),
    );
    assert!(
        !condition_holds(&engine),
        "an opponent's land is no landfall"
    );
    engine.record_event(
        "permanentEnteredBattlefield",
        Some("player-0".to_string()),
        Some("own-land".to_string()),
        json!({ "typeLine": "Basic Land - Mountain", "wasCreature": false }),
    );
    assert!(condition_holds(&engine));
}

fn cast_decisions(symbols: &[&str]) -> BTreeMap<String, Value> {
    BTreeMap::from([(
        "manaPayment".to_string(),
        json!([{
            "lifePaid": 0,
            "mana": symbols,
            "ruleIndex": null,
            "sacrificesSource": false,
            "sourceId": "mana-source",
        }]),
    )])
}

fn mana_spent(symbol: Option<&str>) -> Value {
    let mut amount = json!({ "kind": "manaSpentToCast", "spell": { "kind": "self" } });
    if let Some(symbol) = symbol {
        amount["symbol"] = json!(symbol);
    }
    amount
}

#[test]
fn a_cast_permanent_keeps_the_mana_spent_to_cast_it() {
    let mut engine = engine_with_source(at_least(mana_spent(Some("W")), 2));
    assert!(
        !condition_holds(&engine),
        "a permanent that was not cast had no mana spent to cast it"
    );
    let source = &mut engine.state.players[0].battlefield[0];
    source.flags.insert("wasCast".to_string(), true);
    mana_payment::record_mana_spent_flags(&mut source.flags, &cast_decisions(&["W", "W", "U"]));
    assert!(condition_holds(&engine), "{{W}}{{W}} was spent");

    let mut engine = engine_with_source(at_least(mana_spent(Some("R")), 1));
    let source = &mut engine.state.players[0].battlefield[0];
    source.flags.insert("wasCast".to_string(), true);
    mana_payment::record_mana_spent_flags(&mut source.flags, &cast_decisions(&["W", "W", "U"]));
    assert!(!condition_holds(&engine), "no {{R}} was spent");

    let mut engine = engine_with_source(at_least(mana_spent(None), 3));
    let source = &mut engine.state.players[0].battlefield[0];
    source.flags.insert("wasCast".to_string(), true);
    mana_payment::record_mana_spent_flags(&mut source.flags, &cast_decisions(&["W", "W", "U"]));
    assert!(condition_holds(&engine), "three mana in total");
}

#[test]
fn a_resolving_spell_reads_the_mana_its_cast_decisions_spent() {
    let engine = test_engine(2);
    let decisions = cast_decisions(&["G", "C"]);
    let spell = StackObject {
        id: "stack:spell".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "resolving-spell",
            test_definition("resolving-spell", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: decisions.clone(),
        targets: BTreeMap::new(),
    };
    let read = |symbol| {
        engine.runtime_numeric_expression(
            &mana_spent(symbol),
            &spell,
            &BTreeMap::new(),
            &spell.decisions,
        )
    };
    assert_eq!(read(Some("G")), Some(1), "{{G}} was spent");
    assert_eq!(read(Some("R")), Some(0), "{{R}} was not spent");
    assert_eq!(read(Some("C")), Some(1));
    assert_eq!(read(None), Some(2));

    // The spell a cast trigger observed keeps its own per-symbol record.
    let trigger_decisions = BTreeMap::from([(
        "triggeringSpellManaSpentBySymbol".to_string(),
        mana_payment::triggering_spell_mana_spent_record(&decisions),
    )]);
    let observed = json!({ "kind": "manaSpentToCast", "spell": { "kind": "triggeringSpell" } });
    assert_eq!(
        engine.mana_spent_to_cast(&observed, None, &trigger_decisions),
        Some(2)
    );
}

#[test]
fn an_opponent_extremum_compares_one_measure_of_two_players() {
    let hand = |player: Value| json!({ "kind": "countCards", "zone": { "kind": "hand", "player": player }, "where": null });
    let mut engine = engine_with_source(json!({
        "kind": "compare",
        "operator": ">",
        "left": {
            "kind": "opponentExtremum",
            "extremum": "greatest",
            "player": controller(),
            "value": hand(json!({ "kind": "quantifiedPlayer" })),
        },
        "right": hand(controller()),
    }));
    engine.state.players[0].hand.truncate(2);
    engine.state.players[1].hand.truncate(2);
    assert!(!condition_holds(&engine), "equal hands");
    engine.state.players[1].hand.push(test_instance(
        "extra-card",
        test_definition("extra-card", "Instant"),
        "player-1",
    ));
    assert!(
        condition_holds(&engine),
        "an opponent has more cards in hand than you"
    );
    assert!(opponent_extremum::opponent_extremum_supported(&json!({
        "kind": "opponentExtremum",
        "extremum": "least",
        "player": controller(),
        "value": { "kind": "lifeTotal", "player": { "kind": "quantifiedPlayer" } },
    })));
    assert!(
        !opponent_extremum::opponent_extremum_supported(&json!({
            "kind": "opponentExtremum",
            "extremum": "greatest",
            "player": controller(),
            "value": hand(controller()),
        })),
        "a measure naming another player than the quantified one is refused"
    );
}

#[test]
fn a_departed_target_is_read_on_the_type_line_its_departure_recorded() {
    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:destroy".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "destroying-spell",
            test_definition("destroying-spell", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "target-0".to_string(),
            TargetRef::Permanent {
                instance_id: "destroyed-land".to_string(),
            },
        )]),
    };
    let snow_land = json!({
        "kind": "departedObjectMatches",
        "object": { "kind": "chosenTarget", "id": "target-0" },
        "where": {
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Snow" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        },
    });
    assert!(event_history::departed_object_condition_supported(
        &snow_land
    ));
    assert!(
        !engine.runtime_condition_value(
            &snow_land,
            &stack_object,
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        "no departure record and no current object: no characteristic"
    );
    engine.record_event(
        "permanentLeftBattlefield",
        Some("player-1".to_string()),
        Some("destroyed-land".to_string()),
        json!({
            "destination": "graveyard",
            "typeLine": "Snow Land - Forest",
            "wasCreature": false,
            "owner": "player-1",
        }),
    );
    assert!(engine.runtime_condition_value(
        &snow_land,
        &stack_object,
        &BTreeMap::new(),
        &BTreeMap::new()
    ));
    let mut not_snow = snow_land.clone();
    not_snow["where"] = json!({
        "kind": "not",
        "operand": { "kind": "typeLineContains", "value": "Snow" },
    });
    assert!(!engine.runtime_condition_value(
        &not_snow,
        &stack_object,
        &BTreeMap::new(),
        &BTreeMap::new()
    ));
    assert!(
        !event_history::departed_object_condition_supported(&json!({
            "kind": "departedObjectMatches",
            "object": { "kind": "chosenTarget", "id": "target-0" },
            "where": { "kind": "colorContains", "value": "Blue" },
        })),
        "a color is not recorded by a departure"
    );
}
