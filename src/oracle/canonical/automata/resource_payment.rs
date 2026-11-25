//! Shared resource syntax; callers supply the payer and capability context.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActingSubject, ArgumentFrame, ManaCostNode, NumericAtom, NumericExpression, PayLifeCostNode,
    VerbLemma, recognize_mana_cost, recognize_numeric_atom, recognize_operand_verb,
    recognize_pay_life_cost, recognize_pay_life_cost_with_verb,
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

/// `[PLAYER] pay(s) RESOURCE` as an ordinary instruction of a program (CR
/// 608.2c, 119.4, 107.14): the payer pays if able; an impossible payment
/// performs nothing, so `If you do` / `If you don't` and the offer outcomes
/// observe whether it was actually paid. Any parent that reads an
/// instruction (a permission body, a sentence condition, an `Otherwise`
/// branch, a delayed trigger, an offer) reads it with the same reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentInstructionNode {
    /// A fixed payment of the shared resource child: mana, `N life` or
    /// `{E}{E}` / `eight {E}`.
    Fixed(ResourcePaymentNode),
    /// A counted resource whose amount is an expression or a number the
    /// payer chooses while the instruction resolves.
    Amount {
        resource: PaidResource,
        amount: PaidAmount,
    },
}

/// The counted resource of a payment amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaidResource {
    /// The payer's life (CR 119.4).
    Life,
    /// Counters of one kind on the payer (CR 122.1, 107.14).
    PlayerCounters(CounterName),
}

/// How many of the resource a payment instruction pays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaidAmount {
    /// `life equal to EXPR`, `half your life, rounded up`, `an amount of
    /// {E} equal to EXPR`: the shared numeric expression.
    Expression(NumericExpression),
    /// `any amount of R` (zero or more) / `one or more R`: a number the
    /// payer chooses as it resolves, at most what the payer has. The chosen
    /// number is the amount a later `that much` restates.
    Chosen { minimum: i64 },
}

impl PaymentInstructionNode {
    /// The numeric expression the payment's amount reads, if it has one.
    pub(in crate::oracle::canonical) fn amount_expression(&self) -> Option<&NumericExpression> {
        match self {
            Self::Amount {
                amount: PaidAmount::Expression(expression),
                ..
            } => Some(expression),
            Self::Fixed(_)
            | Self::Amount {
                amount: PaidAmount::Chosen { .. },
                ..
            } => None,
        }
    }

    /// `pay any amount of R`: a payment whose chosen amount admits zero, so
    /// paying it is never more than the payer's own choice of how much (a
    /// written permission over it adds no decision, CR 608.2d).
    pub(in crate::oracle::canonical) fn admits_zero(&self) -> bool {
        matches!(
            self,
            Self::Amount {
                amount: PaidAmount::Chosen { minimum: 0 },
                ..
            }
        )
    }
}

/// Lookahead only: the payment verb in `agreement` at the cursor.
pub(in crate::oracle::canonical) fn payment_instruction_starts(
    cursor: &OracleTokenCursor<'_>,
    agreement: super::VerbAgreement,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case(super::verb_entry(VerbLemma::Pay).surface(agreement.form()))
    }) && cursor.peek_ahead(1).is_some()
}

