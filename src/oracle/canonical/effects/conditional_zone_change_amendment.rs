use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims `destroy|exile that ... instead ...` amending the spell's primary
/// target. Any ability-word label was already stripped at the ability
/// boundary (CR 207.2c), so the claim reads only the body.
pub(in crate::oracle::canonical) fn claims_promoted_conditional_zone_change_amendment_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let start = ability_word_body_start(tokens).unwrap_or(0);
    tokens.get(start).is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy") || token.raw.eq_ignore_ascii_case("exile")
    }) && tokens
        .get(start + 1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("that"))
        && tokens[start..]
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
                "condition": lower_condition(&node.amendment_condition),
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
