use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, LocalObjectReference, NumericAtom, NumericExpression, NumericRounding,
    TargetBinding, recognize_numeric_primary_expression, recognize_that_many_event_value,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// Recognizes one complete arithmetic expression with conventional
/// precedence and left associativity. The surrounding parent owns any noun or
/// clause following the expression.
pub(in crate::oracle::canonical) fn recognize_arithmetic_expression(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        antecedent,
        None,
        NumericExpressionReadContext::ordinary(),
    )
}

/// Reuses the same precedence grammar with a parent-owned event quantity.
/// The binding is never inferred from the words `that many` themselves.
pub(in crate::oracle::canonical) fn recognize_event_bound_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
    binding: EventNumericBinding,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        None,
        Some(binding),
        NumericExpressionReadContext::ordinary(),
    )
}

/// Query consumers opt into source properties using explicit availability proof.
/// Precedence, operators, rounding and separators remain owned by one grammar.
pub(in crate::oracle::canonical) fn recognize_query_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        None,
        None,
        NumericExpressionReadContext {
            kind: NumericReadKind::Query,
            subject: None,
        },
    )
}

/// Reads the right-hand side of an object-query comparison (`with power
/// greater than EXPR`). It is the definition grammar (counts, player facts,
/// sacrificed-cost and source properties, devotion, arithmetic) without the
/// possessive families that name an antecedent: inside a candidate filter,
/// `its power` is the candidate under test, never the environment's object,
/// so the comparison parent reads (or refuses) it itself.
pub(in crate::oracle::canonical) fn recognize_comparison_rhs_expression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        None,
        None,
        NumericExpressionReadContext {
            kind: NumericReadKind::Comparison,
            subject: None,
        },
    )
}

/// Reads the value of a postposed variable definition (`where X is EXPR`).
/// It is the query grammar plus the `the number of COUNTABLE` quantity; the
/// parent that owns the sentence decides which actions the value binds.
pub(in crate::oracle::canonical) fn recognize_variable_definition_expression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        None,
        None,
        NumericExpressionReadContext {
            kind: NumericReadKind::Definition,
            subject: None,
        },
    )
}

/// Reads the quantity of an `ACTION equal to EXPR` frame (damage, cards
/// drawn, ...). It is the complete definition grammar; the parent action may
/// additionally supply its own subject, the only antecedent of a possessive
/// `its` inside that frame (`it deals damage equal to its power`).
pub(in crate::oracle::canonical) fn recognize_equal_to_amount_expression(
    cursor: &mut OracleTokenCursor<'_>,
    subject: Option<LocalObjectReference>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_with_event_binding(
        cursor,
        None,
        None,
        NumericExpressionReadContext {
            kind: NumericReadKind::Definition,
            subject,
        },
    )
}

/// Lexical lookahead for the quantity families an `equal to` amount offers
/// beyond a referenced object's property. Each child still proves itself.
pub(in crate::oracle::canonical) fn equal_to_quantity_family_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    super::sacrificed_cost_object_property_starts(cursor)
        || super::greatest_property_among_starts(cursor)
        || super::number_of_count_starts(cursor)
        || super::action_result_quantity_starts(cursor)
        || super::mana_spent_amount_starts(cursor)
        || super::history_life_amount_starts(cursor)
        || super::event_damage_amount_starts(cursor)
        || super::excess_damage_result_starts(cursor)
        || super::random_result_starts(cursor)
        || devotion_starts(cursor)
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("half"))
        || cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("twice") || raw.eq_ignore_ascii_case("thrice")
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumericReadKind {
    Ordinary,
    Query,
    Definition,
    /// A comparison right-hand side: the definition families without the
    /// antecedent possessives and without program action results.
    Comparison,
}

impl NumericReadKind {
    /// Whether the definition quantity families (`the number of`, player
    /// facts, sacrificed-cost properties, devotion) are readable.
    fn reads_definition_families(self) -> bool {
        matches!(self, Self::Definition | Self::Comparison)
    }
}

