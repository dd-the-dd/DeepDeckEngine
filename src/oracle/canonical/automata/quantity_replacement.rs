use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, NumericAtom, NumericExpression, QuantityEventDomain, QuantityEventNode,
    QuantityEventRecognition, QuantityEventVoice, ReplacementObjectBinding,
    recognize_event_bound_numeric_expression, recognize_event_counter_kind, recognize_numeric_atom,
    try_recognize_quantity_event,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct QuantityReplacementNode {
    pub event: QuantityEventNode,
    pub quantity: NumericExpression,
}

fn quantity_expression_has_runtime_binding(expression: &NumericExpression) -> bool {
    match expression {
        NumericExpression::Atom(NumericAtom::Integer(value)) => i32::try_from(*value).is_ok(),
        NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity) => true,
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            quantity_expression_has_runtime_binding(left)
                && quantity_expression_has_runtime_binding(right)
        }
        _ => false,
    }
}

fn recognize_event_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    binding: ReplacementObjectBinding,
) -> MachineResult<ReplacementObjectBinding> {
    cursor.enter("quantity-replacement.object-anaphor");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        cursor.consume("it")?;
    } else {
        // `that permanent` / `that creature`: a nominal anaphor of the one
        // permanent the event puts counters on (CR 122.6).
        cursor.consume("that")?;
        let noun = cursor
            .peek()
            .filter(|raw| {
                [
                    "permanent",
                    "creature",
                    "artifact",
                    "enchantment",
                    "land",
                    "planeswalker",
                    "battle",
                ]
                .iter()
                .any(|noun| raw.eq_ignore_ascii_case(noun))
            })
            .map(str::to_owned)
            .ok_or_else(|| cursor.fail("a permanent noun naming the event's object"))?;
        cursor.consume(&noun)?;
    }
    Ok(binding)
}

fn recognize_preserved_subject(
    cursor: &mut OracleTokenCursor<'_>,
    event: &QuantityEventNode,
) -> MachineResult<()> {
    cursor.enter("quantity-replacement.subject-identity");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
    {
        cursor.consume("of")?;
        // `of each of those kinds of counters` (CR 122.6): every kind the
        // event puts, each with its own quantity.
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
        {
            cursor.enter("quantity-replacement.subject-identity.each-kind");
            cursor.consume_phrase(&["each", "of", "those", "kinds", "of"])?;
        } else {
            cursor.consume("those")?;
        }
        cursor.consume(match event.domain {
            QuantityEventDomain::TokenCreation => "tokens",
            QuantityEventDomain::CounterPlacement => "counters",
        })?;
    } else {
        match event.domain {
            QuantityEventDomain::TokenCreation => cursor.consume("tokens")?,
            QuantityEventDomain::CounterPlacement => {
                let counter = recognize_event_counter_kind(cursor)?;
                cursor.enter("quantity-replacement.counter-identity");
                cursor.require(
                    counter == event.counter,
                    "the same counter identity as the event",
                )?;
            }
        }
    }
    Ok(())
}

/// `that many -1/-1 counters minus one`: an offset written after the
/// counted noun applies to the event's quantity it follows (CR 614.1a).
fn recognize_postposed_quantity_offset(
    cursor: &mut OracleTokenCursor<'_>,
    quantity: NumericExpression,
) -> MachineResult<NumericExpression> {
    let Some(operator) = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("plus") || raw.eq_ignore_ascii_case("minus"))
        .map(str::to_ascii_lowercase)
    else {
        return Ok(quantity);
    };
    cursor.enter("quantity-replacement.postposed-offset");
    cursor.require(
        quantity == NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity),
        "an offset after the event's own quantity (`that many`)",
    )?;
    cursor.consume(&operator)?;
    let offset = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 && i32::try_from(value).is_ok() => value,
        _ => return Err(cursor.fail("a positive literal offset")),
    };
    let left = Box::new(quantity);
    let right = Box::new(NumericExpression::Atom(NumericAtom::Integer(offset)));
    Ok(if operator == "plus" {
        NumericExpression::Add { left, right }
    } else {
        NumericExpression::Subtract { left, right }
    })
}

