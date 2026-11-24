//! Typed reminders of keyword actions a program performs (CR 701): each one
//! restates its action's definition and is validated against the action's
//! own operand or quantity, never skipped as text (CR 207.2).
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, NumericAtom, NumericExpression, PermanentOperandNode,
    recognize_numeric_expression,
};

/// `(Create a Clue token. It's an artifact with "{2}, Sacrifice this token:
/// Draw a card.")` (CR 701.16a, 111.10f). Owns both parentheses.
pub(in crate::oracle::canonical) fn recognize_investigate_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    cursor.enter("reminder.investigate.open");
    cursor.consume("(")?;
    cursor.enter("reminder.investigate.create");
    super::consume_lemma(cursor, super::VerbLemma::Create, super::VerbAgreement::Base)?;
    cursor.enter("reminder.investigate.quantity");
    cursor.consume("a")?;
    cursor.enter("reminder.investigate.token-name");
    cursor.consume_phrase(&["clue", "token", "."])?;
    cursor.enter("reminder.investigate.token-definition");
    cursor.consume_phrase(&["it's", "an", "artifact", "with", "\""])?;
    cursor.enter("reminder.investigate.activation-cost");
    cursor.consume_phrase(&["{2}", ","])?;
    super::consume_lemma(
        cursor,
        super::VerbLemma::Sacrifice,
        super::VerbAgreement::Base,
    )?;
    cursor.consume_phrase(&["this", "token", ":"])?;
    cursor.enter("reminder.investigate.activation-effect");
    cursor.consume_phrase(&["draw", "a", "card", ".", "\""])?;
    cursor.enter("reminder.investigate.close");
    cursor.consume(")")
}

/// `(Exile cards from the top of your library until you exile a nonland card
/// with mana value N or less. Cast it without paying its mana cost or put it
/// into your hand. Put the rest on the bottom in a random order.)`
/// (CR 701.57a), with N equal to the action's literal quantity.
pub(in crate::oracle::canonical) fn recognize_discover_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    quantity: Option<&NumericExpression>,
) -> MachineResult<()> {
    cursor.enter("reminder.discover.quantity-source");
    let Some(quantity @ NumericExpression::Atom(NumericAtom::Integer(_))) = quantity else {
        return Err(cursor.fail("a literal discover quantity for reminder validation"));
    };
    cursor.enter("reminder.discover.open");
    cursor.consume("(")?;
    cursor.enter("reminder.discover.exile-until");
    cursor.consume_phrase(&[
        "exile", "cards", "from", "the", "top", "of", "your", "library", "until", "you", "exile",
        "a", "nonland", "card", "with", "mana", "value",
    ])?;
    cursor.enter("reminder.discover.quantity-match");
    let restated = recognize_numeric_expression(cursor, None)?;
    cursor.require(
        restated == *quantity,
        "the same literal quantity as the discover action",
    )?;
    cursor.enter("reminder.discover.cast-or-hand");
    cursor.consume_phrase(&[
        "or", "less", ".", "cast", "it", "without", "paying", "its", "mana", "cost", "or", "put",
        "it", "into", "your", "hand", ".",
    ])?;
    cursor.enter("reminder.discover.rest");
    cursor.consume_phrase(&[
        "put", "the", "rest", "on", "the", "bottom", "in", "a", "random", "order", ".",
    ])?;
    cursor.enter("reminder.discover.close");
    cursor.consume(")")
}

/// The object a permanent keyword action's reminder names: the source as
/// `this creature`, a target or bound object as `that creature`. A
/// collection has no singular reminder object (CR 701.44d, 701.50c).
fn reminder_object_phrase(operand: &PermanentOperandNode) -> Option<[&'static str; 2]> {
    match operand {
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf) => {
            Some(["this", "creature"])
        }
        PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_) => {
            Some(["that", "creature"])
        }
        PermanentOperandNode::Collection { .. } => None,
    }
}

/// `(Reveal the top card of your library. Put that card into your hand if
/// it's a land. Otherwise, put a +1/+1 counter on OBJECT, then put the card
/// back or put it into your graveyard.)` (CR 701.44a).
pub(in crate::oracle::canonical) fn recognize_explore_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    operand: &PermanentOperandNode,
) -> MachineResult<()> {
    cursor.enter("reminder.explore.object");
    let object = reminder_object_phrase(operand)
        .ok_or_else(|| cursor.fail("one exploring creature named by the reminder"))?;
    cursor.enter("reminder.explore.open");
    cursor.consume("(")?;
    cursor.enter("reminder.explore.reveal");
    cursor.consume_phrase(&[
        "reveal", "the", "top", "card", "of", "your", "library", ".", "put", "that", "card",
        "into", "your", "hand", "if", "it's", "a", "land", ".",
    ])?;
    cursor.enter("reminder.explore.counter");
    cursor.consume_phrase(&["otherwise", ",", "put", "a", "+1/+1", "counter", "on"])?;
    cursor.consume_phrase(&object)?;
    cursor.enter("reminder.explore.revealed-card");
    cursor.consume_phrase(&[
        ",",
        "then",
        "put",
        "the",
        "card",
        "back",
        "or",
        "put",
        "it",
        "into",
        "your",
        "graveyard",
        ".",
    ])?;
    cursor.enter("reminder.explore.close");
    cursor.consume(")")
}

/// `(Draw a card, then discard a card. If you discarded a nonland card, put a
/// +1/+1 counter on OBJECT.)` (CR 701.50a), for the unparameterized action.
pub(in crate::oracle::canonical) fn recognize_connive_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    operand: &PermanentOperandNode,
    quantity: Option<&NumericExpression>,
) -> MachineResult<()> {
    cursor.enter("reminder.connive.object");
    let object = reminder_object_phrase(operand)
        .filter(|_| quantity.is_none())
        .ok_or_else(|| cursor.fail("one creature conniving once, named by the reminder"))?;
    cursor.enter("reminder.connive.open");
    cursor.consume("(")?;
    cursor.enter("reminder.connive.loot");
    cursor.consume_phrase(&[
        "draw", "a", "card", ",", "then", "discard", "a", "card", ".",
    ])?;
    cursor.enter("reminder.connive.counter");
    cursor.consume_phrase(&[
        "if",
        "you",
        "discarded",
        "a",
        "nonland",
        "card",
        ",",
        "put",
        "a",
        "+1/+1",
        "counter",
        "on",
    ])?;
    cursor.consume_phrase(&object)?;
    cursor.consume(".")?;
    cursor.enter("reminder.connive.close");
    cursor.consume(")")
}
