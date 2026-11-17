use crate::oracle::syntax::recognize_entities;

use super::{LocalActivationCostNode, OracleTokenCursor, recognize_activation_cost_sequence};

#[test]
fn mana_symbols_form_one_typed_activation_cost() {
    let tokens = recognize_entities("{2}{U}{B}");
    let mut cursor = OracleTokenCursor::new("test.activation-cost", &tokens);

    let sequence = recognize_activation_cost_sequence(&mut cursor).expect("mana cost parses");
    cursor.finish().expect("cost sequence consumes all symbols");

    let [LocalActivationCostNode::PayMana(mana)] = sequence.costs.as_slice() else {
        panic!("expected one mana-payment cost");
    };
    assert_eq!(mana.as_oracle_cost(), "{2}{U}{B}");
    assert!(sequence.connectors.is_empty());
}

#[test]
fn mana_payment_composes_with_existing_cost_children() {
    let tokens = recognize_entities("{2}, {T}, Pay 1 life");
    let mut cursor = OracleTokenCursor::new("test.activation-cost-sequence", &tokens);

    let sequence = recognize_activation_cost_sequence(&mut cursor).expect("cost sequence parses");
    cursor.finish().expect("cost sequence consumes all costs");

    assert!(matches!(
        sequence.costs.as_slice(),
        [
            LocalActivationCostNode::PayMana(_),
            LocalActivationCostNode::TapSelf(_),
            LocalActivationCostNode::PayLife(_),
        ]
    ));
    assert_eq!(sequence.connectors.len(), 2);
}
