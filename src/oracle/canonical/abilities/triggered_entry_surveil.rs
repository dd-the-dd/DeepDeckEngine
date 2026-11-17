use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the event/effect boundary for a source-entry trigger whose atomic
/// effect is surveil. Source reference, numeric expression, and reminder
/// grammar remain committed child-machine responsibilities.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_surveil_family(
    input: &AbilityInput<'_>,
) -> bool {
    let Some(separator) = input.tokens.iter().position(|token| token.raw == ",") else {
        return false;
    };
    let event = &input.tokens[..separator];
    let effect = &input.tokens[separator + 1..];
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("when"))
        && event
            .last()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("enters"))
        && effect
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("surveil"))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_surveil(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-enters-surveil-trigger", input.tokens);
    cursor.enter("trigger.source-enters-surveil.envelope");
    cursor.consume("when")?;
    let event = recognize_source_entered_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.source-enters-surveil.separator");
    cursor.consume(",")?;
    let surveil = recognize_surveil_effect(&mut cursor, None)?;
    cursor.enter("trigger.source-enters-surveil.terminator");
    cursor.consume(".")?;
    if cursor.peek().is_some_and(|raw| raw == "(") {
        recognize_surveil_reminder(&mut cursor, &surveil.count)?;
    }
    cursor.enter("trigger.source-enters-surveil.full-consumption");
    cursor.finish()?;

    cursor.enter("trigger.source-enters-surveil.event-validation");
    let LocalTriggerEventNode::SourceEntered { source } = event else {
        return Err(cursor.fail("the current source entering the battlefield"));
    };
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source entering the battlefield",
    )?;
    cursor.require(
        surveil.player == LocalPlayerReference::Controller,
        "the source controller surveilling",
    )?;
    let effects = lower_targetless_local_effects(
        &LocalEffectNode::Surveil(surveil),
        TargetlessDeclarationSupport::None,
        None,
    )
    .map_err(|error| cursor.fail(error.expectation()))?;
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
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "effects": effects,
        }),
        &[
            "Resolve the source reference through the shared name/self automaton",
            "Recognize the source-entered event through the shared event automaton",
            "Recognize surveil and its count as one atomic typed effect",
            "Validate the optional canonical surveil reminder against that count",
            "Expand the library partition into primitives followed by its single registered completion signal",
            &trace,
        ],
    ))
}
