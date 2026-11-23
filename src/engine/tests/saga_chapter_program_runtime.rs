//! Roadmap #2 (CR 714.2b, 714.3b): a Saga compiled from real Oracle text
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
