use super::super::{MachineResult, OracleTokenCursor};
use super::{EarlyTurnEntryNode, recognize_ordinal_atom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EarlyTurnEntryState {
    Subject,
    EntryState,
    Unless,
    FirstOrdinal,
    ConnectorOrTurn,
    NextOrdinal,
    TurnNoun,
    GameScope,
    Terminator,
    Complete,
}

#[derive(Default)]
struct EarlyTurnEntryFrame {
    maximum_controller_turn: Option<i64>,
}

/// Recognizes an entry replacement whose exemption is the controller's
/// consecutive early turns. The ordinal list is syntax, not a set of special
/// card cases: it must start at `first` and advance without gaps.
pub(in crate::oracle::canonical) fn recognize_early_turn_entry(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EarlyTurnEntryNode> {
    let mut state = EarlyTurnEntryState::Subject;
    let mut frame = EarlyTurnEntryFrame::default();
    loop {
        state = match state {
            EarlyTurnEntryState::Subject => {
                cursor.enter("replacement.early-turn-entry.subject");
                cursor.consume_phrase(&["this", "land"])?;
                EarlyTurnEntryState::EntryState
            }
            EarlyTurnEntryState::EntryState => {
                cursor.enter("replacement.early-turn-entry.entry-state");
                cursor.consume_phrase(&["enters", "tapped"])?;
                EarlyTurnEntryState::Unless
            }
            EarlyTurnEntryState::Unless => {
                cursor.enter("replacement.early-turn-entry.unless");
                cursor.consume_phrase(&["unless", "it's", "your"])?;
                EarlyTurnEntryState::FirstOrdinal
            }
            EarlyTurnEntryState::FirstOrdinal => {
                cursor.enter("replacement.early-turn-entry.first-ordinal");
                let value = recognize_ordinal_atom(cursor)?;
                cursor.require(value == 1, "`first` as the initial turn ordinal")?;
                frame.maximum_controller_turn = Some(value);
                EarlyTurnEntryState::ConnectorOrTurn
            }
            EarlyTurnEntryState::ConnectorOrTurn => {
                cursor.enter("replacement.early-turn-entry.connector-or-turn");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                    {
                        cursor.consume("or")?;
                    }
                    EarlyTurnEntryState::NextOrdinal
                } else if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    EarlyTurnEntryState::NextOrdinal
                } else {
                    EarlyTurnEntryState::TurnNoun
                }
            }
            EarlyTurnEntryState::NextOrdinal => {
                cursor.enter("replacement.early-turn-entry.next-ordinal");
                let ordinal_token = cursor.peek_token().map(|token| {
                    (
                        cursor.position(),
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    )
                });
                let value = recognize_ordinal_atom(cursor)?;
                let expected = frame
                    .maximum_controller_turn
                    .ok_or_else(|| cursor.fail("a prior ordinal"))?
                    + 1;
                if value != expected {
                    cursor.enter("replacement.early-turn-entry.next-ordinal");
                    let (token_index, span_start, span_end, found) =
                        cursor.require_value(ordinal_token, "the next consecutive turn ordinal")?;
                    return Err(cursor.fail_with_span(
                        "the next consecutive turn ordinal",
                        token_index,
                        span_start,
                        span_end,
                        found,
                    ));
                }
                frame.maximum_controller_turn = Some(value);
                EarlyTurnEntryState::ConnectorOrTurn
            }
            EarlyTurnEntryState::TurnNoun => {
                cursor.enter("replacement.early-turn-entry.turn-noun");
                cursor.consume("turn")?;
                EarlyTurnEntryState::GameScope
            }
            EarlyTurnEntryState::GameScope => {
                cursor.enter("replacement.early-turn-entry.game-scope");
                cursor.consume_phrase(&["of", "the", "game"])?;
                EarlyTurnEntryState::Terminator
            }
            EarlyTurnEntryState::Terminator => {
                cursor.enter("replacement.early-turn-entry.terminator");
                cursor.consume(".")?;
                EarlyTurnEntryState::Complete
            }
            EarlyTurnEntryState::Complete => {
                cursor.enter("replacement.early-turn-entry.complete");
                cursor.finish()?;
                return Ok(EarlyTurnEntryNode {
                    maximum_controller_turn: cursor.require_value(
                        frame.maximum_controller_turn,
                        "a populated consecutive turn threshold",
                    )?,
                });
            }
        };
    }
}
