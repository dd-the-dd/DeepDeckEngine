use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericAtom, PaymentActionNode, recognize_numeric_atom, recognize_payment_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BlightCostNode {
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalAdditionalCostNode {
    pub cost: BlightCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalBlightState {
    Envelope,
    Keyword,
    Amount,
    Terminator,
    ReminderOrComplete,
    ReminderAmount,
    ReminderCounter,
    ReminderObject,
    Complete,
}

#[derive(Default)]
struct OptionalBlightFrame {
    amount: Option<i64>,
    reminder_amount: Option<i64>,
}

/// Recognizes an optional Blight casting cost and validates its reminder from
/// the same immutable token stream. Reminder text is syntax evidence only; the
/// typed cost remains one reusable `putCounters` payment.
pub(in crate::oracle::canonical) fn recognize_optional_blight_additional_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OptionalAdditionalCostNode> {
    let mut state = OptionalBlightState::Envelope;
    let mut frame = OptionalBlightFrame::default();
    loop {
        state = match state {
            OptionalBlightState::Envelope => {
                cursor.enter("additional-cost.optional.envelope");
                cursor.consume_phrase(&[
                    "as",
                    "an",
                    "additional",
                    "cost",
                    "to",
                    "cast",
                    "this",
                    "spell",
                    ",",
                    "you",
                    "may",
                ])?;
                OptionalBlightState::Keyword
            }
            OptionalBlightState::Keyword => {
                cursor.enter("additional-cost.blight.keyword");
                cursor.require(
                    cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("blight")),
                    "`blight`",
                )?;
                OptionalBlightState::Amount
            }
            OptionalBlightState::Amount => {
                cursor.enter("additional-cost.blight.amount");
                let PaymentActionNode::Blight { amount } = recognize_payment_action(cursor)? else {
                    return Err(cursor.fail("a Blight payment action"));
                };
                frame.amount = Some(amount);
                OptionalBlightState::Terminator
            }
            OptionalBlightState::Terminator => {
                cursor.enter("additional-cost.blight.terminator");
                cursor.consume(".")?;
                OptionalBlightState::ReminderOrComplete
            }
            OptionalBlightState::ReminderOrComplete => {
                cursor.enter("additional-cost.blight.reminder-or-complete");
                if cursor.peek().is_none() {
                    OptionalBlightState::Complete
                } else {
                    cursor.consume_phrase(&["(", "you", "may", "put"])?;
                    OptionalBlightState::ReminderAmount
                }
            }
            OptionalBlightState::ReminderAmount => {
                cursor.enter("additional-cost.blight.reminder-amount");
                frame.reminder_amount = Some(positive_literal(
                    cursor,
                    "a positive literal reminder counter amount",
                )?);
                OptionalBlightState::ReminderCounter
            }
            OptionalBlightState::ReminderCounter => {
                cursor.enter("additional-cost.blight.reminder-counter");
                cursor.consume("-1/-1")?;
                let counter_noun = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`counter` or `counters`"))?
                    .to_string();
                cursor.require(
                    counter_noun.eq_ignore_ascii_case("counter")
                        || counter_noun.eq_ignore_ascii_case("counters"),
                    "`counter` or `counters`",
                )?;
                cursor.consume(&counter_noun)?;
                OptionalBlightState::ReminderObject
            }
            OptionalBlightState::ReminderObject => {
                cursor.enter("additional-cost.blight.reminder-object");
                cursor.consume_phrase(&["on", "a", "creature", "you", "control", ".", ")"])?;
                cursor.enter("additional-cost.blight.reminder-validation");
                cursor.require(
                    frame.reminder_amount == frame.amount,
                    "a reminder amount equal to the Blight amount",
                )?;
                OptionalBlightState::Complete
            }
            OptionalBlightState::Complete => {
                cursor.enter("additional-cost.blight.complete");
                cursor.finish()?;
                return Ok(OptionalAdditionalCostNode {
                    cost: BlightCostNode {
                        amount: cursor.require_value(frame.amount, "a populated Blight amount")?,
                    },
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn positive_literal(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &str,
) -> MachineResult<i64> {
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("a") || token.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
        return Ok(1);
    }
    let NumericAtom::Integer(amount) = recognize_numeric_atom(cursor)? else {
        return Err(cursor.fail(expected));
    };
    cursor.require(amount > 0, expected)?;
    Ok(amount)
}