/// Which quantity families a consumer can evaluate, plus the parent-owned
/// subject antecedent. Precedence and operators never depend on it.
#[derive(Clone, Copy)]
struct NumericExpressionReadContext {
    kind: NumericReadKind,
    subject: Option<LocalObjectReference>,
}

impl NumericExpressionReadContext {
    fn ordinary() -> Self {
        Self {
            kind: NumericReadKind::Ordinary,
            subject: None,
        }
    }
}

fn recognize_arithmetic_with_event_binding(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    event_binding: Option<EventNumericBinding>,
    query_context: NumericExpressionReadContext,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.start");
    let expression = recognize_sum(cursor, antecedent, event_binding, query_context)?;
    // `A or B, whichever is greater` (CR 107.1): the greater operand.
    if super::whichever_is_greater_follows(cursor) {
        cursor.enter("numeric.arithmetic.greatest-of");
        cursor.consume("or")?;
        let right = recognize_sum(cursor, antecedent, event_binding, query_context)?;
        let expression = super::recognize_whichever_is_greater_close(cursor, expression, right)?;
        cursor.enter("numeric.arithmetic.complete");
        return Ok(expression);
    }
    cursor.enter("numeric.arithmetic.complete");
    Ok(expression)
}

fn recognize_sum(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    event_binding: Option<EventNumericBinding>,
    query_context: NumericExpressionReadContext,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.sum.left");
    let mut expression = recognize_product(cursor, antecedent, event_binding, query_context)?;
    loop {
        cursor.enter("numeric.arithmetic.sum.operator-or-complete");
        let Some(operator @ (ArithmeticOperator::Add | ArithmeticOperator::Subtract)) =
            peek_operator(cursor)?
        else {
            return Ok(expression);
        };
        consume_operator(cursor, operator)?;
        cursor.enter("numeric.arithmetic.sum.right");
        cursor.require(
            cursor.peek().is_some(),
            "a right operand after the additive operator",
        )?;
        let right = recognize_product(cursor, antecedent, event_binding, query_context)?;
        expression = match operator {
            ArithmeticOperator::Add => NumericExpression::add(expression, right),
            ArithmeticOperator::Subtract => NumericExpression::subtract(expression, right),
            ArithmeticOperator::Multiply | ArithmeticOperator::Divide => {
                unreachable!("the sum loop only accepts additive operators")
            }
        };
    }
}

fn recognize_product(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    event_binding: Option<EventNumericBinding>,
    query_context: NumericExpressionReadContext,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.product.left");
    let mut expression = recognize_primary(cursor, antecedent, event_binding, query_context)?;
    loop {
        cursor.enter("numeric.arithmetic.product.operator-or-complete");
        let Some(operator @ (ArithmeticOperator::Multiply | ArithmeticOperator::Divide)) =
            peek_operator(cursor)?
        else {
            return Ok(expression);
        };
        consume_operator(cursor, operator)?;
        cursor.enter("numeric.arithmetic.product.right");
        cursor.require(
            cursor.peek().is_some(),
            "a right operand after the multiplicative operator",
        )?;
        let right = recognize_primary(cursor, antecedent, event_binding, query_context)?;
        expression = match operator {
            ArithmeticOperator::Multiply => NumericExpression::multiply(expression, right),
            ArithmeticOperator::Divide => {
                cursor.enter("numeric.arithmetic.division.nonzero");
                cursor.require(
                    !matches!(
                        right,
                        NumericExpression::Atom(NumericAtom::Integer(value)) if value <= 0
                    ),
                    "a positive literal divisor",
                )?;
                let rounding = recognize_optional_rounding(cursor)?;
                NumericExpression::divide(expression, right, rounding)
                    .expect("the positive literal divisor was validated before construction")
            }
            ArithmeticOperator::Add | ArithmeticOperator::Subtract => {
                unreachable!("the product loop only accepts multiplicative operators")
            }
        };
    }
}

