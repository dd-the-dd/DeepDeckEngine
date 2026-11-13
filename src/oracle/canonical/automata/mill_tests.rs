use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalEffectNode, NumericAtom, NumericExpression, PlayerPredicate, TargetSelector,
    recognize_effect_sequence,
};
use super::OracleTokenCursor;

#[test]
fn targeted_player_mill_is_an_independent_numeric_effect() {
    let tokens = recognize_entities("Target player mills four cards.");
    let mut cursor = OracleTokenCursor::new("test.mill-cards", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("mill effect parses");

    let [LocalEffectNode::MillCards(effect)] = sequence.effects.as_slice() else {
        panic!("expected one mill-cards effect");
    };
    assert_eq!(
        effect.count,
        NumericExpression::Atom(NumericAtom::Integer(4))
    );
    assert!(matches!(
        effect.target.slots.as_slice(),
        [slot] if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
    ));
}

#[test]
fn mill_card_noun_must_agree_with_literal_count() {
    let tokens = recognize_entities("Target player mills one cards.");
    let mut cursor = OracleTokenCursor::new("test.invalid-mill-cards", &tokens);

    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("singular mill count needs singular card noun");

    assert_eq!(failure.actual.as_deref(), Some("cards"));
    assert!(failure.expected.contains("noun agreement"));
}
