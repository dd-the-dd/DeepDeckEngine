use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_source_cast_x_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    source_cast_x_entry_family_starts(
        input.tokens,
        input.name_links,
        input.name_resolution_failure,
    ) && input.tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("enters")
            && window[1].raw == ","
            && window[2].raw.eq_ignore_ascii_case("put")
    }) && has_source_cast_x_counter_placement(input.tokens)
}

fn has_source_cast_x_counter_placement(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        if !token.raw.eq_ignore_ascii_case("put")
            || !tokens.get(index + 1).is_some_and(|quantity| {
                quantity.token_type == "quantity" && quantity.raw.eq_ignore_ascii_case("x")
            })
        {
            return false;
        }
        let mut position = index + 2;
        let mut name_parts = 0;
        while tokens.get(position).is_some_and(|part| {
            matches!(part.token_type.as_str(), "counterNamePart" | "counterValue")
        }) {
            name_parts += 1;
            position += 1;
        }
        name_parts > 0
            && tokens
                .get(position)
                .is_some_and(|noun| noun.raw.eq_ignore_ascii_case("counters"))
    })
}

fn lower_source(reference: &LocalObjectReference) -> Value {
    match reference {
        LocalObjectReference::SourceSelf => self_ref(),
    }
}

fn lower_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_) => {
            unreachable!("source-entry draw grammar binds the source controller")
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_source_cast_x_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-cast-x-entry", input.tokens);
    let node = recognize_source_cast_x_entry_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("source-cast-x-entry.sequence-shape");
    let [
        LocalEffectNode::PutCounters(counters),
        LocalEffectNode::DrawCards(draw),
    ] = node.effects.effects.as_slice()
    else {
        return Err(cursor.fail("putCounters followed by drawCards"));
    };
    cursor.require(
        node.effects.connectors.as_slice() == [EffectConnector::Then],
        "one ordered `then` connector",
    )?;
    cursor.require(
        counters.permanent == PermanentOperandNode::Reference(node.source.clone()),
        "the entering source as counter recipient",
    )?;
    cursor.require(
        counters.count == NumericExpression::SourceCastX,
        "the direct source cast X value as counter count",
    )?;
    let expected_draw_count = NumericExpression::divide_rounded_down(counters.count.clone(), 2)
        .ok_or_else(|| cursor.fail("a positive literal divisor"))?;
    cursor.require(
        draw.player == LocalPlayerReference::Controller && draw.count == expected_draw_count,
        "the source controller drawing half source cast X rounded down",
    )?;
    cursor.enter("source-cast-x-entry.lower-counter-count");
    let counter_count = lower_numeric_expression(&counters.count)
        .ok_or_else(|| cursor.fail("a target-independent counter count"))?;
    cursor.enter("source-cast-x-entry.lower-draw-count");
    let draw_count = lower_numeric_expression(&draw.count)
        .ok_or_else(|| cursor.fail("a target-independent draw count"))?;
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
                "object": lower_source(&node.source),
            },
            "effects": [
                {
                    "kind": "putCounters",
                    "permanent": lower_source(&node.source),
                    "counter": counters.counter.clone(),
                    "count": counter_count,
                },
                {
                    "kind": "drawCards",
                    "player": lower_player(&draw.player),
                    "count": draw_count,
                },
            ],
        }),
        &[
            "Recognize an entry event bound to the current source",
            "Reuse one source-cast X expression across independent effects",
            "Delegate division and rounding to the shared numeric-expression automaton",
            "Sequence atomic putCounters then drawCards effects",
            &trace,
        ],
    ))
}
