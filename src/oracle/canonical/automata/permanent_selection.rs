use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardMoveOwnerNode, CardZoneName, CollectionOperandAttempt, NumericAtom, NumericExpression,
    ObjectQueryNode, RecognizedEffectFragment, TargetCount, recognize_object_query,
    recognize_target_count, try_recognize_card_move_destination,
};

/// A resolution choice is not a target declaration. Its bounds and query are
/// independent of the ordinary action that subsequently consumes the objects.
/// `bounds: None` is `all QUERY`: every matching permanent, without a choice.
/// The bounds are the one count node shared with target declarations
/// (`N`, `up to N`, `X`, `any number of`, `one or more`); the choice is made
/// on resolution (CR 608.2d) and chooses as many as possible when fewer
/// permanents qualify (CR 609.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentSelectionOperandNode {
    pub bounds: Option<TargetCount>,
    pub query: ObjectQueryNode,
}

impl PermanentSelectionOperandNode {
    /// Whether more than one permanent may be chosen (or every member of a
    /// collection is affected): possessive agreement follows it.
    pub(in crate::oracle::canonical) fn is_plural(&self) -> bool {
        self.bounds.as_ref().is_none_or(TargetCount::is_plural)
    }
}

/// The ordinary action applied to every selected permanent. The selection
/// operand and the action are independent children of one choose-then-act
/// frame; a new verb reuses the operand without another selection grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SelectedPermanentAction {
    Untap,
    /// Each selected permanent becomes tapped (CR 701.26a); an already
    /// tapped permanent is not tapped again.
    Tap,
    /// Each selected permanent moves to its owner's hand (CR 400.3).
    ReturnToOwnerHand,
    /// Each selected permanent moves to exile (CR 406.2).
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SelectedPermanentsEffectNode {
    pub action: SelectedPermanentAction,
    pub operand: PermanentSelectionOperandNode,
    /// `exile all QUERY until SOURCE leaves the battlefield` (CR 610.3): the
    /// sentence parent's adjunct of an exile; `None` for a lasting move.
    pub until: Option<super::ZoneChangeUntilNode>,
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
    bounds: Option<TargetCount>,
    query: Option<ObjectQueryNode>,
}

/// Lexical lookahead for a resolution count that does not head a target
/// (`any number of untapped creatures`, `one or more lands`, `up to two
/// lands`, `X creatures`): the count is the shared declared-count child, and
/// the noun after it is not `[other] target`. A `target` head after the same
/// count belongs to the target-list child (CR 115.1). Dispatch hint only.
pub(in crate::oracle::canonical) fn resolution_selection_count_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let counted = super::expressed_target_count_starts(cursor)
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("up"))
        || cursor
            .peek_token()
            .is_some_and(|token| token.token_type == "quantity");
    if !counted {
        return false;
    }
    let mut probe = cursor.clone();
    if probe.attempt(recognize_target_count).is_err() {
        return false;
    }
    if probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
    {
        let _ = probe.consume("other");
    }
    !probe.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target") || raw.eq_ignore_ascii_case("targets")
    })
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
                // The shared count child consumes only the count, never a
                // target head or a target slot. `all` quantifies over every
                // matching permanent instead.
                frame.bounds = if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
                {
                    cursor.consume("all")?;
                    None
                } else {
                    Some(recognize_target_count(cursor)?)
                };
                cursor.enter("permanent.selection.non-target");
                let other = cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
                let head = cursor.peek_ahead(usize::from(other));
                cursor.require(
                    !head.is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("target") || raw.eq_ignore_ascii_case("targets")
                    }),
                    "a resolution choice; a `target` head is a declaration of the target-list child",
                )?;
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
        operand.bounds.as_ref().is_some_and(|bounds| {
            bounds.minimum == NumericExpression::Atom(NumericAtom::Integer(0))
        }),
        "an up-to resolution quantity; exact-count best-effort selection is not implemented",
    )?;
    Ok(RecognizedEffectFragment {
        node: SelectedPermanentsEffectNode {
            action: SelectedPermanentAction::Untap,
            operand,
            until: None,
        },
        end_position: cursor.position(),
    })
}

/// Whether the operand after `untap` (the cursor at the verb) opens the
/// untap choice's own grammar: `all QUERY` or a resolution count heading no
/// target. Any other head (a reference such as `them`, a declared target)
/// belongs to the shared permanent operand, whose diagnostic is then the
/// authoritative one.
pub(in crate::oracle::canonical) fn untap_selection_head(cursor: &OracleTokenCursor<'_>) -> bool {
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        return true;
    }
    let mut operand = cursor.clone();
    operand.advance_to(cursor.position() + 1).is_ok() && resolution_selection_count_starts(&operand)
}

/// `tap N|up to N|X|any number of|one or more PERMANENTS`: the same
/// resolution choice as the untap action (CR 608.2d), consumed by the tap
/// action. If fewer permanents qualify, all of them are chosen (CR 609.3);
/// the tapped set is the action's result (`for each creature tapped this
/// way`, CR 608.2c). `tap all|each ...` and declared targets stay with the
/// shared permanent operand of the tap frame.
pub(in crate::oracle::canonical) fn recognize_tap_selected_permanents_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedEffectFragment<SelectedPermanentsEffectNode>> {
    cursor.enter("effect.tap-selected.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Tap,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("effect.tap-selected.count");
    cursor.require(
        resolution_selection_count_starts(cursor),
        "a resolution count that heads no target",
    )?;
    let operand = recognize_permanent_selection_operand(cursor)?;
    cursor.enter("effect.tap-selected.tail");
    super::refuse_unread_relative_tail(cursor)?;
    Ok(RecognizedEffectFragment {
        node: SelectedPermanentsEffectNode {
            action: SelectedPermanentAction::Tap,
            operand,
            until: None,
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
            until: None,
        },
        end_position: cursor.position(),
    })
}

#[cfg(test)]
#[path = "permanent_selection_tests.rs"]
mod tests;
