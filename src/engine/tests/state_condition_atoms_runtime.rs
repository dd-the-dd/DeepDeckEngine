//! Runtime evaluation of the player-designation, turn and zone atoms of the
//! condition language (`state_condition_atoms.rs`): the monarch (CR 724),
//! the starting player (CR 103.1), whose turn it is (CR 500.1) and the zone
//! an object is in (CR 400.1), through the continuous evaluator, a player
//! quantifier and the resolution evaluator.
use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn source_card(engine: &GameEngine) -> CardInstance {
    engine.state.players[0].battlefield[0].clone()
}

fn engine_with_source() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(test_instance(
        "condition-source",
        test_definition("condition-source", "Creature - Test"),
        "player-0",
    ));
    engine
}

fn holds_for_source(engine: &GameEngine, condition: &Value) -> bool {
    let source = source_card(engine);
    assert!(condition_supported(condition), "{condition}");
    engine.condition_value_with_source(condition, "player-0", &BTreeMap::new(), Some(&source))
}

fn ability_on_stack(engine: &GameEngine, decisions: BTreeMap<String, Value>) -> StackObject {
    StackObject {
        id: "stack:state-condition".to_string(),
        controller: "player-0".to_string(),
        card: source_card(engine),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    }
}

fn holds_at_resolution(engine: &GameEngine, condition: &Value, stack: &StackObject) -> bool {
    assert!(condition_supported(condition), "{condition}");
    engine.runtime_condition_value(condition, stack, &BTreeMap::new(), &stack.decisions)
}

#[test]
fn the_monarch_atom_reads_the_one_monarch_designation_for_each_player_reference() {
    let mut engine = engine_with_source();
    let you = json!({ "kind": "playerIsMonarch", "player": controller() });
    let an_opponent = json!({
        "kind": "playerQuantifier",
        "quantifier": "some",
        "players": { "kind": "opponentsOf", "player": controller() },
        "condition": { "kind": "playerIsMonarch", "player": { "kind": "quantifiedPlayer" } },
    });
    assert!(
        !holds_for_source(&engine, &you),
        "nobody is the monarch yet"
    );
    assert!(!holds_for_source(&engine, &an_opponent));

    engine.become_monarch("player-1");
    assert!(!holds_for_source(&engine, &you));
    assert!(holds_for_source(&engine, &an_opponent));

    // CR 724.4: a new monarch replaces the previous one.
    engine.become_monarch("player-0");
    assert!(holds_for_source(&engine, &you));
    assert!(!holds_for_source(&engine, &an_opponent));

    // At resolution, the player the triggering event records.
    let event_player =
        json!({ "kind": "playerIsMonarch", "player": { "kind": "triggeringPlayer" } });
    let stack = ability_on_stack(
        &engine,
        BTreeMap::from([("triggeringPlayerId".to_string(), json!("player-1"))]),
    );
    assert!(!holds_at_resolution(&engine, &event_player, &stack));
    engine.become_monarch("player-1");
    assert!(holds_at_resolution(&engine, &event_player, &stack));
}

#[test]
fn the_starting_player_is_a_fact_of_the_game_not_of_the_current_turn() {
    let mut engine = engine_with_source();
    let you = json!({ "kind": "playerWasStartingPlayer", "player": controller() });
    let not_you = json!({ "kind": "not", "operand": you.clone() });
    assert!(
        holds_for_source(&engine, &you),
        "player-0 took the first turn"
    );
    // Another player's turn does not make them the starting player.
    engine.state.active_player = 1;
    assert!(holds_for_source(&engine, &you));
    assert!(!engine.condition_value_with_source(
        &not_you,
        "player-0",
        &BTreeMap::new(),
        Some(&source_card(&engine)),
    ));
    // The quantified member is the reference player.
    let an_opponent = json!({
        "kind": "playerQuantifier",
        "quantifier": "some",
        "players": { "kind": "opponentsOf", "player": controller() },
        "condition": {
            "kind": "playerWasStartingPlayer",
            "player": { "kind": "quantifiedPlayer" },
        },
    });
    assert!(!holds_for_source(&engine, &an_opponent));
}

#[test]
fn another_players_turn_reads_the_active_player_against_the_event_player() {
    let mut engine = engine_with_source();
    let their_turn = json!({ "kind": "isPlayersTurn", "player": { "kind": "triggeringPlayer" } });
    let stack = ability_on_stack(
        &engine,
        BTreeMap::from([("triggeringPlayerId".to_string(), json!("player-1"))]),
    );
    engine.state.active_player = 0;
    assert!(!holds_at_resolution(&engine, &their_turn, &stack));
    engine.state.active_player = 1;
    assert!(holds_at_resolution(&engine, &their_turn, &stack));
    // Without a recorded event player the atom is false, never the
    // controller's turn by default.
    let unrecorded = ability_on_stack(&engine, BTreeMap::new());
    assert!(!holds_at_resolution(&engine, &their_turn, &unrecorded));
}

