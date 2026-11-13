use crate::oracle::syntax::recognize_entities;

use super::{CardColor, OracleTokenCursor, recognize_card_color_set};

#[test]
fn conjunctive_color_set_preserves_order_and_identity() {
    let tokens = recognize_entities("blue and black Shark");
    let mut cursor = OracleTokenCursor::new("test.color-set", &tokens);
    let colors = recognize_card_color_set(&mut cursor).expect("color set parses");
    assert_eq!(colors.colors, [CardColor::Blue, CardColor::Black]);
    assert_eq!(cursor.peek(), Some("Shark"));
}

#[test]
fn repeated_color_is_rejected() {
    let tokens = recognize_entities("blue and blue creature");
    let mut cursor = OracleTokenCursor::new("test.invalid-color-set", &tokens);
    let failure =
        recognize_card_color_set(&mut cursor).expect_err("colors must be semantically distinct");
    assert_eq!(failure.actual.as_deref(), Some("blue"));
    assert!(failure.expected.contains("distinct"));
}
