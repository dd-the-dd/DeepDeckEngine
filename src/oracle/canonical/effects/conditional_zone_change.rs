use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a targeted battlefield zone change whose effect is guarded at
/// resolution by an `if` clause over the chosen object.
pub(in crate::oracle::canonical) fn claims_promoted_conditional_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let action = tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy")
            || token.raw.eq_ignore_ascii_case("exile")
            || token.raw.eq_ignore_ascii_case("return")
    });
    let targeted = tokens.get(1).is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("target") || token.raw.eq_ignore_ascii_case("up")
    });
    let first_terminator = tokens.iter().position(|token| token.raw == ".");
    action
        && targeted
        && first_terminator.is_some_and(|end| {
            tokens[2..end]
                .iter()
                .any(|token| token.raw.eq_ignore_ascii_case("if"))
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-targeted-zone-change", input.tokens);
    let node = recognize_conditional_zone_change(&mut cursor)?;
    let (target_id, candidates, minimum, maximum) = lower_single_target(&node.zone_change.target);
    let target = chosen_target(target_id);
    let operation =
        lower_targeted_zone_change_node(&mut cursor, &node.zone_change, target.clone())?;
    cursor.require(
        node.condition.object
            == (TargetBinding {
                effect_index: 0,
                slot_index: 0,
            }),
        "a condition bound to the zone-change target",
    )?;
    let condition = json!({
        "kind": "objectMatchesFilter",
        "object": target,
        "where": lower_permanent_object_predicate(&node.condition.predicate),
    });
    let mut decisions = Vec::new();
    if matches!(
        node.condition.predicate,
        ObjectPredicate::NumericComparison {
            right: NumericAtom::VariableX,
            ..
        }
    ) {
        decisions.push(x_value());
    }
    decisions.push(target_decision(target_id, candidates, minimum, maximum));
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
            },
            "effects": [{
                "kind": "conditionalEffect",
                "condition": condition,
                "then": [operation],
                "else": [],
            }],
        }),
        &[
            "Recognize a targeted zone change without narrowing target legality",
            "Bind the following numeric property condition to that target",
            "Evaluate the condition only when the primitive effect resolves",
            &trace,
        ],
    ))
}