#[test]
fn an_object_zone_atom_reads_the_current_zone_of_the_source_instance() {
    let mut engine = engine_with_source();
    let on_battlefield = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["battlefield"],
    });
    let command_or_battlefield = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["commandZone", "battlefield"],
    });
    let in_graveyard = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["graveyard"],
    });
    let stack = ability_on_stack(&engine, BTreeMap::new());
    assert!(holds_at_resolution(&engine, &on_battlefield, &stack));
    assert!(holds_at_resolution(
        &engine,
        &command_or_battlefield,
        &stack
    ));
    assert!(!holds_at_resolution(&engine, &in_graveyard, &stack));

    let departed = engine.state.players[0].battlefield.remove(0);
    engine.state.players[0].graveyard.push(departed);
    assert!(!holds_at_resolution(&engine, &on_battlefield, &stack));
    assert!(holds_at_resolution(&engine, &in_graveyard, &stack));
}

#[test]
fn malformed_state_atoms_are_refused_by_the_validator() {
    for condition in [
        json!({ "kind": "playerIsMonarch" }),
        json!({ "kind": "playerIsMonarch", "player": { "kind": "eachPlayer" } }),
        json!({ "kind": "objectInZone", "object": { "kind": "self" }, "zones": [] }),
        json!({ "kind": "objectInZone", "object": { "kind": "self" }, "zones": ["sideboard"] }),
    ] {
        assert!(!condition_supported(&condition), "{condition}");
    }
}

/// Rite of the Serpent (`Destroy target creature. If that creature had a
/// +1/+1 counter on it, ...`), Desculpting Blast (bounce, `if it was
/// attacking`) and Brackish Blunder (bounce, `if it was tapped`): the
/// departed target is read from the state its departure recorded (CR
/// 608.2h), never from the card in its new zone, whose counters and status
/// are gone. Every battlefield writer (graveyard, hand, library) publishes
/// that state.
#[test]
fn a_departed_target_answers_its_last_known_counters_and_combat_state() {
    for destination in ["graveyard", "hand", "library"] {
        let mut engine = test_engine(2);
        let mut creature = test_instance(
            "departed-creature",
            test_definition("departed-creature", "Creature - Snake"),
            "player-1",
        );
        creature.counters.insert("+1/+1".to_string(), 1);
        creature.flags.insert("attacking".to_string(), true);
        creature.tapped = true;
        engine.state.players[1].battlefield.push(creature);
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let stack_object = StackObject {
            id: "stack:departure".to_string(),
            controller: "player-0".to_string(),
            card: test_instance(
                "departure-source",
                test_definition("departure-source", "Sorcery"),
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
                    instance_id: "departed-creature".to_string(),
                },
            )]),
        };
        let departed = |filter: Value| {
            let condition = json!({
                "kind": "departedObjectMatches",
                "object": { "kind": "chosenTarget", "id": "target-0" },
                "where": filter,
            });
            assert!(event_history::departed_object_condition_supported(
                &condition
            ));
            condition
        };
        let had_counter = departed(json!({ "kind": "hasCounter", "counter": "+1/+1" }));
        let was_attacking = departed(json!({ "kind": "isAttacking" }));
        let was_tapped = departed(json!({ "kind": "isTapped" }));
        let was_blocking = departed(json!({ "kind": "isBlocking" }));
        let holds = |engine: &GameEngine, condition: &Value| {
            engine.runtime_condition_value(
                condition,
                &stack_object,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
        };
        match destination {
            "graveyard" => engine
                .move_permanent_to_graveyard("departed-creature", "destroyed")
                .expect("the creature is destroyed"),
            "hand" => engine
                .return_permanent_to_hand("departed-creature")
                .expect("the creature is bounced"),
            _ => engine
                .put_permanent_on_owner_library("departed-creature", "top")
                .expect("the creature is put on top of its owner's library"),
        }
        assert!(
            engine.permanent_position("departed-creature").is_none(),
            "{destination}"
        );
        assert!(holds(&engine, &had_counter), "{destination}");
        assert!(holds(&engine, &was_attacking), "{destination}");
        assert!(holds(&engine, &was_tapped), "{destination}");
        assert!(!holds(&engine, &was_blocking), "{destination}");
        let mut two_counters = had_counter.clone();
        two_counters["where"]["minimum"] = json!(2);
        assert!(!holds(&engine, &two_counters), "{destination}");
        let mut no_counters = had_counter.clone();
        no_counters["where"] = json!({ "kind": "hasCounter", "minimum": 0, "maximum": 0 });
        assert!(!holds(&engine, &no_counters), "{destination}");
        // The record also carries the owner and last-known creature status
        // every departure consumer reads.
        let record = engine
            .state
            .events
            .iter()
            .rev()
            .find(|event| event.kind == "permanentLeftBattlefield")
            .expect("the departure is recorded");
        assert_eq!(record.detail["destination"], destination);
        assert_eq!(record.detail["owner"], "player-1");
        assert_eq!(record.detail["wasCreature"], true);
    }
}

