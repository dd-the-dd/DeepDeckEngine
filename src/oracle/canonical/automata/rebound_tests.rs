use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{rebound_keyword_starts, recognize_rebound_keyword};

#[test]
fn bare_and_reminded_rebound_are_one_keyword() {
    for text in [
        "Rebound",
        "Rebound (If you cast this spell from your hand, exile it as it resolves. At the beginning of your next upkeep, you may cast this card from exile without paying its mana cost.)",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("rebound-test", &tokens);
        assert!(rebound_keyword_starts(&cursor), "{text}");
        recognize_rebound_keyword(&mut cursor).expect(text);
    }
}

#[test]
fn rebound_followed_by_rules_text_is_not_claimed() {
    let tokens = recognize_entities("Rebound target creature to its owner's hand.");
    let cursor = OracleTokenCursor::new("rebound-test", &tokens);
    assert!(!rebound_keyword_starts(&cursor));
    let tokens = recognize_entities("Rebound (unclosed reminder");
    let mut cursor = OracleTokenCursor::new("rebound-test", &tokens);
    let failure = recognize_rebound_keyword(&mut cursor).expect_err("unbalanced reminder");
    assert_eq!(failure.state, "keyword.rebound.reminder-or-complete");
}
