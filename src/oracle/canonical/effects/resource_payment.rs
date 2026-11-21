//! Resource mechanics shared by activation and keyword consumers. Payer roles
//! remain explicit: Ward's targeting player is bound by the trigger runtime.
use super::super::*;

#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) enum ResourcePaymentPayer {
    ActivationController,
    WardTargetingPlayer,
}

pub(in crate::oracle::canonical) fn lower_resource_mana_payment(cost: &ManaCostNode) -> Value {
    json!({"kind":"payMana","manaCost":cost.as_oracle_cost()})
}

pub(in crate::oracle::canonical) fn lower_resource_life_payment(
    cost: &PayLifeCostNode,
    payer: ResourcePaymentPayer,
) -> Value {
    let mut value = json!({"kind":"payLife","amount":integer(cost.amount)});
    if matches!(payer, ResourcePaymentPayer::ActivationController) {
        value["player"] = controller();
    }
    value
}

pub(in crate::oracle::canonical) fn lower_resource_payment(
    cost: &ResourcePaymentNode,
    payer: ResourcePaymentPayer,
) -> Value {
    match cost {
        ResourcePaymentNode::Mana(cost) => lower_resource_mana_payment(cost),
        ResourcePaymentNode::Life(cost) => lower_resource_life_payment(cost, payer),
    }
}

#[cfg(test)]
#[path = "resource_payment_tests.rs"]
mod tests;
