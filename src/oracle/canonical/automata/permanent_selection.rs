use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardMoveOwnerNode, CardZoneName, CollectionOperandAttempt, ExpressedTargetCardinality,
    ObjectQueryNode, RecognizedEffectFragment, recognize_expressed_target_cardinality,
    recognize_object_query, try_recognize_card_move_destination,
};

/// A resolution choice is not a target declaration. Its bounds and query are
/// independent of the ordinary action that subsequently consumes the objects.
/// `bounds: None` is `all QUERY`: every matching permanent, without a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentSelectionOperandNode {
    pub bounds: Option<ExpressedTargetCardinality>,
    pub query: ObjectQueryNode,
}

/// The ordinary action applied to every selected permanent. The selection
/// operand and the action are independent children of one choose-then-act
/// frame; a new verb reuses the operand without another selection grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SelectedPermanentAction {
    Untap,
    /// Each selected permanent moves to its owner's hand (CR 400.3).
    ReturnToOwnerHand,
    /// Each selected permanent moves to exile (CR 406.2).
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SelectedPermanentsEffectNode {
    pub action: SelectedPermanentAction,
    pub operand: PermanentSelectionOperandNode,
}

#[derive(Clone, Copy)]
enum State {
    Bounds,
    Query,
    Domain,
    Complete,
}

#[derive(Default)]
struct AttemptFrame {
    bounds: Option<ExpressedTargetCardinality>,
    query: Option<ObjectQueryNode>,
}

pub(in crate::oracle::canonical) fn recognize_permanent_selection_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermanentSelectionOperandNode> {
    let mut state = State::Bounds;
    let mut frame = AttemptFrame::default();
    loop {
        state = match state {
            State::Bounds => {
                cursor.enter("permanent.selection.bounds");
                // The shared reader consumes only number/up-to-number, never
                // a target head or a target slot; positive literals/X only.
                // `all` quantifies over every matching permanent instead.
                frame.bounds = if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
                {
                    cursor.consume("all")?;
                    None
                } else {
                    Some(recognize_expressed_target_cardinality(cursor)?)
                };
                State::Query
            }
            State::Query => {
                cursor.enter("permanent.selection.query");
                frame.query = Some(recognize_object_query(cursor)?);
                State::Domain
            }
            State::Domain => {
                cursor.enter("permanent.selection.domain");
                let query = frame
                    .query
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated permanent query"))?;
                cursor.require(
                    query.denotes_battlefield_permanents(),
                    "battlefield permanents, not players, spells or cards in another zone",
                )?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("permanent.selection.complete");
                return Ok(PermanentSelectionOperandNode {
                    bounds: frame.bounds,
                    query: cursor.require_value(frame.query, "selection query")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_untap_permanents_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedEffectFragment<SelectedPermanentsEffectNode>> {
    cursor.enter("effect.untap-permanents.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Untap,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    let operand = recognize_permanent_selection_operand(cursor)?;
    cursor.enter("effect.untap-permanents.quantity-policy");
    cursor.require(
        operand
            .bounds
            .is_some_and(|bounds| bounds.minimum == super::NumericAtom::Integer(0)),
        "an up-to resolution quantity; exact-count best-effort selection is not implemented",
    )?;
    Ok(RecognizedEffectFragment {
        node: SelectedPermanentsEffectNode {
            action: SelectedPermanentAction::Untap,
            operand,
        },
        end_position: cursor.position(),
    })
}

/// `return N PERMANENTS to its/their owner's hand`: a resolution choice of
/// permanents (not a target, CR 115.1) followed by the shared destination
/// child. If fewer permanents qualify, all of them are chosen (CR 609.3).
/// `return all PERMANENTS to their owners' hands` moves every match.
pub(in crate::oracle::canonical) fn recognize_return_selected_permanents_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedEffectFragment<SelectedPermanentsEffectNode>> {
    cursor.enter("effect.return-selected.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    let operand = recognize_permanent_selection_operand(cursor)?;
    cursor.enter("effect.return-selected.destination");
    let destination = match try_recognize_card_move_destination(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("a destination for the selected permanents"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.enter("effect.return-selected.destination-capability");
    cursor.require(
        destination.zone == CardZoneName::Hand
            && destination.owner == CardMoveOwnerNode::EachCardOwner
            && destination.position.is_none()
            && destination.order.is_none(),
        "each selected permanent's owner's hand",
    )?;
    Ok(RecognizedEffectFragment {
        node: SelectedPermanentsEffectNode {
            action: SelectedPermanentAction::ReturnToOwnerHand,
            operand,
        },
        end_position: cursor.position(),
    })
}

#[cfg(test)]
#[path = "permanent_selection_tests.rs"]
mod tests;
