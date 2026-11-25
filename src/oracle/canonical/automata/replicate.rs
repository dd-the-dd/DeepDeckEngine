use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, recognize_mana_cost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReplicateKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplicateState {
    Keyword,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    CastTrigger,
    CopyInstruction,
    PaidCostReference,
    FirstSentenceTerminator,
    NewTargetsOrClose,
    NewTargetsPermission,
    SecondSentenceTerminator,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct ReplicateAttemptFrame {
    cost: Option<ManaCostNode>,
}

/// Recognizes a repeatable mana additional-cost keyword and validates its
/// canonical reminder without reparsing a text slice. The optional second
/// reminder sentence is independent of the printed cost and only describes
/// retargeting the copies.
pub(in crate::oracle::canonical) fn recognize_mana_replicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReplicateKeywordNode> {
    let mut state = ReplicateState::Keyword;
    let mut frame = ReplicateAttemptFrame::default();

    loop {
        state = match state {
            ReplicateState::Keyword => {
                cursor.enter("keyword.replicate.keyword");
                cursor.consume("replicate")?;
                ReplicateState::Cost
            }
            ReplicateState::Cost => {
                cursor.enter("keyword.replicate.cost");
                frame.cost = Some(recognize_mana_cost(cursor)?);
                ReplicateState::ReminderOrComplete
            }
            ReplicateState::ReminderOrComplete => {
                cursor.enter("keyword.replicate.reminder-or-complete");
                match cursor.peek() {
                    None => ReplicateState::Complete,
                    Some("(") => ReplicateState::ReminderOpen,
                    _ => {
                        return Err(cursor.fail("a canonical Replicate reminder or end of input"));
                    }
                }
            }
            ReplicateState::ReminderOpen => {
                cursor.enter("keyword.replicate.reminder.open");
                cursor.consume("(")?;
                ReplicateState::CastTrigger
            }
            ReplicateState::CastTrigger => {
                cursor.enter("keyword.replicate.reminder.cast-trigger");
                cursor.consume_phrase(&["when", "you", "cast", "this", "spell", ","])?;
                ReplicateState::CopyInstruction
            }
            ReplicateState::CopyInstruction => {
                cursor.enter("keyword.replicate.reminder.copy-instruction");
                cursor.consume_phrase(&["copy", "it", "for", "each", "time"])?;
                ReplicateState::PaidCostReference
            }
            ReplicateState::PaidCostReference => {
                cursor.enter("keyword.replicate.reminder.paid-cost-reference");
                cursor.consume_phrase(&["you", "paid", "its", "replicate", "cost"])?;
                ReplicateState::FirstSentenceTerminator
            }
            ReplicateState::FirstSentenceTerminator => {
                cursor.enter("keyword.replicate.reminder.first-terminator");
                cursor.consume(".")?;
                ReplicateState::NewTargetsOrClose
            }
            ReplicateState::NewTargetsOrClose => {
                cursor.enter("keyword.replicate.reminder.new-targets-or-close");
                match cursor.peek() {
                    Some(")") => ReplicateState::ReminderClose,
                    Some(raw) if raw.eq_ignore_ascii_case("you") => {
                        ReplicateState::NewTargetsPermission
                    }
                    _ => return Err(cursor.fail("`)` or the canonical new-target permission")),
                }
            }
            ReplicateState::NewTargetsPermission => {
                cursor.enter("keyword.replicate.reminder.new-targets-permission");
                cursor.consume_phrase(&["you", "may", "choose", "new", "targets", "for", "the"])?;
                // The copies of every paid instance (`copies`), or the one
                // copy a printing that names a single payment states.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("copy"))
                {
                    cursor.consume("copy")?;
                } else {
                    cursor.consume("copies")?;
                }
                ReplicateState::SecondSentenceTerminator
            }
            ReplicateState::SecondSentenceTerminator => {
                cursor.enter("keyword.replicate.reminder.second-terminator");
                cursor.consume(".")?;
                ReplicateState::ReminderClose
            }
            ReplicateState::ReminderClose => {
                cursor.enter("keyword.replicate.reminder.close");
                cursor.consume(")")?;
                ReplicateState::Complete
            }
            ReplicateState::Complete => {
                cursor.enter("keyword.replicate.complete");
                cursor.finish()?;
                return Ok(ReplicateKeywordNode {
                    cost: cursor.require_value(frame.cost, "a populated Replicate cost")?,
                });
            }
        };
    }
}
