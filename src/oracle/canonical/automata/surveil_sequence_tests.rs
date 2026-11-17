use crate::oracle::syntax::recognize_entities;

use super::super::{
    EffectConnector, LocalEffectNode, LocalPlayerReference, NumericAtom, NumericExpression,
};
use super::{OracleTokenCursor, recognize_effect_sequence};

#[test]
fn surveil_is_an_independent_effect_in_a_sequence() {
    let tokens = recognize_entities("Surveil 2. Then draw a card.");
    let mut cursor = OracleTokenCursor::new("surveil-sequence-test", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("effect sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    let [
        LocalEffectNode::Surveil(surveil),
        LocalEffectNode::DrawCards(draw),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected surveil followed by draw");
    };
    assert_eq!(surveil.player, LocalPlayerReference::Controller);
    assert_eq!(
        surveil.count,
        NumericExpression::Atom(NumericAtom::Integer(2))
    );
    assert_eq!(draw.count, NumericExpression::Atom(NumericAtom::Integer(1)));
}

#[test]
fn unsupported_surveil_quantity_fails_in_the_numeric_child() {
    let tokens = recognize_entities("Surveil several.");
    let mut cursor = OracleTokenCursor::new("surveil-sequence-test", &tokens);

    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("unsupported quantity must remain visible");

    assert_eq!(failure.actual.as_deref(), Some("several"));
    assert!(failure.expected.contains("integer"));
}
