use crate::oracle::syntax::recognize_entities;

use super::super::{AddManaEffectNode, EffectConnector, LocalEffectNode};
use super::{OracleTokenCursor, recognize_effect_sequence};

#[test]
fn fixed_mana_production_is_an_independent_sequence_effect() {
    let tokens = recognize_entities("Add {B}{B}{B}.");
    let mut cursor = OracleTokenCursor::new("test.add-mana-effect", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("mana effect sequence parses");

    assert!(sequence.connectors.is_empty());
    let [LocalEffectNode::AddMana(AddManaEffectNode::Fixed(effect))] = sequence.effects.as_slice()
    else {
        panic!("expected one fixed mana-production effect");
    };
    assert_eq!(effect.mana.as_oracle_cost(), "{B}{B}{B}");
}

#[test]
fn mana_production_composes_with_a_followup_effect() {
    let tokens = recognize_entities("Add {G}. Then you gain 1 life.");
    let mut cursor = OracleTokenCursor::new("test.add-mana-sequence", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("composed sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    assert!(matches!(
        sequence.effects.as_slice(),
        [LocalEffectNode::AddMana(_), LocalEffectNode::GainLife(_)]
    ));
}

#[test]
fn generic_cost_symbols_are_not_accepted_as_mana_production() {
    let tokens = recognize_entities("Add {1}.");
    let mut cursor = OracleTokenCursor::new("test.invalid-add-mana-effect", &tokens);

    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("generic cost symbols are not fixed mana output");

    assert_eq!(failure.actual.as_deref(), Some("{1}"));
    assert!(failure.expected.contains("fixed producible mana symbol"));
}
