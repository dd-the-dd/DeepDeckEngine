use super::super::*;
use crate::oracle::syntax::AbilityInput;

const DELAYED_DRAW_COUNT_ID: &str = "delayedDrawCount";

#[cfg(test)]
#[path = "delayed_draw_optional_amount_tests.rs"]
mod optional_amount_tests;

pub(in crate::oracle::canonical) fn claims_promoted_delayed_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 10
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("draw"))
        && tokens.windows(5).any(|window| {
            window[0].raw.eq_ignore_ascii_case("beginning")
                && window[1].raw.eq_ignore_ascii_case("of")
                && window[2].raw.eq_ignore_ascii_case("the")
                && window[3].raw.eq_ignore_ascii_case("next")
                && (window[4].raw.eq_ignore_ascii_case("turn's")
                    || window[4].raw.eq_ignore_ascii_case("turn’s"))
        })
}

fn lower_delayed_player(
    cursor: &OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    captured: Option<(TargetBinding, &'static str)>,
) -> MachineResult<Value> {
    match player {
        LocalPlayerReference::Controller => Ok(controller()),
        LocalPlayerReference::ControllerOfQualified(_) => {
            Err(cursor.fail("a proved nominal/controller binding for the delayed drawing player"))
        }
        LocalPlayerReference::ControllerOf(binding) => {
            let Some((expected, binding_id)) = captured else {
                return Err(cursor.fail("a captured antecedent for the delayed drawing player"));
            };
            cursor.require(
                binding == expected,
                "the delayed drawing player to reference the counter target",
            )?;
            Ok(json!({ "kind": "boundValue", "id": binding_id }))
        }
    }
}

pub(in crate::oracle::canonical) fn lower_delayed_draw_schedule(
    cursor: &OracleTokenCursor<'_>,
    delayed: &DelayedStepEffectNode,
    captured: Option<(TargetBinding, &'static str)>,
) -> MachineResult<Value> {
    cursor.require(
        delayed.turn == DelayedTurnReference::NextTurn,
        "the next turn as the delayed turn reference",
    )?;
    let step = delayed.step.runtime_step();
    let mut effects = Vec::new();
    match delayed.effect.as_ref() {
        LocalEffectNode::DrawCards(draw) => {
            let player = lower_delayed_player(cursor, draw.player, captured)?;
            effects.push(json!({
                "kind": "drawCards",
                "player": player,
                "count": lower_numeric_expression(&draw.count)
                    .expect("delayed draw grammar has no target-property numeric binding"),
            }));
        }
        LocalEffectNode::OptionalDrawCards(draw) => {
            let player = lower_delayed_player(cursor, draw.player, captured)?;
            if !draw.up_to && draw.maximum != NumericExpression::Atom(NumericAtom::Integer(1)) {
                effects.push(json!({
                    "kind": "optionalEffects", "player": player.clone(),
                    "effects": [{
                        "kind": "drawCards", "player": player,
                        "count": lower_numeric_expression(&draw.maximum)
                            .expect("optional delayed draw has no target-property numeric binding"),
                    }],
                }));
            } else {
                effects.push(json!({
                    "kind": "chooseNumber",
                    "id": DELAYED_DRAW_COUNT_ID,
                    "player": player.clone(),
                    "minimum": integer(0),
                    "maximum": lower_numeric_expression(&draw.maximum)
                        .expect("optional delayed draw has no target-property numeric binding"),
                }));
                effects.push(json!({
                    "kind": "drawCards",
                    "player": player,
                    "count": {
                        "kind": "decisionResult",
                        "decisionId": DELAYED_DRAW_COUNT_ID,
                    },
                }));
            }
        }
        _ => return Err(cursor.fail("a required or optional draw child effect")),
    }

    let mut schedule = json!({
        "kind": "installDelayedStepTrigger",
        "step": step,
        "controller": controller(),
        "effects": effects,
    });
    if let Some((_, binding_id)) = captured {
        schedule["captureBindings"] = json!([binding_id]);
    }
    Ok(schedule)
}

pub(in crate::oracle::canonical) fn parse_promoted_delayed_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("delayed-draw", input.tokens);
    let delayed = recognize_delayed_draw(&mut cursor, None)?.delayed;
    cursor.enter("effect.delayed-draw.terminator");
    cursor.consume(".")?;
    cursor.enter("effect.delayed-draw.complete");
    cursor.finish()?;
    let schedule = lower_delayed_draw_schedule(&cursor, &delayed, None)?;
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
            "effects": [schedule],
        }),
        &[
            "Recognize an explicit drawing player and count",
            "Wrap the primitive draw in a typed next-turn step schedule",
            &trace,
        ],
    ))
}
