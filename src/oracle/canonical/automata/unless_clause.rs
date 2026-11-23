//! `EFFECT unless PAYER COST` (CR 118.12) and `EFFECT unless CONDITION`.
//!
//! ```text
//! UnlessClause := "unless" (Payer CostSequence | Condition)
//! Payer        := "you" | "its" "controller" | "that" "player"
//! CostSequence := payment actions conjugated for the payer, joined by "and"
//! Condition    := Boolean list of the one condition language's atoms
//! ```
//!
//! The sequence parent owns the frame: the guarded actions (those of the
//! sentence after its last `then`), the sentence end and the scope of a
//! following `where X`. This child reads only the clause. The payer is a
//! shared player reference bound by the guarded actions, never guessed. The
//! cost is the shared `CostSequenceNode` of ordinary payment actions projected
//! onto the resolution `CostProgram`; the parent lowering adds the `unless`
//! semantics (the payer chooses; an unpayable cost counts as not paid).
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::ActingSubject;
use super::{
    ConditionContext, ConditionNode, ConditionScope, CostListGrammar, PaymentActionNode,
    PaymentObjectFilter, PaymentObjectsNode, PlayerTargetBinding, ResolutionCostNode,
    TargetBinding, conjugated_payment_action_starts, project_resolution_cost_sequence,
    recognize_anaphoric_target_reference, recognize_boolean_condition_list_before_separator,
    recognize_condition_atom, recognize_conjugated_cost_sequence,
};

/// The player who may pay. Each variant is a binding the guarded actions
/// proved; the lowering resolves it to a game player at resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UnlessPayerNode {
    /// `you`: the ability's controller.
    Controller,
    /// `its controller`: the controller of the guarded action's target.
    ControllerOf(TargetBinding),
    /// `that player`: the player target declared by the guarded actions.
    BoundPlayer(PlayerTargetBinding),
    /// `they`: each player of the guarded quantified subject (`each player
    /// loses 2 life unless they ...`). The frame distributes over that
    /// population; each player pays or suffers the action (CR 101.4).
    QuantifiedPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UnlessClauseNode {
    /// The guarded actions happen unless the payer pays the whole cost.
    Payment {
        payer: UnlessPayerNode,
        cost: ResolutionCostNode,
    },
    /// The guarded actions happen unless the condition holds at resolution.
    Condition(ConditionNode),
}

/// One `unless` frame over the actions `first_effect..end_effect` of the
/// enclosing sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UnlessFrameNode {
    pub first_effect: usize,
    pub end_effect: usize,
    pub clause: UnlessClauseNode,
}

impl UnlessFrameNode {
    /// The same frame over a sequence whose actions are shifted by `offset`.
    pub(in crate::oracle::canonical) fn shifted(self, offset: usize) -> Self {
        let shift_target = |binding: TargetBinding| TargetBinding {
            effect_index: binding.effect_index + offset,
            ..binding
        };
        let clause = match self.clause {
            UnlessClauseNode::Payment { payer, cost } => UnlessClauseNode::Payment {
                payer: match payer {
                    UnlessPayerNode::Controller => UnlessPayerNode::Controller,
                    UnlessPayerNode::ControllerOf(binding) => {
                        UnlessPayerNode::ControllerOf(shift_target(binding))
                    }
                    UnlessPayerNode::BoundPlayer(binding) => {
                        UnlessPayerNode::BoundPlayer(PlayerTargetBinding {
                            effect_index: binding.effect_index + offset,
                        })
                    }
                    UnlessPayerNode::QuantifiedPlayer => UnlessPayerNode::QuantifiedPlayer,
                },
                cost,
            },
            condition @ UnlessClauseNode::Condition(_) => condition,
        };
        Self {
            first_effect: self.first_effect + offset,
            end_effect: self.end_effect + offset,
            clause,
        }
    }
}

