use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum PhaseTriggerParseAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

fn lower_step(step: TurnStep) -> &'static str {
    match step {
        TurnStep::Upkeep => "upkeep",
        TurnStep::Draw => "draw",
        TurnStep::PrecombatMain => "precombatMain",
        TurnStep::BeginCombat => "beginCombat",
        TurnStep::PostcombatMain => "postcombatMain",
        TurnStep::EndStep => "endStep",
    }
}

fn lower_event(event: StepEventNode) -> Value {
    let player = match event.player {
        StepPlayerScope::AbilityController => controller(),
        StepPlayerScope::EachPlayer => json!({ "kind": "eachPlayer" }),
        StepPlayerScope::OpponentsOfAbilityController => {
            json!({ "kind": "opponentsOf", "player": controller() })
        }
    };
    json!({
        "kind": "stepBegan",
        "step": lower_step(event.step),
        "player": player,
    })
}

fn has_unsupported_trigger_limit_suffix(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    const SUFFIX_TOKEN_COUNT: usize = 9;
    if tokens.len() < SUFFIX_TOKEN_COUNT {
        return false;
    }
    let mut cursor = OracleTokenCursor::new("phase-trigger-limit-suffix", tokens);
    if cursor
        .advance_to(tokens.len().saturating_sub(SUFFIX_TOKEN_COUNT))
        .is_err()
    {
        return false;
    }
    cursor.enter("trigger.limit.sentence-boundary");
    cursor.consume(".").is_ok()
        && {
            cursor.enter("trigger.limit.suffix");
            cursor
                .consume_phrase(&[
                    "this", "ability", "triggers", "only", "once", "each", "turn",
                ])
                .is_ok()
        }
        && cursor.consume(".").is_ok()
        && cursor.finish().is_ok()
}

fn lower_node(
    cursor: &mut OracleTokenCursor<'_>,
    node: PhaseTriggerSequenceNode,
    lowered: LoweredEffectSequence,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable atomic effect",
    )?;
    let effects = if node.envelope.optional {
        vec![json!({
            "kind": "optionalEffects",
            "player": controller(),
            "effects": lowered.effects,
        })]
    } else {
        lowered.effects
    };
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": lower_event(node.envelope.event),
        "effects": effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize a typed turn-step event and player scope",
            "Delegate the complete body to the shared EffectSequence",
            "Lower each action to independent primitive IR in Oracle order",
            &trace,
        ],
    ))
}

/// Parse-once boundary for unconditional beginning-of-step triggers. Headers
/// outside the typed StepEvent language and bodies whose first action is not
/// shared remain explicitly available to older families during migration.
pub(in crate::oracle::canonical) fn try_parse_promoted_phase_trigger_sequence(
    input: &AbilityInput<'_>,
) -> PhaseTriggerParseAttempt {
    let mut cursor = OracleTokenCursor::new("phase-trigger-sequence", input.tokens);
    let envelope = match recognize_phase_trigger_envelope(&mut cursor) {
        Ok(envelope) => envelope,
        Err(failure) => {
            let committed = failure
                .trace
                .iter()
                .any(|transition| transition.state == "trigger.phase.body");
            return if committed {
                PhaseTriggerParseAttempt::Failed(failure)
            } else {
                PhaseTriggerParseAttempt::NotClaimed
            };
        }
    };

    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
        || has_unsupported_trigger_limit_suffix(input.tokens)
    {
        return PhaseTriggerParseAttempt::NotClaimed;
    }

    let effects = match try_recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::default(),
    ) {
        EffectSequenceParseAttempt::Parsed(effects) => effects,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            return PhaseTriggerParseAttempt::Unpromoted(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return PhaseTriggerParseAttempt::Failed(failure);
        }
    };
    if envelope.optional {
        cursor.enter("trigger.phase.optional-scope");
        if let Err(failure) = cursor.require(
            effects.effects.len() == 1 && effects.connectors.is_empty(),
            "exactly one atomic effect governed by `you may`",
        ) {
            return PhaseTriggerParseAttempt::Unpromoted(failure);
        }
    }
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &effects) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return PhaseTriggerParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure) => {
            return PhaseTriggerParseAttempt::Failed(failure);
        }
        EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return PhaseTriggerParseAttempt::Failed(failure);
        }
    };
    match lower_node(
        &mut cursor,
        PhaseTriggerSequenceNode { envelope, effects },
        lowered,
    ) {
        Ok(draft) => PhaseTriggerParseAttempt::Parsed(draft),
        Err(failure) => PhaseTriggerParseAttempt::Failed(failure),
    }
}

#[cfg(test)]
#[path = "phase_trigger_sequence_tests.rs"]
mod tests;
