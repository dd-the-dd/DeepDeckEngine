use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the structural event envelope only. Effect details remain committed
/// child-machine decisions once the generic library-search event is selected.
pub(in crate::oracle::canonical) fn claims_promoted_library_search_trigger_family(
    input: &AbilityInput<'_>,
) -> bool {
    let event_end = input
        .tokens
        .iter()
        .position(|token| token.raw == ",")
        .unwrap_or(input.tokens.len());
    let event = &input.tokens[..event_end];
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("opponent"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("searches"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("library"))
}

pub(in crate::oracle::canonical) fn parse_promoted_library_search_trigger(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("opponent-library-search-trigger", input.tokens);
    let ability = recognize_library_search_triggered_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.library-search.validation");
    cursor.require(
        ability.event.player == PlayerPredicate::Opponent,
        "an opponent library-search event",
    )?;
    // The body is any effect program: the shared effect-sequence lowering
    // owns every action, performer and binding (`and an opponent draws`).
    let lowered = lower_local_effect_sequence(&mut cursor, &ability.effects)?;

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": {
            "kind": "librarySearched",
            "anyPlayer": true,
            "opponentOfSourceController": true,
        },
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    Ok(draft(
        rule,
        &[
            "Recognize a library-search event and opponent relation from the shared token stream",
            "Lower the triggered body through the shared effect-sequence lowering",
            "Resolve the source reference through lexical name links or a typed source anaphor",
            "Delegate `and` and sentence-level `then` semantics to the shared effect sequence",
            &trace,
        ],
    ))
}
