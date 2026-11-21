use super::super::*;

/// Lowers the shared Boolean condition AST through the existing atomic
/// condition lowerer. Intervening-if timing stays in the trigger parent and
/// the engine; it must not become an effect-level conditional action.
pub(in crate::oracle::canonical) fn lower_trigger_guard(
    condition: &BooleanConditionNode<LocalConditionNode>,
) -> Value {
    match condition {
        BooleanConditionNode::Atom(atom) => lower_local_condition(atom),
        BooleanConditionNode::And(operands) => {
            and(operands.iter().map(lower_trigger_guard).collect())
        }
        BooleanConditionNode::Or(operands) => {
            or(operands.iter().map(lower_trigger_guard).collect())
        }
        BooleanConditionNode::Not(operand) => not(lower_trigger_guard(operand)),
    }
}
