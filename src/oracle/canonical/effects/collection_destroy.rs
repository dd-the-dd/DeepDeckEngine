use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn lower_destroy_permanent_collection_effect(
    effect: &DestroyPermanentCollectionEffectNode,
) -> Value {
    let mut permanent = json!({
        "kind": "eachPermanent",
        "where": lower_permanent_object_predicate(&effect.objects.predicate),
    });
    if let Some(controller_scope) = effect.objects.controller {
        permanent["player"] = lower_object_controller_scope(controller_scope);
    }
    json!({
        "kind": "destroyPermanent",
        "permanent": permanent,
    })
}

/// This is deliberately only a token-level family hint. The shared
/// EffectSequence performs the committed parse exactly once.
pub(in crate::oracle::canonical) fn claims_promoted_collection_destroy_family(
    input: &AbilityInput<'_>,
) -> bool {
    matches!(input.tokens, [first, second, ..]
        if first.raw.eq_ignore_ascii_case("destroy")
            && (second.raw.eq_ignore_ascii_case("all")
                || second.raw.eq_ignore_ascii_case("each")))
}

pub(in crate::oracle::canonical) fn parse_promoted_collection_destroy(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("collection-destroy-effect-sequence", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.collection-destroy.first-action");
    cursor.require(
        matches!(
            sequence.effects.first(),
            Some(LocalEffectNode::DestroyCollection(_))
        ),
        "a permanent-collection destruction as the first independent action",
    )?;
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)?;
    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize a non-targeting permanent collection through the shared selector automata",
            "Keep collection destruction as one ordinary EffectSequence action",
            "Preserve declarations and target bindings for independent sibling actions",
            "Lower to destroyPermanent(eachPermanent) without a combined destroy-all operation",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "collection_destroy_tests.rs"]
mod tests;
