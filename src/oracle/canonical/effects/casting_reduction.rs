use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

/// Compatibility name; the reader/lowerer themselves accept one semantic node.
pub(in crate::oracle::canonical) fn claims_promoted_permanent_scaled_casting_reduction_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|t| t.raw.eq_ignore_ascii_case("this"))
        && input
            .tokens
            .get(1)
            .is_some_and(|t| t.raw.eq_ignore_ascii_case("spell"))
        && input
            .tokens
            .get(2)
            .is_some_and(|t| t.raw.eq_ignore_ascii_case("costs"))
        && input.tokens.windows(3).any(|window| {
            ["less", "to", "cast"]
                .iter()
                .zip(window)
                .all(|(word, token)| token.raw.eq_ignore_ascii_case(word))
        })
}

pub(in crate::oracle::canonical) fn lower_casting_cost_reduction(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingCostReductionNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("casting-reduction.amount-capability");
    let amount = lower_numeric_expression_with_query_support(
        &node.amount,
        NumericQueryLoweringSupport::CastingCost,
    )
    .ok_or_else(|| {
        cursor.fail("a symbolic reduction amount with a proved query/player/zone context")
    })?;
    let rule = match node.subject {
        CastingReductionSubject::CurrentSpell => {
            let mut modifier = json!({"kind":"reduceOwnGenericCastingCost","amount":amount});
            if let Some(condition) = &node.condition {
                cursor.enter("casting-reduction.condition-lowering");
                modifier["condition"] = lower_sentence_condition(
                    cursor,
                    condition,
                    &ConditionObjectBindings {
                        permanent_targets: &[],
                        card_targets: &[],
                        card_focus: None,
                        previous_action_result: None,
                    },
                )?;
            }
            if let Some(query) = &node.target_query {
                let filter = lower_permanent_object_predicate(&query.predicate);
                cursor.require(
                    !filter.is_null(),
                    "a lowerable permanent predicate for the targeted object",
                )?;
                modifier["targetWhere"] = filter;
            }
            json!({
                "kind":"staticAbility","source":self_ref(),
                "activeWhile":{"kind":"inZone","object":self_ref(),"zone":{"kind":"stackOrCast"}},
                "modifiers":[modifier],
            })
        }
        CastingReductionSubject::SpellSet(spells) => {
            cursor.enter("static.spell-cost-reduction.source-zone");
            cursor.require(
                node.condition.is_none(),
                "an unconditional reduction for a spell-set subject",
            )?;
            cursor.require(
                type_line_has_permanent_card_type(input.face_type_line),
                "a permanent card face for a battlefield-active static ability",
            )?;
            cursor.require(
                spells.caster == Some(SpellCasterScope::AbilityController),
                "a proved caster relation for the reduction subject",
            )?;
            json!({"kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),
                "modifiers":[{"kind":"reduceCastingCost","player":controller(),
                    "where":lower_spell_object_predicate(&spells.predicate).unwrap_or(Value::Null),"amount":amount}]})
        }
    };
    Ok(draft(
        rule,
        &[
            "Read the reduction subject with the shared spell/object query",
            "Retain a symbolic numeric amount and optional ordinary Count(Query)",
            "Keep zone holders independent of object ownership and controller",
            "Project existing reduction and battlefield count primitives without changing their meaning",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_permanent_scaled_casting_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("permanent-scaled-casting-reduction", input.tokens);
    let node = recognize_permanent_scaled_casting_reduction(&mut cursor)?;
    lower_casting_cost_reduction(input, &mut cursor, node)
}

#[cfg(test)]
#[path = "casting_reduction_tests.rs"]
mod tests;
