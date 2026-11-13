use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims only the promoted spell-effect head. Selector, zones, possessive
/// owner relation, punctuation, and full consumption remain owned by the
/// shared effect-sequence and targeted-card-move children.
pub(in crate::oracle::canonical) fn claims_promoted_target_card_move_spell_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let targeted_card_from = tokens
        .get(1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.windows(2).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("card")
                || window[0].raw.eq_ignore_ascii_case("cards"))
                && window[1].raw.eq_ignore_ascii_case("from")
        });
    if !targeted_card_from {
        return false;
    }

    match tokens.first().map(|token| token.raw.as_str()) {
        Some(raw) if raw.eq_ignore_ascii_case("return") => {
            contains_token_phrase(tokens, &["from", "your", "graveyard"])
        }
        Some(raw) if raw.eq_ignore_ascii_case("put") => {
            contains_token_phrase(tokens, &["from", "a", "graveyard"])
                && (contains_token_phrase(tokens, &["on", "top"])
                    || contains_token_phrase(tokens, &["on", "the", "top"]))
        }
        _ => false,
    }
}

fn contains_token_phrase(
    tokens: &[crate::oracle::model::RecognizedEntity],
    phrase: &[&str],
) -> bool {
    tokens.windows(phrase.len()).any(|window| {
        window
            .iter()
            .zip(phrase)
            .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_target_card_move_spell(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("target-card-move-spell", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("target-card-move-spell.validation");
    cursor.require(
        sequence.effects.first().is_some_and(|effect| {
            matches!(
                effect,
                LocalEffectNode::TargetCardZoneChange(TargetCardZoneChangeEffectNode {
                    action: CardZoneChangeAction::Move(_),
                    ..
                })
            )
        }),
        "a targeted card move to a supported destination",
    )?;
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)?;
    cursor.require(
        !lowered.decisions.is_empty() && !lowered.effects.is_empty(),
        "a declared target and executable card movement",
    )?;
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
            "Recognize one targeted nonbattlefield card movement",
            "Delegate the card predicate and zone relations to shared child automata",
            "Resolve the chosen card's owner independently from the ability controller",
            "Lower to chooseTargets followed by moveCards",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "target_card_move_tests.rs"]
mod tests;
