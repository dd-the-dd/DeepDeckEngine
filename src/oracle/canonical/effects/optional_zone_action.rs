use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the stable attack-trigger and optional-zone-action prefix. The
/// source, count, zone, continuation, and terminators remain owned by child
/// automata so a claimed near miss reports its first local failure.
pub(in crate::oracle::canonical) fn claims_promoted_optional_zone_action_trigger_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        || !source_reference_starts_at(
            1,
            tokens.get(1).map(|token| token.raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        )
    {
        return false;
    }
    let Some(index) = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("attacks"))
    else {
        return false;
    };
    let fixed = [
        (1, ","),
        (2, "you"),
        (3, "may"),
        (4, "exile"),
        (7, "from"),
        (8, "your"),
        (9, "graveyard"),
        (10, "."),
        (11, "if"),
        (12, "you"),
        (13, "do"),
        (14, ","),
    ];
    if !fixed.iter().all(|(offset, expected)| {
        tokens
            .get(index + offset)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    }) || !tokens
        .get(index + 5)
        .is_some_and(|token| token.token_type == "quantity")
        || !tokens.get(index + 6).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("card") || token.raw.eq_ignore_ascii_case("cards")
        })
    {
        return false;
    }

    let continuation_start = index + 15;
    let Some(becomes_index) = tokens.len().checked_sub(3) else {
        return false;
    };
    continuation_start < becomes_index
        && source_or_anaphor_reference_starts_at(
            continuation_start,
            tokens
                .get(continuation_start)
                .map(|token| token.raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        )
        && tokens
            .get(becomes_index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("becomes"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_optional_zone_action_trigger(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("optional-zone-action-trigger", input.tokens);
    cursor.enter("optional-zone-action-trigger.trigger");
    cursor.consume("whenever")?;
    let event = recognize_source_attacked_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("optional-zone-action-trigger.separator");
    cursor.consume(",")?;
    let action = recognize_optional_exact_count_zone_action(&mut cursor)?;
    cursor.enter("optional-zone-action-trigger.action-terminator");
    cursor.consume(".")?;
    recognize_if_performed_connector(&mut cursor)?;
    let continuation = recognize_source_state_effect(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        Some(&LocalObjectReference::SourceSelf),
    )?;
    cursor.enter("optional-zone-action-trigger.complete");
    cursor.finish()?;

    cursor.enter("optional-zone-action-trigger.validation");
    cursor.require(
        event
            == LocalTriggerEventNode::SourceAttacked {
                source: LocalObjectReference::SourceSelf,
            },
        "the current source as the attacking object",
    )?;
    cursor.require(
        action.player == LocalPlayerReference::Controller
            && action.action == ZoneActionKind::Exile
            && action.zone == PlayerZoneKind::Graveyard,
        "an optional exile action from the source controller's graveyard",
    )?;
    cursor.require(
        continuation.object == LocalObjectReference::SourceSelf,
        "a performed-action continuation applied to the source",
    )?;
    let NumericAtom::Integer(count) = action.count else {
        return Err(cursor.fail("an exact literal exile count"));
    };
    let player = controller();
    let continuation_effect = match continuation.state {
        SourceStateChange::Prepared => json!({
            "kind": "setPrepared",
            "object": self_ref(),
            "value": true,
        }),
        SourceStateChange::Tapped => json!({
            "kind": "tapPermanent",
            "permanent": self_ref(),
        }),
    };
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
            "effects": [{
                "kind": "optionalAction",
                "player": player.clone(),
                "action": {
                    "kind": "exileCards",
                    "count": count,
                    "from": graveyard(player),
                },
                "onPerformed": [continuation_effect],
            }],
        }),
        &[
            "Recognize a source-attack event",
            "Recognize one optional exact-count zone action",
            "Compose the performed connector with one independent source-state effect",
            &trace,
        ],
    ))
}
