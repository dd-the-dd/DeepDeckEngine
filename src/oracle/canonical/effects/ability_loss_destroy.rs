use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims a sequence from its first independent set-ability-loss effect only.
/// It deliberately does not inspect or predict the later effect; the shared
/// EffectSequence parent owns sentence boundaries and effect continuation.
pub(in crate::oracle::canonical) fn claims_promoted_set_ability_loss_sequence_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("each"))
        && input
            .tokens
            .iter()
            .take_while(|token| token.raw != ".")
            .any(|token| is_verb_form(&token.raw, VerbLemma::Lose, VerbForm::ThirdPersonSingular))
}

pub(in crate::oracle::canonical) fn parse_promoted_set_ability_loss_sequence(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("effect-sequence", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.ability-loss-destroy.shape");
    let [
        LocalEffectNode::LoseAbilities(loss),
        LocalEffectNode::DestroyReferencedSet(destroy),
    ] = sequence.effects.as_slice()
    else {
        return Err(cursor
            .fail("a set ability-loss effect followed by destruction of its bound set reference"));
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one sentence-boundary sequence connector",
    )?;
    cursor.require(
        loss.objects.binding == destroy.objects.antecedent.binding,
        "both effects linked to the same object-set binding",
    )?;
    cursor.enter("ability-loss-destroy.lowering.duration");
    cursor.require(
        loss.duration == EffectDurationNode::UntilEndOfCurrentTurn,
        "an until-end-of-current-turn duration",
    )?;

    let affected_filter = lower_permanent_object_predicate(&loss.objects.predicate);
    let affected = bound_objects("affectedPermanents");
    let current = json!({ "kind": "boundValue", "id": "affectedPermanent" });
    let decisions = object_predicate_uses_x(&loss.objects.predicate)
        .then(|| vec![x_value()])
        .unwrap_or_default();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": [
            {
                "kind": "bind",
                "id": "affectedPermanents",
                "value": {
                    "kind": "eachPermanent",
                    "where": affected_filter,
                },
            },
            {
                "kind": "forEachObject",
                "objects": affected.clone(),
                "bind": "affectedPermanent",
                "effects": [{
                    "kind": "installLoseAllAbilities",
                    "object": current,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            },
            {
                "kind": "destroyPermanent",
                "permanent": affected,
            },
        ],
    });
    if !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }

    Ok(draft(
        rule,
        &[
            "Delegate the first instruction to the independent set ability-loss effect machine",
            "Delegate `those` to the typed object-set reference machine",
            "Compose both effects through the shared EffectSequence sentence boundary",
            "Bind the affected set before applying any primitive effect",
            "Install temporary ability loss before destroying the same bound set",
            &trace,
        ],
    ))
}
