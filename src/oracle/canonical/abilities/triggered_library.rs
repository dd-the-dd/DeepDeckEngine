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
    let [
        LocalEffectNode::PutCounters(put_counters),
        LocalEffectNode::DrawCards(draw_cards),
    ] = ability.effects.effects.as_slice()
    else {
        return Err(cursor.fail("putCounters followed by drawCards"));
    };
    cursor.require(
        put_counters.permanent == PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        "counter placement on the source permanent",
    )?;
    cursor.require(
        draw_cards.player == LocalPlayerReference::Controller,
        "a draw for the source controller",
    )?;
    let counter_count = cursor.require_value(
        lower_numeric_expression(&put_counters.count),
        "a target-independent counter count",
    )?;
    let draw_count = cursor.require_value(
        lower_numeric_expression(&draw_cards.count),
        "a target-independent draw count",
    )?;

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "librarySearched",
                "anyPlayer": true,
                "opponentOfSourceController": true,
            },
            "effects": [
                {
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": put_counters.counter,
                    "count": counter_count,
                },
                {
                    "kind": "drawCards",
                    "player": controller(),
                    "count": draw_count,
                },
            ],
        }),
        &[
            "Recognize a library-search event and opponent relation from the shared token stream",
            "Compose counter placement and card draw as independent atomic effects",
            "Resolve the source reference through lexical name links or a typed source anaphor",
            "Delegate `and` and sentence-level `then` semantics to the shared effect sequence",
            &trace,
        ],
    ))
}
