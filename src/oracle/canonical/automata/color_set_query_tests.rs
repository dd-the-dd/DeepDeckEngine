use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn color_query_is_a_set_query_and_leaves_the_action_boundary_to_parent() {
    let tokens = recognize_entities(
        "For each color among permanents you control, add one mana of that color.",
    );
    assert_eq!(
        tokens.iter().find(|t| t.raw == "among").unwrap().token_type,
        "relationWord"
    );
    let mut cursor = OracleTokenCursor::new("test.colors-query", &tokens);
    assert_eq!(
        recognize_color_set_query(&mut cursor).unwrap(),
        ColorSetQueryNode::PermanentsControlledByAbilityController
    );
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn color_query_refuses_identity_mana_types_and_unproved_controller_domains() {
    for text in [
        "For each color in your commander's color identity",
        "For each mana type among permanents you control",
        "For each color among permanents your opponents control",
        "For each color among creatures you control",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.colors-query-negative", &tokens);
        assert!(recognize_color_set_query(&mut cursor).is_err(), "{text}");
    }
}
