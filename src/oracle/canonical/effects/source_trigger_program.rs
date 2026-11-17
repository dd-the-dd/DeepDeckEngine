use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum SourceTriggerProgramParseAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

#[cfg(test)]
#[path = "source_trigger_program_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_trigger_union_tests.rs"]
mod union_tests;

/// One source envelope, one shared body recognition, one shared lowering pass.
/// Action-level optionality stays inside the corresponding action child.
pub(in crate::oracle::canonical) fn try_parse_promoted_source_trigger_program(
    input: &AbilityInput<'_>,
) -> SourceTriggerProgramParseAttempt {
    let mut cursor = OracleTokenCursor::new("source-trigger-program", input.tokens);
    let envelope = match try_recognize_source_trigger_envelope(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        SourceTriggerEnvelopeAttempt::NotClaimed => {
            return SourceTriggerProgramParseAttempt::NotClaimed;
        }
        SourceTriggerEnvelopeAttempt::Unpromoted(failure) => {
            return SourceTriggerProgramParseAttempt::Unpromoted(failure);
        }
        SourceTriggerEnvelopeAttempt::Recognized(envelope) => envelope,
    };
    cursor.enter("trigger.source-program.permanent-face");
    if let Err(failure) = cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        if envelope.event == SourceTriggerEventKind::Tapped {
            "a permanent source face for a tapped source"
        } else {
            "a permanent source face for an entering or dying source"
        },
    ) {
        return SourceTriggerProgramParseAttempt::Unpromoted(failure);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.enter("trigger.source-program.intervening-condition");
        return SourceTriggerProgramParseAttempt::Unpromoted(
            cursor.fail("a source trigger without an unsupported intervening-if condition"),
        );
    }
    let sequence = match try_recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::for_source(envelope.source)
            .with_source_event_subject(envelope.source)
            .with_sentence_conditionals()
            .with_unless_frames(),
    ) {
        EffectSequenceParseAttempt::Parsed(sequence) => sequence,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            return SourceTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return SourceTriggerProgramParseAttempt::Failed(failure);
        }
    };
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &sequence) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return SourceTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure)
        | EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return SourceTriggerProgramParseAttempt::Failed(failure);
        }
    };
    cursor.enter("trigger.source-program.executable-effects");
    if let Err(failure) = cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable source-trigger effect",
    ) {
        return SourceTriggerProgramParseAttempt::Failed(failure);
    }
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": lower_source_transition_event(&envelope),
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    SourceTriggerProgramParseAttempt::Parsed(draft(
        rule,
        &[
            "Recognize the current source identity and its battlefield transition",
            "Recognize and lower each effect through the shared typed sequence",
            "Keep action permission, choices and numeric bindings in their owning children",
        ],
    ))
}
