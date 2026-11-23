use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

pub(in crate::oracle::canonical) fn claims_promoted_optional_life_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    token_is(input, 0, "as")
        && token_is(input, 1, "this")
        && token_is(input, 3, "enters")
        && token_is(input, 4, ",")
        && token_is(input, 5, "you")
        && token_is(input, 6, "may")
        && token_is(input, 7, "pay")
        && input
            .tokens
            .get(8)
            .is_some_and(|token| token.token_type == "quantity")
        && input.tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("enters")
                && window[1].raw.eq_ignore_ascii_case("tapped")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_optional_life_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("optional-life-entry", input.tokens);
    let node = recognize_optional_life_entry(&mut cursor)?;
    cursor.enter("optional-life-entry.binding-validation");
    cursor.require(
        matches!(
            &node.unpaid_entry.condition,
            BooleanConditionNode::Atom(ConditionAtom::ActionOutcome(
                ActionOutcomeNode::CostNotPaid { payment }
            )) if *payment == node.payment.binding
        ),
        "the unpaid condition linked to the optional life payment",
    )?;
    let payment_id = lower_choice_binding(node.payment.binding);
    let unpaid_condition = lower_condition(&node.unpaid_entry.condition);
    let _object_kind = node.object_kind;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "decisions": [{
                "id": payment_id,
                "kind": "chooseWhetherToPay",
                "player": controller(),
                "cost": {
                    "kind": "payLife",
                    "amount": integer(node.payment.amount),
                },
            }],
            "replacement": [{
                "kind": "conditional",
                "condition": unpaid_condition,
                "then": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": node.unpaid_entry.effect.tapped,
                }],
            }],
        }),
        &[
            "Recognize the entering object from the shared token stream",
            "Bind an optional life payment",
            "Link the unpaid condition to that payment binding",
            "Apply one atomic tapped entering-state consequence",
            &trace,
        ],
    ))
}
