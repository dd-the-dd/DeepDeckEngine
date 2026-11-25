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
    // `on this creature` / its name: the source as the counter recipient.
    cursor.replace_source_names(Some(input.name_links.into()));
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
    let program = lower_quantity_replacement_program(cursor, replacement)?;
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [program],
        }),
        &[
            "Recognize a quantity-bearing event with explicit effect/any provenance",
            "Retain shared object-query and opaque counter-name predicates",
            "Resolve arithmetic against the event's typed quantity binding",
            "Preserve event identity and replace only its quantity",
        ],
    ))
}

/// The `replaceEventQuantity` program of a static or an effect-created
/// quantity replacement (CR 614.1a, 611.2a).
pub(in crate::oracle::canonical) fn lower_quantity_replacement_program(
    cursor: &mut OracleTokenCursor<'_>,
    replacement: QuantityReplacementNode,
) -> MachineResult<Value> {
    let event = replacement.event;
    cursor.enter("quantity-replacement.affected-player");
    // The player whose tokens are created or whose permanent receives the
    // counters; no player named means every player's (CR 111.2, 122.6).
    let player = match event.affected_controller {
        None => json!({ "kind": "eachPlayer" }),
        Some(ObjectControllerScope::AbilityController) => controller(),
        Some(ObjectControllerScope::OpponentOfAbilityController) => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
        Some(_) => {
            return Err(cursor.fail(
                "the ability controller, an opponent, or every player as the affected player",
            ));
        }
    };
    cursor.require(
        event.recipient_source != RecipientSourceRelation::Source
            || event.affected_controller.is_none(),
        "the source recipient without another controller relation",
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
        "player": player,
    });
    match event.recipient_source {
        RecipientSourceRelation::Any => {}
        RecipientSourceRelation::ExcludeSource => event_ir["excludeSource"] = json!(true),
        RecipientSourceRelation::Source => event_ir["object"] = self_ref(),
    }
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
    Ok(json!({ "kind": "replaceEventQuantity", "event": event_ir, "quantity": quantity }))
}

#[cfg(test)]
#[path = "quantity_replacement_tests.rs"]
mod tests;
