//! The payment instruction (`[PLAYER] pay(s) RESOURCE`, CR 608.2c, 119.4,
//! 107.14) lowered onto the shared resolution cost program: `payCost`, which
//! the payer pays if able. The payment is an ordinary instruction, so every
//! program parent (a permission, a sentence condition, an offer, an outcome
//! branch) measures it with the shared performance witness instead of a
//! dedicated optional-cost block.
//!
//! A chosen amount (`any amount of {E}`, `one or more {E}`) is a
//! `chooseNumber` step bounded by what the payer has, followed by the
//! payment of exactly that number: the chosen number is the quantity a later
//! `that much` or `for each {E} paid this way` restates (the shared
//! action-result quantity binds it, `action_result.rs`).
use super::super::*;

/// The binding the chosen amount of a payment is recorded in: the payment
/// reads it, and a later restated quantity reuses it.
fn chosen_amount_binding(chosen_number: &str) -> String {
    format!("{chosen_number}:paid")
}

/// The resolution cost program paying `amount` of a counted resource.
fn counted_cost(resource: &PaidResource, amount: Value) -> Value {
    match resource {
        PaidResource::Life => json!({ "kind": "payLife", "amount": amount }),
        PaidResource::PlayerCounters(counter) => json!({
            "kind": "payPlayerCounters",
            "counter": counter.as_str(),
            "count": amount,
        }),
    }
}

/// How much of a counted resource the payer has: the most a chosen amount
/// can pay (CR 119.4, 107.14).
fn held_amount(resource: &PaidResource, player: &Value) -> Value {
    match resource {
        PaidResource::Life => json!({ "kind": "lifeTotal", "player": player }),
        PaidResource::PlayerCounters(counter) => json!({
            "kind": "countPlayerCounters",
            "player": player,
            "counter": counter.as_str(),
        }),
    }
}

fn pay_cost(player: Value, cost: Value) -> Value {
    json!({ "kind": "payCost", "player": player, "cost": cost })
}

/// Lowers one payment instruction performed by `player`. `amount` is the
/// lowered expression of an expression amount; `chosen_number` names the
/// decision of a chosen amount (`resolution_object_choice.rs`).
pub(in crate::oracle::canonical) fn lower_payment_instruction(
    payment: &PaymentInstructionNode,
    player: Value,
    amount: Option<Value>,
    chosen_number: &str,
) -> Result<Vec<Value>, &'static str> {
    match payment {
        PaymentInstructionNode::Fixed(resource) => Ok(vec![pay_cost(
            player,
            lower_resolution_cost(&ResolutionCostNode::Payment(resource.clone())),
        )]),
        PaymentInstructionNode::Amount {
            resource,
            amount: PaidAmount::Expression(_),
        } => {
            let amount = amount.ok_or("a lowered amount for the payment instruction")?;
            Ok(vec![pay_cost(player, counted_cost(resource, amount))])
        }
        PaymentInstructionNode::Amount {
            resource,
            amount: PaidAmount::Chosen { minimum },
        } => {
            let binding = chosen_amount_binding(chosen_number);
            Ok(vec![
                json!({
                    "kind": "chooseNumber",
                    "id": chosen_number,
                    "player": player.clone(),
                    "minimum": integer(*minimum),
                    "maximum": held_amount(resource, &player),
                    "bind": binding,
                }),
                pay_cost(
                    player,
                    counted_cost(resource, json!({ "kind": "boundValue", "id": binding })),
                ),
            ])
        }
    }
}

/// `you may pay any amount of R`: the permission's only instruction is the
/// controller's payment of an amount the controller chooses from zero, so
/// the permission adds no decision of its own (CR 608.2d: paying nothing is
/// one of the amounts). The payment then runs unwrapped, and its chosen
/// amount stays a top-level quantity a later `that much` restates.
pub(in crate::oracle::canonical) fn optional_payment_admits_zero(
    body: &EffectSequenceNode,
) -> bool {
    body.conditionals.is_empty()
        && body.unless.is_empty()
        && body.delayed.is_empty()
        && body.where_x.is_empty()
        && matches!(
            body.effects.as_slice(),
            [LocalEffectNode::PlayerAction(action)]
                if action.player == PlayerEffectSubject::Controller
                    && matches!(&action.action, SingularPlayerAction::Pay(payment)
                        if payment.admits_zero())
        )
}
