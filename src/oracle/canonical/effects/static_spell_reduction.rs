use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticSpellReductionParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

fn lower_node(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    node: StaticSpellReductionNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.spell-cost-reduction.source-zone");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for a battlefield-active static ability",
    )?;

    let where_filter = lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null);
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
                "kind": "reduceCastingCost",
                "player": controller(),
                "where": where_filter,
                "amount": integer(node.amount),
            }],
        }),
        &[
            "Recognize the controller-scoped spell collection through the shared selector",
            "Compose an optional relative characteristic predicate without duplicating vocabulary",
            "Delegate the fixed mana token to the shared mana-cost automaton",
            "Lower one atomic generic casting-cost reduction modifier",
            &trace,
        ],
    ))
}

/// Parse-once promoted boundary. Failures before the spell collection and
/// `cost`/`costs` boundary are not this family; once that boundary is consumed,
/// malformed amount, scope, direction, or punctuation remains a native error.
pub(in crate::oracle::canonical) fn try_parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> StaticSpellReductionParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticSpellReductionParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    let node = match recognize_static_spell_reduction(&mut cursor) {
        Ok(node) => node,
        Err(failure) => {
            let committed = failure
                .trace
                .iter()
                .any(|transition| transition.state == "static.spell-cost-reduction.caster-scope");
            return if committed {
                StaticSpellReductionParseAttempt::Failed(failure)
            } else {
                StaticSpellReductionParseAttempt::NotClaimed
            };
        }
    };
    match lower_node(input, &mut cursor, node) {
        Ok(draft) => StaticSpellReductionParseAttempt::Parsed(draft),
        Err(failure) => StaticSpellReductionParseAttempt::Failed(failure),
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    let node = recognize_static_spell_reduction(&mut cursor)?;
    lower_node(input, &mut cursor, node)
}

#[cfg(test)]
#[path = "static_spell_reduction_tests.rs"]
mod tests;
