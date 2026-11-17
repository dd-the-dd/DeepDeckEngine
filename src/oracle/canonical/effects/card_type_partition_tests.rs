use crate::oracle::syntax::recognize_entities;

use super::claims_card_type_partition_tokens;

#[test]
fn claim_requires_the_per_category_partition_anchor() {
    let promoted = recognize_entities(
        "When this creature enters, reveal the top three cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order.",
    );
    assert!(claims_card_type_partition_tokens(&promoted));

    let ordinary = recognize_entities(
        "When this creature enters, reveal the top three cards of your library. Put a creature card from among them into your hand. Put the rest on the bottom of your library in a random order.",
    );
    assert!(!claims_card_type_partition_tokens(&ordinary));
}
