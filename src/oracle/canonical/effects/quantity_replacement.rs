use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum QuantityReplacementParseAttempt {
    Parsed(CanonicalRuleDraft),
    NotClaimed,
    Failed(MachineFailure),
}

/// Parse once, then lower the typed replacement. Envelope checks only retain
/// parent ownership of multi-sentence or nested conditional programs.
pub(in crate::oracle::canonical) fn try_parse_promoted_quantity_replacement(
    input: &AbilityInput<'_>,
) -> QuantityReplacementParseAttempt {
    if input.tokens.iter().filter(|token| token.raw == ".").count() != 1
        || !input.tokens.last().is_some_and(|token| token.raw == ".")
        || input
            .tokens
            .iter()
            .skip(1)
            .any(|token| token.raw.eq_ignore_ascii_case("if"))
    {
        return QuantityReplacementParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("quantity-replacement", input.tokens);
    let replacement = match try_recognize_quantity_replacement(&mut cursor) {
        QuantityReplacementRecognition::Parsed(node) => node,
        QuantityReplacementRecognition::NotClaimed => {
            return QuantityReplacementParseAttempt::NotClaimed;
        }
        QuantityReplacementRecognition::Failed(failure) => {
            return QuantityReplacementParseAttempt::Failed(failure);
        }
    };
    cursor.enter("quantity-replacement.source-domain");
    if let Err(failure) = cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active quantity replacement",
    ) {
        return QuantityReplacementParseAttempt::Failed(failure);
    }
    match lower_quantity_replacement(&mut cursor, replacement) {
        Ok(draft) => QuantityReplacementParseAttempt::Parsed(draft),
        Err(failure) => QuantityReplacementParseAttempt::Failed(failure),
    }
}

fn lower_quantity_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    replacement: QuantityReplacementNode,
) -> MachineResult<CanonicalRuleDraft> {
    let event = replacement.event;
    cursor.require(
        event.affected_controller == ObjectControllerScope::AbilityController,
        "the typed affected controller retained by the event",
    )?;
    let mut event_ir = json!({
        "kind": match event.domain {
            QuantityEventDomain::TokenCreation => "tokenCreation",
            QuantityEventDomain::CounterPlacement => "counterPlacement",
        },
        "origin": match event.origin {
            QuantityEventOrigin::Any => "any",
            QuantityEventOrigin::Effect => "effect",
        },
        "minimum": event.minimum,
        "player": controller(),
    });
    if event.domain == QuantityEventDomain::CounterPlacement {
        event_ir["recipientKind"] = json!("permanent");
    }
    if let Some(object) = &event.object {
        let predicate = match event.domain {
            // A created token is matched by its definition (CR 111.4).
            QuantityEventDomain::TokenCreation => lower_card_object_predicate(object),
            QuantityEventDomain::CounterPlacement => lower_permanent_object_predicate(object),
        };
        if !predicate.is_null() {
            event_ir["where"] = predicate;
        }
    }
    if let Some(counter) = &event.counter {
        event_ir["counter"] = json!(counter.as_str());
    }
    let quantity =
        lower_numeric_expression_for_event(&replacement.quantity, event.quantity_binding)
            .ok_or_else(|| cursor.fail("a lowerable event-bound quantity expression"))?;
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{ "kind": "replaceEventQuantity", "event": event_ir, "quantity": quantity }],
        }),
        &[
            "Recognize a quantity-bearing event with explicit effect/any provenance",
            "Retain shared object-query and opaque counter-name predicates",
            "Resolve arithmetic against the event's typed quantity binding",
            "Preserve event identity and replace only its quantity",
        ],
    ))
}

#[cfg(test)]
#[path = "quantity_replacement_tests.rs"]
mod tests;