/// Transactional parent for one complete quantity replacement. Arithmetic,
/// object predicates and opaque counter names remain shared child concepts.
pub(in crate::oracle::canonical) fn recognize_quantity_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<QuantityReplacementNode> {
    match try_recognize_quantity_replacement(cursor) {
        QuantityReplacementRecognition::Parsed(node) => Ok(node),
        QuantityReplacementRecognition::Failed(failure) => Err(failure),
        QuantityReplacementRecognition::NotClaimed => {
            Err(cursor.fail("a quantity replacement event envelope"))
        }
    }
}

pub(in crate::oracle::canonical) enum QuantityReplacementRecognition {
    Parsed(QuantityReplacementNode),
    NotClaimed,
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_recognize_quantity_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> QuantityReplacementRecognition {
    let checkpoint = cursor.checkpoint();
    cursor.enter("quantity-replacement.envelope");
    if cursor.consume("if").is_err() {
        cursor.rollback(checkpoint);
        return QuantityReplacementRecognition::NotClaimed;
    }
    let event = match try_recognize_quantity_event(cursor) {
        QuantityEventRecognition::Parsed(event) => event,
        QuantityEventRecognition::Failed(failure) => {
            return QuantityReplacementRecognition::Failed(failure);
        }
        QuantityEventRecognition::NotClaimed => {
            cursor.rollback(checkpoint);
            return QuantityReplacementRecognition::NotClaimed;
        }
    };
    match recognize_quantity_replacement_body(cursor, event) {
        Ok(node) => QuantityReplacementRecognition::Parsed(node),
        Err(failure) => QuantityReplacementRecognition::Failed(failure),
    }
}

fn recognize_quantity_replacement_body(
    cursor: &mut OracleTokenCursor<'_>,
    event: QuantityEventNode,
) -> MachineResult<QuantityReplacementNode> {
    cursor.enter("quantity-replacement.separator");
    cursor.consume(",")?;
    if event.voice == QuantityEventVoice::ActiveEffect {
        cursor.enter("quantity-replacement.active-subject");
        cursor.consume("it")?;
        cursor.consume(match event.domain {
            QuantityEventDomain::TokenCreation => "creates",
            QuantityEventDomain::CounterPlacement => "puts",
        })?;
    }
    let quantity = recognize_event_bound_numeric_expression(cursor, event.quantity_binding)?;
    cursor.enter("quantity-replacement.numeric-binding");
    cursor.require(quantity_expression_has_runtime_binding(&quantity), "an i32 arithmetic expression over this replacement event's quantity, without external variables")?;
    recognize_preserved_subject(cursor, &event)?;
    let quantity = recognize_postposed_quantity_offset(cursor, quantity)?;
    if event.domain == QuantityEventDomain::TokenCreation
        && event.voice == QuantityEventVoice::Passive
    {
        // `twice that many of those tokens are created instead`.
        cursor.enter("quantity-replacement.passive-tokens");
        cursor.consume_phrase(&["are", "created"])?;
    }
    if event.domain == QuantityEventDomain::CounterPlacement {
        if event.voice == QuantityEventVoice::Passive {
            cursor.consume_phrase(&["are", "put"])?;
        }
        cursor.consume("on")?;
        let object = recognize_event_object_reference(cursor, event.object_binding)?;
        cursor.require(object == event.object_binding, "the same event object")?;
    }
    cursor.enter("quantity-replacement.instead");
    cursor.consume("instead")?;
    cursor.consume(".")?;
    cursor.enter("quantity-replacement.full-consumption");
    cursor.finish()?;
    Ok(QuantityReplacementNode { event, quantity })
}

#[cfg(test)]
#[path = "quantity_replacement_tests.rs"]
mod tests;
