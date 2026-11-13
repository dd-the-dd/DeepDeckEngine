use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, NumericExpression, NumericRounding, TargetBinding,
    recognize_numeric_primary_expression,
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
    cursor.enter("numeric.arithmetic.start");
    let expression = recognize_sum(cursor, antecedent)?;
    cursor.enter("numeric.arithmetic.complete");
    Ok(expression)
}

fn recognize_sum(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.sum.left");
    let mut expression = recognize_product(cursor, antecedent)?;
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
        let right = recognize_product(cursor, antecedent)?;
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
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.product.left");
    let mut expression = recognize_primary(cursor, antecedent)?;
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
        let right = recognize_primary(cursor, antecedent)?;
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
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.arithmetic.primary");
    if cursor.peek().is_some_and(|raw| raw == "(") {
        require_separator(cursor, "(")?;
        cursor.enter("numeric.arithmetic.parenthesized.expression");
        let expression = recognize_sum(cursor, antecedent)?;
        cursor.enter("numeric.arithmetic.parenthesized.close");
        require_separator(cursor, ")")?;
        return Ok(expression);
    }
    recognize_numeric_primary_expression(cursor, antecedent)
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
