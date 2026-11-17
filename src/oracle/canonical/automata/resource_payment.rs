//! Shared resource syntax; callers supply the payer and capability context.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ManaCostNode, PayLifeCostNode, PayLifeVerbForm, recognize_mana_cost, recognize_pay_life_cost,
    recognize_pay_life_cost_with_verb,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ResourcePaymentNode {
    Mana(ManaCostNode),
    Life(PayLifeCostNode),
}

impl ResourcePaymentNode {
    pub(in crate::oracle::canonical) fn same_resource(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mana(left), Self::Mana(right)) => left.same_symbols(right),
            (Self::Life(left), Self::Life(right)) => left.amount == right.amount,
            _ => false,
        }
    }
}

pub(in crate::oracle::canonical) fn recognize_resource_payment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResourcePaymentNode> {
    cursor.enter("cost.resource.kind");
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        recognize_mana_cost(cursor).map(ResourcePaymentNode::Mana)
    } else {
        recognize_pay_life_cost(cursor).map(ResourcePaymentNode::Life)
    }
}

/// Third-person verb belongs to the same payment child, not to a reminder's
/// hardcoded numeric parser. Mana and life keep their preserved typed data.
pub(in crate::oracle::canonical) fn recognize_resource_payment_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResourcePaymentNode> {
    cursor.enter("cost.resource.predicate");
    let checkpoint = cursor.checkpoint();
    cursor.consume("pays")?;
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        recognize_mana_cost(cursor).map(ResourcePaymentNode::Mana)
    } else {
        cursor.rollback(checkpoint);
        recognize_pay_life_cost_with_verb(cursor, PayLifeVerbForm::ThirdPerson)
            .map(ResourcePaymentNode::Life)
    }
}
