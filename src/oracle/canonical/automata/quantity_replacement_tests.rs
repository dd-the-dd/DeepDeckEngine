use super::*;
use crate::oracle::canonical::automata::{
    ObjectCardType, ObjectPredicate, QuantityEventOrigin, recognize_numeric_expression,
};
use crate::oracle::syntax::{recognize_entities, resolve_counter_name_spans};

fn parse(text: &str) -> MachineResult<QuantityReplacementNode> {
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("quantity-replacement-test", &tokens);
    recognize_quantity_replacement(&mut cursor)
}

#[test]
fn passive_named_counter_event_exports_amount_and_object_bindings() {
    let node = parse("If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead.").unwrap();
    assert_eq!(node.event.origin, QuantityEventOrigin::Any);
    assert_eq!(node.event.domain, QuantityEventDomain::CounterPlacement);
    assert_eq!(node.event.minimum, 1);
    assert_eq!(node.event.counter.as_ref().unwrap().as_str(), "+1/+1");
    assert_eq!(
        node.event.object,
        Some(ObjectPredicate::CardType(ObjectCardType::Creature))
    );
    assert_eq!(
        node.quantity,
        NumericExpression::add(
            NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity),
            NumericExpression::from(1)
        )
    );
}

#[test]
fn active_effect_events_preserve_origin_and_leave_counter_kind_unrestricted() {
    for (text, domain) in [
        (
            "If an effect would create one or more tokens under your control, it creates twice that many of those tokens instead.",
            QuantityEventDomain::TokenCreation,
        ),
        (
            "If an effect would put one or more counters on a permanent you control, it puts twice that many of those counters on that permanent instead.",
            QuantityEventDomain::CounterPlacement,
        ),
    ] {
        let node = parse(text).unwrap();
        assert_eq!(node.event.domain, domain);
        assert_eq!(node.event.origin, QuantityEventOrigin::Effect);
        assert_eq!(node.event.counter, None);
        assert_eq!(
            node.quantity,
            NumericExpression::multiply(
                NumericExpression::from(2),
                NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity)
            )
        );
    }
}

#[test]
fn opaque_counter_and_coordinated_object_query_use_the_same_children() {
    let node = parse("If one or more flux capacitor calibration counters would be put on an artifact or creature you control, that many plus two flux capacitor calibration counters are put on it instead.").unwrap();
    assert_eq!(
        node.event.counter.as_ref().unwrap().as_str(),
        "flux capacitor calibration"
    );
    assert_eq!(
        node.event.object,
        Some(ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Creature)
        ]))
    );
}

#[test]
fn event_quantity_uses_existing_arithmetic_precedence_and_explicit_binding() {
    let tokens = recognize_entities("that many plus two times three counters");
    let mut cursor = OracleTokenCursor::new("event-bound-arithmetic", &tokens);
    let expression = recognize_event_bound_numeric_expression(
        &mut cursor,
        EventNumericBinding::ReplacementQuantity,
    )
    .unwrap();
    assert_eq!(
        expression,
        NumericExpression::add(
            NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity),
            NumericExpression::multiply(NumericExpression::from(2), NumericExpression::from(3))
        )
    );
    assert_eq!(cursor.peek(), Some("counters"));

    let mut unbound = OracleTokenCursor::new("unbound-arithmetic", &tokens);
    let failure = recognize_numeric_expression(&mut unbound, None).unwrap_err();
    assert_eq!(failure.state, "numeric.event-anaphor.resolve");
}

#[test]
fn reject_identity_change_external_x_and_unconsumed_remainder() {
    for (text, state) in [
        (
            "If one or more +1/+1 counters would be put on a creature you control, that many plus one -1/-1 counters are put on it instead.",
            "quantity-replacement.counter-identity",
        ),
        (
            "If one or more ice counters would be put on a creature you control, that many plus X ice counters are put on it instead.",
            "quantity-replacement.numeric-binding",
        ),
        (
            "If one or more ice counters would be put on a creature you control, that many plus one ice counters are put on it instead. Draw a card.",
            "quantity-replacement.full-consumption",
        ),
    ] {
        let failure = parse(text).unwrap_err();
        assert_eq!(failure.state, state);
    }
}

#[test]
fn lexical_operator_and_reference_words_are_typed_before_recognition() {
    let tokens = recognize_entities("effect puts twice thrice that many counters are put");
    assert!(tokens.iter().all(|t| t.token_type != "unknown"));
    for raw in ["twice", "thrice"] {
        assert_eq!(
            tokens.iter().find(|t| t.raw == raw).unwrap().token_type,
            "numericOperatorWord"
        );
    }
    let node = parse("If an effect would create one or more tokens under your control, it creates thrice that many of those tokens instead.").unwrap();
    assert_eq!(
        node.quantity,
        NumericExpression::multiply(
            NumericExpression::from(3),
            NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity)
        )
    );
}

#[test]
fn event_quantity_cannot_escape_its_producer_during_lowering() {
    use crate::oracle::canonical::{lower_numeric_expression, lower_numeric_expression_for_event};
    let bound = NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity);
    let expression = NumericExpression::add(bound.clone(), NumericExpression::from(1));
    assert!(lower_numeric_expression(&bound).is_none());
    assert!(lower_numeric_expression(&expression).is_none());
    assert!(
        lower_numeric_expression_for_event(&expression, EventNumericBinding::DamageAmount)
            .is_none()
    );
    let valid =
        lower_numeric_expression_for_event(&expression, EventNumericBinding::ReplacementQuantity)
            .unwrap();
    assert_eq!(valid["left"]["kind"], "eventQuantity");
    assert_eq!(valid["right"]["value"], 1);
}

#[test]
fn passive_token_creation_is_the_same_quantity_event() {
    for (text, object) in [
        (
            "If one or more tokens would be created under your control, twice that many of those tokens are created instead.",
            None,
        ),
        (
            "If one or more creature tokens would be created under your control, twice that many of those tokens are created instead.",
            Some(ObjectPredicate::CardType(ObjectCardType::Creature)),
        ),
    ] {
        let node = parse(text).unwrap();
        assert_eq!(
            node.event.domain,
            QuantityEventDomain::TokenCreation,
            "{text}"
        );
        assert_eq!(node.event.origin, QuantityEventOrigin::Any, "{text}");
        assert_eq!(node.event.voice, QuantityEventVoice::Passive, "{text}");
        assert_eq!(node.event.object, object, "{text}");
        assert_eq!(
            node.quantity,
            NumericExpression::multiply(
                NumericExpression::from(2),
                NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity)
            )
        );
    }
}

#[test]
fn passive_token_creation_needs_the_controller_and_the_same_tokens() {
    // Every player's tokens, or other tokens created instead, are other events.
    for text in [
        "If one or more tokens would be created, twice that many of those tokens are created instead.",
        "If one or more tokens would be created under your control, those tokens plus a Clue token are created instead.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}
