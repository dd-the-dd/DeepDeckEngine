use crate::oracle::{
    characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, ObjectCardType, recognize_source_reference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceEntryState {
    Prepared,
    Tapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryStateNode {
    pub source: LocalObjectReference,
    /// Retained when Oracle names the source structurally (`this land`). A
    /// printed current-card/current-face name already identifies the source,
    /// so its permanent type is validated by the enclosing card context.
    pub source_type: Option<ObjectCardType>,
    pub state: SourceEntryState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceEntryMachineState {
    Source,
    Event,
    State,
    Terminator,
    Reminder,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrepareReminderMachineState {
    Open,
    PreparedCondition,
    Permission,
    CopySpell,
    SentenceBreak,
    Unprepare,
    Close,
    Complete,
}

/// Consumes Prepare's canonical reminder text when it follows a `prepared`
/// entering state. The reminder documents the keyword's behavior but does not
/// lower to an additional rule: `setPrepared` remains the only semantic IR.
fn recognize_optional_prepare_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    entering_state: SourceEntryState,
) -> MachineResult<()> {
    if entering_state != SourceEntryState::Prepared
        || !cursor.peek().is_some_and(|token| token == "(")
    {
        return Ok(());
    }

    let mut machine_state = PrepareReminderMachineState::Open;
    loop {
        machine_state = match machine_state {
            PrepareReminderMachineState::Open => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.open");
                cursor.consume("(")?;
                PrepareReminderMachineState::PreparedCondition
            }
            PrepareReminderMachineState::PreparedCondition => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.condition");
                cursor.consume("while")?;
                cursor.consume("it's")?;
                cursor.consume("prepared")?;
                cursor.consume(",")?;
                PrepareReminderMachineState::Permission
            }
            PrepareReminderMachineState::Permission => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.permission");
                cursor.consume("you")?;
                cursor.consume("may")?;
                cursor.consume("cast")?;
                PrepareReminderMachineState::CopySpell
            }
            PrepareReminderMachineState::CopySpell => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.copy-spell");
                cursor.consume("a")?;
                cursor.consume("copy")?;
                cursor.consume("of")?;
                cursor.consume("its")?;
                cursor.consume("spell")?;
                PrepareReminderMachineState::SentenceBreak
            }
            PrepareReminderMachineState::SentenceBreak => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.sentence-break");
                cursor.consume(".")?;
                PrepareReminderMachineState::Unprepare
            }
            PrepareReminderMachineState::Unprepare => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.unprepare");
                cursor.consume("doing")?;
                cursor.consume("so")?;
                cursor.consume("unprepares")?;
                cursor.consume("it")?;
                cursor.consume(".")?;
                PrepareReminderMachineState::Close
            }
            PrepareReminderMachineState::Close => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.close");
                cursor.consume(")")?;
                PrepareReminderMachineState::Complete
            }
            PrepareReminderMachineState::Complete => {
                cursor.enter("replacement.source-entry-state.prepare-reminder.complete");
                return Ok(());
            }
        };
    }
}

/// Recognizes the unconditional replacement `SOURCE enters STATE.` where
/// `SOURCE` is either a typed `this PERMANENT_TYPE` reference or a current
/// card/current-face name resolved by the shared lexical name linker. Only
/// states with atomic engine primitives are admitted here.
pub(in crate::oracle::canonical) fn recognize_source_entry_state(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEntryStateNode> {
    let mut machine_state = SourceEntryMachineState::Source;
    let mut source = None;
    let mut source_type = None;
    let mut entering_state = None;
    loop {
        machine_state = match machine_state {
            SourceEntryMachineState::Source => {
                cursor.enter("replacement.source-entry-state.source");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                {
                    let checkpoint = cursor.checkpoint();
                    cursor.consume("this")?;
                    let type_token = cursor
                        .peek_token()
                        .ok_or_else(|| cursor.fail("a permanent card type after `this`"))?;
                    source_type = Some(
                        lookup_permanent_card_type(&type_token.raw, AcceptedForms::SingularOnly)
                            .ok_or_else(|| cursor.fail("a permanent card type after `this`"))?,
                    );
                    cursor.rollback(checkpoint);
                }
                source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                SourceEntryMachineState::Event
            }
            SourceEntryMachineState::Event => {
                cursor.enter("replacement.source-entry-state.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                SourceEntryMachineState::State
            }
            SourceEntryMachineState::State => {
                cursor.enter("replacement.source-entry-state.state");
                let token = cursor.consume_type("stateWord")?;
                entering_state = Some(if token.raw.eq_ignore_ascii_case("tapped") {
                    SourceEntryState::Tapped
                } else if token.raw.eq_ignore_ascii_case("prepared") {
                    SourceEntryState::Prepared
                } else {
                    return Err(cursor.fail("the `tapped` or `prepared` entering state"));
                });
                SourceEntryMachineState::Terminator
            }
            SourceEntryMachineState::Terminator => {
                cursor.enter("replacement.source-entry-state.terminator");
                cursor.consume(".")?;
                SourceEntryMachineState::Reminder
            }
            SourceEntryMachineState::Reminder => {
                cursor.enter("replacement.source-entry-state.reminder");
                recognize_optional_prepare_reminder(
                    cursor,
                    cursor.require_value(entering_state, "a populated entering state")?,
                )?;
                SourceEntryMachineState::Complete
            }
            SourceEntryMachineState::Complete => {
                cursor.enter("replacement.source-entry-state.complete");
                cursor.finish()?;
                let source = cursor.require_value(source, "a populated source reference")?;
                return Ok(SourceEntryStateNode {
                    source,
                    source_type,
                    state: cursor.require_value(entering_state, "a populated entering state")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "source_entry_tests.rs"]
mod tests;
