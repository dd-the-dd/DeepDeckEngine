use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, recognize_mana_cost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OverloadKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverloadState {
    Keyword,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    CastPermission,
    KeywordCostReference,
    FirstSentenceTerminator,
    AlternativeCondition,
    SubstitutionAction,
    QuotedTarget,
    TextRelation,
    QuotedEach,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct OverloadAttemptFrame {
    cost: Option<ManaCostNode>,
}

/// Recognizes Overload from one immutable token stream. The reminder is
/// validated as documentary grammar; semantic substitution is emitted as a
/// typed compilation directive by the lowering layer, never as text rewriting.
pub(in crate::oracle::canonical) fn recognize_mana_overload(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OverloadKeywordNode> {
    let mut state = OverloadState::Keyword;
    let mut frame = OverloadAttemptFrame::default();

    loop {
        state = match state {
            OverloadState::Keyword => {
                cursor.enter("keyword.overload.keyword");
                cursor.consume("overload")?;
                OverloadState::Cost
            }
            OverloadState::Cost => {
                cursor.enter("keyword.overload.cost");
                frame.cost = Some(recognize_mana_cost(cursor)?);
                OverloadState::ReminderOrComplete
            }
            OverloadState::ReminderOrComplete => {
                cursor.enter("keyword.overload.reminder-or-complete");
                match cursor.peek() {
                    None => OverloadState::Complete,
                    Some("(") => OverloadState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Overload reminder or end of input")),
                }
            }
            OverloadState::ReminderOpen => {
                cursor.enter("keyword.overload.reminder.open");
                cursor.consume("(")?;
                OverloadState::CastPermission
            }
            OverloadState::CastPermission => {
                cursor.enter("keyword.overload.reminder.cast-permission");
                cursor.consume_phrase(&["you", "may", "cast", "this", "spell", "for", "its"])?;
                OverloadState::KeywordCostReference
            }
            OverloadState::KeywordCostReference => {
                cursor.enter("keyword.overload.reminder.cost-reference");
                cursor.consume_phrase(&["overload", "cost"])?;
                OverloadState::FirstSentenceTerminator
            }
            OverloadState::FirstSentenceTerminator => {
                cursor.enter("keyword.overload.reminder.first-terminator");
                cursor.consume(".")?;
                OverloadState::AlternativeCondition
            }
            OverloadState::AlternativeCondition => {
                cursor.enter("keyword.overload.reminder.alternative-condition");
                cursor.consume_phrase(&["if", "you", "do", ","])?;
                OverloadState::SubstitutionAction
            }
            OverloadState::SubstitutionAction => {
                cursor.enter("keyword.overload.reminder.substitution-action");
                cursor.consume("change")?;
                OverloadState::QuotedTarget
            }
            OverloadState::QuotedTarget => {
                cursor.enter("keyword.overload.reminder.quoted-target");
                cursor.consume_phrase(&["\"", "target", "\""])?;
                OverloadState::TextRelation
            }
            OverloadState::TextRelation => {
                cursor.enter("keyword.overload.reminder.text-relation");
                cursor.consume_phrase(&["in", "its", "text", "to"])?;
                OverloadState::QuotedEach
            }
            OverloadState::QuotedEach => {
                cursor.enter("keyword.overload.reminder.quoted-each");
                cursor.consume_phrase(&["\"", "each", ".", "\""])?;
                OverloadState::ReminderClose
            }
            OverloadState::ReminderClose => {
                cursor.enter("keyword.overload.reminder.close");
                cursor.consume(")")?;
                OverloadState::Complete
            }
            OverloadState::Complete => {
                cursor.enter("keyword.overload.complete");
                cursor.finish()?;
                return Ok(OverloadKeywordNode {
                    cost: cursor.require_value(frame.cost, "a populated Overload cost")?,
                });
            }
        };
    }
}
