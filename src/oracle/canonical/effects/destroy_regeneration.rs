use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_destroy_regeneration_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("destroy"))
        && tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("target") || token.raw.eq_ignore_ascii_case("up")
        })
        && (tokens.windows(4).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("it")
                && (window[2].raw.eq_ignore_ascii_case("can't")
                    || window[2].raw.eq_ignore_ascii_case("can’t"))
                && window[3].raw.eq_ignore_ascii_case("be")
        }) || tokens.windows(7).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("a")
                && window[3].raw.eq_ignore_ascii_case("destroyed")
                && window[4].raw.eq_ignore_ascii_case("this")
                && window[5].raw.eq_ignore_ascii_case("way")
                && (window[6].raw.eq_ignore_ascii_case("can't")
                    || window[6].raw.eq_ignore_ascii_case("can’t"))
        }))
}

pub(in crate::oracle::canonical) fn parse_promoted_destroy_regeneration(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("destroy-regeneration-prohibition", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.destroy-regeneration.validation");
    cursor.require(
        sequence.effects.iter().any(|effect| {
            matches!(
                effect,
                LocalEffectNode::ZoneChange(ZoneChangeEffectNode {
                    action: ZoneChangeAction::Destroy,
                    regeneration: ZoneChangeRegenerationPolicy::Prohibited { .. },
                    ..
                })
            )
        }),
        "a targeted destroy with a regeneration prohibition in the sequence",
    )?;
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)?;
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
                "decisions": lowered.decisions,
            },
            "effects": lowered.effects,
        }),
        &[
            "Recognize the targeted destroy through the shared zone-change automaton",
            "Resolve the second-sentence object anaphor to the first target slot",
            "Represent regeneration prohibition as an independent destroy modifier",
            "Delegate lowering to the shared effect-sequence boundary",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "destroy_regeneration_tests.rs"]
mod tests;
