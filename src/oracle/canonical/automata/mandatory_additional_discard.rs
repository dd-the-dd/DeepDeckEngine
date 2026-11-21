use super::super::{MachineFailure, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    DiscardVerbForm, recognize_discard_action, selection_method::recognize_postfix_selection_method,
};
use super::{
    CollectionSelectionMethod, NumericAtom, ObjectKind, ObjectPredicate, object_predicate_uses_x,
    recognize_mandatory_additional_cost_header, recognize_numeric_atom,
    recognize_search_card_selector,
};

// The current adapter expands one scalar discardCard consumer per chosen card.
// This is an explicit compiler support limit, never a truncated payment.
pub(in crate::oracle::canonical) const MAX_SCALAR_ADDITIONAL_DISCARD_CARDS: i64 = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MandatoryAdditionalDiscardCostNode {
    pub quantity: i64,
    pub filter: ObjectPredicate,
    pub selection: CollectionSelectionMethod,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) enum MandatoryAdditionalDiscardCostAttempt {
    Parsed(MandatoryAdditionalDiscardCostNode),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
}

/// The same typed discard action used by player-discard effects and existing
/// alternative/self costs delegates its operand to the shared card selector.
/// The casting envelope supplies own-hand/payment semantics, not the action.
pub(in crate::oracle::canonical) fn try_recognize_mandatory_additional_discard_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MandatoryAdditionalDiscardCostAttempt {
    let checkpoint = cursor.checkpoint();
    cursor.enter("additional-cost.mandatory-discard.envelope");
    if let Err(failure) = recognize_mandatory_additional_cost_header(cursor) {
        return MandatoryAdditionalDiscardCostAttempt::Failed(failure);
    }
    let mut malformed_quantity = false;
    let action = recognize_discard_action(cursor, DiscardVerbForm::Imperative, |cursor| {
        cursor.enter("additional-cost.mandatory-discard.quantity");
        let quantity = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
        {
            let article = cursor.peek().expect("checked article").to_string();
            cursor.consume(&article)?;
            1
        } else {
            match recognize_numeric_atom(cursor)? {
                NumericAtom::Integer(value) if value > 0 => value,
                NumericAtom::Integer(_) => {
                    malformed_quantity = true;
                    return Err(cursor.fail("a positive discard-card quantity"));
                }
                NumericAtom::VariableX => {
                    return Err(cursor.fail(
                        "a literal discard-card quantity before contextual quantity support",
                    ));
                }
            }
        };
        if quantity > MAX_SCALAR_ADDITIONAL_DISCARD_CARDS {
            return Err(cursor.fail("a quantity within the bounded scalar cost adapter"));
        }
        cursor.enter("additional-cost.mandatory-discard.card-query");
        let filter = recognize_search_card_selector(cursor, quantity != 1)?;
        cursor.require(
            filter.supports_domain(ObjectKind::Card),
            "a card-domain discard operand",
        )?;
        Ok((quantity, filter))
    });
    let (quantity, filter) = match action {
        Ok(action) => action.operand,
        Err(failure) if malformed_quantity => {
            return MandatoryAdditionalDiscardCostAttempt::Failed(failure);
        }
        Err(failure) => {
            cursor.rollback(checkpoint);
            return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
        }
    };
    cursor.enter("additional-cost.mandatory-discard.numeric-context");
    if object_predicate_uses_x(&filter) {
        let failure = cursor
            .fail("contextual numeric producer support before X in the discarded-card predicate");
        cursor.rollback(checkpoint);
        return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
    }
    let selection = match recognize_postfix_selection_method(cursor) {
        Ok(selection) => selection,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
        }
    };
    cursor.enter("additional-cost.mandatory-discard.selection-support");
    if selection != CollectionSelectionMethod::Choice {
        let failure =
            cursor.fail("choice-based discard cost support before random casting-time payment");
        cursor.rollback(checkpoint);
        return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
    }
    cursor.enter("additional-cost.mandatory-discard.terminator");
    if let Err(failure) = cursor.consume(".") {
        cursor.rollback(checkpoint);
        return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
    }
    cursor.enter("additional-cost.mandatory-discard.complete");
    if let Err(failure) = cursor.finish() {
        cursor.rollback(checkpoint);
        return MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure);
    }
    MandatoryAdditionalDiscardCostAttempt::Parsed(MandatoryAdditionalDiscardCostNode {
        quantity,
        filter,
        selection,
    })
}

#[cfg(test)]
#[path = "mandatory_additional_discard_tests.rs"]
mod tests;
