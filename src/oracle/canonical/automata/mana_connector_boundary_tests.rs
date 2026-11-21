use super::*;
use crate::oracle::syntax::recognize_entities;
#[test]
fn a_sequence_comma_is_owned_by_the_parent_not_the_mana_choice() {
    for connector in ["then", "and"] {
        let text = format!("Add {{G}}, {connector} discard a card.");
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("test.mana-connector-owner", &tokens);
        let AddManaEffectNode::Fixed(node) = recognize_add_mana_effect(&mut cursor).unwrap() else {
            panic!("fixed output");
        };
        assert_eq!(node.mana.as_oracle_cost(), "{G}");
        assert_eq!(cursor.peek(), Some(","));
        assert!(
            cursor
                .peek_ahead(1)
                .unwrap()
                .eq_ignore_ascii_case(connector)
        );
    }
}
#[test]
fn mana_choice_lists_keep_their_own_commas_and_reject_a_missing_closing_or() {
    for text in ["Add {G} or {R}.", "Add {G}, {R}, or {B}."] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.mana-choice-owner", &tokens);
        let AddManaEffectNode::Choice(choice) = recognize_add_mana_effect(&mut cursor).unwrap()
        else {
            panic!("mana choices");
        };
        assert!(choice.options.len() >= 2);
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
    }
    let tokens = recognize_entities("Add {G}, {R}, then discard a card.");
    let mut cursor = OracleTokenCursor::new("test.malformed-mana-choice", &tokens);
    assert!(recognize_add_mana_effect(&mut cursor).is_err());
}
