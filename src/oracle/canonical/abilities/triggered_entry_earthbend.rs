use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims source-entry triggers whose first effect is the reusable Earthbend
/// keyword action. The event, quantity, connector, selector, destination, and
/// shuffle syntax remain owned by their respective child automata.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_earthbend_sequence_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("source-entry-earthbend-claim", input.tokens);
    if cursor.consume("when").is_err() {
        return false;
    }
    let Ok(LocalTriggerEventNode::SourceEntered { source }) = recognize_source_entered_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) else {
        return false;
    };
    source == LocalObjectReference::SourceSelf
        && cursor.peek() == Some(",")
        && cursor
            .peek_ahead(1)
            .is_some_and(|token| token.eq_ignore_ascii_case("earthbend"))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_earthbend_sequence(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-earthbend-sequence", input.tokens);
    cursor.enter("trigger.source-entry-earthbend.envelope");
    cursor.consume("when")?;
    let event = recognize_source_entered_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.source-entry-earthbend.separator");
    cursor.consume(",")?;
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;

    cursor.enter("trigger.source-entry-earthbend.event-validation");
    let LocalTriggerEventNode::SourceEntered { source } = event else {
        return Err(cursor.fail("the current source entering the battlefield"));
    };
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source entering the battlefield",
    )?;

    cursor.enter("trigger.source-entry-earthbend.sequence-validation");
    let earthbend_effect_index = sequence
        .effects
        .iter()
        .position(|effect| matches!(effect, LocalEffectNode::Earthbend(_)))
        .ok_or_else(|| cursor.fail("one Earthbend action in the effect sequence"))?;
    let [
        LocalEffectNode::Earthbend(earthbend),
        LocalEffectNode::ControllerLibrarySearch(search),
    ] = sequence.effects.as_slice()
    else {
        return Err(
            cursor.fail("an Earthbend action followed by one controller-owned library search")
        );
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "Earthbend followed by the library search",
    )?;

    cursor.enter("earthbend.action.variable-binding");
    cursor.require(
        !numeric_atom_uses_x(&earthbend.quantity),
        "an Earthbend quantity independent of an unavailable X binding",
    )?;
    cursor.enter("trigger.source-entry-earthbend.search-validation");
    cursor.require(
        search.player == LocalPlayerReference::Controller,
        "the source controller searching their library",
    )?;

    let lowered_earthbend = lower_earthbend_action(
        earthbend,
        &earthbend_target_id(earthbend_effect_index),
        json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Land"),
        }),
        1,
        1,
    );
    let SearchDestination::Battlefield { .. } = search.destination else {
        return Err(cursor.fail("a battlefield destination after Earthbend"));
    };
    let mut effects = lowered_earthbend.effects;
    effects.extend(lower_library_search_effects(
        &mut cursor,
        search,
        controller(),
    )?);
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
                "kind": "enterBattlefield",
                "object": self_ref(),
            },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [lowered_earthbend.decision],
            },
            "effects": effects,
        }),
        &[
            "Recognize source entry through the shared trigger-event automaton",
            "Compose Earthbend and the controller-owned search through the shared effect sequence",
            "Declare the Earthbend land target before the triggered ability resolves",
            "Lower Earthbend animation, haste, counters, delayed return, and signal independently",
            "Lower search, move, shuffle, and search record independently",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "triggered_entry_earthbend_tests.rs"]
mod tests;
