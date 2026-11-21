use crate::oracle::syntax::recognize_entities;

use super::{LoyaltyAtomicEffectNode, OracleTokenCursor, recognize_loyalty_ability};

#[test]
fn each_loyalty_body_is_a_sequence_of_independent_typed_effects() {
    for (text, expected_len) in [
        (
            "+2: Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -1/-0 until end of turn.",
            1,
        ),
        (
            "−3: Return target instant or sorcery card from your graveyard to your hand. If it's a green card, add one mana of any color.",
            2,
        ),
        (
            "−7: Draw cards equal to half the number of cards in your library, rounded up. You get an emblem with \"You have no maximum hand size.\"",
            2,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("loyalty-ability-test", &tokens);
        let ability =
            recognize_loyalty_ability(&mut cursor, &[], None).expect("loyalty ability parses");
        assert_eq!(ability.effects.len(), expected_len);
        assert_eq!(cursor.position(), tokens.len());
    }

    let tokens = recognize_entities(
        "−3: Return target instant or sorcery card from your graveyard to your hand. If it's a green card, add one mana of any color.",
    );
    let mut cursor = OracleTokenCursor::new("loyalty-conditional-test", &tokens);
    let ability = recognize_loyalty_ability(&mut cursor, &[], None).expect("ability parses");
    assert!(matches!(
        ability.effects.as_slice(),
        [
            LoyaltyAtomicEffectNode::ReturnTargetCard(_),
            LoyaltyAtomicEffectNode::Conditional(_)
        ]
    ));
}
