use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the pregame opening-hand family from typed lexical landmarks. The
/// complete source, anaphora, modality, and destination remain owned by the
/// child automaton, so a claimed near miss produces a local diagnostic instead
/// of falling through to the legacy static parser.
pub(in crate::oracle::canonical) fn claims_promoted_opening_hand_battlefield_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.first().is_some_and(|token| {
        token.token_type == "logicWord" && token.raw.eq_ignore_ascii_case("if")
    }) && source_reference_starts_at(
        1,
        input.tokens.get(1).map(|token| token.raw.as_str()),
        input.name_links,
        input.name_resolution_failure,
    ) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("opening")
            && window[1].token_type == "zoneWord"
            && window[1].raw.eq_ignore_ascii_case("hand")
    }) && input.tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("begin")
            && window[1].raw.eq_ignore_ascii_case("the")
            && window[2].raw.eq_ignore_ascii_case("game")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_opening_hand_battlefield(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("opening-hand-battlefield", input.tokens);
    let node = recognize_opening_hand_battlefield(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("pregame.opening-hand.validation");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the current card as the opening-hand object",
    )?;
    cursor.require(
        node.entering_object == node.source,
        "the opening-hand source as the object entering the battlefield",
    )?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": { "kind": "openingHandBattlefield" },
        }),
        &[
            "Resolve the opening-hand object through the shared source-reference machine",
            "Bind the later pronoun to the same source object",
            "Recognize the optional pregame battlefield move",
            "Lower to the existing openingHandBattlefield primitive",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "opening_hand_battlefield_tests.rs"]
mod tests;
