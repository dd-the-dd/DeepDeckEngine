//! Keyword action `regenerate OBJECT` (CR 701.15). The object slot is the
//! shared permanent operand; the printed reminder is validated as typed
//! metadata of the keyword action and never accepted when its wording differs.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, PermanentKeywordAction, PermanentKeywordActionNode,
    PermanentOperandNode, recognize_contextual_permanent_operand,
};

/// Recognizes `regenerate OBJECT`. Punctuation and reminder text stay with the
/// enclosing sequence.
pub(in crate::oracle::canonical) fn recognize_regenerate_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.regenerate.action");
    let mut trial = cursor.clone();
    trial.consume("regenerate")?;
    cursor.enter("effect.regenerate.object");
    let targets =
        recognize_contextual_permanent_operand(&mut trial, name_links, name_failure, context)?;
    *cursor = trial;
    Ok(PermanentKeywordActionNode {
        action: PermanentKeywordAction::Regenerate,
        targets,
        quantity: None,
    })
}

/// Recognizes the keyword action `goad OBJECT` (CR 701.38a) over the shared
/// permanent operand. The goading player is the ability controller.
pub(in crate::oracle::canonical) fn recognize_goad_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<PermanentKeywordActionNode> {
    cursor.enter("effect.goad.action");
    let mut trial = cursor.clone();
    trial.consume("goad")?;
    cursor.enter("effect.goad.object");
    let targets =
        recognize_contextual_permanent_operand(&mut trial, name_links, name_failure, context)?;
    *cursor = trial;
    Ok(PermanentKeywordActionNode {
        action: PermanentKeywordAction::Goad,
        targets,
        quantity: None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReminderState {
    Open,
    Time,
    Subject,
    Condition,
    Replacement,
    Tap,
    RemoveFromCombat,
    Heal,
    Close,
    Complete,
}

fn objective_pronoun(subject: &str) -> Option<&'static str> {
    match subject.to_ascii_lowercase().as_str() {
        "it" => Some("it"),
        "he" => Some("him"),
        "she" => Some("her"),
        _ => None,
    }
}

/// Validates `(The next time SUBJECT would be destroyed this turn, instead tap
/// PRONOUN, remove PRONOUN from combat, and heal all damage on PRONOUN.)`
/// against the object of the preceding `regenerate` action. The subject is a
/// pronoun or `this|that|the NOUN` (noun phrases denote an object of one
/// singular kind); a plural or collection operand has no printed reminder.
pub(in crate::oracle::canonical) fn recognize_regenerate_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    operand: &PermanentOperandNode,
) -> MachineResult<()> {
    cursor.enter("effect.regenerate.reminder.operand");
    cursor.require(
        match operand {
            PermanentOperandNode::Reference(_) | PermanentOperandNode::BoundReference(_) => true,
            PermanentOperandNode::Targets(targets) => {
                targets.slots.len() == 1 && targets.slots[0].cardinality.maximum == 1
            }
            PermanentOperandNode::Collection { .. } => false,
        },
        "a singular regenerated object for the printed reminder",
    )?;
    let mut state = ReminderState::Open;
    let mut pronoun = "it";
    loop {
        state = match state {
            ReminderState::Open => {
                cursor.enter("effect.regenerate.reminder.open");
                cursor.consume("(")?;
                ReminderState::Time
            }
            ReminderState::Time => {
                cursor.enter("effect.regenerate.reminder.time");
                cursor.consume_phrase(&["the", "next", "time"])?;
                ReminderState::Subject
            }
            ReminderState::Subject => {
                cursor.enter("effect.regenerate.reminder.subject");
                let head = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a reminder subject"))?
                    .to_owned();
                if let Some(objective) = objective_pronoun(&head) {
                    pronoun = objective;
                    cursor.consume(&head)?;
                } else if ["this", "that", "the"]
                    .iter()
                    .any(|determiner| head.eq_ignore_ascii_case(determiner))
                {
                    cursor.consume(&head)?;
                    let noun = cursor
                        .peek()
                        .filter(|raw| raw.chars().all(char::is_alphabetic))
                        .ok_or_else(|| cursor.fail("one noun naming the regenerated object"))?
                        .to_owned();
                    cursor.consume(&noun)?;
                } else {
                    return Err(cursor.fail("`it` or `this|that|the` and a noun"));
                }
                ReminderState::Condition
            }
            ReminderState::Condition => {
                cursor.enter("effect.regenerate.reminder.condition");
                cursor.consume_phrase(&["would", "be", "destroyed", "this", "turn", ","])?;
                ReminderState::Replacement
            }
            ReminderState::Replacement => {
                cursor.enter("effect.regenerate.reminder.replacement");
                cursor.consume("instead")?;
                ReminderState::Tap
            }
            ReminderState::Tap => {
                cursor.enter("effect.regenerate.reminder.tap");
                cursor.consume("tap")?;
                cursor.consume(pronoun)?;
                cursor.consume(",")?;
                ReminderState::RemoveFromCombat
            }
            ReminderState::RemoveFromCombat => {
                cursor.enter("effect.regenerate.reminder.combat");
                cursor.consume("remove")?;
                cursor.consume(pronoun)?;
                cursor.consume_phrase(&["from", "combat", ",", "and"])?;
                ReminderState::Heal
            }
            ReminderState::Heal => {
                cursor.enter("effect.regenerate.reminder.heal");
                cursor.consume_phrase(&["heal", "all", "damage", "on"])?;
                cursor.consume(pronoun)?;
                cursor.consume(".")?;
                ReminderState::Close
            }
            ReminderState::Close => {
                cursor.enter("effect.regenerate.reminder.close");
                cursor.consume(")")?;
                ReminderState::Complete
            }
            ReminderState::Complete => return Ok(()),
        };
    }
}
