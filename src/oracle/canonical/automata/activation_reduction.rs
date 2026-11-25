//! `This ability costs DELTA less to activate [AMOUNT TAIL] [CONDITION].`
//!
//! The activated-ability subject of the one cost-adjustment frame (CR 601.2f
//! via 602.2b). The delta, the `for each` and `where X is` amounts and the
//! condition are the same children the casting reduction reads; this parent
//! owns only the `this ability ... to activate` envelope. It returns
//! metadata, never an effect node.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CostAdjustmentCondition, NumericExpression, ObjectControllerScope, ObjectPredicate,
    ReductionDelta, recognize_cost_adjustment_condition, recognize_mana_cost,
    recognize_object_query, recognize_optional_amount_reminder, recognize_where_x_definition,
    where_x_definition_starts,
};

/// The amount an activation reduction removes from the generic component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationReductionAmount {
    /// `{N} less to activate for each PERMANENT you control`.
    PerControlledPermanent {
        generic: i64,
        selector: ObjectPredicate,
    },
    /// `{N} less`, or `{X} less ..., where X is EXPR` (CR 107.3).
    Expression(NumericExpression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationCostReductionNode {
    pub amount: ActivationReductionAmount,
    /// `if COND`, `during your turn`, `if it targets a QUERY`.
    pub condition: Option<CostAdjustmentCondition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationReductionState {
    Subject,
    Action,
    Amount,
    Reduction,
    Quantifier,
    Selector,
    Condition,
    Terminator,
    Complete,
}

/// Recognizes the activation cost reduction metadata of one activated
/// ability.
pub(in crate::oracle::canonical) fn recognize_activation_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostReductionNode> {
    let mut state = ActivationReductionState::Subject;
    let mut delta = None;
    let mut amount = None;
    let mut condition = None;
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
                if cursor
                    .peek_token()
                    .is_none_or(|token| token.token_type != "manaSymbol")
                {
                    return Err(cursor.fail("a positive generic mana symbol or {X}"));
                }
                let cost = recognize_mana_cost(cursor)?;
                cursor.enter("activation-reduction.amount");
                delta = Some(cursor.require_value(
                    ReductionDelta::of(&cost),
                    "a positive generic mana symbol or {X}",
                )?);
                ActivationReductionState::Reduction
            }
            ActivationReductionState::Reduction => {
                cursor.enter("activation-reduction.reduction");
                cursor.consume_phrase(&["less", "to", "activate"])?;
                ActivationReductionState::Quantifier
            }
            ActivationReductionState::Quantifier => {
                cursor.enter("activation-reduction.quantifier");
                match delta {
                    Some(ReductionDelta::Variable) => {
                        cursor.require(
                            where_x_definition_starts(cursor),
                            "`, where X is` defining the {X} reduction",
                        )?;
                        amount = Some(ActivationReductionAmount::Expression(
                            recognize_where_x_definition(cursor)?,
                        ));
                        ActivationReductionState::Condition
                    }
                    Some(ReductionDelta::Generic(_))
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("for")) =>
                    {
                        cursor.consume_phrase(&["for", "each"])?;
                        ActivationReductionState::Selector
                    }
                    Some(ReductionDelta::Generic(generic)) => {
                        amount = Some(ActivationReductionAmount::Expression(
                            NumericExpression::from(i64::from(generic)),
                        ));
                        ActivationReductionState::Condition
                    }
                    None => return Err(cursor.fail("a populated reduction delta")),
                }
            }
            ActivationReductionState::Selector => {
                cursor.enter("activation-reduction.selector");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller == Some(ObjectControllerScope::AbilityController),
                    "a permanent selector controlled by the ability controller",
                )?;
                cursor.require(
                    query.denotes_battlefield_permanents(),
                    "a battlefield-permanent selector",
                )?;
                let Some(ReductionDelta::Generic(generic)) = delta else {
                    return Err(cursor.fail("a generic reduction per permanent"));
                };
                amount = Some(ActivationReductionAmount::PerControlledPermanent {
                    generic: i64::from(generic),
                    selector: query.predicate,
                });
                ActivationReductionState::Condition
            }
            ActivationReductionState::Condition => {
                cursor.enter("activation-reduction.condition");
                condition = recognize_cost_adjustment_condition(cursor)?;
                ActivationReductionState::Terminator
            }
            ActivationReductionState::Terminator => {
                cursor.enter("activation-reduction.terminator");
                cursor.consume(".")?;
                if let Some(ActivationReductionAmount::Expression(expression)) = &amount {
                    recognize_optional_amount_reminder(cursor, expression)?;
                }
                ActivationReductionState::Complete
            }
            ActivationReductionState::Complete => {
                cursor.enter("activation-reduction.complete");
                return Ok(ActivationCostReductionNode {
                    amount: cursor
                        .require_value(amount, "a populated activation reduction amount")?,
                    condition,
                });
            }
        };
    }
}
