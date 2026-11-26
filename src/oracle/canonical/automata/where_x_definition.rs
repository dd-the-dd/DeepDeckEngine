//! Postposed numeric definition `, where X is EXPR` (CR 107.3).
//!
//! The sentence parent owns the scope: it records which of its actions precede
//! the clause and binds the value for all of them. This child owns only the
//! clause itself, and reads `EXPR` with the shared numeric expression grammar.
//! It never chooses an action, never lowers, and never substitutes zero for an
//! expression it cannot represent.
//!
//! ```text
//! WhereX := "," "where" "X" "is" Expression
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericExpression, recognize_variable_definition_expression};

/// Binding of the variable X for the actions `first_effect..end_effect` of an
/// effect sequence. The expression is evaluated once, when the sentence's
/// first bound action resolves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct WhereXBindingNode {
    pub first_effect: usize,
    pub end_effect: usize,
    pub expression: NumericExpression,
}

impl WhereXBindingNode {
    /// The defining sentence or a later one: X keeps the defined value for
    /// the rest of the program (CR 107.3), up to another definition.
    pub(in crate::oracle::canonical) fn reaches(&self, effect_index: usize) -> bool {
        self.first_effect <= effect_index
    }
}

/// True at `, where X is`: the comma, `where`, the variable and the copula.
pub(in crate::oracle::canonical) fn where_x_definition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    [",", "where", "x", "is"]
        .iter()
        .enumerate()
        .all(|(i, word)| {
            cursor
                .peek_ahead(i)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads the optional parenthesized reminder that restates the rule of a
/// defined amount, after the sentence terminator. Only a reminder that
/// restates the amount actually read is accepted: `(Each {B} in the mana
/// costs of permanents you control counts toward your devotion to black.)`
/// after `your devotion to black` (CR 700.5). Any other parenthesis fails;
/// a reminder never changes the amount.
pub(in crate::oracle::canonical) fn recognize_optional_amount_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    amount: &NumericExpression,
) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    cursor.enter("reminder.amount.kind");
    let NumericExpression::Devotion(colors) = amount else {
        return Err(cursor.fail("a reminder restating the defined amount (devotion)"));
    };
    // Only the one-color reminder is read; a multicolor devotion reminder
    // (CR 700.5a) fails here rather than being approximated.
    let [color] = colors.as_slice() else {
        return Err(cursor.fail("a single-color devotion reminder"));
    };
    cursor.enter("reminder.devotion.open");
    cursor.consume_phrase(&["(", "each"])?;
    cursor.enter("reminder.devotion.symbol");
    let symbol = format!("{{{}}}", color.ir_symbol());
    cursor.require(
        cursor.peek_token().is_some_and(|token| {
            token.token_type == "manaSymbol" && token.raw.eq_ignore_ascii_case(&symbol)
        }),
        "the mana symbol of the devotion color",
    )?;
    cursor.consume_type("manaSymbol")?;
    cursor.enter("reminder.devotion.body");
    cursor.consume_phrase(&[
        "in",
        "the",
        "mana",
        "costs",
        "of",
        "permanents",
        "you",
        "control",
        "counts",
        "toward",
        "your",
        "devotion",
        "to",
    ])?;
    cursor.enter("reminder.devotion.color");
    cursor.consume(color.oracle_name())?;
    cursor.enter("reminder.devotion.close");
    cursor.consume_phrase(&[".", ")"])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhereXState {
    Separator,
    Where,
    Variable,
    Copula,
    Value,
    Complete,
}

/// Reads `, where X is EXPR` and returns EXPR. A definition that mentions X
/// itself is circular and fails explicitly.
pub(in crate::oracle::canonical) fn recognize_where_x_definition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let mut state = WhereXState::Separator;
    let mut expression = None;
    loop {
        state = match state {
            WhereXState::Separator => {
                cursor.enter("numeric.where-x.separator");
                cursor.consume(",")?;
                WhereXState::Where
            }
            WhereXState::Where => {
                cursor.enter("numeric.where-x.where");
                cursor.consume("where")?;
                WhereXState::Variable
            }
            WhereXState::Variable => {
                cursor.enter("numeric.where-x.variable");
                cursor.consume("x")?;
                WhereXState::Copula
            }
            WhereXState::Copula => {
                cursor.enter("numeric.where-x.copula");
                cursor.consume("is")?;
                WhereXState::Value
            }
            WhereXState::Value => {
                cursor.enter("numeric.where-x.value");
                expression = Some(recognize_variable_definition_expression(cursor)?);
                // `as this ability resolves` restates when the definition is
                // evaluated: the binding is computed as the ability's first
                // bound action resolves (CR 608.2h), never earlier.
                if ["as", "this", "ability", "resolves"]
                    .iter()
                    .enumerate()
                    .all(|(offset, word)| {
                        cursor
                            .peek_ahead(offset)
                            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
                    })
                {
                    cursor.enter("numeric.where-x.resolution-moment");
                    cursor.consume_phrase(&["as", "this", "ability", "resolves"])?;
                }
                WhereXState::Complete
            }
            WhereXState::Complete => {
                cursor.enter("numeric.where-x.complete");
                let expression = cursor.require_value(expression, "a populated definition of X")?;
                cursor.require(
                    !expression.uses_x(),
                    "a definition of X that does not mention X",
                )?;
                return Ok(expression);
            }
        };
    }
}
