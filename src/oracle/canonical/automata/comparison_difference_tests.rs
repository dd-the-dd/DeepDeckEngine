//! Reader-level contract of `the difference`: it restates the comparison of
//! the nearest preceding condition, with the number stated after `than`.
use crate::oracle::syntax::recognize_entities;

use super::super::{PlayerCardZone, PlayerStateMeasure};
use super::*;

fn read(text: &str) -> Result<NumericExpression, String> {
    let tokens = recognize_entities(text);
    let start = tokens
        .windows(2)
        .rposition(|pair| {
            pair[0].raw.eq_ignore_ascii_case("the")
                && pair[1].raw.eq_ignore_ascii_case("difference")
        })
        .expect("the difference");
    let mut cursor = OracleTokenCursor::new("comparison-difference-test", &tokens);
    cursor.advance_to(start).expect("a token position");
    assert!(comparison_difference_starts(&cursor));
    recognize_comparison_difference(&mut cursor).map_err(|failure| failure.state.to_string())
}

#[test]
fn fewer_than_n_is_n_minus_the_measure() {
    let expression = read(
        "At the beginning of your upkeep, if you have fewer than seven cards in hand, draw cards equal to the difference.",
    )
    .expect("the difference of a stated hand size");
    let NumericExpression::Subtract { left, right } = expression else {
        panic!("a subtraction");
    };
    assert_eq!(*left, NumericExpression::from(7));
    let NumericExpression::Quantity(quantity) = *right else {
        panic!("a controller measure");
    };
    assert!(matches!(
        *quantity,
        NumericQuantity::ControllerMeasure(PlayerStateMeasure::CardsInZone {
            zone: PlayerCardZone::Hand,
            ..
        })
    ));
}

#[test]
fn a_threshold_without_than_or_without_a_condition_is_refused() {
    assert_eq!(
        read("If you have six or fewer cards in hand, draw cards equal to the difference."),
        Err("numeric.difference.antecedent".to_string())
    );
    assert_eq!(
        read("Draw cards equal to the difference."),
        Err("numeric.difference.antecedent".to_string())
    );
}
