use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_additional_cost_paid_followup_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("destroy"))
        && tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("if")
                && window[1].raw.eq_ignore_ascii_case("this")
                && window[2].raw.eq_ignore_ascii_case("spell's")
                && window[3].raw.eq_ignore_ascii_case("additional")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_additional_cost_paid_followup(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("additional-cost-paid-followup", input.tokens);
    let sequence = recognize_additional_cost_paid_followup(&mut cursor)?;
    let LocalObjectReference::SourceSelf = sequence.condition.spell;
    let (target_id, candidates, minimum, maximum) =
        lower_single_target(&sequence.zone_change.target);
    let zone_change_effect =
        lower_targeted_zone_change_effect(sequence.zone_change.action, chosen_target(target_id));
    let player = match sequence.then_effect.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            return Err(cursor.fail("the spell controller as conditional life recipient"));
        }
    };
    let amount = match sequence.then_effect.amount {
        NumericExpression::Atom(NumericAtom::Integer(value)) => integer(value),
        NumericExpression::Atom(NumericAtom::VariableX)
        | NumericExpression::ObjectProperty { .. }
        | NumericExpression::PowerOf(_)
        | NumericExpression::ManaValueOf(_)
        | NumericExpression::QualifiedTargetProperty { .. }
        | NumericExpression::CountQuery(_)
        | NumericExpression::CountCounters { .. }
        | NumericExpression::CountersOnRecipient { .. }
        | NumericExpression::PermanentCount(_)
        | NumericExpression::SourceCastX
        | NumericExpression::ColorsOfManaSpentToCastSource
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::OpponentCount(_)
        | NumericExpression::EventHistoryCount(_)
        | NumericExpression::Devotion(_)
        | NumericExpression::ActionResultQuantity { .. }
        | NumericExpression::Add { .. }
        | NumericExpression::Subtract { .. }
        | NumericExpression::Multiply { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::BoundObjectProperty { .. }
        | NumericExpression::GreatestPropertyAmong { .. }
        | NumericExpression::CardsInZone { .. }
        | NumericExpression::ActionResultCount { .. }
        | NumericExpression::Divide { .. } => {
            return Err(cursor.fail("a literal conditional life amount"));
        }
    };
    let trace = transition_trace(&cursor);
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(target_id, candidates, minimum, maximum)],
            },
            "effects": [
                zone_change_effect,
                {
                    "kind": "conditionalEffect",
                    "condition": selection("additionalCostMode", "pay"),
                    "then": [{
                        "kind": "gainLife",
                        "player": player,
                        "amount": amount,
                    }],
                    "else": [],
                },
            ],
        }),
        &[
            "Reuse the shared selector for the zone change and its mana-value qualifier",
            "Bind the paid-additional-cost condition to the current spell",
            "Compose destroy/exile and conditional gainLife primitives in order",
            &trace,
        ],
    ))
}

fn transition_trace(cursor: &OracleTokenCursor<'_>) -> String {
    cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ")
}