fn recognize_primary(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    event_binding: Option<EventNumericBinding>,
    query_context: NumericExpressionReadContext,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.primary");
    // `that NOUN's PROPERTY`: the genitive of a bound object.
    if super::bound_object_property_starts(cursor) {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_bound_object_property(cursor, &environment);
    }
    // `the power of that creature`, `the mana value of the exiled card`:
    // the postposed genitive of the same bound object.
    if super::of_genitive_property_starts(cursor) {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_of_genitive_property(cursor, &environment);
    }
    // `that number`: a number the enclosing frame already read, resolved
    // by the binding environment like every other anaphor.
    if super::prior_number_reference_starts(cursor) {
        return super::recognize_prior_number_reference(cursor);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
    {
        return recognize_that_many_event_value(cursor, event_binding);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("twice") || raw.eq_ignore_ascii_case("thrice"))
    {
        cursor.enter("numeric.arithmetic.multiplier-prefix");
        let token = cursor.consume_type("numericOperatorWord")?;
        let factor = if token.raw.eq_ignore_ascii_case("twice") {
            2
        } else {
            3
        };
        let operand = recognize_primary(cursor, antecedent, event_binding, query_context)?;
        return Ok(NumericExpression::multiply(
            NumericExpression::from(factor),
            operand,
        ));
    }
    if cursor.peek().is_some_and(|raw| raw == "(") {
        require_separator(cursor, "(")?;
        cursor.enter("numeric.arithmetic.parenthesized.expression");
        let expression = recognize_sum(cursor, antecedent, event_binding, query_context)?;
        cursor.enter("numeric.arithmetic.parenthesized.close");
        require_separator(cursor, ")")?;
        return Ok(expression);
    }
    if let Some(subject) = query_context.subject
        && super::subject_possessive_property_starts(cursor)
    {
        return super::recognize_subject_possessive_property(cursor, subject);
    }
    // `where X is its power`: without a parent subject, the possessive
    // names what the binding environment binds to `it`.
    if query_context.kind == NumericReadKind::Definition
        && query_context.subject.is_none()
        && super::environment_possessive_property_starts(cursor)
    {
        let environment = super::installed_binding_environment(cursor);
        return super::recognize_environment_possessive_property(cursor, &environment);
    }
    if matches!(
        query_context.kind,
        NumericReadKind::Query | NumericReadKind::Definition | NumericReadKind::Comparison
    ) {
        // The latest random outcome (`the result`, `the number of coins
        // that came up heads`) before the generic `the number of` count.
        if query_context.kind == NumericReadKind::Definition && super::random_result_starts(cursor)
        {
            return super::recognize_random_result(cursor);
        }
        if query_context.kind.reads_definition_families()
            && super::player_numeric_fact_starts(cursor)
        {
            return super::recognize_player_numeric_fact(cursor);
        }
        if query_context.kind.reads_definition_families()
            && super::sacrificed_cost_object_property_starts(cursor)
        {
            return super::recognize_sacrificed_cost_object_property(cursor);
        }
        if query_context.kind.reads_definition_families()
            && super::greatest_property_among_starts(cursor)
        {
            return super::recognize_greatest_property_among(cursor);
        }
        // `your highest commander tax among your commanders` (CR 903.8).
        if query_context.kind.reads_definition_families()
            && super::highest_commander_tax_starts(cursor)
        {
            return super::recognize_highest_commander_tax(cursor);
        }
        // `the difference`: the amount by which the preceding condition's
        // comparison holds (`comparison_difference.rs`).
        if query_context.kind == NumericReadKind::Definition
            && super::comparison_difference_starts(cursor)
        {
            return super::recognize_comparison_difference(cursor);
        }
        // `the number of times this ability has resolved this turn`: the
        // resolving ability's own record, before the collection count.
        if query_context.kind == NumericReadKind::Definition
            && super::ability_history_count_starts(cursor)
        {
            return super::recognize_ability_history_count(cursor);
        }
        if query_context.kind.reads_definition_families() && super::property_total_starts(cursor) {
            return super::recognize_property_total(
                cursor,
                query_context.kind == NumericReadKind::Definition,
            );
        }
        if query_context.kind.reads_definition_families()
            && super::number_of_count_starts(cursor)
            && !super::source_counter_count_starts(cursor)
        {
            return super::recognize_number_of_count(cursor);
        }
        if super::source_counter_count_starts(cursor) {
            return super::recognize_source_counter_count(cursor);
        }
        if query_context.kind == NumericReadKind::Definition
            && super::action_result_quantity_starts(cursor)
        {
            return super::recognize_action_result_quantity(cursor);
        }
        if query_context.kind.reads_definition_families() && devotion_starts(cursor) {
            return recognize_devotion(cursor);
        }
        // Cast records, turn history and event amounts (CR 601.2h, 119.3,
        // 120.1): definition quantities of one shared reader each.
        if query_context.kind == NumericReadKind::Definition {
            if super::mana_spent_amount_starts(cursor) {
                return super::recognize_mana_spent_to_cast_amount(cursor);
            }
            if super::history_life_amount_starts(cursor) {
                return super::recognize_history_life_amount(cursor);
            }
            if super::event_damage_amount_starts(cursor) {
                return super::recognize_event_damage_amount(cursor);
            }
            if super::excess_damage_result_starts(cursor) {
                return super::recognize_excess_damage_result(cursor);
            }
        }
    }
    if cursor
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("half"))
    {
        return recognize_half(cursor, antecedent, event_binding, query_context);
    }
    recognize_numeric_primary_expression(cursor, antecedent)
}