fn permanent_with_cost(id: &str, type_line: &str, mana_cost: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    test_instance(id, definition, "player-0")
}

#[test]
fn the_top_card_atom_reads_the_last_card_of_the_ordered_zone_and_an_empty_zone_has_none() {
    let mut engine = engine_with_source();
    let creature_on_top = json!({
        "kind": "topCardMatches",
        "player": controller(),
        "zone": "library",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    engine.state.players[0].library.clear();
    assert!(!holds_for_source(&engine, &creature_on_top), "no top card");
    engine.state.players[0].library.push(test_instance(
        "library-creature",
        test_definition("library-creature", "Creature - Goblin"),
        "player-0",
    ));
    engine.state.players[0].library.push(test_instance(
        "library-land",
        test_definition("library-land", "Basic Land - Mountain"),
        "player-0",
    ));
    // CR 401.1: the last card of the library is its top card.
    assert!(!holds_for_source(&engine, &creature_on_top));
    engine.state.players[0].library.pop();
    assert!(holds_for_source(&engine, &creature_on_top));
    // The graveyard is another ordered zone of the same atom.
    let graveyard_top = json!({
        "kind": "topCardMatches",
        "player": controller(),
        "zone": "graveyard",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    assert!(!holds_for_source(&engine, &graveyard_top));
    // An unfiltered top-card atom is not a supported shape.
    let unfiltered = json!({
        "kind": "topCardMatches",
        "player": controller(),
        "zone": "library",
        "where": null,
    });
    assert!(!condition_supported(&unfiltered));
}

#[test]
fn a_color_ranks_most_common_with_or_without_a_tie_among_effective_colors() {
    let mut engine = engine_with_source();
    let strict = json!({
        "kind": "colorIsMostCommon",
        "color": "Red",
        "where": null,
        "tieAllowed": false,
    });
    let tied = json!({
        "kind": "colorIsMostCommon",
        "color": "Red",
        "where": null,
        "tieAllowed": true,
    });
    engine.state.players[0]
        .battlefield
        .push(permanent_with_cost("red-one", "Creature - Goblin", "{R}"));
    engine.state.players[1]
        .battlefield
        .push(permanent_with_cost("blue-one", "Creature - Merfolk", "{U}"));
    assert!(!holds_for_source(&engine, &strict), "red ties blue");
    assert!(holds_for_source(&engine, &tied));
    engine.state.players[1]
        .battlefield
        .push(permanent_with_cost(
            "red-two",
            "Creature - Goblin",
            "{1}{R}",
        ));
    assert!(holds_for_source(&engine, &strict));
    engine.state.players[1]
        .battlefield
        .push(permanent_with_cost("blue-two", "Creature - Merfolk", "{U}"));
    engine.state.players[1]
        .battlefield
        .push(permanent_with_cost(
            "blue-three",
            "Creature - Merfolk",
            "{U}{U}",
        ));
    assert!(!holds_for_source(&engine, &tied), "blue is now ahead");
}

#[test]
fn a_cast_choice_reads_the_spell_decisions_and_the_permanent_record() {
    let mut engine = engine_with_source();
    let gift = json!({ "kind": "castChoiceMade", "spell": { "kind": "self" }, "choice": "gift" });
    let teamwork =
        json!({ "kind": "castChoiceMade", "spell": { "kind": "self" }, "choice": "teamwork" });
    let surge = json!({
        "kind": "castChoiceMade",
        "spell": { "kind": "self" },
        "choice": "alternativeCost",
        "mode": "surge",
    });
    for condition in [&gift, &teamwork, &surge] {
        assert!(condition_supported(condition), "{condition}");
    }
    // Both a mode and a mana cost is not one choice.
    let mut ambiguous = surge.clone();
    ambiguous["manaCost"] = json!("{1}{B}");
    assert!(!condition_supported(&ambiguous));

    // The permanent keeps the flags its cast decisions recorded.
    assert!(!holds_for_source(&engine, &gift));
    let definition = engine.state.players[0].battlefield[0].definition.clone();
    let flags = engine.cast_choice_flags(
        &definition,
        &BTreeMap::from([
            ("giftRecipient".to_string(), json!("player-1")),
            ("useAlternativeCost".to_string(), json!(true)),
            ("alternativeCostMode".to_string(), json!("surge")),
        ]),
    );
    for flag in flags {
        engine.state.players[0].battlefield[0]
            .flags
            .insert(flag, true);
    }
    assert!(holds_for_source(&engine, &gift));
    assert!(holds_for_source(&engine, &surge));
    assert!(!holds_for_source(&engine, &teamwork));
    // A gift not promised records no flag.
    assert!(
        engine
            .cast_choice_flags(
                &definition,
                &BTreeMap::from([("giftRecipient".to_string(), Value::Null)]),
            )
            .is_empty()
    );

    // A resolving spell reads its own decisions, not a permanent record.
    let mut spell = ability_on_stack(
        &engine,
        BTreeMap::from([("teamworkPaid".to_string(), json!(true))]),
    );
    spell.ability_kind = None;
    assert!(holds_at_resolution(&engine, &teamwork, &spell));
    assert!(!holds_at_resolution(&engine, &gift, &spell));
}

#[test]
fn a_printed_alternative_mana_cost_compares_by_its_symbols() {
    let engine = engine_with_source();
    let mut definition = test_definition("mastery", "Instant");
    definition.rules.push(json!({
        "kind": "keywordAbility",
        "ability": {
            "kind": "alternativeCost",
            "costs": [{ "kind": "payMana", "manaCost": "{1}{B}" }],
        },
    }));
    let paid = BTreeMap::from([("useAlternativeCost".to_string(), json!(true))]);
    let flags = engine.cast_choice_flags(&definition, &paid);
    let condition = |cost: &str| {
        json!({
            "kind": "castChoiceMade",
            "spell": { "kind": "self" },
            "choice": "alternativeCost",
            "manaCost": cost,
        })
    };
    let flag_of = |cost: &str| {
        cast_events::cast_choice_flag(&condition(cost)).expect("a mana-cost choice flag")
    };
    assert!(flags.contains(&flag_of("{1}{B}")));
    assert!(
        flags.contains(&flag_of("{B}{1}")),
        "the symbols, not their order"
    );
    assert!(!flags.contains(&flag_of("{2}{B}")));
    // Without the alternative cost, the printed cost was paid.
    assert!(
        engine
            .cast_choice_flags(
                &definition,
                &BTreeMap::from([("useAlternativeCost".to_string(), json!(false))]),
            )
            .is_empty()
    );
}

#[test]
fn a_spell_cast_ordinal_is_fixed_by_the_records_up_to_its_own_cast() {
    let mut engine = engine_with_source();
    let cast = |engine: &mut GameEngine, stack_id: &str, type_line: &str, player: &str| {
        engine.record_event(
            "spellCast",
            Some(player.to_string()),
            Some(format!("card:{stack_id}")),
            json!({ "stackId": stack_id, "typeLine": type_line }),
        );
    };
    let second_creature = json!({
        "kind": "spellCastOrdinal",
        "spell": { "kind": "triggeringSpell" },
        "ordinal": 2,
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    assert!(condition_supported(&second_creature));
    let observed =
        |stack_id: &str| BTreeMap::from([("triggeringStackId".to_string(), json!(stack_id))]);
    let source = source_card(&engine);
    cast(&mut engine, "stack:1", "Creature - Elf", "player-0");
    cast(&mut engine, "stack:2", "Sorcery", "player-0");
    cast(&mut engine, "stack:3", "Creature - Elemental", "player-1");
    cast(&mut engine, "stack:4", "Creature - Beast", "player-0");
    let holds = |engine: &GameEngine, stack_id: &str| {
        engine.condition_value_with_source(
            &second_creature,
            "player-0",
            &observed(stack_id),
            Some(&source),
        )
    };
    assert!(!holds(&engine, "stack:1"), "the first creature spell");
    assert!(!holds(&engine, "stack:2"), "not a creature spell");
    assert!(!holds(&engine, "stack:3"), "another caster's first");
    assert!(holds(&engine, "stack:4"));
    // A later creature spell does not move the observed spell's position.
    cast(&mut engine, "stack:5", "Creature - Ooze", "player-0");
    assert!(holds(&engine, "stack:4"));
    assert!(!holds(&engine, "stack:5"));
    // Without the observed record the atom is false.
    assert!(!holds(&engine, "stack:missing"));
}

#[test]
fn a_this_combat_window_counts_the_records_since_the_latest_combat_phase() {
    let mut engine = engine_with_source();
    let source = source_card(&engine);
    let attack = |engine: &mut GameEngine, power: i32| {
        engine.record_event(
            "attackerDeclared",
            Some("player-0".to_string()),
            None,
            json!({ "power": power }),
        );
    };
    let total_power = json!({
        "kind": "countEventsThisTurn",
        "event": "attackerDeclared",
        "player": controller(),
        "where": null,
        "measure": "power",
        "since": "combatPhaseStarted",
    });
    assert!(event_history::event_history_supported(&total_power));
    // No combat phase began this turn: no window, nothing counts.
    attack(&mut engine, 5);
    assert_eq!(
        engine.event_history_count(&total_power, "player-0", Some(&source)),
        Some(0)
    );
    engine.record_event(
        "combatPhaseStarted",
        Some("player-0".to_string()),
        None,
        json!({}),
    );
    attack(&mut engine, 4);
    attack(&mut engine, 3);
    assert_eq!(
        engine.event_history_count(&total_power, "player-0", Some(&source)),
        Some(7)
    );
    // A second combat phase opens a new window.
    engine.record_event(
        "combatPhaseStarted",
        Some("player-0".to_string()),
        None,
        json!({}),
    );
    attack(&mut engine, 2);
    assert_eq!(
        engine.event_history_count(&total_power, "player-0", Some(&source)),
        Some(2)
    );
    // The ordinal of the current combat phase is the turn's count of them.
    let first_combat = json!({
        "kind": "compare",
        "operator": "==",
        "left": {
            "kind": "countEventsThisTurn",
            "event": "combatPhaseStarted",
            "where": null,
        },
        "right": { "kind": "integer", "value": 1 },
    });
    assert!(!holds_for_source(&engine, &first_combat));
    // A window is a this-turn window only.
    let mut previous = total_power.clone();
    previous["kind"] = json!("countEventsPreviousTurn");
    assert!(!event_history::event_history_supported(&previous));
}

#[test]
fn player_quantities_read_party_starting_life_and_multicolor_devotion() {
    let mut engine = engine_with_source();
    let party = json!({ "kind": "partySize", "player": controller() });
    let starting = json!({ "kind": "startingLifeTotal", "player": controller() });
    // The shared devotion quantity (CR 700.5a) over a two-color list.
    let devotion = json!({
        "kind": "devotion",
        "player": controller(),
        "colors": ["Blue", "Black"],
    });
    for expression in [&party, &starting] {
        assert_eq!(
            numeric_quantities::numeric_quantity_supported(expression),
            Some(true)
        );
    }
    assert_eq!(
        engine.player_quantity(&starting, "player-0"),
        Some(Some(20))
    );
    engine.state.players[0].starting_life = 40;
    assert_eq!(
        engine.player_quantity(&starting, "player-0"),
        Some(Some(40))
    );

    engine.state.players[0].battlefield.clear();
    engine.state.players[0].battlefield.extend([
        permanent_with_cost("cleric-rogue", "Creature - Human Cleric Rogue", "{U}{B}"),
        permanent_with_cost("cleric", "Creature - Elf Cleric", "{U/B}"),
        permanent_with_cost("enchantment", "Enchantment", "{B}{R}"),
    ]);
    // CR 700.8: one creature fills one role; the Cleric Rogue is the Rogue.
    assert_eq!(engine.player_quantity(&party, "player-0"), Some(Some(2)));
    engine.state.players[0]
        .battlefield
        .push(permanent_with_cost(
            "warrior-wizard",
            "Creature - Human Warrior Wizard",
            "{2}",
        ));
    assert_eq!(engine.player_quantity(&party, "player-0"), Some(Some(3)));
    // CR 700.5a: a hybrid symbol of both colors counts once.
    assert_eq!(engine.numeric_expression(&devotion, "player-0"), Some(4));
    assert!(numeric_expression_supported(&devotion));
    for colors in [json!(["Blue", "X"]), json!([]), json!(["Blue", "Blue"])] {
        let mut malformed = devotion.clone();
        malformed["colors"] = colors;
        assert!(!numeric_expression_supported(&malformed), "{malformed}");
    }
    let mut legacy = devotion.clone();
    legacy["kind"] = json!("devotionToColors");
    assert!(!numeric_expression_supported(&legacy));
}
