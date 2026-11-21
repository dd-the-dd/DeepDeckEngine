use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn draw_permission_retains_exact_versus_up_to_quantity() {
    for (text, up_to) in [
        ("you may draw two cards", false),
        ("you may draw up to two cards", true),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("optional-action-test", &tokens);
        let LocalEffectNode::OptionalDrawCards(node) =
            recognize_referenced_draw_effect(&mut cursor, None).unwrap()
        else {
            panic!("explicit may draw has an optional typed amount")
        };
        assert_eq!(node.up_to, up_to);
        assert_eq!(
            node.maximum,
            super::super::NumericExpression::Atom(super::super::NumericAtom::Integer(2))
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn optional_search_possessive_agrees_with_the_parsed_player() {
    let tokens = recognize_entities(
        "you may search your library for a basic land card, put that card onto the battlefield tapped, then shuffle",
    );
    let mut cursor = OracleTokenCursor::new("optional-search-player", &tokens);
    let node = recognize_optional_library_search(&mut cursor, None).unwrap();
    assert_eq!(node.player, super::super::LocalPlayerReference::Controller);
    cursor.finish().unwrap();
    let tokens = recognize_entities(
        "you may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle",
    );
    let mut cursor = OracleTokenCursor::new("optional-search-possession-near-miss", &tokens);
    let failure = recognize_optional_library_search(&mut cursor, None)
        .expect_err("a searching player cannot silently change the owner of the library");
    assert_eq!(failure.state, "effect.library-search.possessive-library");
    assert_eq!(failure.actual.as_deref(), Some("their"));
}
