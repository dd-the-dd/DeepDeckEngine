//! Roadmap #13: aggregate counts compiled from real Oracle shapes run on the
//! engine aggregates they lower to (opponents' permanents, greatest mana
//! value, the Domain count), and a condition's comma returned to its clause
//! gates the clause it belongs to.
use super::*;

fn compiled(name: &str, type_line: &str, oracle: &str) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{1}{G}".to_string()),
            oracle_text: Some(oracle.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{name}: {:#?}",
        compilation.parser_result.diagnostics
    );
    compilation.card
}

fn permanent(id: &str, type_line: &str, mana_cost: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, controller)
}

/// Casts a target-free sorcery from player 0's hand and resolves it; the
/// library holds `library` cards to draw.
fn cast_and_resolve(engine: &mut GameEngine, spell: CardDefinition, library: usize) {
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = ["C", "G"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
    engine.state.players[0].hand = vec![test_instance("spell", spell, "player-0")];
    engine.state.players[0].library = (0..library)
        .map(|index| {
            permanent(
                &format!("card-{index}"),
                "Creature \u{2014} Bear",
                "{1}",
                "player-0",
            )
        })
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("spell")
        })
        .expect("the spell can be cast");
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(5))
        .expect("cast the spell");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(6))
        .expect("resolve the spell");
}

fn drawn(engine: &GameEngine, library: usize) -> usize {
    library - engine.state.players[0].library.len()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn opponents_counts_and_greatest_mana_value_read_the_right_permanents() {
    // `for each creature your opponents control`: two opposing creatures,
    // not the controller's own.
    let spell = compiled(
        "Count Probe",
        "Sorcery",
        "Draw a card for each creature your opponents control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![permanent(
        "own",
        "Creature \u{2014} Bear",
        "{1}",
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![
        permanent("theirs-a", "Creature \u{2014} Bear", "{1}", "player-1"),
        permanent("theirs-b", "Creature \u{2014} Bear", "{1}", "player-1"),
        permanent("their-relic", "Artifact", "{1}", "player-1"),
    ];
    cast_and_resolve(&mut engine, spell, 5);
    assert_eq!(drawn(&engine, 5), 2);

    // `the greatest mana value among artifacts you control`: the controller's
    // largest artifact, never an opponent's or a non-artifact.
    let spell = compiled(
        "Greatest Probe",
        "Sorcery",
        "Draw cards equal to the greatest mana value among artifacts you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("small", "Artifact", "{1}", "player-0"),
        permanent("large", "Artifact", "{3}", "player-0"),
        permanent("beast", "Creature \u{2014} Beast", "{5}", "player-0"),
    ];
    engine.state.players[1].battlefield =
        vec![permanent("their-big", "Artifact", "{6}", "player-1")];
    cast_and_resolve(&mut engine, spell, 8);
    assert_eq!(drawn(&engine, 8), 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn basic_land_types_among_lands_you_control_count_distinct_types() {
    let spell = compiled(
        "Domain Probe",
        "Sorcery",
        "Draw cards equal to the number of basic land types among lands you control.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("forest-a", "Basic Land \u{2014} Forest", "", "player-0"),
        permanent("forest-b", "Basic Land \u{2014} Forest", "", "player-0"),
        permanent("island", "Basic Land \u{2014} Island", "", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![permanent(
        "their-swamp",
        "Basic Land \u{2014} Swamp",
        "",
        "player-1",
    )];
    cast_and_resolve(&mut engine, spell, 5);
    assert_eq!(drawn(&engine, 5), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_condition_comma_returned_to_its_clause_gates_that_clause() {
    // `If you control an artifact, creatures or planeswalkers you control
    // get +1/+1`: the condition counts artifacts; the pump reaches the
    // creatures and planeswalkers.
    let spell = compiled(
        "Clause Probe",
        "Sorcery",
        "If you control an artifact, creatures or planeswalkers you control get +1/+1 until end of turn.",
    );
    for (artifact, expected) in [(true, 3), (false, 2)] {
        let mut engine = test_engine(2);
        let mut battlefield = vec![permanent(
            "bear",
            "Creature \u{2014} Bear",
            "{1}",
            "player-0",
        )];
        if artifact {
            battlefield.push(permanent("relic", "Artifact", "{1}", "player-0"));
        }
        engine.state.players[0].battlefield = battlefield;
        cast_and_resolve(&mut engine, spell.clone(), 0);
        let (player, index) = engine
            .permanent_position("bear")
            .expect("on the battlefield");
        let bear = &engine.state.players[player].battlefield[index];
        assert_eq!(engine.current_power(bear), expected, "artifact: {artifact}");
        assert_eq!(
            engine.current_toughness(bear),
            expected,
            "artifact: {artifact}"
        );
    }
}
