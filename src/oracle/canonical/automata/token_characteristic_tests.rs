use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{
    NumericAtom, ObjectCardType, ObjectPredicate, recognize_characteristic_token_activation,
};

#[test]
fn recognizes_generic_counted_characteristic_token_activation() {
    let tokens = recognize_entities(
        "{3}, {T}: Create a 1/1 green Golem artifact creature token with 'This token gets +2/+2 for each enchantment you control.'",
    );
    let mut cursor = OracleTokenCursor::new("characteristic-token-test", &tokens);
    let ability = recognize_characteristic_token_activation(&mut cursor).expect("ability parses");

    assert_eq!(ability.effect.quantity, NumericAtom::Integer(1));
    assert_eq!(
        ability.effect.token.types,
        vec![ObjectCardType::Artifact, ObjectCardType::Creature]
    );
    assert_eq!(ability.effect.token.subtypes, vec!["Golem"]);
    assert_eq!(
        ability.effect.token.ability.counted,
        ObjectPredicate::CardType(ObjectCardType::Enchantment)
    );
    assert_eq!(ability.effect.token.ability.delta.power, 2);
}

#[test]
fn rejects_unclosed_local_quote_at_its_boundary() {
    let tokens = recognize_entities(
        "{2}, {T}: Create a 0/0 colorless Construct artifact creature token with 'This token gets +1/+1 for each artifact you control.",
    );
    let mut cursor = OracleTokenCursor::new("characteristic-token-test", &tokens);
    let failure = recognize_characteristic_token_activation(&mut cursor).expect_err("must fail");
    assert_eq!(failure.state, "effect.characteristic-token.close-quote");
}
