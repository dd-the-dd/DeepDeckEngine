//! The player-chosen cardinality slot read before an operand's noun
//! (CR 107.1, 601.2c, 601.2h, 608.2d).
//!
//! ```text
//! ChosenRange := "any" "number" ["of"]            AnyNumber   (zero or more)
//!              | "one" "or" "more"                 OneOrMore   (at least one)
//!              | "up" "to" NumericExpression       UpTo(N)     (zero to N)
//! CostQuantity := ChosenRange | PositiveQuantity   (`a`, `an`, N, X)
//! ```
//!
//! The slot is one shared child: a resolution operand (`sacrifice any number
//! of artifacts`) and a cost operand (`remove any number of storage counters
//! from this land`) read the same words, and each parent decides which range
//! its own contract represents. An exact count keeps the shared numeric
//! grammar (`a`, `an`, a literal, X, `that many`, `half X`); this child owns
//! only the ranges a player picks a member of.
//!
//! A cost range is a number the payer chooses while paying (CR 601.2h via
//! 602.2b): the activation declares it (`costChosenAmount`) and pays exactly
//! that many, all or nothing (CR 118.3). Because every cost is paid in full,
//! the chosen number is the number of objects or counters the cost acted on,
//! so a later `for each storage counter removed this way`, `the number of
//! cards exiled this way` or `that many` restates it. The restatement is read
//! only when the activation published the cost that produced it
//! ([`CostChosenAmountSlot`]); without that producer it fails, and a verb an
//! earlier action of the program also produces is left to that action.
use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    ActionResultVerb, NumericAtom, NumericExpression, NumericQuantity, ObjectKind, ObjectPredicate,
    action_result_reference_starts, installed_binding_environment, positive_quantity,
    recognize_action_result_reference, recognize_counter_name, recognize_numeric_expression,
};

/// The declared number a cost range pays: one per activation, chosen by the
/// payer while paying (CR 601.2h via 602.2b).
pub(in crate::oracle::canonical) const COST_CHOSEN_AMOUNT: &str = "costChosenAmount";

/// A range of members the player picks from (the exact count is the shared
/// numeric grammar's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenQuantityRange {
    /// `up to N`: zero to N members.
    UpTo(NumericExpression),
    /// `any number of`: zero or more members.
    AnyNumber,
    /// `one or more`: at least one member.
    OneOrMore,
}

impl ChosenQuantityRange {
    /// The fewest members the range admits.
    pub(in crate::oracle::canonical) const fn minimum(&self) -> i64 {
        match self {
            Self::UpTo(_) | Self::AnyNumber => 0,
            Self::OneOrMore => 1,
        }
    }

    /// The most members the range admits; `None` when only the candidates
    /// bound it.
    pub(in crate::oracle::canonical) fn maximum(&self) -> Option<&NumericExpression> {
        match self {
            Self::UpTo(maximum) => Some(maximum),
            Self::AnyNumber | Self::OneOrMore => None,
        }
    }

    /// Noun agreement: only `up to one` heads a singular noun.
    pub(in crate::oracle::canonical) fn is_plural(&self) -> bool {
        !matches!(
            self,
            Self::UpTo(NumericExpression::Atom(NumericAtom::Integer(1)))
        )
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Lookahead only: a chosen range begins at the cursor. `up to` must be
/// followed by a count, and `one or more` by the operand's own word: `one or
/// more of the following` counts modes, not members.
pub(in crate::oracle::canonical) fn chosen_quantity_range_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_at(cursor, 0, "any") && word_at(cursor, 1, "number"))
        || (word_at(cursor, 0, "one")
            && word_at(cursor, 1, "or")
            && word_at(cursor, 2, "more")
            && cursor
                .peek_ahead(3)
                .is_some_and(|raw| raw.chars().all(char::is_alphanumeric) || raw.starts_with('+'))
            && !word_at(cursor, 3, "of"))
        || (word_at(cursor, 0, "up")
            && word_at(cursor, 1, "to")
            && cursor.peek_ahead(2).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("x")
                    || raw.eq_ignore_ascii_case("that")
                    || raw.eq_ignore_ascii_case("half")
                    || raw.eq_ignore_ascii_case("twice")
                    || parse_number_word(raw).is_some_and(|value| value > 0)
            }))
}

