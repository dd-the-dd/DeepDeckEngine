use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalEffectNode, NumericAtom, NumericExpression, PlayerEffectSubject, PlayerPredicate,
    SingularPlayerAction, TargetSelector, recognize_effect_sequence,
};
use super::OracleTokenCursor;

#[test]
fn targeted_player_mill_is_an_independent_numeric_effect() {
    let tokens = recognize_entities("Target player mills four cards.");
    let mut cursor = OracleTokenCursor::new("test.mill-cards", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("mill effect parses");

    // Mill is a shared player verb: the declared player subject and the count
    // are independent children of the ordinary player action.
    let [LocalEffectNode::PlayerAction(effect)] = sequence.effects.as_slice() else {
        panic!("expected one shared player action");
    };
    assert_eq!(
        effect.action,
        SingularPlayerAction::Mill(NumericExpression::Atom(NumericAtom::Integer(4)))
    );
    let PlayerEffectSubject::DeclaredTarget { target, .. } = &effect.player else {
        panic!("mill subject declares its player target");
    };
    assert!(matches!(
        target.slots.as_slice(),
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
