use super::super::*;
use crate::oracle::syntax::AbilityInput;

const EXILED_BINDING: &str = "delayedExiledPermanent";
const OWNER_BINDING: &str = "delayedExiledOwner";
const RETURNED_BINDING: &str = "delayedReturnedPermanent";

/// Claims the stable attack/exile/delayed-return/conditional-counter family
/// from lexical anchors only. The committed automaton validates every token
/// and owns all anaphoric bindings.
pub(in crate::oracle::canonical) fn claims_promoted_attack_delayed_return_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && source_reference_starts_at(
            1,
            tokens.get(1).map(|token| token.raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        )
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("attacks")
                && window[1].raw == ","
                && window[2].raw.eq_ignore_ascii_case("exile")
        })
        && tokens.windows(5).any(|window| {
            window[0].raw.eq_ignore_ascii_case("at")
                && window[1].raw.eq_ignore_ascii_case("the")
                && window[2].raw.eq_ignore_ascii_case("beginning")
                && window[3].raw.eq_ignore_ascii_case("of")
                && window[4].raw.eq_ignore_ascii_case("the")
        })
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("return")
                && window[1].raw.eq_ignore_ascii_case("that")
                && window[2].raw.eq_ignore_ascii_case("card")
        })
        && tokens.windows(4).any(|window| {
            window[0].raw.eq_ignore_ascii_case("if")
                && window[1].raw.eq_ignore_ascii_case("it")
                && window[2].raw.eq_ignore_ascii_case("entered")
                && window[3].raw.eq_ignore_ascii_case("under")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_attack_delayed_return_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("attack-delayed-return-counter", input.tokens);
    let node = recognize_attack_delayed_return_counter(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;

    cursor.enter("trigger.attack-delayed-return.validation");
    cursor.require(
        node.event
            == LocalTriggerEventNode::SourceAttacked {
                source: LocalObjectReference::SourceSelf,
            },
        "the current source as the attacking object",
    )?;
    cursor.require(
        node.exile.action == ZoneChangeAction::Exile && node.exile.target.slots.len() == 1,
        "one atomic targeted exile effect",
    )?;
    cursor.require(
        node.delayed_return.object == node.condition.object,
        "one returned-object binding shared by the delayed return and condition",
    )?;
    cursor.require(
        node.counters.permanent
            == PermanentOperandNode::Reference(LocalObjectReference::SourceSelf),
        "a conditional counter effect on the current source",
    )?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&node.exile.target);
    let target = chosen_target(target_id);
    let counter_effect = lower_targetless_local_effect(
        &LocalEffectNode::PutCounters(node.counters),
        TargetlessDeclarationSupport::None,
    )
    .map_err(|error| cursor.fail(error.expectation()))?;
    let delayed_step = match node.delayed_return.step {
        DelayedTurnStep::Upkeep => "upkeep",
        DelayedTurnStep::EndStep => "endStep",
    };
    let exiled = json!({
        "kind": "singleBoundObject",
        "binding": EXILED_BINDING,
    });
    let returned = json!({
        "kind": "singleBoundObject",
        "binding": RETURNED_BINDING,
    });
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
            "effects": [
                {
                    "kind": "exilePermanent",
                    "permanent": target,
                    "bind": EXILED_BINDING,
                },
                {
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "bindingNotEmpty",
                        "binding": EXILED_BINDING,
                    },
                    "then": [
                        {
                            "kind": "bind",
                            "id": OWNER_BINDING,
                            "value": { "kind": "ownerOf", "object": exiled },
                        },
                        {
                            "kind": "installDelayedStepTrigger",
                            "step": delayed_step,
                            "controller": controller(),
                            "captureBindings": [EXILED_BINDING, OWNER_BINDING],
                            "effects": [
                                {
                                    "kind": "moveCards",
                                    "cards": {
                                        "kind": "boundObjects",
                                        "binding": EXILED_BINDING,
                                    },
                                    "from": {
                                        "kind": "exile",
                                        "player": { "kind": "boundValue", "id": OWNER_BINDING },
                                    },
                                    "to": {
                                        "kind": "battlefield",
                                        "player": { "kind": "boundValue", "id": OWNER_BINDING },
                                        "tapped": false,
                                    },
                                    "bind": RETURNED_BINDING,
                                },
                                {
                                    "kind": "conditionalEffect",
                                    "condition": {
                                        "kind": "playersEqual",
                                        "left": {
                                            "kind": "controllerOf",
                                            "object": returned,
                                        },
                                        "right": controller(),
                                    },
                                    "then": [counter_effect],
                                    "else": [],
                                },
                            ],
                        },
                    ],
                    "else": [],
                },
            ],
        }),
        &[
            "Recognize the source attack event independently",
            "Retain optional cardinality, source exclusion, and permanent predicate in the shared target slot",
            "Bind only the object successfully exiled by the atomic exile primitive",
            "Snapshot its owner after the zone change",
            "Install a delayed step trigger with an explicit captured binding",
            "Return the bound card under its owner's control with the generic move primitive",
            "Compare the returned permanent's controller to the ability controller",
            "Apply the independent counter effect to the source only when the condition holds",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "delayed_blink_tests.rs"]
mod tests;
