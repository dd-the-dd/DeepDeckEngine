use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims only the event/effect boundary of a source-attack trigger whose
/// first effect targets a player for milling. Event, target, numeric, and
/// sentence grammar remain independently committed child-machine concerns.
pub(in crate::oracle::canonical) fn claims_promoted_source_attack_mill_family(
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
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && event
            .last()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("attacks"))
        && effect
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && effect
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("mills"))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_attack_mill(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-attacks-mill-trigger", input.tokens);
    cursor.enter("trigger.source-attacks.envelope");
    cursor.consume("whenever")?;
    let event = recognize_source_attacked_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.source-attacks.separator");
    cursor.consume(",")?;
    let effects =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;

    cursor.enter("trigger.source-attacks.event-validation");
    let LocalTriggerEventNode::SourceAttacked { source } = event else {
        return Err(cursor.fail("the current source being declared as an attacker"));
    };
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source being declared as an attacker",
    )?;
    cursor.enter("trigger.source-attacks.effect-validation");
    let [LocalEffectNode::MillCards(mill)] = effects.effects.as_slice() else {
        return Err(cursor.fail("one atomic targeted mill effect"));
    };
    cursor.require(
        effects.connectors.is_empty(),
        "one atomic targeted mill effect",
    )?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&mill.target);
    let count = cursor.require_value(
        lower_numeric_expression(&mill.count),
        "an executable mill count",
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
                "kind": "declaredAttacker",
                "object": self_ref(),
            },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    minimum,
                    maximum,
                )],
            },
            "effects": [{
                "kind": "mill",
                "player": chosen_target(target_id),
                "count": count,
            }],
        }),
        &[
            "Recognize the source-attacked event through the shared event automaton",
            "Bind one player target through the shared target-list automaton",
            "Lower milling as one independent primitive effect",
            &trace,
        ],
    ))
}
