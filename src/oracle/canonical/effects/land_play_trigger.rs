use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the stable land-play envelope. The event child owns the object-type
/// validation, while the shared effect-sequence automaton owns every effect
/// after the comma. This keeps land-play reusable for draw, sacrifice, and
/// future targetless primitive effects.
pub(in crate::oracle::canonical) fn claims_promoted_land_play_trigger_family(
    input: &AbilityInput<'_>,
) -> bool {
    matches!(input.tokens, [trigger, player, action, determiner, _, separator, _, ..]
        if (trigger.raw.eq_ignore_ascii_case("when")
            || trigger.raw.eq_ignore_ascii_case("whenever"))
            && player.raw.eq_ignore_ascii_case("you")
            && (action.raw.eq_ignore_ascii_case("play")
                || action.raw.eq_ignore_ascii_case("plays"))
            && (determiner.raw.eq_ignore_ascii_case("a")
                || determiner.raw.eq_ignore_ascii_case("another"))
            && separator.raw == ",")
        && input.tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_land_play_trigger(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("land-play-trigger", input.tokens);
    cursor.enter("land-play-trigger.trigger");
    let trigger = cursor.consume_type("triggerWord")?;
    cursor.require(
        trigger.raw.eq_ignore_ascii_case("when") || trigger.raw.eq_ignore_ascii_case("whenever"),
        "the `when` or `whenever` trigger",
    )?;
    let event = recognize_land_played_event(&mut cursor)?;
    cursor.enter("land-play-trigger.separator");
    cursor.consume(",")?;
    let effects = recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf),
    )?;

    let LocalTriggerEventNode::LandPlayed {
        player,
        exclude_source,
        from_exile: false,
    } = event
    else {
        return Err(cursor.fail("a populated land-play event"));
    };
    cursor.enter("land-play-trigger.validation");
    cursor.require(
        player == LocalPlayerReference::Controller,
        "the source controller as the land-playing player",
    )?;
    // `A or B` is one resolution choice (CR 608.2d): only the shared program
    // lowering frames it; this family lowers a plain sequence.
    cursor.require(
        !effects.connectors.contains(&EffectConnector::Or),
        "a plain effect sequence without an `or` choice",
    )?;
    let mut lowered_effects = Vec::new();
    for (effect_index, effect) in effects.effects.iter().enumerate() {
        lowered_effects.extend(
            lower_targetless_local_effects(
                effect,
                TargetlessDeclarationSupport::None,
                Some(&format!("landPlayEffect{effect_index}")),
            )
            .map_err(|error| cursor.fail(error.expectation()))?,
        );
    }

    let mut event = json!({
        "kind": "landPlayed",
        "player": controller(),
    });
    if exclude_source {
        event["excludeSource"] = Value::Bool(true);
    }
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
            "event": event,
            "effects": lowered_effects,
        }),
        &[
            "Recognize a reusable player land-play event",
            "Retain `another` as source exclusion",
            "Compose the shared targetless effect sequence",
            &trace,
        ],
    ))
}
