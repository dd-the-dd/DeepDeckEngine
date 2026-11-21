use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_conditional_zone_change_amendment_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.token_type == "abilityWord")
        && input.tokens.get(1).is_some_and(|token| token.raw == "—")
        && input
            .tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("instead"))
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_zone_change_amendment(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-zone-change-amendment", input.tokens);
    let node = recognize_conditional_zone_change_amendment(&mut cursor)?;
    cursor.require(
        node.target_condition.object
            == (TargetBinding {
                effect_index: 0,
                slot_index: 0,
            }),
        "the inherited primary spell target",
    )?;
    let target = chosen_target("targetPermanent");
    let target_condition = json!({
        "kind": "objectMatchesFilter",
        "object": target.clone(),
        "where": lower_permanent_object_predicate(&node.target_condition.predicate),
    });
    let operation = lower_targeted_zone_change_effect(node.action, target);
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
            "effects": [{
                "kind": "conditionalEffect",
                "condition": lower_local_condition(&node.amendment_condition),
                "then": [{
                    "kind": "conditionalEffect",
                    "condition": target_condition,
                    "then": [operation],
                    "else": [],
                }],
                "else": [],
            }],
        }),
        &[
            "Recognize an ability-word-labeled amendment without dispatching on its label",
            "Resolve `that permanent` to the primary spell target binding",
            "Evaluate event history before the amended target threshold",
            &trace,
        ],
    ))
}
