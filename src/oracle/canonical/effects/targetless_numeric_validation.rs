//! Validate numeric math without constructing IR for any child node.
//! Role/property/query leaves retain their existing lowering capabilities.
use super::{
    NumericAtom, NumericExpression, NumericRounding, TargetlessDeclarationSupport,
    TargetlessLoweringError,
};

#[derive(Clone, Copy)]
enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide(NumericRounding),
}

pub(in crate::oracle::canonical) fn validate(
    expression: &NumericExpression,
    declarations: TargetlessDeclarationSupport,
) -> Result<(), TargetlessLoweringError> {
    // Keep the existing targetless capability errors when callers validate
    // without building temporary IR before their shared action lowerer.
    if matches!(
        expression,
        NumericExpression::PowerOf(_)
            | NumericExpression::ManaValueOf(_)
            | NumericExpression::QualifiedTargetProperty { .. }
    ) {
        return Err(TargetlessLoweringError::RequiresTarget);
    }
    if expression.requires_target_binding() {
        return Err(TargetlessLoweringError::UnsupportedEffect);
    }
    // uses_x visits only arithmetic children and ordinary VariableX atoms.
    // It intentionally does not reinterpret SourceCastX or query predicates.
    if declarations == TargetlessDeclarationSupport::None && expression.uses_x() {
        return Err(TargetlessLoweringError::RequiresDeclaration);
    }
    visit(expression).map(|_| ())
}

fn visit(expression: &NumericExpression) -> Result<Option<i32>, TargetlessLoweringError> {
    let (operation, left, right) = match expression {
        NumericExpression::Atom(NumericAtom::Integer(n)) => {
            return i32::try_from(*n)
                .map(Some)
                .map_err(|_| TargetlessLoweringError::RuntimeIntegerRange);
        }
        NumericExpression::Atom(NumericAtom::VariableX) => return Ok(None),
        NumericExpression::Add { left, right } => (Operation::Add, left, right),
        NumericExpression::Subtract { left, right } => (Operation::Subtract, left, right),
        NumericExpression::Multiply { left, right } => (Operation::Multiply, left, right),
        NumericExpression::Divide {
            left,
            right,
            rounding,
        } => (Operation::Divide(*rounding), left, right),
        NumericExpression::ObjectProperty { .. }
        | NumericExpression::CountQuery(_)
        | NumericExpression::PowerOf(_)
        | NumericExpression::ManaValueOf(_)
        | NumericExpression::QualifiedTargetProperty { .. }
        | NumericExpression::CountCounters { .. }
        | NumericExpression::CountersOnRecipient { .. }
        | NumericExpression::PermanentCount(_)
        | NumericExpression::SourceCastX
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::OpponentCount(_)
        | NumericExpression::Devotion(_)
        | NumericExpression::ActionResultQuantity { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::BoundObjectProperty { .. }
        | NumericExpression::GreatestPropertyAmong { .. }
        | NumericExpression::ActionResultCount { .. }
        | NumericExpression::CardsInZone { .. } => return Ok(None),
    };
    // Visit both branches even when the first is symbolic: a bad literal or
    // impossible constant subtree on the other side must not be hidden.
    let left = visit(left)?;
    let right = visit(right)?;
    if matches!(operation, Operation::Divide(_)) && right == Some(0) {
        return Err(TargetlessLoweringError::InvalidNumericArithmetic);
    }
    let (Some(left), Some(right)) = (left, right) else {
        return Ok(None);
    };
    let result = match operation {
        Operation::Add => left.checked_add(right),
        Operation::Subtract => left.checked_sub(right),
        Operation::Multiply => left.checked_mul(right),
        Operation::Divide(rounding) => {
            let quotient = left
                .checked_div(right)
                .ok_or(TargetlessLoweringError::RuntimeArithmeticRange)?;
            let remainder = left
                .checked_rem(right)
                .ok_or(TargetlessLoweringError::RuntimeArithmeticRange)?;
            if remainder == 0 {
                Some(quotient)
            } else {
                let same_sign = (left < 0) == (right < 0);
                match rounding {
                    NumericRounding::Up if same_sign => quotient.checked_add(1),
                    NumericRounding::Down if !same_sign => quotient.checked_sub(1),
                    NumericRounding::Up | NumericRounding::Down => Some(quotient),
                }
            }
        }
    };
    result
        .map(Some)
        .ok_or(TargetlessLoweringError::RuntimeArithmeticRange)
}

#[cfg(test)]
#[path = "targetless_numeric_validation_tests.rs"]
mod tests;
