//! Lowers the Boolean program of a sentence conditional (CR 608.2c) through
//! the existing atomic condition lowerers. Referenced objects are resolved
//! here, at the condition's position in the shared sequence lowering, to the
//! declaration or bound card result their symbolic binding names.
use super::super::*;

/// Object bindings visible at the condition's position in the sequence.
pub(in crate::oracle::canonical) struct ConditionObjectBindings<'a> {
    pub permanent_targets: &'a [(TargetBinding, String)],
    pub card_targets: &'a [(TargetBinding, String)],
    /// The singular card result of the current card-collection scope.
    pub card_focus: Option<Value>,
}

pub(in crate::oracle::canonical) fn lower_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    condition: &SentenceConditionNode,
    bindings: &ConditionObjectBindings<'_>,
) -> MachineResult<Value> {
    Ok(match condition {
        BooleanConditionNode::Atom(atom) => lower_sentence_condition_atom(cursor, atom, bindings)?,
        BooleanConditionNode::And(operands) => and(operands
            .iter()
            .map(|operand| lower_sentence_condition(cursor, operand, bindings))
            .collect::<MachineResult<Vec<_>>>()?),
        BooleanConditionNode::Or(operands) => or(operands
            .iter()
            .map(|operand| lower_sentence_condition(cursor, operand, bindings))
            .collect::<MachineResult<Vec<_>>>()?),
        BooleanConditionNode::Not(operand) => {
            not(lower_sentence_condition(cursor, operand, bindings)?)
        }
    })
}

fn lower_sentence_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    atom: &SentenceConditionAtom,
    bindings: &ConditionObjectBindings<'_>,
) -> MachineResult<Value> {
    Ok(match atom {
        SentenceConditionAtom::Local(local) => lower_local_condition(local),
        SentenceConditionAtom::State(state) => {
            lower_static_condition(&BooleanConditionNode::Atom(state.clone()))
        }
        SentenceConditionAtom::ControllerTurn(proposition) => {
            let turn = json!({ "kind": "duringControllerTurn", "player": controller() });
            if proposition.is_controller_turn {
                turn
            } else {
                not(turn)
            }
        }
        SentenceConditionAtom::CastFrom {
            spell: LocalObjectReference::SourceSelf,
            zone,
            negated,
        } => {
            let fact = match zone {
                None => json!({ "kind": "wasCast", "object": self_ref() }),
                Some(SourceCastZone::Hand) => {
                    json!({ "kind": "wasCastFromHand", "object": self_ref() })
                }
                Some(SourceCastZone::Graveyard) => json!({
                    "kind": "wasCastFromZone", "object": self_ref(), "zone": "graveyard",
                }),
                Some(SourceCastZone::Exile) => json!({
                    "kind": "wasCastFromZone", "object": self_ref(), "zone": "exile",
                }),
            };
            if *negated { not(fact) } else { fact }
        }
        SentenceConditionAtom::NumericComparison { left, threshold } => {
            cursor.enter("lower.sentence-condition.numeric");
            let left = lower_numeric_expression(left)
                .ok_or_else(|| cursor.fail("a numeric expression without a target binding"))?;
            compare(
                match threshold.operator {
                    ThresholdOperator::AtLeast => ">=",
                    ThresholdOperator::AtMost => "<=",
                },
                left,
                integer(threshold.value),
            )
        }
        SentenceConditionAtom::NumericRelation {
            left,
            operator,
            right,
        } => {
            cursor.enter("lower.sentence-condition.relation");
            let left = lower_numeric_expression(left)
                .ok_or_else(|| cursor.fail("a numeric expression without a target binding"))?;
            let right = lower_numeric_expression(right)
                .ok_or_else(|| cursor.fail("a numeric expression without a target binding"))?;
            compare(operator.ir_operator(), left, right)
        }
        SentenceConditionAtom::ReferencedObject(node) => {
            cursor.enter("lower.sentence-condition.object");
            let (object, filter) = match node.object {
                ConditionObjectReference::Permanent(reference) => (
                    lower_bound_permanent_reference(cursor, reference, bindings.permanent_targets)?,
                    lower_permanent_object_predicate(&node.predicate),
                ),
                ConditionObjectReference::CardTarget(binding) => (
                    bindings
                        .card_targets
                        .iter()
                        .find_map(|(candidate, id)| {
                            (*candidate == binding).then(|| chosen_target(id))
                        })
                        .ok_or_else(|| {
                            cursor
                                .fail("the singular card target declared earlier in this sequence")
                        })?,
                    lower_card_object_predicate(&node.predicate),
                ),
                ConditionObjectReference::CardFocus => (
                    bindings.card_focus.clone().ok_or_else(|| {
                        cursor.fail("a singular card result in the current card-collection scope")
                    })?,
                    lower_card_object_predicate(&node.predicate),
                ),
            };
            cursor.enter("lower.sentence-condition.object-filter");
            cursor.require(
                !filter.is_null(),
                "a characteristic that lowers to a nonempty object filter",
            )?;
            json!({ "kind": "objectMatchesFilter", "object": object, "where": filter })
        }
    })
}

#[cfg(test)]
#[path = "sentence_condition_tests.rs"]
mod tests;
