use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalPlayerReference, LocalZone, NumericAtom, NumericExpression, NumericRounding,
};
use super::{OracleTokenCursor, recognize_half_controller_library_count};

#[test]
fn half_controller_library_count_retains_zone_and_rounding() {
    for (direction, rounding) in [("up", NumericRounding::Up), ("down", NumericRounding::Down)] {
        let tokens = recognize_entities(&format!(
            "half the number of cards in your library, rounded {direction}"
        ));
        let mut cursor = OracleTokenCursor::new("library-count-test", &tokens);

        assert_eq!(
            recognize_half_controller_library_count(&mut cursor).expect("expression parses"),
            NumericExpression::Divide {
                left: Box::new(NumericExpression::CardsInZone {
                    player: LocalPlayerReference::Controller,
                    zone: LocalZone::Library,
                }),
                right: Box::new(NumericExpression::Atom(NumericAtom::Integer(2))),
                rounding,
            }
        );
    }
}

#[test]
fn a_different_zone_fails_at_the_zone_token() {
    let tokens = recognize_entities("half the number of cards in your hand, rounded up");
    let mut cursor = OracleTokenCursor::new("library-count-test", &tokens);

    let failure = recognize_half_controller_library_count(&mut cursor)
        .expect_err("the library expression must not silently widen to hand");

    assert_eq!(failure.actual.as_deref(), Some("hand"));
    assert_eq!(failure.expected, "library");
}

#[test]
fn uses_x_recurses_through_composed_numeric_expressions() {
    let expression = NumericExpression::divide_rounded_down(
        NumericExpression::add(
            NumericExpression::Atom(NumericAtom::VariableX),
            NumericExpression::Atom(NumericAtom::Integer(1)),
        ),
        2,
    )
    .expect("positive divisor");

    assert!(expression.uses_x());
    assert!(
        NumericExpression::multiply(
            NumericExpression::Atom(NumericAtom::Integer(2)),
            NumericExpression::subtract(
                NumericExpression::Atom(NumericAtom::Integer(3)),
                NumericExpression::Atom(NumericAtom::VariableX),
            ),
        )
        .uses_x()
    );
    assert!(!NumericExpression::Atom(NumericAtom::Integer(2)).uses_x());
}
