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
    pub(in crate::oracle::canonical) fn covers(&self, effect_index: usize) -> bool {
        (self.first_effect..self.end_effect).contains(&effect_index)
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
