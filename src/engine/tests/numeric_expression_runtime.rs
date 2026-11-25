//! The numeric-expression family (C7) runs on the engine's quantities.
use super::*;

fn compiled(name: &str, type_line: &str, oracle: &str) -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: name.to_ascii_lowercase().replace(' ', "-"),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{3}{B}{B}".to_string()),
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

fn resolve(engine: &mut GameEngine, card: &CardDefinition) {
    let rule = card.rules[0].clone();
    let spell = StackObject {
        id: "stack:probe".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("probe", card.clone(), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &spell,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the spell resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn devotion_counts_mana_symbols_and_halves_round_as_stated() {
    // CR 700.5: {B}{B}, {1}{B} and the hybrid {W/B} give a devotion to
    // black of 4; a black card without black symbols adds none.
    let drain = compiled(
        "Devotion Probe",
        "Sorcery",
        "Each opponent loses X life, where X is your devotion to black.",
    );
    let mut engine = test_engine(2);
    let permanent = |id: &str, cost: &str| {
        let mut definition = test_definition(id, "Enchantment");
        definition.mana_cost = cost.to_string();
        test_instance(id, definition, "player-0")
    };
    engine.state.players[0].battlefield = vec![
        permanent("double", "{B}{B}"),
        permanent("single", "{1}{B}"),
        permanent("hybrid", "{W/B}"),
        permanent("colorless", "{3}"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let life = engine.state.players[1].life;
    resolve(&mut engine, &drain);
    assert_eq!(engine.state.players[1].life, life - 4);

    // The same operand over two colors (CR 700.5a): {B}{R} gives 2, the
    // hybrid {B/R} counts once, {R} gives 1, {W} none: 4, not 5.
    let two_color = compiled(
        "Two Color Devotion Probe",
        "Sorcery",
        "Each opponent loses X life, where X is your devotion to black and red.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        permanent("gold", "{B}{R}"),
        permanent("hybrid-br", "{B/R}"),
        permanent("red", "{R}"),
        permanent("white", "{W}"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let life = engine.state.players[1].life;
    resolve(&mut engine, &two_color);
    assert_eq!(engine.state.players[1].life, life - 4);

    // CR 107.1a: `half your life total, rounded down`.
    let gain = compiled(
        "Half Probe",
        "Sorcery",
        "You gain life equal to half your life total, rounded down.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].life = 15;
    resolve(&mut engine, &gain);
    assert_eq!(engine.state.players[0].life, 22);
}
