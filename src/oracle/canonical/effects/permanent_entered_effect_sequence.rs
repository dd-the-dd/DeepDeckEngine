use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum PermanentEnteredParseAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

fn lower_event(envelope: &PermanentEnteredEnvelopeNode) -> Value {
    let mut event = json!({
        "kind": "permanentEntered",
        "where": lower_permanent_object_predicate(&envelope.object.predicate),
    });
    match envelope.object.controller {
        Some(ObjectControllerScope::AbilityController) => event["player"] = controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => {
            event["player"] = json!({ "kind": "opponentsOf", "player": controller() });
        }
        Some(ObjectControllerScope::NotAbilityController) => {
            unreachable!("the envelope rejects negative controller scopes")
        }
        None => event["anyController"] = Value::Bool(true),
    }
    if envelope.exclude_source {
        event["excludeSource"] = Value::Bool(true);
    }
    event
}

fn lower_node(
    cursor: &mut OracleTokenCursor<'_>,
    node: PermanentEnteredEffectSequenceNode,
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
        "event": lower_event(&node.envelope),
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
            "Recognize a generic permanent-entry ObjectQuery",
            "Delegate the complete body to the shared EffectSequence",
            "Preserve target declarations and sequence bindings",
            &trace,
        ],
    ))
}

/// Parse-once promoted-family boundary. A native body child is the authority:
/// failure before that child consumes its first token means this family does
/// not claim the ability; later failures remain explicit native diagnostics.
pub(in crate::oracle::canonical) fn try_parse_promoted_permanent_entered_effect_sequence(
    input: &AbilityInput<'_>,
) -> PermanentEnteredParseAttempt {
    let mut cursor = OracleTokenCursor::new("permanent-entered-effect-sequence", input.tokens);
    let envelope = match recognize_permanent_entered_envelope(&mut cursor) {
        Ok(envelope) => envelope,
        Err(failure) => {
            // Commit only after the object query and `enters` token both
            // succeeded. Merely reaching the object state must not swallow
            // unrelated triggers such as `Whenever you cast ...`.
            let recognized_trigger = failure
                .trace
                .iter()
                .any(|transition| transition.state == "trigger.permanent-entered.separator");
            return if recognized_trigger {
                PermanentEnteredParseAttempt::Failed(failure)
            } else {
                PermanentEnteredParseAttempt::NotClaimed
            };
        }
    };
    let effects = match try_recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        // Explicit source names and `this OBJECT` remain available through the
        // lexical source-reference automaton. A bare `it`, however, refers to
        // the entering permanent in this envelope and is intentionally left
        // unbound until triggering-object bindings and LKI are modeled.
        &EffectRecognitionContext::default(),
    ) {
        EffectSequenceParseAttempt::Parsed(effects) => effects,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            return PermanentEnteredParseAttempt::Unpromoted(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return PermanentEnteredParseAttempt::Failed(failure);
        }
    };
    if envelope.optional {
        cursor.enter("trigger.permanent-entered.optional-scope");
        if let Err(failure) = cursor.require(
            effects.effects.len() == 1 && effects.connectors.is_empty(),
            "exactly one atomic effect governed by `you may`",
        ) {
            return PermanentEnteredParseAttempt::Unpromoted(failure);
        }
    }
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &effects) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return PermanentEnteredParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure) => {
            return PermanentEnteredParseAttempt::Failed(failure);
        }
        EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return PermanentEnteredParseAttempt::Failed(failure);
        }
    };
    let node = PermanentEnteredEffectSequenceNode { envelope, effects };
    match lower_node(&mut cursor, node, lowered) {
        Ok(draft) => PermanentEnteredParseAttempt::Parsed(draft),
        Err(failure) => PermanentEnteredParseAttempt::Failed(failure),
    }
}

#[cfg(test)]
#[path = "permanent_entered_effect_sequence_tests.rs"]
mod tests;
