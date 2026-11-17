use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum SacrificeTriggerProgramParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    FailedSupport(MachineFailure),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_promoted_sacrifice_trigger_program(
    input: &AbilityInput<'_>,
) -> SacrificeTriggerProgramParseAttempt {
    let mut cursor = OracleTokenCursor::new("sacrifice-trigger-program", input.tokens);
    let event = match try_recognize_sacrifice_event(&mut cursor) {
        SacrificeEventAttempt::NotClaimed => {
            return SacrificeTriggerProgramParseAttempt::NotClaimed;
        }
        SacrificeEventAttempt::Recognized(event) => event,
        SacrificeEventAttempt::Failed(failure) => {
            return SacrificeTriggerProgramParseAttempt::Failed(failure);
        }
    };
    cursor.enter("trigger.sacrifice.permanent-face");
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return SacrificeTriggerProgramParseAttempt::FailedSupport(
            cursor.fail("a permanent source face for a battlefield sacrifice trigger"),
        );
    }
    let event_ir = match lower_sacrifice_event(&mut cursor, &event) {
        Ok(event) => event,
        Err(failure) => return SacrificeTriggerProgramParseAttempt::FailedSupport(failure),
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.enter("trigger.sacrifice.intervening-condition");
        return SacrificeTriggerProgramParseAttempt::FailedSupport(
            cursor.fail("a sacrifice trigger without an unsupported intervening-if condition"),
        );
    }
    let sequence = match try_recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
            .with_sentence_conditionals(),
    ) {
        EffectSequenceParseAttempt::Parsed(sequence) => sequence,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            return SacrificeTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return SacrificeTriggerProgramParseAttempt::Failed(failure);
        }
    };
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &sequence) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return SacrificeTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure) => {
            return SacrificeTriggerProgramParseAttempt::FailedSupport(failure);
        }
        EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return SacrificeTriggerProgramParseAttempt::Failed(failure);
        }
    };
    cursor.enter("trigger.sacrifice.complete-body");
    if lowered.effects.is_empty() {
        return SacrificeTriggerProgramParseAttempt::Failed(
            cursor.fail("one fully consumed executable trigger program"),
        );
    }
    let mut rule = json!({"kind":"triggeredAbility","source":self_ref(),"event":event_ir,"effects":lowered.effects});
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({"kind":"castingDeclaration","decisions":lowered.decisions});
    }
    SacrificeTriggerProgramParseAttempt::Parsed(draft(
        rule,
        &[
            "Read one shared event actor independently of object ownership",
            "Delegate the sacrificed-object predicate to shared ObjectQuery",
            "Delegate the complete body and target declaration to EffectSequence",
        ],
    ))
}

#[cfg(test)]
#[path = "sacrifice_trigger_program_tests.rs"]
mod tests;
