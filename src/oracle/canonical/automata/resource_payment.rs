//! Shared resource syntax; callers supply the payer and capability context.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActingSubject, ArgumentFrame, ManaCostNode, NumericAtom, PayLifeCostNode, VerbLemma,
    recognize_mana_cost, recognize_numeric_atom, recognize_operand_verb, recognize_pay_life_cost,
    recognize_pay_life_cost_with_verb,
};
use crate::oracle::counter_name::CounterName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ResourcePaymentNode {
    Mana(ManaCostNode),
    Life(PayLifeCostNode),
    /// Counters the payer removes from themself (CR 122.1, 107.14).
    PlayerCounters(PlayerCounterPaymentNode),
}

/// `pay {E}{E}` / `pay eight {E}`: N counters of one kind removed from the
/// payer. A payer without N such counters cannot pay (CR 118.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerCounterPaymentNode {
    pub counter: CounterName,
    pub count: i64,
}

impl ResourcePaymentNode {
    pub(in crate::oracle::canonical) fn same_resource(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Mana(left), Self::Mana(right)) => left.same_symbols(right),
            (Self::Life(left), Self::Life(right)) => left.amount == right.amount,
            (Self::PlayerCounters(left), Self::PlayerCounters(right)) => left == right,
            _ => false,
        }
    }
}

pub(in crate::oracle::canonical) fn recognize_resource_payment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResourcePaymentNode> {
    cursor.enter("cost.resource.kind");
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        recognize_mana_cost(cursor).map(ResourcePaymentNode::Mana)
    } else {
        recognize_pay_life_cost(cursor).map(ResourcePaymentNode::Life)
    }
}

/// Third-person verb belongs to the same payment child, not to a reminder's
/// hardcoded numeric parser. Mana and life keep their preserved typed data.
pub(in crate::oracle::canonical) fn recognize_resource_payment_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ResourcePaymentNode> {
    cursor.enter("cost.resource.predicate");
    let checkpoint = cursor.checkpoint();
    let verb = recognize_operand_verb(
        cursor,
        ActingSubject::THIRD_SINGULAR.agreement,
        &[VerbLemma::Pay],
        &[ArgumentFrame::ResourcePayment],
    )?;
    cursor.require(verb.is_some(), "`pays`")?;
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        recognize_mana_cost(cursor).map(ResourcePaymentNode::Mana)
    } else {
        cursor.rollback(checkpoint);
        recognize_pay_life_cost_with_verb(cursor, ActingSubject::THIRD_SINGULAR)
            .map(ResourcePaymentNode::Life)
    }
}

/// Lookahead only: a player-counter symbol run (`{E}{E}`) or a quantity
/// followed by one symbol (`eight {E}`).
pub(in crate::oracle::canonical) fn player_counter_payment_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let type_at = |offset: usize, token_type: &str| {
        let start = cursor.position() + offset;
        cursor
            .token_slice(start, start + 1)
            .first()
            .is_some_and(|token| token.token_type == token_type)
    };
    type_at(0, "counterSymbol") || (type_at(0, "quantity") && type_at(1, "counterSymbol"))
}

/// Reads the counters of a player-counter payment. Each `{E}` symbol is one
/// energy counter (CR 107.14); a written quantity counts one symbol. Only the
/// registered energy symbol is a player-counter symbol.
pub(in crate::oracle::canonical) fn recognize_player_counter_payment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerCounterPaymentNode> {
    cursor.enter("cost.player-counters.quantity");
    let written = if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity")
    {
        let NumericAtom::Integer(count) = recognize_numeric_atom(cursor)? else {
            return Err(cursor.fail("a literal quantity of counter symbols"));
        };
        cursor.require(count > 0, "a positive quantity of counter symbols")?;
        Some(count)
    } else {
        None
    };
    cursor.enter("cost.player-counters.symbol");
    let mut symbols = 0_i64;
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        let symbol = cursor.consume_type("counterSymbol")?;
        cursor.require(
            symbol.raw.eq_ignore_ascii_case("{E}"),
            "the registered energy-counter symbol `{E}`",
        )?;
        symbols += 1;
    }
    cursor.require(symbols > 0, "an energy-counter symbol `{E}`")?;
    let count = match written {
        Some(count) => {
            cursor.require(symbols == 1, "one counter symbol after a written quantity")?;
            count
        }
        None => symbols,
    };
    cursor.enter("cost.player-counters.complete");
    Ok(PlayerCounterPaymentNode {
        counter: CounterName::from_parts(["energy"]).expect("a non-empty counter name"),
        count,
    })
}