/// `half OPERAND, rounded up|down` (CR 107.1a): half of an operand of this
/// grammar, with the rounding the text states, in every consumer's slot
/// (`You lose half your life, rounded up`). The operand is a player fact
/// (`your life`), a `the number of COUNTABLE` aggregate, or any primary the
/// consumer reads. Without a stated rounding the expression is refused.
fn recognize_half(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
    event_binding: Option<EventNumericBinding>,
    query_context: NumericExpressionReadContext,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.half");
    cursor.consume("half")?;
    cursor.enter("numeric.arithmetic.half.operand");
    // `half the number of cards in your library`: the same `the number of
    // COUNTABLE` aggregate every definition reads, over the zone-holder
    // domain of the shared scoped query.
    let operand = if super::number_of_count_starts(cursor) {
        super::recognize_number_of_count(cursor)?
    } else if super::acting_player_zone_starts(cursor) {
        // `half their library`: the acting player's cards in that zone.
        super::recognize_acting_player_zone(cursor)?
    } else if super::player_numeric_fact_starts(cursor) {
        super::recognize_player_numeric_fact(cursor)?
    } else {
        recognize_primary(cursor, antecedent, event_binding, query_context)?
    };
    cursor.enter("numeric.arithmetic.half.rounding");
    let rounding = recognize_stated_rounding(cursor)?;
    Ok(NumericExpression::Divide {
        left: Box::new(operand),
        right: Box::new(NumericExpression::from(2)),
        rounding,
    })
}

/// `, rounded up|down` (CR 107.1a): a fraction is rounded as the text
/// says; without a stated rounding the expression is refused.
fn recognize_stated_rounding(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericRounding> {
    if cursor.peek().is_some_and(|raw| raw == ",") {
        require_separator(cursor, ",")?;
    }
    cursor.consume("rounded")?;
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("up") => {
            cursor.consume("up")?;
            Ok(NumericRounding::Up)
        }
        Some("down") => {
            cursor.consume("down")?;
            Ok(NumericRounding::Down)
        }
        _ => Err(cursor.fail("`rounded up` or `rounded down`")),
    }
}

