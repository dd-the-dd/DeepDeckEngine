use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionalEnteringStateNode, LocalObjectReference, OptionalLifeEntryNode,
    OptionalLifePaymentNode, recognize_cost_not_paid_condition, recognize_entry_object_kind,
    recognize_optional_life_payment, recognize_tapped_entry_effect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalLifeEntryState {
    As,
    Source,
    ObjectKind,
    Event,
    Separator,
    Payment,
    ConditionalEntry,
    Complete,
}

/// Recognizes an entering replacement composed from an optional life-payment
/// decision and an unpaid tapped-state consequence. Numeric payment, condition
/// binding and entering state remain owned by their shared child automata.
pub(in crate::oracle::canonical) fn recognize_optional_life_entry(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OptionalLifeEntryNode> {
    let mut state = OptionalLifeEntryState::As;
    let mut object_kind = None;
    let mut payment = None;
    let mut unpaid_entry = None;
    loop {
        state = match state {
            OptionalLifeEntryState::As => {
                cursor.enter("replacement.optional-life-entry.as");
                cursor.consume("as")?;
                OptionalLifeEntryState::Source
            }
            OptionalLifeEntryState::Source => {
                cursor.enter("replacement.optional-life-entry.source");
                cursor.consume("this")?;
                OptionalLifeEntryState::ObjectKind
            }
            OptionalLifeEntryState::ObjectKind => {
                cursor.enter("replacement.optional-life-entry.object-kind");
                object_kind = Some(recognize_entry_object_kind(cursor)?);
                OptionalLifeEntryState::Event
            }
            OptionalLifeEntryState::Event => {
                cursor.enter("replacement.optional-life-entry.event");
                cursor.consume("enters")?;
                OptionalLifeEntryState::Separator
            }
            OptionalLifeEntryState::Separator => {
                cursor.enter("replacement.optional-life-entry.separator");
                cursor.consume(",")?;
                OptionalLifeEntryState::Payment
            }
            OptionalLifeEntryState::Payment => {
                cursor.enter("replacement.optional-life-entry.payment");
                payment = Some(recognize_optional_life_payment(cursor, None)?);
                OptionalLifeEntryState::ConditionalEntry
            }
            OptionalLifeEntryState::ConditionalEntry => {
                cursor.enter("replacement.optional-life-entry.conditional-entry");
                let binding = payment
                    .as_ref()
                    .map(|node: &OptionalLifePaymentNode| node.binding)
                    .ok_or_else(|| cursor.fail("a bound optional life payment"))?;
                let condition = recognize_cost_not_paid_condition(cursor, binding)?;
                let effect = recognize_tapped_entry_effect(cursor)?;
                cursor.require(
                    effect.object == LocalObjectReference::SourceSelf && effect.tapped,
                    "a tapped entering state applied to the source",
                )?;
                unpaid_entry = Some(ConditionalEnteringStateNode { condition, effect });
                OptionalLifeEntryState::Complete
            }
            OptionalLifeEntryState::Complete => {
                cursor.enter("replacement.optional-life-entry.complete");
                cursor.finish()?;
                return Ok(OptionalLifeEntryNode {
                    object_kind: cursor.require_value(object_kind, "an entering-object kind")?,
                    payment: cursor.require_value(payment, "an optional life payment")?,
                    unpaid_entry: cursor
                        .require_value(unpaid_entry, "an unpaid tapped-entry consequence")?,
                });
            }
        };
    }
}