/// Reads one chosen range, or nothing when none begins at the cursor. The
/// count of `up to` is the shared numeric expression. A consumed range never
/// rolls back: a malformed bound fails at this child's state.
pub(in crate::oracle::canonical) fn recognize_chosen_quantity_range(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ChosenQuantityRange>> {
    if !chosen_quantity_range_starts(cursor) {
        return Ok(None);
    }
    cursor.enter("quantity-slot.range");
    if word_at(cursor, 0, "any") {
        cursor.consume_phrase(&["any", "number"])?;
        if word_at(cursor, 0, "of") {
            cursor.consume("of")?;
        }
        return Ok(Some(ChosenQuantityRange::AnyNumber));
    }
    if word_at(cursor, 0, "one") {
        cursor.consume_phrase(&["one", "or", "more"])?;
        return Ok(Some(ChosenQuantityRange::OneOrMore));
    }
    cursor.consume_phrase(&["up", "to"])?;
    cursor.enter("quantity-slot.up-to.bound");
    let maximum = recognize_numeric_expression(cursor, None)?;
    cursor.require(
        !matches!(maximum, NumericExpression::Atom(NumericAtom::Integer(value)) if value <= 0),
        "a positive upper bound after `up to`",
    )?;
    Ok(Some(ChosenQuantityRange::UpTo(maximum)))
}

/// How many members a resolution selection takes: a stated count of the
/// shared numeric grammar, or a range the acting player picks a member of
/// while the effect resolves (CR 608.2d).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SelectionCount {
    Exactly(NumericExpression),
    Range(ChosenQuantityRange),
}

impl SelectionCount {
    /// Noun agreement of the operand that follows.
    pub(in crate::oracle::canonical) fn is_plural(&self) -> bool {
        match self {
            Self::Exactly(count) => *count != NumericExpression::from(1),
            Self::Range(range) => range.is_plural(),
        }
    }

    /// The count a parent that shares one quota across a coordinated
    /// predicate reads (`two artifacts or creatures`): a range is plural.
    pub(in crate::oracle::canonical) fn quota(&self) -> NumericExpression {
        match self {
            Self::Exactly(count) => count.clone(),
            Self::Range(range) if range.is_plural() => NumericExpression::from(2),
            Self::Range(_) => NumericExpression::from(1),
        }
    }
}

/// A stated count compares with the shared expression; a range never
/// equals a stated count.
impl PartialEq<NumericExpression> for SelectionCount {
    fn eq(&self, other: &NumericExpression) -> bool {
        matches!(self, Self::Exactly(count) if count == other)
    }
}

/// The bounds of a number the payer chooses while paying a cost: a literal
/// minimum and, for `up to N`, a literal maximum. Only the payable objects
/// or counters bound an open range (CR 118.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenAmountBounds {
    pub minimum: i64,
    pub maximum: Option<i64>,
}

/// The quantity slot of a counted cost action: exactly a literal or the
/// announced X, or a number the payer chooses while paying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostQuantity {
    Exactly(NumericAtom),
    Chosen(ChosenAmountBounds),
}

impl CostQuantity {
    /// The literal value of an exact count.
    pub(in crate::oracle::canonical) fn literal(self) -> Option<i64> {
        match self {
            Self::Exactly(NumericAtom::Integer(value)) => Some(value),
            Self::Exactly(NumericAtom::VariableX) | Self::Chosen(_) => None,
        }
    }

    /// The exact count, when the text states one.
    pub(in crate::oracle::canonical) fn exact(self) -> Option<NumericAtom> {
        match self {
            Self::Exactly(atom) => Some(atom),
            Self::Chosen(_) => None,
        }
    }

    /// The payer-chosen bounds, when the payer picks the number.
    pub(in crate::oracle::canonical) fn chosen(self) -> Option<ChosenAmountBounds> {
        match self {
            Self::Exactly(_) => None,
            Self::Chosen(bounds) => Some(bounds),
        }
    }

    /// Noun agreement: only an exact one is singular (`up to one` cannot
    /// head a cost: it is read as a chosen range whose noun is plural only
    /// beyond one).
    pub(in crate::oracle::canonical) fn is_plural(self) -> bool {
        match self {
            Self::Exactly(atom) => atom != NumericAtom::Integer(1),
            Self::Chosen(bounds) => bounds.maximum != Some(1),
        }
    }
}

