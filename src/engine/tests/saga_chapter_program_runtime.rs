//! Roadmap #2 (CR 715.2b, 715.2c): a Saga compiled from real Oracle text
//! runs each chapter's shared effect program when its lore threshold is
//! crossed, and only that chapter's program.
use super::*;

const SAGA: &str = "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI \u{2014} You gain 3 life.\nII, III \u{2014} Each opponent loses 2 life.";

fn compiled_saga() -> CardDefinition {
    let compilation = compile_playable_card(PlayableCardInput {
        id: "probe-saga".to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        oracle: OracleCardParseRequest {
            card_name: "Probe Saga".to_string(),
            type_line: "Enchantment \u{2014} Saga".to_string(),
            mana_cost: Some("{2}{W}".to_string()),
            oracle_text: Some(SAGA.to_string()),
            layout: Some("saga".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the Saga compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{:#?}",
        compilation.parser_result.diagnostics
    );
    compilation.card
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn each_crossed_chapter_resolves_its_own_shared_program() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![test_instance("probe-saga", compiled_saga(), "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (life, opponent) = (engine.state.players[0].life, engine.state.players[1].life);

    // Chapter I: only the life gain triggers.
    assert_eq!(engine.put_counters_on_permanent("probe-saga", "lore", 1), 1);
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut RandomAi::seeded(3))
        .expect("chapter I resolves");
    assert_eq!(engine.state.players[0].life, life + 3);
    assert_eq!(engine.state.players[1].life, opponent);

    // Chapter II: the grouped chapter's program, not chapter I again.
    assert_eq!(engine.put_counters_on_permanent("probe-saga", "lore", 1), 1);
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut RandomAi::seeded(4))
        .expect("chapter II resolves");
    assert_eq!(engine.state.players[0].life, life + 3);
    assert_eq!(engine.state.players[1].life, opponent - 2);
}

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Bear");
    definition.power = Some("3".to_string());
    definition.toughness = Some("3".to_string());
    test_instance(id, definition, controller)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn a_targeted_grouped_chapter_declares_its_target_each_time_it_triggers() {
    // CR 715.2c: `I, II` is two chapter abilities; each one declares its
    // own target as it is put on the stack (CR 603.3d).
    let compilation = compile_playable_card(PlayableCardInput {
        id: "targeting-saga".to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        oracle: OracleCardParseRequest {
            card_name: "Targeting Saga".to_string(),
            type_line: "Enchantment \u{2014} Saga".to_string(),
            mana_cost: Some("{1}{B}".to_string()),
            oracle_text: Some("(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI, II \u{2014} Target creature an opponent controls gets -1/-1 until end of turn.\nIII \u{2014} You gain 2 life.".to_string()),
            layout: Some("saga".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the Saga compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{:#?}",
        compilation.parser_result.diagnostics
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("targeting-saga", compilation.card, "player-0"),
        creature("own-bear", "player-0"),
    ];
    engine.state.players[1].battlefield = vec![creature("their-bear", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let toughness = |engine: &GameEngine, id: &str| {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.current_toughness(&engine.state.players[player].battlefield[index])
    };
    for (chapter, expected) in [(1, 2), (2, 1)] {
        assert_eq!(
            engine.put_counters_on_permanent("targeting-saga", "lore", 1),
            1
        );
        // Targets are declared as the trigger is put on the stack (CR
        // 603.3d), before anyone receives priority.
        engine
            .commit_pending_triggered_targets(&mut RandomAi::seeded(chapter + 10))
            .expect("the chapter declares its target");
        let [stacked] = engine.state.stack.as_slice() else {
            panic!(
                "chapter {chapter} alone triggers: {:#?}",
                engine.state.stack
            );
        };
        // The declaration is made as the trigger is put on the stack: the
        // stacked ability already names the only legal target.
        let declared = format!("{:?} {:?}", stacked.targets, stacked.decisions);
        assert!(
            declared.contains("their-bear") && !declared.contains("own-bear"),
            "chapter {chapter} declares the only legal target: {declared}"
        );
        engine
            .resolve_top_stack(&mut RandomAi::seeded(chapter))
            .expect("the chapter resolves");
        assert_eq!(
            toughness(&engine, "their-bear"),
            expected,
            "chapter {chapter}"
        );
        assert_eq!(toughness(&engine, "own-bear"), 3);
    }
}