fn devotion_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["your", "devotion", "to"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// `your devotion to COLOR` (CR 700.5). A devotion to several colors counts
/// each hybrid symbol once (CR 700.5a) and is not read here.
fn recognize_devotion(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.devotion");
    cursor.consume_phrase(&["your", "devotion", "to"])?;
    cursor.enter("numeric.devotion.color");
    let color = cursor
        .peek()
        .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
        .ok_or_else(|| cursor.fail("a color"))?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    cursor.require(
        !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and")),
        "a devotion to one color",
    )?;
    Ok(NumericExpression::Devotion(color))
}

fn peek_operator(cursor: &OracleTokenCursor<'_>) -> MachineResult<Option<ArithmeticOperator>> {
    let Some(token) = cursor.peek_token() else {
        return Ok(None);
    };
    let normalized = token.raw.to_ascii_lowercase();
    let operator = match normalized.as_str() {
        "+" | "plus" => ArithmeticOperator::Add,
        "-" | "−" | "minus" => ArithmeticOperator::Subtract,
        "*" | "times" | "multiplied" => ArithmeticOperator::Multiply,
        "/" | "divided" => ArithmeticOperator::Divide,
        _ => return Ok(None),
    };
    let expected_type = if matches!(normalized.as_str(), "+" | "-" | "−" | "*" | "/") {
        "separator"
    } else {
        "numericOperatorWord"
    };
    cursor.require(
        token.token_type == expected_type,
        "a lexically typed arithmetic operator",
    )?;
    Ok(Some(operator))
}

fn consume_operator(
    cursor: &mut OracleTokenCursor<'_>,
    operator: ArithmeticOperator,
) -> MachineResult<()> {
    cursor.enter("numeric.arithmetic.operator");
    match operator {
        ArithmeticOperator::Add => consume_one_of(cursor, &["+", "plus"]),
        ArithmeticOperator::Subtract => consume_one_of(cursor, &["-", "−", "minus"]),
        ArithmeticOperator::Multiply => {
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("multiplied"))
            {
                cursor.consume_phrase(&["multiplied", "by"])
            } else {
                consume_one_of(cursor, &["*", "times"])
            }
        }
        ArithmeticOperator::Divide => {
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("divided"))
            {
                cursor.consume_phrase(&["divided", "by"])
            } else {
                consume_one_of(cursor, &["/"])
            }
        }
    }
}

fn consume_one_of(cursor: &mut OracleTokenCursor<'_>, expected: &[&str]) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("an arithmetic operator"))?
        .to_string();
    cursor.require(
        expected
            .iter()
            .any(|candidate| raw.eq_ignore_ascii_case(candidate)),
        "an arithmetic operator accepted at this precedence",
    )?;
    cursor.consume(&raw)
}

fn recognize_optional_rounding(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericRounding> {
    cursor.enter("numeric.arithmetic.division.rounding-or-complete");
    let comma_before_rounding = cursor.peek().is_some_and(|raw| raw == ",")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("rounded"));
    if comma_before_rounding {
        cursor.consume(",")?;
    }
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("rounded"))
    {
        // Comprehensive Rules 107.1b rounds integer division down unless the
        // effect explicitly says otherwise. Retain that policy in the typed
        // expression so the canonical IR never emits an implicit rounding
        // contract.
        return Ok(NumericRounding::Down);
    }
    cursor.enter("numeric.arithmetic.division.rounding");
    let rounded = cursor.consume_type("roundingWord")?;
    cursor.require(
        rounded.raw.eq_ignore_ascii_case("rounded"),
        "the `rounded` arithmetic qualifier",
    )?;
    cursor.enter("numeric.arithmetic.division.rounding-direction");
    let direction = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`up` or `down` after `rounded`"))?
        .to_string();
    let policy = if direction.eq_ignore_ascii_case("up") {
        NumericRounding::Up
    } else if direction.eq_ignore_ascii_case("down") {
        NumericRounding::Down
    } else {
        return Err(cursor.fail("`up` or `down` after `rounded`"));
    };
    cursor.consume(&direction)?;
    Ok(policy)
}

fn require_separator(cursor: &mut OracleTokenCursor<'_>, raw: &str) -> MachineResult<()> {
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("a parenthesis token"))?;
    cursor.require(
        token.raw == raw && token.token_type == "separator",
        "a lexically typed parenthesis token",
    )?;
    cursor.consume(raw)
}

#[cfg(test)]
#[path = "arithmetic_expression_tests.rs"]
mod tests;
