use super::super::*;
use crate::oracle::syntax::AbilityInput;

const TRIGGER_DAMAGE_TARGET_ID: &str = "triggerDamageTarget";

/// Claims the reusable event-envelope shape, leaving every event and effect
/// token to the committed child automata.
pub(in crate::oracle::canonical) fn claims_promoted_entry_or_repeat_trigger_family(
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
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("when"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("enters"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && event
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("draws"))
}

pub(in crate::oracle::canonical) fn parse_promoted_entry_or_repeat_trigger(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("entry-or-repeat-trigger", input.tokens);
    let ability = recognize_entry_or_repeat_triggered_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;

    cursor.enter("trigger.entry-or-repeat.event-shape");
    let [
        LocalTriggerEventNode::SourceEntered { source },
        LocalTriggerEventNode::CardDrawn { player, exception },
    ] = ability.events.events.as_slice()
    else {
        return Err(cursor.fail("source entry followed by a card-draw event"));
    };
    cursor.require(
        *source == LocalObjectReference::SourceSelf,
        "the current source entering",
    )?;
    cursor.require(
        *player == PlayerPredicate::Opponent
            && *exception == Some(DrawEventException::FirstInEachDrawStep),
        "an opponent draw except the first draw in each draw step",
    )?;

    cursor.enter("trigger.entry-or-repeat.effect-shape");
    let [
        LocalEffectNode::DealDamage(damage),
        amass @ LocalEffectNode::Amass(_),
    ] = ability.effects.effects.as_slice()
    else {
        return Err(cursor.fail("dealDamage followed by amass"));
    };
    cursor.require(
        ability.effects.connectors.as_slice() == [EffectConnector::Then],
        "one ordered `then` connector",
    )?;

    cursor.enter("trigger.entry-or-repeat.lower-damage");
    let damage = cursor.require_value(
        lower_targeted_damage_effect(damage, TRIGGER_DAMAGE_TARGET_ID),
        "targeted damage with one supported target slot",
    )?;
    cursor.enter("trigger.entry-or-repeat.lower-amass");
    let amass = lower_targetless_local_effect(amass, TargetlessDeclarationSupport::None)
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
            "event": {
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "enterBattlefield",
                        "object": self_ref(),
                    },
                    {
                        "kind": "cardDrawn",
                        "opponentOfSourceController": true,
                        "exceptFirstInDrawStep": true,
                    },
                ],
            },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [damage.decision],
            },
            "effects": [damage.effect, amass],
        }),
        &[
            "Recognize entry and repeated draw events as one typed event union",
            "Resolve the draw-step exception through the event child automaton",
            "Compose targeted damage and amass as independent atomic effects",
            "Delegate effect ordering to the shared effect-sequence automaton",
            &trace,
        ],
    ))
}
