use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

pub(in crate::oracle::canonical) fn claims_promoted_chosen_land_type_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 24
        && token_is(input, 0, "as")
        && token_is(input, 1, "this")
        && token_is(input, 3, "enters")
        && token_is(input, 4, ",")
        && token_is(input, 5, "choose")
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("basic")
                && window[1].raw.eq_ignore_ascii_case("land")
                && window[2].raw.eq_ignore_ascii_case("type")
        })
        && tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("then")
                && window[1].raw.eq_ignore_ascii_case("you")
                && window[2].raw.eq_ignore_ascii_case("may")
                && window[3].raw.eq_ignore_ascii_case("pay")
        })
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("enters")
                && window[1].raw.eq_ignore_ascii_case("tapped")
        })
}

pub(in crate::oracle::canonical) fn claims_promoted_retain_chosen_land_type_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.len() == 7
        && token_is(input, 0, "this")
        && token_is(input, 2, "is")
        && token_is(input, 3, "the")
        && token_is(input, 5, "type")
        && token_is(input, 6, ".")
}

fn lower_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) => {
            unreachable!("entry choices currently bind the entering object's controller")
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_chosen_land_type_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("chosen-land-type-entry", input.tokens);
    let node = recognize_chosen_land_type_entry(&mut cursor)?;

    cursor.enter("chosen-land-type-entry.binding-validation");
    cursor.require(
        node.choice.binding == ChoiceBinding::BasicLandType,
        "a basic-land-type choice binding",
    )?;
    cursor.require(
        matches!(
            &node.unpaid_entry.condition,
            LocalConditionNode::CostNotPaid { payment }
                if *payment == node.payment.binding
        ),
        "the unpaid condition linked to the optional life payment",
    )?;
    cursor.require(
        node.unpaid_entry.effect.object == LocalObjectReference::SourceSelf
            && node.unpaid_entry.effect.tapped,
        "a tapped entering state applied to the source object",
    )?;

    let _object_kind = node.object_kind;
    let land_type_id = lower_choice_binding(node.choice.binding);
    let payment_id = lower_choice_binding(node.payment.binding);
    let unpaid_condition = lower_local_condition(&node.unpaid_entry.condition);
    let choice_player = lower_player(&node.choice.player);
    let payment_player = lower_player(&node.payment.player);
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
            "decisions": [
                {
                    "id": land_type_id,
                    "kind": "chooseBasicLandType",
                    "player": choice_player,
                    "options": ["Plains", "Island", "Swamp", "Mountain", "Forest"],
                },
                {
                    "id": payment_id,
                    "kind": "chooseWhetherToPay",
                    "player": payment_player,
                    "cost": {
                        "kind": "payLife",
                        "amount": integer(node.payment.amount),
                    },
                },
            ],
            "replacement": [
                {
                    "kind": "setBasicLandType",
                    "object": self_ref(),
                    "decisionId": land_type_id,
                },
                {
                    "kind": "conditional",
                    "condition": unpaid_condition,
                    "then": [{
                        "kind": "setEnteringState",
                        "object": self_ref(),
                        "tapped": node.unpaid_entry.effect.tapped,
                    }],
                },
            ],
        }),
        &[
            "Recognize the entering object from the shared token stream",
            "Bind a basic-land-type choice",
            "Bind an optional life payment and its unpaid condition",
            "Lower the choices and entering-state consequence as independent primitives",
            &trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_retain_chosen_land_type(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("retain-chosen-land-type", input.tokens);
    let node = recognize_retain_chosen_land_type(&mut cursor)?;
    cursor.enter("retain-chosen-land-type.binding-validation");
    cursor.require(
        node.choice == ChoiceBinding::BasicLandType,
        "the basic-land-type choice binding",
    )?;
    let _object_kind = node.object_kind;
    let decision_id = lower_choice_binding(node.choice);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "retainChosenBasicLandType",
                "decisionId": decision_id,
            }],
        }),
        &[
            "Recognize a source-object reference and chosen-property anaphora",
            "Resolve the typed basic-land-type choice binding",
            "Retain the chosen land characteristics while the source is on the battlefield",
            &trace,
        ],
    ))
}
