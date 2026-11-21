use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CurrentSpellManaCostReferenceNode, ObjectControllerScope, ObjectKind, ObjectPredicate,
    recognize_current_spell_mana_cost_reference, recognize_object_query,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReturnControlledPermanentCostNode {
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AlternativeReturnCostEnvelopeNode {
    pub cost: ReturnControlledPermanentCostNode,
    pub replaces: CurrentSpellManaCostReferenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReturnControlledPermanentCostState {
    Action,
    Determiner,
    Query,
    DestinationRelation,
    DestinationReference,
    DestinationOwner,
    DestinationZone,
    Complete,
}

/// Recognizes returning one controlled permanent to its owner's hand as an
/// atomic cost. Object characteristics and the postnominal controller
/// relation remain owned by the shared object-query automaton.
fn recognize_return_controlled_permanent_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReturnControlledPermanentCostNode> {
    let mut state = ReturnControlledPermanentCostState::Action;
    let mut predicate = None;
    loop {
        state = match state {
            ReturnControlledPermanentCostState::Action => {
                cursor.enter("cost.return-controlled.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Return,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                ReturnControlledPermanentCostState::Determiner
            }
            ReturnControlledPermanentCostState::Determiner => {
                cursor.enter("cost.return-controlled.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an"),
                    "an indefinite permanent determiner",
                )?;
                ReturnControlledPermanentCostState::Query
            }
            ReturnControlledPermanentCostState::Query => {
                cursor.enter("cost.return-controlled.query");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller == Some(ObjectControllerScope::AbilityController),
                    "a permanent controlled by the casting player",
                )?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent),
                    "a permanent-domain object predicate",
                )?;
                predicate = Some(query.predicate);
                ReturnControlledPermanentCostState::DestinationRelation
            }
            ReturnControlledPermanentCostState::DestinationRelation => {
                cursor.enter("cost.return-controlled.destination-relation");
                cursor.consume("to")?;
                ReturnControlledPermanentCostState::DestinationReference
            }
            ReturnControlledPermanentCostState::DestinationReference => {
                cursor.enter("cost.return-controlled.destination-reference");
                cursor.consume("its")?;
                ReturnControlledPermanentCostState::DestinationOwner
            }
            ReturnControlledPermanentCostState::DestinationOwner => {
                cursor.enter("cost.return-controlled.destination-owner");
                let owner = cursor.consume_type("possessivePlayerRoleWord")?;
                cursor.require(
                    owner.raw.eq_ignore_ascii_case("owner's")
                        || owner.raw.eq_ignore_ascii_case("owner’s"),
                    "the possessive owner reference",
                )?;
                ReturnControlledPermanentCostState::DestinationZone
            }
            ReturnControlledPermanentCostState::DestinationZone => {
                cursor.enter("cost.return-controlled.destination-zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(zone.raw.eq_ignore_ascii_case("hand"), "the hand zone")?;
                ReturnControlledPermanentCostState::Complete
            }
            ReturnControlledPermanentCostState::Complete => {
                cursor.enter("cost.return-controlled.complete");
                return Ok(ReturnControlledPermanentCostNode {
                    predicate: cursor
                        .require_value(predicate, "a controlled-permanent predicate")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlternativeReturnEnvelopeState {
    Subject,
    Permission,
    Cost,
    Substitution,
    PaymentAction,
    ReplacedCost,
    Terminator,
    Complete,
}

#[derive(Default)]
struct AlternativeReturnEnvelopeFrame {
    cost: Option<ReturnControlledPermanentCostNode>,
    replaces: Option<CurrentSpellManaCostReferenceNode>,
}

/// Recognizes a permission to replace the current spell's mana cost with one
/// independent controlled-permanent return cost.
pub(in crate::oracle::canonical) fn recognize_alternative_return_cost_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AlternativeReturnCostEnvelopeNode> {
    let mut state = AlternativeReturnEnvelopeState::Subject;
    let mut frame = AlternativeReturnEnvelopeFrame::default();
    loop {
        state = match state {
            AlternativeReturnEnvelopeState::Subject => {
                cursor.enter("alternative-return.subject");
                cursor.consume("you")?;
                AlternativeReturnEnvelopeState::Permission
            }
            AlternativeReturnEnvelopeState::Permission => {
                cursor.enter("alternative-return.permission");
                let permission = cursor.consume_type("modalityWord")?;
                cursor.require(permission.raw.eq_ignore_ascii_case("may"), "`may`")?;
                AlternativeReturnEnvelopeState::Cost
            }
            AlternativeReturnEnvelopeState::Cost => {
                cursor.enter("alternative-return.cost");
                frame.cost = Some(recognize_return_controlled_permanent_cost(cursor)?);
                AlternativeReturnEnvelopeState::Substitution
            }
            AlternativeReturnEnvelopeState::Substitution => {
                cursor.enter("alternative-return.substitution");
                cursor.consume("rather")?;
                cursor.consume("than")?;
                AlternativeReturnEnvelopeState::PaymentAction
            }
            AlternativeReturnEnvelopeState::PaymentAction => {
                cursor.enter("alternative-return.payment-action");
                let action = cursor.consume_type("costActionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Pay, VerbForm::Base),
                    "`pay`",
                )?;
                AlternativeReturnEnvelopeState::ReplacedCost
            }
            AlternativeReturnEnvelopeState::ReplacedCost => {
                cursor.enter("alternative-return.replaced-cost");
                frame.replaces = Some(recognize_current_spell_mana_cost_reference(cursor)?);
                AlternativeReturnEnvelopeState::Terminator
            }
            AlternativeReturnEnvelopeState::Terminator => {
                cursor.enter("alternative-return.terminator");
                cursor.consume(".")?;
                AlternativeReturnEnvelopeState::Complete
            }
            AlternativeReturnEnvelopeState::Complete => {
                cursor.enter("alternative-return.complete");
                cursor.finish()?;
                return Ok(AlternativeReturnCostEnvelopeNode {
                    cost: cursor.require_value(frame.cost, "a return-permanent cost")?,
                    replaces: cursor
                        .require_value(frame.replaces, "the current spell's mana cost")?,
                });
            }
        };
    }
}