/// `pay(s) MANA | N life | {E}... | any amount of R | one or more R | an
/// amount of R equal to EXPR | life equal to EXPR | AMOUNT life`, the verb in
/// the payer's agreement. `antecedent` is the default object of anaphora in
/// an amount (`that permanent's mana value`).
pub(in crate::oracle::canonical) fn recognize_payment_instruction_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    agreement: super::VerbAgreement,
    antecedent: Option<super::TargetBinding>,
) -> MachineResult<PaymentInstructionNode> {
    cursor.enter("player-action.pay.verb");
    cursor.consume(super::verb_entry(VerbLemma::Pay).surface(agreement.form()))?;
    cursor.enter("player-action.pay.resource");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        let mana = recognize_mana_cost(cursor)?;
        // `pay {X}` names a number no producer of this instruction declares
        // (CR 107.3): it is never read as zero.
        cursor.enter("player-action.pay.mana-variable");
        cursor.require(
            !mana.contains_variable_x(),
            "a mana payment without X (no declaration produces it here)",
        )?;
        return Ok(PaymentInstructionNode::Fixed(ResourcePaymentNode::Mana(
            mana,
        )));
    }
    if player_counter_payment_starts(cursor) {
        return recognize_player_counter_payment(cursor).map(|counters| {
            PaymentInstructionNode::Fixed(ResourcePaymentNode::PlayerCounters(counters))
        });
    }
    // `any amount of R` / `one or more R`: a number chosen as it resolves.
    let chosen = if word(cursor, 0, "any") && word(cursor, 1, "amount") && word(cursor, 2, "of") {
        Some((3, 0))
    } else if word(cursor, 0, "one") && word(cursor, 1, "or") && word(cursor, 2, "more") {
        Some((3, 1))
    } else {
        None
    };
    if let Some((width, minimum)) = chosen {
        cursor.enter("player-action.pay.chosen-amount");
        cursor.advance_to(cursor.position() + width)?;
        let resource = recognize_paid_resource(cursor)?;
        return Ok(PaymentInstructionNode::Amount {
            resource,
            amount: PaidAmount::Chosen { minimum },
        });
    }
    // `an amount of R equal to EXPR`.
    if word(cursor, 0, "an") && word(cursor, 1, "amount") && word(cursor, 2, "of") {
        cursor.enter("player-action.pay.equal-amount");
        cursor.consume_phrase(&["an", "amount", "of"])?;
        let resource = recognize_paid_resource(cursor)?;
        cursor.enter("player-action.pay.equal-amount.expression");
        cursor.require(word(cursor, 0, "equal"), "`equal to` an amount")?;
        let expression = super::recognize_numeric_expression(cursor, antecedent)?;
        return Ok(PaymentInstructionNode::Amount {
            resource,
            amount: PaidAmount::Expression(expression),
        });
    }
    // `life equal to EXPR`.
    if word(cursor, 0, "life") {
        cursor.enter("player-action.pay.life-equal");
        cursor.consume("life")?;
        cursor.require(word(cursor, 0, "equal"), "`equal to` an amount of life")?;
        let expression = super::recognize_numeric_expression(cursor, antecedent)?;
        return Ok(PaymentInstructionNode::Amount {
            resource: PaidResource::Life,
            amount: PaidAmount::Expression(expression),
        });
    }
    // `N life` (the shared literal life payment) or `AMOUNT life` / `half
    // your life, rounded up` (an amount whose operand already names life).
    cursor.enter("player-action.pay.life-amount");
    let amount_start = cursor.position();
    let expression = super::recognize_numeric_expression(cursor, antecedent)?;
    let names_life = cursor
        .token_slice(amount_start, cursor.position())
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("life"));
    if !names_life {
        cursor.enter("player-action.pay.life-resource");
        cursor.consume("life")?;
    }
    Ok(match expression {
        NumericExpression::Atom(NumericAtom::Integer(amount)) if amount > 0 => {
            PaymentInstructionNode::Fixed(ResourcePaymentNode::Life(PayLifeCostNode { amount }))
        }
        expression => PaymentInstructionNode::Amount {
            resource: PaidResource::Life,
            amount: PaidAmount::Expression(expression),
        },
    })
}

/// `life` or one player-counter symbol (`{E}`) after an amount phrase.
fn recognize_paid_resource(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<PaidResource> {
    cursor.enter("player-action.pay.counted-resource");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("life"))
    {
        cursor.consume("life")?;
        return Ok(PaidResource::Life);
    }
    let symbol = cursor.consume_type("counterSymbol")?;
    cursor.require(
        symbol.raw.eq_ignore_ascii_case("{E}"),
        "`life` or the registered energy-counter symbol `{E}`",
    )?;
    Ok(PaidResource::PlayerCounters(
        CounterName::from_parts(["energy"]).expect("a non-empty counter name"),
    ))
}

/// Lookahead only: `RESOURCE paid this way` (`{E} paid this way`, `life
/// paid this way`): the amount an earlier payment instruction of the same
/// program paid (CR 608.2c).
pub(in crate::oracle::canonical) fn paid_resource_result_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let resource = cursor.peek_token().is_some_and(|token| {
        token.token_type == "counterSymbol" || token.raw.eq_ignore_ascii_case("life")
    });
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    resource && word(1, "paid") && word(2, "this") && word(3, "way")
}

/// Reads `RESOURCE paid this way` as the amount the immediately preceding
/// instruction paid: the shared action-result quantity, bound by the
/// program lowering to the payment's chosen amount, or failing explicitly
/// when that instruction recorded none. The first action of a program has
/// no antecedent.
pub(in crate::oracle::canonical) fn recognize_paid_resource_result(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.paid-this-way.resource");
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterSymbol")
    {
        let symbol = cursor.consume_type("counterSymbol")?;
        cursor.require(
            symbol.raw.eq_ignore_ascii_case("{E}"),
            "the registered energy-counter symbol `{E}`",
        )?;
    } else {
        cursor.consume("life")?;
    }
    cursor.enter("numeric.paid-this-way.verb");
    cursor.consume_phrase(&["paid", "this", "way"])?;
    cursor.enter("numeric.paid-this-way.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier payment instruction of this program whose amount the phrase restates",
    )?;
    Ok(NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: super::ActionResultQuantity::Amount,
    })
}
