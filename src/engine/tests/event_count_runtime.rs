//! The event count reads its type filter and player population, and refuses
//! a filter it cannot read (`countEventsThisTurn`).
use super::*;

fn died(engine: &mut GameEngine, id: &str, type_line: &str, player: &str) {
    engine.record_event(
        "permanentDied",
        Some(player.to_string()),
        Some(id.to_string()),
        json!({ "typeLine": type_line, "wasCreature": type_line.contains("Creature") }),
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_event_count_reads_its_type_filter_and_its_player_population() {
    let mut engine = test_engine(2);
    died(&mut engine, "bear", "Creature \u{2014} Bear", "player-0");
    died(&mut engine, "relic", "Artifact", "player-1");
    died(
        &mut engine,
        "golem",
        "Artifact Creature \u{2014} Golem",
        "player-1",
    );
    let creatures = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    assert_eq!(engine.count_matching_events(&creatures, None), Some(2));
    // Sibling: a negated type atom and an opponent population.
    let noncreature = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Creature" } },
    });
    assert_eq!(engine.count_matching_events(&noncreature, None), Some(1));
    let opponents = vec!["player-1".to_string()];
    assert_eq!(
        engine.count_matching_events(&creatures, Some(&opponents)),
        Some(1)
    );
    // `Creature` is a word of the type line, not a substring of a subtype.
    let subtype = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "subtypeContains", "value": "Bea" },
    });
    assert_eq!(engine.count_matching_events(&subtype, None), Some(0));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_event_count_refuses_a_filter_it_cannot_read() {
    // Near misses: a non-type atom, a type filter on an event that records
    // no type line, and a previous-turn count with a filter.
    for expression in [
        json!({
            "kind": "countEventsThisTurn",
            "event": "permanentDied",
            "where": { "kind": "isToken" },
        }),
        json!({
            "kind": "countEventsThisTurn",
            "event": "spellCast",
            "where": { "kind": "cardTypeContains", "value": "Instant" },
        }),
    ] {
        assert!(
            !event_count::event_count_supported(&expression),
            "{expression}"
        );
        assert!(!numeric_expression_supported(&expression), "{expression}");
    }
    assert!(!numeric_expression_supported(&json!({
        "kind": "countEventsPreviousTurn",
        "event": "spellCast",
        "aggregate": "total",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    })));
    assert!(event_count::event_count_supported(&json!({
        "kind": "countEventsThisTurn",
        "event": "spellCast",
        "player": { "kind": "opponentsOf", "player": { "kind": "abilityController" } },
        "where": null,
    })));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_event_count_reads_phrases_last_known_status_and_the_owner_relation() {
    let mut engine = test_engine(2);
    died(
        &mut engine,
        "doctor",
        "Creature \u{2014} Time Lord Doctor",
        "player-0",
    );
    died(
        &mut engine,
        "worker",
        "Artifact Creature \u{2014} Assembly-Worker",
        "player-1",
    );
    // An animated land that died: printed type line, last-known creature.
    engine.record_event(
        "permanentDied",
        Some("player-1".to_string()),
        Some("manland".to_string()),
        json!({ "typeLine": "Land", "wasCreature": true, "owner": "player-0" }),
    );
    let count = |where_: Value| json!({ "kind": "countEventsThisTurn", "event": "permanentDied", "where": where_ });
    for (phrase, expected) in [
        ("Time Lord", 1),
        ("Assembly-Worker", 1),
        ("Lord Doctor", 1),
        ("Assembly", 0),
    ] {
        assert_eq!(
            engine.count_matching_events(
                &count(json!({ "kind": "subtypeContains", "value": phrase })),
                None
            ),
            Some(expected),
            "{phrase}"
        );
    }
    assert_eq!(
        engine.count_matching_events(
            &count(json!({ "kind": "cardTypeContains", "value": "Creature" })),
            None
        ),
        Some(3)
    );
    // `relation: owner`: the manland belongs to player 0, its controller
    // was player 1; a player who has since lost is still counted.
    engine.state.players[1].has_lost = true;
    let everyone = engine
        .event_count_players(
            player_query::PlayerQueryContext::source("player-0", None),
            &json!({ "kind": "eachPlayer" }),
        )
        .expect("each player");
    assert_eq!(everyone.len(), 2);
    let owned = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "player": { "kind": "controller" },
        "relation": "owner",
    });
    assert!(event_count::event_count_supported(&owned));
    let mine = vec!["player-0".to_string()];
    assert_eq!(engine.count_matching_events(&owned, Some(&mine)), Some(1));
    let controlled = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "player": { "kind": "controller" },
    });
    assert_eq!(
        engine.count_matching_events(&controlled, Some(&mine)),
        Some(1)
    );
    // Near miss: an owner relation on an event that records no owner.
    assert!(!event_count::event_count_supported(&json!({
        "kind": "countEventsThisTurn",
        "event": "spellCast",
        "player": { "kind": "controller" },
        "relation": "owner",
    })));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn morbid_reads_a_creature_death_this_turn_from_oracle_text() {
    // Hollowhenge Scavenger: `if a creature died this turn` compiles to the
    // event count and gates the life gain at resolution.
    let request = OracleCardParseRequest {
        card_name: "Hollowhenge Scavenger".to_string(),
        type_line: "Creature \u{2014} Elemental".to_string(),
        mana_cost: Some("{3}{G}{G}".to_string()),
        oracle_text: Some(
            "Morbid \u{2014} When this creature enters, if a creature died this turn, you gain 5 life."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let compiled = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            compile_playable_card(PlayableCardInput {
                id: "scavenger".to_string(),
                face_id: None,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                power: Some("4".to_string()),
                toughness: Some("5".to_string()),
                oracle: request,
            })
        })
        .expect("compiler thread starts")
        .join()
        .expect("compiler thread completes")
        .expect("the card compiles");
    assert_eq!(compiled.engine_status, "executable");
    let rule = compiled
        .card
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("triggeredAbility"))
        .expect("the morbid trigger")
        .clone();
    assert!(rule.to_string().contains("\"countEventsThisTurn\""));
    for (a_creature_died, life) in [(false, 20), (true, 25)] {
        let mut engine = test_engine(2);
        let source = test_instance("scavenger", compiled.card.clone(), "player-0");
        engine.state.players[0].battlefield = vec![source.clone()];
        if a_creature_died {
            died(&mut engine, "bear", "Creature \u{2014} Bear", "player-1");
        }
        engine.state.stack.push(StackObject {
            id: "stack:morbid".to_string(),
            controller: "player-0".to_string(),
            card: source,
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("triggeredAbility".to_string()),
            ability_rule: Some(rule.clone()),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        });
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .unwrap_or_else(|error| panic!("morbid resolves: {}", error.message));
        assert_eq!(engine.state.players[0].life, life, "died={a_creature_died}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn players_except_is_a_supported_player_set_but_not_a_single_player() {
    // CR 102.3: every player but the controller, teammates included
    // (lane A's staged support hunk). Near miss: a complement of an
    // unsupported anchor.
    let complement = json!({ "kind": "playersExcept", "player": { "kind": "abilityController" } });
    assert!(player_reference_supported(&complement));
    assert!(!singular_player_reference_supported(&complement));
    assert!(!player_reference_supported(&json!({
        "kind": "playersExcept",
        "player": { "kind": "randomPlayer" },
    })));
}