/// An exact cost count compares with the shared atom; a chosen number never
/// equals one.
impl PartialEq<NumericAtom> for CostQuantity {
    fn eq(&self, other: &NumericAtom) -> bool {
        matches!(self, Self::Exactly(atom) if atom == other)
    }
}

/// Reads a cost quantity: a chosen range (`any number of`, `one or more`,
/// `up to N` with a literal N) or the shared positive quantity (`a`, `an`,
/// N, X). An `up to X` cost names two numbers the payer picks and fails.
pub(in crate::oracle::canonical) fn recognize_cost_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    expected: &str,
) -> MachineResult<CostQuantity> {
    let Some(range) = recognize_chosen_quantity_range(cursor)? else {
        return positive_quantity(cursor, expected).map(CostQuantity::Exactly);
    };
    cursor.enter("quantity-slot.cost-bound");
    let maximum = match range.maximum() {
        None => None,
        Some(NumericExpression::Atom(NumericAtom::Integer(value))) => Some(*value),
        Some(_) => {
            return Err(
                cursor.fail("a literal upper bound for a number chosen while paying (`up to N`)")
            );
        }
    };
    Ok(CostQuantity::Chosen(ChosenAmountBounds {
        minimum: range.minimum(),
        maximum,
    }))
}

/// The cost action whose payer-chosen number a later phrase restates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostAmountVerb {
    /// Counters removed from the source (`counters removed this way`).
    Removed,
    Sacrificed,
    Exiled,
    Discarded,
    Tapped,
    Returned,
    Revealed,
}

impl CostAmountVerb {
    /// The restated participle this cost's result shares with an effect's.
    fn from_action_result(verb: ActionResultVerb) -> Option<Self> {
        Some(match verb {
            ActionResultVerb::Sacrificed => Self::Sacrificed,
            ActionResultVerb::Exiled => Self::Exiled,
            ActionResultVerb::Discarded => Self::Discarded,
            ActionResultVerb::Tapped => Self::Tapped,
            ActionResultVerb::Returned => Self::Returned,
            ActionResultVerb::Revealed => Self::Revealed,
            ActionResultVerb::Destroyed
            | ActionResultVerb::Milled
            | ActionResultVerb::DealtDamage => return None,
        })
    }
}

/// The payer-chosen number a cost of the enclosing activation paid: the
/// verb it restates and the noun the cost guarantees (the counter kind or
/// the chosen objects' predicate). Only the activation parent publishes it,
/// from its own typed costs; every other frame leaves it unbound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CostChosenAmountSlot {
    pub verb: CostAmountVerb,
    /// The removed counters' kind (`Removed` only).
    pub counter: Option<CounterName>,
    /// The chosen objects' predicate (object verbs only).
    pub predicate: Option<ObjectPredicate>,
}

fn published_cost_amount(cursor: &OracleTokenCursor<'_>) -> Option<CostChosenAmountSlot> {
    cursor
        .binding_environment()
        .and_then(|environment| environment.frame.cost_chosen_amount.clone())
}

/// The action being read is the first of its program: no earlier action can
/// be the antecedent of a restated result, so only a cost can.
fn reads_first_action(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .is_none_or(|index| index == 0)
}

/// Offset of `removed this way` after `[KIND] counter(s)`; lookahead only.
fn removed_counters_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let noun = (0..4).find(|offset| {
        word_at(cursor, *offset, "counter") || word_at(cursor, *offset, "counters")
    })?;
    (word_at(cursor, noun + 1, "removed")
        && word_at(cursor, noun + 2, "this")
        && word_at(cursor, noun + 3, "way"))
    .then_some(noun + 4)
}

/// Lookahead only: the counted collection restates the payer-chosen number
/// of this activation's cost (`storage counter removed this way`, `cards
/// exiled this way` while no earlier action of the program exists).
pub(in crate::oracle::canonical) fn cost_chosen_amount_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let Some(slot) = published_cost_amount(cursor) else {
        return false;
    };
    if slot.verb == CostAmountVerb::Removed {
        return removed_counters_width(cursor).is_some();
    }
    reads_first_action(cursor) && action_result_reference_starts(cursor)
}

