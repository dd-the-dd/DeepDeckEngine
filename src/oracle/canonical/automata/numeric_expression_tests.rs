use crate::oracle::syntax::recognize_entities;

use super::super::{
    CardZoneName, LocalPlayerReference, NumericAtom, NumericExpression, NumericRounding,
    ObjectKind, ZoneHolderQueryNode, recognize_numeric_expression,
    recognize_query_numeric_expression, with_prior_number,
};
use super::OracleTokenCursor;

/// `the number of cards in ZONE` is the shared `the number of QUERY`
/// aggregate over the zone-holder domain, not a fixed library phrase.
fn counted_zone(expression: &NumericExpression) -> Option<(CardZoneName, ZoneHolderQueryNode)> {
    let NumericExpression::Divide { left, .. } = expression else {
        return None;
    };
    let NumericExpression::CountQuery(query) = left.as_ref() else {
        return None;
    };
    if query.domain != ObjectKind::Card || query.zones.len() != 1 {
        return None;
    }
    Some((query.zones[0], query.holders?))
}

#[test]
fn half_controller_library_count_retains_zone_and_rounding() {
    for (direction, rounding) in [("up", NumericRounding::Up), ("down", NumericRounding::Down)] {
        let tokens = recognize_entities(&format!(
            "half the number of cards in your library, rounded {direction}"
        ));
        let mut cursor = OracleTokenCursor::new("library-count-test", &tokens);

        // `half` is the shared arithmetic operator; the library count is the
        // shared aggregate over cards in the controller's library.
        let read = recognize_query_numeric_expression(&mut cursor).expect("expression parses");
        assert_eq!(
            counted_zone(&read),
            Some((
                CardZoneName::Library,
                ZoneHolderQueryNode::Player(LocalPlayerReference::Controller)
            )),
            "{read:?}"
        );
        assert!(matches!(
            read,
            NumericExpression::Divide { ref right, rounding: actual, .. }
                if **right == NumericExpression::Atom(NumericAtom::Integer(2))
                    && actual == rounding
        ));
    }
}

#[test]
fn a_different_zone_is_not_read_as_the_library_count() {
    // Near miss: the hand count is the same aggregate over another zone; it
    // never silently becomes the library count.
    let tokens = recognize_entities("half the number of cards in your hand, rounded up");
    let mut cursor = OracleTokenCursor::new("library-count-test", &tokens);
    let read = recognize_query_numeric_expression(&mut cursor);
    assert!(
        read.as_ref()
            .ok()
            .and_then(counted_zone)
            .is_none_or(|(zone, _)| zone == CardZoneName::Hand),
        "{read:?}"
    );
}

#[test]
fn that_number_reads_the_prior_number_of_the_binding_environment() {
    let tokens = recognize_entities("that number plus 1");
    let mut cursor = OracleTokenCursor::new("prior-number-test", &tokens);
    let prior = NumericExpression::Atom(NumericAtom::VariableX);
    let read = with_prior_number(&mut cursor, prior.clone(), |cursor| {
        recognize_numeric_expression(cursor, None)
    })
    .expect("a bound prior number");
    assert_eq!(
        read,
        NumericExpression::add(prior, NumericExpression::Atom(NumericAtom::Integer(1)))
    );
    // Unused combination: the prior number under a multiplicative operator.
    let tokens = recognize_entities("twice that number");
    let mut cursor = OracleTokenCursor::new("prior-number-test", &tokens);
    assert!(
        with_prior_number(
            &mut cursor,
            NumericExpression::Atom(NumericAtom::Integer(3)),
            |cursor| recognize_numeric_expression(cursor, None),
        )
        .is_ok()
    );
    // Without a binding `that number` fails; it never becomes a literal.
    let tokens = recognize_entities("that number plus 1");
    let mut cursor = OracleTokenCursor::new("prior-number-test", &tokens);
    let failure = recognize_numeric_expression(&mut cursor, None).unwrap_err();
    assert_eq!(failure.state, "numeric.prior-number-anaphor.resolve");
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
#[test]
fn numeric_context_resolves_articles_without_changing_their_lexical_role() {
    for word in ["a", "an", "A", "AN"] {
        let tokens = crate::oracle::syntax::recognize_entities(word);
        assert_eq!(tokens[0].token_type, "quantifierWord");
        let mut cursor = super::OracleTokenCursor::new("numeric-article", &tokens);
        assert_eq!(
            super::recognize_numeric_atom(&mut cursor).unwrap(),
            super::NumericAtom::Integer(1)
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn nonnumeric_quantifiers_are_not_substituted_with_one() {
    for word in ["any", "each", "another", "all"] {
        let tokens = crate::oracle::syntax::recognize_entities(word);
        let mut cursor = super::OracleTokenCursor::new("numeric-article-near-miss", &tokens);
        assert!(
            super::recognize_numeric_atom(&mut cursor).is_err(),
            "{word}"
        );
    }
}
