use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

/// Outcomes preserve ownership from the typed event envelope. Cursor
/// advancement and diagnostic labels never decide semantic commitment.
pub(in crate::oracle::canonical) enum GenericTriggerProgramAttempt {
    NotApplicable,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(CanonicalRuleDraft),
}

/// The common parent never reparses source text. Once the event is a complete
/// typed envelope, this parent owns its guard/program and any failure there.
pub(in crate::oracle::canonical) fn try_parse_generic_trigger_program(
    input: &AbilityInput<'_>,
) -> GenericTriggerProgramAttempt {
    let mut cursor = OracleTokenCursor::new("generic-trigger-program", input.tokens);
    let envelope = match try_recognize_generic_trigger_envelope(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        GenericTriggerEnvelopeAttempt::NotClaimed => {
            return GenericTriggerProgramAttempt::NotApplicable;
        }
        GenericTriggerEnvelopeAttempt::RejectedCandidate(failure) => {
            return GenericTriggerProgramAttempt::RejectedCandidate(failure);
        }
        GenericTriggerEnvelopeAttempt::FailedCommitted(failure) => {
            return GenericTriggerProgramAttempt::FailedCommitted(failure);
        }
        GenericTriggerEnvelopeAttempt::Recognized(envelope) => envelope,
    };
    match compile_recognized_trigger_program(input, &mut cursor, envelope) {
        Ok(draft) => GenericTriggerProgramAttempt::Parsed(draft),
        Err(failure) => GenericTriggerProgramAttempt::FailedCommitted(failure),
    }
}

fn compile_recognized_trigger_program(
    input: &AbilityInput<'_>,
    cursor: &mut OracleTokenCursor<'_>,
    envelope: GenericTriggerEnvelopeNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("trigger.program.permanent-face");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for this battlefield triggered ability",
    )?;
    let event = lower_generic_trigger_event(cursor, &envelope.event)?;
    let mut construction = vec![(
        "Resolve the event envelope; leave the guard and body pending".into(),
        json!({"syntax":"triggeredProgram", "event":event,
            "eventTokens":[0,envelope.body_start], "pendingFromToken":envelope.body_start}),
    )];
    let node = recognize_trigger_program_tail(
        cursor,
        SyntaxNode {
            tokens: 0..envelope.body_start,
            value: envelope.event,
        },
        input.name_links,
        input.name_resolution_failure,
    )?;
    // `other` needs an explicit exclusion identity and incarnation. The
    // current count contract cannot prove the observed-object binding or
    // source departure at resolution. Keep new compositions unsupported.
    if node
        .guard
        .as_ref()
        .is_some_and(|guard| guard_excludes_source(&guard.value))
    {
        cursor.enter("trigger.program.guard-other-binding");
        return Err(cursor
            .fail("a proved exclusion identity and current battlefield incarnation for other"));
    }
    let condition = node
        .guard
        .as_ref()
        .map(|guard| {
            lower_sentence_condition(
                cursor,
                &guard.value,
                &ConditionObjectBindings {
                    permanent_targets: &[],
                    card_targets: &[],
                    card_focus: None,
                },
            )
        })
        .transpose()?;
    construction.push((
        "Compose the typed condition and effect sequence within their token ranges".into(),
        json!({"syntax":"triggeredProgram", "event":event,
            "eventTokens":[node.event.tokens.start,node.event.tokens.end],
            "condition":condition,
            "conditionTokens":node.guard.as_ref().map(|n| vec![n.tokens.start,n.tokens.end]),
            "programTokens":[node.program.tokens.start,node.program.tokens.end],
            "actionCount":node.program.value.action_count()}),
    ));
    let lowered = match &node.program.value {
        TriggerBodyNode::Program(program) => lower_effect_program(cursor, program)?,
        TriggerBodyNode::Modal(modal) => lower_effect_sequence_modal(cursor, modal)?,
    };
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable action",
    )?;
    let mut rule = json!({"kind":"triggeredAbility", "source":self_ref(), "event":event, "effects":lowered.effects});
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({"kind":"castingDeclaration", "decisions":lowered.decisions});
    }
    if node.once_each_turn {
        // Unique per ability on this face: the runtime counts per source and id.
        rule["triggerLimit"] = json!({
            "kind": "onceEachTurn",
            "id": format!("onceEachTurn:{}", input.source.line_start),
        });
    }
    construction.push((
        "Lower the composed syntax into canonical primitive IR".into(),
        rule.clone(),
    ));
    let mut result = draft(
        rule,
        &["Compose the shared event, Boolean guard and action program"],
    );
    result.construction = construction;
    Ok(result)
}

fn guard_excludes_source(guard: &SentenceConditionNode) -> bool {
    match guard {
        BooleanConditionNode::Atom(SentenceConditionAtom::Local(
            LocalConditionNode::ControlledPermanentCount(count),
        )) => count.exclude_source,
        BooleanConditionNode::Atom(_) => false,
        BooleanConditionNode::And(items) | BooleanConditionNode::Or(items) => {
            items.iter().any(guard_excludes_source)
        }
        BooleanConditionNode::Not(item) => guard_excludes_source(item),
    }
}

#[cfg(test)]
#[path = "generic_trigger_attempt_tests.rs"]
mod generic_trigger_attempt_tests;

#[cfg(test)]
#[path = "trigger_guard_condition_tests.rs"]
mod trigger_guard_condition_tests;
