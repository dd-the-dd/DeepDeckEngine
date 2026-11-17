use crate::oracle::syntax::recognize_entities;

use super::super::{
    EffectConnector, LocalEffectNode, LocalPlayerReference, NumericAtom, NumericExpression,
    ZoneChangeAction,
};
use super::{OracleTokenCursor, recognize_effect_sequence};

#[test]
fn targeted_zone_change_and_life_loss_are_independent_ordered_effects() {
    let tokens = recognize_entities("Exile target nonland permanent. You lose 3 life.");
    let mut cursor = OracleTokenCursor::new("zone-change-life-sequence-test", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("effect sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    let [
        LocalEffectNode::ZoneChange(zone_change),
        LocalEffectNode::LoseLife(life),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected zone change followed by life loss");
    };
    assert_eq!(zone_change.action, ZoneChangeAction::Exile);
    assert_eq!(life.player, LocalPlayerReference::Controller);
    assert_eq!(
        life.amount,
        NumericExpression::Atom(NumericAtom::Integer(3))
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn action_negative_type_and_literal_amount_can_vary_independently() {
    let tokens = recognize_entities("Destroy target noncreature permanent. You lose two life.");
    let mut cursor = OracleTokenCursor::new("zone-change-life-sequence-test", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("generalized sibling parses");

    let [
        LocalEffectNode::ZoneChange(zone_change),
        LocalEffectNode::LoseLife(life),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected zone change followed by life loss");
    };
    assert_eq!(zone_change.action, ZoneChangeAction::Destroy);
    assert_eq!(
        life.amount,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
}

#[test]
fn unsupported_life_quantity_fails_in_the_numeric_child() {
    let tokens = recognize_entities("Exile target nonland permanent. You lose much life.");
    let mut cursor = OracleTokenCursor::new("zone-change-life-sequence-test", &tokens);

    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("unsupported numeric vocabulary must remain visible");

    assert_eq!(failure.actual.as_deref(), Some("much"));
    assert!(failure.expected.contains("integer"));
}