/// Reads `unless PAYER COST` or `unless CONDITION`, stopping before the
/// sentence end owned by the parent. `target` and `player` are the singular
/// object and player bindings the guarded actions declared, if any;
/// `quantified_subject` is true when the guarded action is one quantified
/// player action whose population `they` names.
pub(in crate::oracle::canonical) fn recognize_unless_clause(
    cursor: &mut OracleTokenCursor<'_>,
    target: Option<TargetBinding>,
    player: Option<PlayerTargetBinding>,
    quantified_subject: bool,
) -> MachineResult<UnlessClauseNode> {
    cursor.enter("sequence.unless.introducer");
    cursor.consume("unless")?;
    cursor.enter("sequence.unless.payer");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    // `you` pays only when a payment action conjugated for it follows; any
    // other clause is a state condition of the one condition language.
    let you_pay = word(cursor, 0, "you") && {
        let mut trial = cursor.clone();
        trial.consume("you").is_ok()
            && conjugated_payment_action_starts(&trial, ActingSubject::CONTROLLER)
    };
    let payer_starts = you_pay
        || (word(cursor, 0, "its") && word(cursor, 1, "controller"))
        || (word(cursor, 0, "that") && word(cursor, 1, "player"))
        || word(cursor, 0, "they")
        || (word(cursor, 0, "any") && word(cursor, 1, "player"));
    let (payer, form) = if !payer_starts {
        cursor.enter("sequence.unless.condition");
        let context = ConditionContext::standalone(ConditionScope::Unless, &[], None, false);
        let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
            recognize_condition_atom(cursor, &context)
        })?;
        return Ok(UnlessClauseNode::Condition(condition));
    } else if word(cursor, 0, "you") {
        cursor.consume("you")?;
        (UnlessPayerNode::Controller, ActingSubject::CONTROLLER)
    } else if word(cursor, 0, "its") && word(cursor, 1, "controller") {
        let binding = recognize_anaphoric_target_reference(cursor, target)?;
        cursor.enter("sequence.unless.payer.controller");
        cursor.consume("controller")?;
        (
            UnlessPayerNode::ControllerOf(binding),
            ActingSubject::THIRD_SINGULAR,
        )
    } else if word(cursor, 0, "that") && word(cursor, 1, "player") {
        cursor.enter("sequence.unless.payer.bound-player");
        let binding =
            player.ok_or_else(|| cursor.fail("a player target declared by the guarded action"))?;
        cursor.consume_phrase(&["that", "player"])?;
        (
            UnlessPayerNode::BoundPlayer(binding),
            ActingSubject::THIRD_SINGULAR,
        )
    } else if word(cursor, 0, "they") {
        cursor.enter("sequence.unless.payer.quantified-player");
        cursor.require(
            quantified_subject,
            "a guarded quantified player action whose players `they` names",
        )?;
        cursor.consume("they")?;
        (
            UnlessPayerNode::QuantifiedPlayer,
            ActingSubject::THIRD_PLURAL,
        )
    } else if word(cursor, 0, "any") && word(cursor, 1, "player") {
        cursor.enter("sequence.unless.payer.any-player");
        return Err(cursor.fail(
            "one payer (`any player may pay` polls players in APNAP order, CR 101.4; unsupported)",
        ));
    } else {
        return Err(cursor.fail(
            "an `unless` payer (`you`, `its controller`, a bound `that player`) or a state condition",
        ));
    };
    cursor.enter("sequence.unless.payment-action");
    cursor.require(
        conjugated_payment_action_starts(cursor, form),
        "a payment action conjugated for the payer (an `unless` state condition is unsupported here)",
    )?;
    let sequence = recognize_conjugated_cost_sequence(
        cursor,
        CostListGrammar {
            comma: false,
            and: true,
        },
        form,
    )?;
    if payer != UnlessPayerNode::Controller {
        cursor.enter("sequence.unless.payer-relative-objects");
        cursor.require(
            sequence.costs.iter().all(payer_relative),
            "objects of the payer's own (`you`/`your` name the ability controller, not this payer)",
        )?;
    }
    let cost = project_resolution_cost_sequence(cursor, sequence)?;
    cursor.enter("sequence.unless.complete");
    Ok(UnlessClauseNode::Payment { payer, cost })
}

/// True when the action reads only the payer's own resources: no `you
/// control`, `your graveyard`, counter holder or source object that names the
/// ability controller's objects instead.
fn payer_relative(payment: &PaymentActionNode) -> bool {
    let chosen_relative = |objects: &PaymentObjectsNode| match objects {
        PaymentObjectsNode::Source(_) => false,
        PaymentObjectsNode::Chosen(chosen) => match &chosen.filter {
            PaymentObjectFilter::Permanents(object) => object.controller.is_none(),
            PaymentObjectFilter::Cards(_) => true,
        },
    };
    match payment {
        PaymentActionNode::Pay(_) | PaymentActionNode::Blight { .. } => true,
        PaymentActionNode::Sacrifice(objects) | PaymentActionNode::Discard(objects) => {
            chosen_relative(objects)
        }
        // Tapped permanents and counter holders are written `you control`;
        // exiled cards come `from your` zone; a source belongs to the ability.
        PaymentActionNode::Tap(_)
        | PaymentActionNode::Exile { .. }
        | PaymentActionNode::RemoveCounters(_)
        | PaymentActionNode::Untap(_)
        | PaymentActionNode::ReturnToHand(_)
        | PaymentActionNode::PutCounters(_)
        | PaymentActionNode::Reveal { .. }
        | PaymentActionNode::Exert(_) => false,
        PaymentActionNode::Mill { .. } => true,
    }
}

#[cfg(test)]
#[path = "unless_clause_tests.rs"]
mod tests;
