use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_optional_blight_cost_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.get(0..4).is_some_and(|head| {
        head[0].raw.eq_ignore_ascii_case("as")
            && head[1].raw.eq_ignore_ascii_case("an")
            && head[2].raw.eq_ignore_ascii_case("additional")
            && head[3].raw.eq_ignore_ascii_case("cost")
    }) && tokens
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("blight"))
}

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

pub(in crate::oracle::canonical) fn parse_promoted_optional_blight_cost(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("optional-blight-additional-cost", input.tokens);
    let optional = recognize_optional_blight_additional_cost(&mut cursor)?;
    let amount = optional.cost.amount;
    let paid = selection("additionalCostMode", "pay");
    let mut blight_target = target_decision(
        "blightCost1",
        json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Creature"),
            "ignoreTargetingRestrictions": true,
        }),
        1,
        1,
    );
    blight_target["condition"] = paid.clone();
    let trace = transition_trace(&cursor);
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "additionalCostMode",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["decline", "pay"],
                    },
                    blight_target,
                ],
                "additionalCosts": [{
                    "kind": "conditional",
                    "condition": paid,
                    "then": [{
                        "kind": "putCounters",
                        "permanent": chosen_target("blightCost1"),
                        "controller": controller(),
                        "counter": "-1/-1",
                        "count": integer(amount),
                    }],
                    "else": [],
                }],
            },
            "effects": [],
        }),
        &[
            "Recognize the optional additional-cost envelope from one token stream",
            "Lower Blight to a conditional primitive putCounters payment",
            "Validate reminder quantity and controlled-creature domain",
            &trace,
        ],
    ))
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
        LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_) => {
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
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
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
