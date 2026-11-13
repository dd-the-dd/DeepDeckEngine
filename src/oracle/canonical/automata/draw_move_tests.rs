use crate::oracle::syntax::recognize_entities;

use super::super::{LocalPlayerReference, NumericAtom, NumericExpression};
use super::{
    EffectRecognitionContext, OracleTokenCursor, recognize_draw_cards_effect_with_context,
};

fn parse_draw(text: &str) -> super::DrawCardsEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("draw-subject-test", &tokens);
    let parsed =
        recognize_draw_cards_effect_with_context(&mut cursor, &EffectRecognitionContext::default())
            .expect("draw effect parses");
    cursor.finish().expect("draw effect consumes input");
    parsed
}

#[test]
fn explicit_controller_subject_preserves_regular_and_equal_expression_draws() {
    let ordinary = parse_draw("you draw a card");
    assert_eq!(ordinary.player, LocalPlayerReference::Controller);
    assert_eq!(
        ordinary.count,
        NumericExpression::Atom(NumericAtom::Integer(1))
    );

    let equal =
        parse_draw("you draw cards equal to half the number of cards in your library, rounded up");
    assert_eq!(equal.player, LocalPlayerReference::Controller);
    assert!(matches!(equal.count, NumericExpression::Divide { .. }));
}
