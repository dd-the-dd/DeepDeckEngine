use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum SpellCastTriggerProgramParseAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

/// The event owns only its header. All body actions, numeric declarations,
/// reminders and choices use the existing shared EffectSequence boundary.
pub(in crate::oracle::canonical) fn try_parse_promoted_spell_cast_trigger_program(
    input: &AbilityInput<'_>,
) -> SpellCastTriggerProgramParseAttempt {
    let mut cursor = OracleTokenCursor::new("spell-cast-trigger-program", input.tokens);
    let event = match try_recognize_spell_cast_event(&mut cursor) {
        SpellCastEventAttempt::NotClaimed => {
            return SpellCastTriggerProgramParseAttempt::NotClaimed;
        }
        SpellCastEventAttempt::Unpromoted(failure) => {
            return SpellCastTriggerProgramParseAttempt::Unpromoted(failure);
        }
        SpellCastEventAttempt::Recognized(event) => event,
    };
    if event.source_spell
        || event.also_copy
        || event.ordinal.as_ref().is_some_and(|ordinal| {
            ordinal.basis != SpellCastOrdinalBasis::AllSpellsByCasterThisTurn
        })
    {
        return SpellCastTriggerProgramParseAttempt::NotClaimed;
    }
    cursor.enter("trigger.spell-cast.permanent-face");
    if let Err(failure) = cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for this battlefield spell-cast trigger",
    ) {
        return SpellCastTriggerProgramParseAttempt::Unpromoted(failure);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.enter("trigger.spell-cast.intervening-condition");
        return SpellCastTriggerProgramParseAttempt::Unpromoted(
            cursor.fail("a spell-cast trigger without an unsupported intervening-if condition"),
        );
    }
    let predicate = lower_spell_object_predicate(&event.query.predicate).unwrap_or(Value::Null);
    // The shared event backend currently evaluates a printed definition,
    // rather than the contextual StackObject. Keep numeric characteristics
    // unsupported here, including a literal threshold on a spell with mana X.
    // Player scope and ordinary ObjectQuery characteristics are shared with
    // the companion runtime player-and-object-filter correction.
    if spell_cast_predicate_requires_numeric_stack_context(&event.query.predicate) {
        cursor.enter("trigger.spell-cast.numeric-stack-context");
        return SpellCastTriggerProgramParseAttempt::Unpromoted(
            cursor.fail("a spell event predicate without unproved numeric stack characteristics"),
        );
    }
    let sequence = match try_recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf)
            .with_sentence_conditionals()
            .with_unless_frames(),
    ) {
        EffectSequenceParseAttempt::Parsed(sequence) => sequence,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            return SpellCastTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return SpellCastTriggerProgramParseAttempt::Failed(failure);
        }
    };
    let lowered = match try_lower_local_effect_sequence(&mut cursor, &sequence) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return SpellCastTriggerProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure)
        | EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return SpellCastTriggerProgramParseAttempt::Failed(failure);
        }
    };
    cursor.enter("trigger.spell-cast.complete-body");
    if let Err(failure) = cursor.require(
        !lowered.effects.is_empty(),
        "one fully consumed executable trigger program",
    ) {
        return SpellCastTriggerProgramParseAttempt::Failed(failure);
    }
    let mut event_ir = json!({"kind": "spellCast", "where": predicate});
    match event.caster {
        SpellCastEventCaster::Controller => event_ir["player"] = controller(),
        SpellCastEventCaster::AnyPlayer => event_ir["anyPlayer"] = json!(true),
        SpellCastEventCaster::Opponent => event_ir["opponentOfSourceController"] = json!(true),
    }
    if let Some(ordinal) = event.ordinal {
        debug_assert_eq!(
            ordinal.basis,
            SpellCastOrdinalBasis::AllSpellsByCasterThisTurn
        );
        event_ir["spellCastOrdinal"] = json!(ordinal.value);
    }
    let mut rule = json!({"kind": "triggeredAbility", "source": self_ref(), "event": event_ir, "effects": lowered.effects});
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({"kind": "castingDeclaration", "decisions": lowered.decisions});
    }
    SpellCastTriggerProgramParseAttempt::Parsed(draft(
        rule,
        &[
            "Recognize the event caster independently from a spell's stack controller",
            "Delegate the spell-domain predicate to shared ObjectQuery",
            "Keep an optional unfiltered ordinal bound to this caster and turn",
            "Recognize and lower the complete body through shared EffectSequence",
        ],
    ))
}

/// A support gate for this consumer, not a second numeric or query grammar.
/// Boolean composition must not hide an unproved numeric characteristic.
fn spell_cast_predicate_requires_numeric_stack_context(predicate: &ObjectPredicate) -> bool {
    predicate.contains_numeric_comparison()
}

#[cfg(test)]
#[path = "spell_cast_trigger_program_tests.rs"]
mod tests;
