use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn shared_resource_lowerer_keeps_activation_controller_and_ward_payer_distinct() {
    let tokens = recognize_entities("Pay 3 life");
    let mut cursor = OracleTokenCursor::new("test.resource-lowering", &tokens);
    let payment = recognize_resource_payment(&mut cursor).unwrap();
    assert_eq!(
        lower_resource_payment(&payment, ResourcePaymentPayer::WardTargetingPlayer),
        json!({"kind":"payLife","amount":{"kind":"integer","value":3}})
    );
    assert_eq!(
        lower_resource_payment(&payment, ResourcePaymentPayer::ActivationController),
        json!({"kind":"payLife","player":controller(),"amount":{"kind":"integer","value":3}})
    );
    let ResourcePaymentNode::Life(life) = payment else {
        panic!("life payment")
    };
    assert_eq!(
        super::super::super::lower_local_activation_cost(&LocalActivationCostNode::PayLife(life)),
        lower_resource_life_payment(&life, ResourcePaymentPayer::ActivationController)
    );
}

#[test]
fn mana_resource_lowering_preserves_old_ward_and_activation_ir() {
    let tokens = recognize_entities("{2}{W}");
    let mut cursor = OracleTokenCursor::new("test.mana-resource-lowering", &tokens);
    let payment = recognize_resource_payment(&mut cursor).unwrap();
    let expected = json!({"kind":"payMana","manaCost":"{2}{W}"});
    assert_eq!(
        lower_resource_payment(&payment, ResourcePaymentPayer::WardTargetingPlayer),
        expected
    );
    assert_eq!(
        lower_resource_payment(&payment, ResourcePaymentPayer::ActivationController),
        expected
    );
}
