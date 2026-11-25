//! `EFFECT unless PAYER COST` (CR 118.12) and `EFFECT unless CONDITION`.
//!
//! ```text
//! UnlessClause := "unless" (Payer CostSequence | Condition)
//! Payer        := "you" | "they" | PERFORMER | QUANTIFIED-PLAYER (refused)
//! PERFORMER    := the shared performer subject (`its controller`, `that
//!                 NOUN's controller`, `that player`, ...), bound by the
//!                 guarded actions
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
    ConditionContext, ConditionNode, ConditionScope, CostListGrammar, ObjectPlayerRelation,
    ObjectSubjectAntecedent, PaymentActionNode, PaymentObjectFilter, PaymentObjectsNode,
    PerformerContext, PerformerNode, PermanentEffectReference, PlayerBinding, PlayerEffectSubject,
    PlayerQuantifier, PlayerTargetBinding, ResolutionCostNode, TargetBinding,
    conjugated_payment_action_starts, object_related_player_head_length,
    project_resolution_cost_sequence, quantified_player_head,
    recognize_boolean_condition_list_before_separator, recognize_condition_atom,
    recognize_conjugated_cost_sequence, recognize_performer,
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
    // `that NOUN's controller` (S3's shared performer head) pays only when
    // a payment action conjugated for it follows, like `you`.
    let related_payer = object_related_player_head_length(cursor).is_some_and(|length| {
        let mut trial = cursor.clone();
        trial.advance_to(trial.position() + length).is_ok()
            && conjugated_payment_action_starts(&trial, ActingSubject::THIRD_SINGULAR)
    });
    // A quantified player of the shared player grammar (`an opponent`, `a
    // player`, `any player`, `each opponent`) followed by a payment
    // conjugated for it is a payer clause too, never a state condition.
    let quantified_payer = quantified_player_head(cursor)
        .filter(|(quantifier, _)| *quantifier != PlayerQuantifier::None)
        .filter(|_| {
            let mut trial = cursor.clone();
            trial.advance_to(trial.position() + 2).is_ok()
                && conjugated_payment_action_starts(&trial, ActingSubject::THIRD_SINGULAR)
        });
    let payer_starts = you_pay
        || quantified_payer.is_some()
        || related_payer
        || (word(cursor, 0, "its") && word(cursor, 1, "controller"))
        || (word(cursor, 0, "that") && word(cursor, 1, "player"))
        || word(cursor, 0, "they");
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
    } else if word(cursor, 0, "that") && word(cursor, 1, "player") {
        cursor.enter("sequence.unless.payer.bound-player");
        cursor.require(
            player.is_some(),
            "a player target declared by the guarded action",
        )?;
        (
            recognize_performed_payer(cursor, target, player)?,
            ActingSubject::THIRD_SINGULAR,
        )
    } else if object_related_player_head_length(cursor).is_some() {
        (
            recognize_performed_payer(cursor, target, player)?,
            ActingSubject::THIRD_SINGULAR,
        )
    } else if word(cursor, 0, "they") && !quantified_subject && player.is_some() {
        // `Target player discards two cards unless they discard an artifact
        // card`: `they` names the one player the guarded actions declared.
        cursor.enter("sequence.unless.payer.declared-player");
        cursor.consume("they")?;
        let player = cursor.require_value(player, "the declared player `they` names")?;
        (
            UnlessPayerNode::BoundPlayer(player),
            ActingSubject::THIRD_PLURAL,
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
    } else if let Some((quantifier, _)) = quantified_payer {
        // Several possible payers are polled in APNAP order (CR 101.4); the
        // frame lowers exactly one payer.
        cursor.enter("sequence.unless.payer.indefinite-player");
        return Err(cursor.fail(match quantifier {
            PlayerQuantifier::Every => {
                "one payer (each player of a population pays or suffers the action, CR 101.4; unsupported)"
            }
            PlayerQuantifier::Some | PlayerQuantifier::None => {
                "one payer (an indefinite or `any` player payer polls players in APNAP order, CR 101.4; unsupported)"
            }
        }));
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

/// A payer named by the shared performer subject (`its controller`, `that
/// player`; #10 PlayerRef): the binding the guarded actions proved, read by
/// the same grammar as a performed clause. A performer the frame has no
/// payer lowering for (an owner, a population, an event player) fails
/// explicitly instead of approximating.
fn recognize_performed_payer(
    cursor: &mut OracleTokenCursor<'_>,
    target: Option<TargetBinding>,
    player: Option<PlayerTargetBinding>,
) -> MachineResult<UnlessPayerNode> {
    let context = PerformerContext {
        player_antecedent: player.map(|binding| PlayerBinding(binding.effect_index)),
        event_player: false,
        defending_player: false,
        object_antecedent: target.map(|binding| ObjectSubjectAntecedent {
            reference: PermanentEffectReference::Target(binding),
            selector: None,
        }),
        object_player_antecedent: None,
    };
    let (performer, _) = recognize_performer(cursor, &context)
        .ok_or_else(|| cursor.fail("an `unless` payer the performer grammar names"))??;
    cursor.enter("sequence.unless.payer.binding");
    match performer {
        PerformerNode::Player(PlayerEffectSubject::Bound(PlayerBinding(effect_index))) => {
            Ok(UnlessPayerNode::BoundPlayer(PlayerTargetBinding {
                effect_index,
            }))
        }
        PerformerNode::Player(PlayerEffectSubject::ObjectRelation {
            relation: ObjectPlayerRelation::Controller,
            object: PermanentEffectReference::Target(binding),
        }) => Ok(UnlessPayerNode::ControllerOf(binding)),
        _ => Err(cursor.fail(
            "an `unless` payer with a lowering (the controller of a declared target, or a declared player)",
        )),
    }
}

/// True when the action reads only the payer's own resources: no `you
/// control`, `your graveyard`, counter holder or source object that names the
/// ability controller's objects instead.
fn payer_relative(payment: &PaymentActionNode) -> bool {
    let chosen_relative = |objects: &PaymentObjectsNode| match objects {
        PaymentObjectsNode::Source(_) => false,
        // `your hand`, `your library`, `your graveyard`: the payer's zones.
        PaymentObjectsNode::Designated(_) => true,
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
        | PaymentActionNode::TapTotalPower(_)
        | PaymentActionNode::Exile { .. }
        | PaymentActionNode::RemoveCounters(_)
        | PaymentActionNode::Untap(_)
        | PaymentActionNode::ReturnToHand(_)
        | PaymentActionNode::PutCounters(_)
        | PaymentActionNode::Reveal { .. }
        | PaymentActionNode::PutInLibrary { .. }
        | PaymentActionNode::Exert(_) => false,
        PaymentActionNode::Mill { .. } => true,
    }
}

#[cfg(test)]
#[path = "unless_clause_tests.rs"]
mod tests;
