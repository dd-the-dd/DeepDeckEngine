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
    // The ability's resolved source names are readable everywhere in the
    // ability, its event and intervening condition included (`if it has
    // greater power than NAME`), not only inside the effect sequence.
    cursor.replace_source_names(Some(input.name_links.into()));
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
    // The zone the ability functions from (CR 113.6) follows from the typed
    // event: the spell's own cast functions on the stack for any spell, a
    // battlefield observer needs a permanent face.
    cursor.enter("trigger.program.permanent-face");
    cursor.require(
        !envelope.event.requires_permanent_face()
            || envelope.event.functions_on_the_stack()
            || type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for this battlefield triggered ability",
    )?;
    let event = lower_generic_trigger_event(cursor, &envelope.event)?;
    // `while CONDITION` / an unmatched turn owner: checked as the event
    // occurs (CR 603.2), never on resolution like the intervening `if`.
    let event_condition = envelope
        .event_condition
        .as_ref()
        .map(|condition| lower_event_time_condition(cursor, condition))
        .transpose()?;
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
        source_attachment_role(input.face_type_line),
    )?;
    // `if you control another Human` / `two or more other creatures` (CR
    // 603.4): `other` excludes the object the ability is about. When that
    // object is the ability's source (its own entry, its own attack, or an
    // event that binds no object), the count excludes the source's current
    // battlefield incarnation, which the runtime authenticates by instance
    // and zone identity both when the trigger is put on the stack and when
    // it resolves; once the source left the battlefield nothing is excluded
    // (CR 400.7). An event that binds another object (`Whenever a Mountain
    // you control enters, if you control at least five other Mountains`)
    // makes `other` exclude that object, which this count cannot express.
    if node
        .guard
        .as_ref()
        .is_some_and(|guard| guard_excludes_source(&guard.value))
        && guard_other_names_event_object(&node.event.value)
    {
        cursor.enter("trigger.program.guard-other-event-object");
        return Err(cursor.fail(
            "an exclusion of the triggering event's object for `other`; the count excludes only the ability's source",
        ));
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
                    previous_action_result: None,
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
    // The lowering reads the frame's event card through the same environment.
    let enclosing =
        cursor.replace_binding_environment(Some(std::sync::Arc::new(BindingEnvironment {
            frame: FrameBindings {
                event_card: event_card_zone(&node.event.value),
                ..FrameBindings::default()
            },
            ..BindingEnvironment::default()
        })));
    let lowered = match &node.program.value {
        // X is lowered provisionally; the card-composition validator accepts
        // it only with a typed producer of this rule (CR 107.3m).
        TriggerBodyNode::Program(program) => lower_effect_program_with_context(
            cursor,
            program,
            EffectSequenceLoweringContext::ValidateXAfterCardComposition,
        ),
        TriggerBodyNode::Modal(modal) => lower_effect_sequence_modal(cursor, modal),
    };
    cursor.replace_binding_environment(enclosing);
    let lowered = lowered?;
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable action",
    )?;
    // CR 605.1b: a trigger on an enchanted permanent's mana ability whose
    // program adds mana is a triggered mana ability. It resolves at once as
    // part of that mana ability (CR 605.4a) when every action adds mana to
    // the enchanted permanent's controller and nothing is declared; any
    // other mana-adding program has no typed runtime context. A program
    // that adds no mana is an ordinary triggered ability.
    if event["kind"] == "attachedPermanentManaAbilityActivated"
        && lowered.effects.iter().any(adds_mana)
        && !(condition.is_none()
            && event_condition.is_none()
            && !node.once_each_turn
            && lowered.decisions.is_empty()
            && attached_mana_trigger_is_folded(&lowered.effects))
    {
        cursor.enter("trigger.program.attached-mana-ability");
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
            "a checked mana discovery for a triggered mana ability of the enchanted permanent",
        ));
    }
    let mut rule = json!({"kind":"triggeredAbility", "source":self_ref(), "event":event, "effects":lowered.effects});
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    if let Some(event_condition) = event_condition {
        rule["eventCondition"] = event_condition;
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

/// Whether a lowered effect adds mana anywhere in its nested programs.
fn adds_mana(effect: &Value) -> bool {
    match effect {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some("addMana")
                || map.values().any(adds_mana)
        }
        Value::Array(items) => items.iter().any(adds_mana),
        _ => false,
    }
}

fn guard_excludes_source(guard: &SentenceConditionNode) -> bool {
    condition_any(guard, &ConditionAtom::excludes_source)
}

/// Whether the triggering event binds an object other than the ability's
/// source, which an `other` of the guard would then exclude instead of the
/// source (`a Forest you control enters, if you control five other Forests`).
fn guard_other_names_event_object(event: &GenericTriggerEventNode) -> bool {
    if event.observed_object_is_source() {
        return false;
    }
    event.observed_object().is_some()
        || event_frame(event)
            .event_object
            .is_some_and(|slot| !matches!(slot.reference, PermanentEffectReference::Source(_)))
}

#[cfg(test)]
#[path = "generic_trigger_attempt_tests.rs"]
mod generic_trigger_attempt_tests;

#[cfg(test)]
#[path = "trigger_guard_condition_tests.rs"]
mod trigger_guard_condition_tests;

#[cfg(test)]
#[path = "trigger_guard_other_tests.rs"]
mod trigger_guard_other_tests;

#[cfg(test)]
#[path = "trigger_frame_event_tests.rs"]
mod trigger_frame_event_tests;

#[cfg(test)]
#[path = "player_action_trigger_tests.rs"]
mod player_action_trigger_tests;

#[cfg(test)]
#[path = "counter_event_trigger_tests.rs"]
mod counter_event_trigger_tests;

#[cfg(test)]
#[path = "damage_event_trigger_tests.rs"]
mod damage_event_trigger_tests;
