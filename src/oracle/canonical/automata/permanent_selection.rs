use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ExpressedTargetCardinality, ObjectKind, ObjectQueryNode, RecognizedEffectFragment,
    recognize_expressed_target_cardinality, recognize_object_query_before_effect_separator,
};

/// A resolution choice is not a target declaration. Its bounds and query are
/// independent of the ordinary action that subsequently consumes the objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentSelectionOperandNode {
    pub bounds: ExpressedTargetCardinality,
    pub query: ObjectQueryNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UntapPermanentsEffectNode {
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
                frame.bounds = Some(recognize_expressed_target_cardinality(cursor)?);
                State::Query
            }
            State::Query => {
                cursor.enter("permanent.selection.query");
                frame.query = Some(recognize_object_query_before_effect_separator(cursor)?);
                State::Domain
            }
            State::Domain => {
                cursor.enter("permanent.selection.domain");
                let query = frame
                    .query
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated permanent query"))?;
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Permanent)
                        && query.predicate.supports_domain(ObjectKind::Permanent),
                    "battlefield permanents, not players, spells or cards in another zone",
                )?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("permanent.selection.complete");
                return Ok(PermanentSelectionOperandNode {
                    bounds: cursor.require_value(frame.bounds, "selection cardinality")?,
                    query: cursor.require_value(frame.query, "selection query")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_untap_permanents_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedEffectFragment<UntapPermanentsEffectNode>> {
    cursor.enter("effect.untap-permanents.action");
    cursor.consume("untap")?;
    let operand = recognize_permanent_selection_operand(cursor)?;
    cursor.enter("effect.untap-permanents.quantity-policy");
    cursor.require(
        operand.bounds.minimum == super::NumericAtom::Integer(0),
        "an up-to resolution quantity; exact-count best-effort selection is not implemented",
    )?;
    Ok(RecognizedEffectFragment {
        node: UntapPermanentsEffectNode { operand },
        end_position: cursor.position(),
    })
}

#[cfg(test)]
#[path = "permanent_selection_tests.rs"]
mod tests;
