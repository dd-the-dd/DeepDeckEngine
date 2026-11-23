use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn named_token_reminders_preserve_exact_treasure_and_semantic_food_projection() {
    for (name, quantity, text) in [
        (
            "Treasure",
            NumericAtom::Integer(2),
            "Draw two cards and create two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        ),
        (
            "Food",
            NumericAtom::Integer(1),
            "Create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain three life.\")",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.named-token-reminder", &tokens);
        let opening = tokens.iter().position(|token| token.raw == "(").unwrap();
        cursor.advance_to(opening).unwrap();
        recognize_named_token_reminder(&mut cursor, &quantity, name)
            .expect("the reminder projects the catalog semantics");
        cursor.finish().expect("the complete reminder is consumed");
    }
}

#[test]
fn semantic_near_miss_keeps_the_containing_ability_diagnostic_span() {
    let text = "Create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain 4 life.\") then";
    let tokens = recognize_entities(text);
    let opening = tokens.iter().position(|token| token.raw == "(").unwrap();
    let trailing = tokens.iter().position(|token| token.raw == "then").unwrap();
    let mut cursor = OracleTokenCursor::new("test.named-token-reminder", &tokens);
    cursor.advance_to(opening).unwrap();
    let failure = recognize_named_token_reminder(&mut cursor, &NumericAtom::Integer(1), "Food")
        .expect_err("a different life amount is not the catalog token");
    assert_eq!(failure.state, "reminder.named-token.semantic-match");
    assert_eq!(failure.token_index, trailing);
    assert_eq!(failure.span_start, tokens[trailing].span_start);
    assert_eq!(failure.span_end, tokens[trailing].span_end);
    assert_eq!(failure.actual.as_deref(), Some("then"));
    assert_eq!(
        failure.expected,
        "the catalog token type line and canonical rules"
    );
}