/// A restated noun is guaranteed by the cost's own predicate when it is the
/// generic domain noun, the same predicate, or one operand of its
/// intersection.
fn guaranteed_by(noun: &ObjectPredicate, cost: &ObjectPredicate) -> bool {
    matches!(
        noun,
        ObjectPredicate::ObjectKind(ObjectKind::Permanent | ObjectKind::Card)
    ) || noun == cost
        || matches!(cost, ObjectPredicate::Intersection(operands) if operands.contains(noun))
}

/// Reads `[KIND] counter(s) removed this way` or `NOUN VERBed this way` as
/// the payer-chosen number of this activation's cost. The restated verb must
/// be the cost's, and the restated noun one the cost guarantees.
pub(in crate::oracle::canonical) fn recognize_cost_chosen_amount_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("quantity-slot.cost-result.producer");
    let slot = cursor.require_value(
        published_cost_amount(cursor),
        "a payer-chosen cost number of this activation",
    )?;
    if removed_counters_width(cursor).is_some() {
        cursor.enter("quantity-slot.cost-result.counter-kind");
        let counter = if word_at(cursor, 0, "counter") || word_at(cursor, 0, "counters") {
            None
        } else {
            Some(recognize_counter_name(cursor)?)
        };
        cursor.enter("quantity-slot.cost-result.counter-noun");
        let noun = cursor.peek().unwrap_or_default().to_owned();
        cursor.require(
            noun.eq_ignore_ascii_case("counter") || noun.eq_ignore_ascii_case("counters"),
            "`counter` or `counters`",
        )?;
        cursor.consume(&noun)?;
        cursor.consume_phrase(&["removed", "this", "way"])?;
        cursor.enter("quantity-slot.cost-result.removed");
        cursor.require(
            slot.verb == CostAmountVerb::Removed && (counter.is_none() || counter == slot.counter),
            "the counters this activation's cost removed (the same kind)",
        )?;
        return Ok(cost_chosen_amount());
    }
    let reference = recognize_action_result_reference(cursor)?;
    cursor.enter("quantity-slot.cost-result.verb");
    cursor.require(
        CostAmountVerb::from_action_result(reference.verb) == Some(slot.verb),
        "the verb of this activation's payer-chosen cost",
    )?;
    cursor.enter("quantity-slot.cost-result.noun");
    cursor.require(
        slot.predicate
            .as_ref()
            .is_some_and(|cost| guaranteed_by(&reference.predicate, cost)),
        "a noun this activation's cost guarantees for its chosen objects",
    )?;
    Ok(cost_chosen_amount())
}

/// `that many` / `that much` read by the first action of an activation
/// whose cost paid a payer-chosen number: that number is the only quantity
/// an earlier step produced. `None` leaves the anaphor to the program.
pub(in crate::oracle::canonical) fn cost_chosen_amount_anaphor(
    cursor: &OracleTokenCursor<'_>,
) -> Option<NumericExpression> {
    (published_cost_amount(cursor).is_some() && reads_first_action(cursor)).then(cost_chosen_amount)
}

/// Reads with `slot` published as the activation's payer-chosen cost number,
/// for a parent that reads an action directly instead of through a program
/// (the compact mana activation). The enclosing environment is restored.
pub(in crate::oracle::canonical) fn with_cost_chosen_amount<T>(
    cursor: &mut OracleTokenCursor<'_>,
    slot: Option<CostChosenAmountSlot>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    // Without a payer-chosen number the enclosing environment is unchanged.
    if slot.is_none() {
        return read(cursor);
    }
    let mut environment = installed_binding_environment(cursor);
    environment.frame.cost_chosen_amount = slot;
    let enclosing = cursor.replace_binding_environment(Some(std::sync::Arc::new(environment)));
    let result = read(cursor);
    cursor.replace_binding_environment(enclosing);
    result
}

/// The payer-chosen cost number as a numeric expression.
pub(in crate::oracle::canonical) fn cost_chosen_amount() -> NumericExpression {
    NumericExpression::Quantity(Box::new(NumericQuantity::CostChosenAmount))
}

#[cfg(test)]
#[path = "quantity_slot_tests.rs"]
mod tests;
