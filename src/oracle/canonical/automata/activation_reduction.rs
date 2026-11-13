use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectControllerScope, ObjectKind, ObjectPredicate, recognize_object_query};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationCostReductionNode {
    pub generic_reduction_per_permanent: i64,
    pub selector: ObjectPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationReductionState {
    Subject,
    Action,
    Amount,
    Reduction,
    Quantifier,
    Selector,
    Terminator,
    Complete,
}

/// Recognizes generic-mana activation-cost reduction metadata scaled by
/// controlled permanents. It returns metadata, never an effect node.
pub(in crate::oracle::canonical) fn recognize_activation_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostReductionNode> {
    let mut state = ActivationReductionState::Subject;
    let mut amount = None;
    let mut selector = None;
    loop {
        state = match state {
            ActivationReductionState::Subject => {
                cursor.enter("activation-reduction.subject");
                cursor.consume_phrase(&["this", "ability"])?;
                ActivationReductionState::Action
            }
            ActivationReductionState::Action => {
                cursor.enter("activation-reduction.action");
                cursor.consume("costs")?;
                ActivationReductionState::Amount
            }
            ActivationReductionState::Amount => {
                cursor.enter("activation-reduction.amount");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a positive generic mana symbol"))?
                    .to_string();
                let numeric = raw
                    .strip_prefix('{')
                    .and_then(|raw| raw.strip_suffix('}'))
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| cursor.fail("a positive generic mana symbol"))?;
                cursor.consume_type("manaSymbol")?;
                amount = Some(numeric);
                ActivationReductionState::Reduction
            }
            ActivationReductionState::Reduction => {
                cursor.enter("activation-reduction.reduction");
                cursor.consume_phrase(&["less", "to", "activate"])?;
                ActivationReductionState::Quantifier
            }
            ActivationReductionState::Quantifier => {
                cursor.enter("activation-reduction.quantifier");
                cursor.consume_phrase(&["for", "each"])?;
                ActivationReductionState::Selector
            }
            ActivationReductionState::Selector => {
                cursor.enter("activation-reduction.selector");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller == Some(ObjectControllerScope::AbilityController),
                    "a permanent selector controlled by the ability controller",
                )?;
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Permanent),
                    "a battlefield-permanent selector",
                )?;
                selector = Some(query.predicate);
                ActivationReductionState::Terminator
            }
            ActivationReductionState::Terminator => {
                cursor.enter("activation-reduction.terminator");
                cursor.consume(".")?;
                ActivationReductionState::Complete
            }
            ActivationReductionState::Complete => {
                cursor.enter("activation-reduction.complete");
                return Ok(ActivationCostReductionNode {
                    generic_reduction_per_permanent: cursor
                        .require_value(amount, "a populated activation reduction amount")?,
                    selector: cursor.require_value(selector, "a populated reduction selector")?,
                });
            }
        };
    }
}
