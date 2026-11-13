use crate::oracle::syntax::recognize_entities;

use super::super::{
    EffectConnector, EffectRecognitionContext, LocalEffectNode, LocalObjectReference, NumericAtom,
    NumericExpression, NumericRounding, SourceFaceChange, SourceMoveController, SourceZone,
};
use super::{OracleTokenCursor, recognize_effect_sequence_with_context};

fn source_context(source_cast_x: bool) -> EffectRecognitionContext {
    EffectRecognitionContext {
        source: Some(LocalObjectReference::SourceSelf),
        source_cast_x,
    }
}

#[test]
fn source_context_keeps_exile_and_transformed_return_as_two_moves() {
    let tokens = recognize_entities(
        "exile this creature, then return her to the battlefield transformed under her owner's control.",
    );
    let mut cursor = OracleTokenCursor::new("effect-sequence-context-test", &tokens);

    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf),
    )
    .expect("source move sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    let [
        LocalEffectNode::MoveSource(exile),
        LocalEffectNode::MoveSource(returned),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected two independent source moves");
    };
    assert_eq!(
        (exile.from, exile.to),
        (SourceZone::Battlefield, SourceZone::Exile)
    );
    assert_eq!(
        (returned.from, returned.to),
        (SourceZone::Exile, SourceZone::Battlefield)
    );
    assert_eq!(returned.face_change, SourceFaceChange::Transform);
    assert_eq!(returned.controller, SourceMoveController::Owner);
    assert_eq!(exile.source, returned.source);
}

#[test]
fn source_context_composes_counter_placement_and_rounded_draw() {
    let tokens =
        recognize_entities("put X +1/+1 counters on him. Then draw half X cards, rounded down.");
    let mut cursor = OracleTokenCursor::new("effect-sequence-context-test", &tokens);

    let sequence =
        recognize_effect_sequence_with_context(&mut cursor, &[], None, &source_context(true))
            .expect("contextual sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    let [
        LocalEffectNode::PutCounters(counters),
        LocalEffectNode::DrawCards(draw),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected independent put-counters and draw effects");
    };
    assert_eq!(counters.permanent, LocalObjectReference::SourceSelf);
    assert_eq!(counters.counter, "+1/+1");
    assert_eq!(counters.count, NumericExpression::SourceCastX);
    assert_eq!(
        draw.count,
        NumericExpression::Divide {
            left: Box::new(NumericExpression::SourceCastX),
            right: Box::new(NumericExpression::Atom(NumericAtom::Integer(2))),
            rounding: NumericRounding::Down,
        }
    );
}

#[test]
fn source_context_composes_literal_effects_with_and() {
    let tokens = recognize_entities("put a +1/+1 counter on him and draw two cards.");
    let mut cursor = OracleTokenCursor::new("effect-sequence-context-test", &tokens);

    let sequence =
        recognize_effect_sequence_with_context(&mut cursor, &[], None, &source_context(false))
            .expect("literal sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::And]);
    let [
        LocalEffectNode::PutCounters(counters),
        LocalEffectNode::DrawCards(draw),
    ] = sequence.effects.as_slice()
    else {
        panic!("expected independent put-counters and draw effects");
    };
    assert_eq!(
        counters.count,
        NumericExpression::Atom(NumericAtom::Integer(1))
    );
    assert_eq!(draw.count, NumericExpression::Atom(NumericAtom::Integer(2)));
}

#[test]
fn source_anaphor_requires_a_bound_source() {
    let tokens = recognize_entities("put a +1/+1 counter on him.");
    let mut cursor = OracleTokenCursor::new("effect-sequence-context-test", &tokens);

    let failure = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .expect_err("unbound source anaphor must fail");

    assert_eq!(failure.actual.as_deref(), Some("him"));
    assert!(failure.expected.contains("source binding"));
}

#[test]
fn source_cast_x_failure_reports_the_invalid_operand() {
    let tokens = recognize_entities("draw half 3 cards, rounded down.");
    let mut cursor = OracleTokenCursor::new("effect-sequence-context-test", &tokens);

    let failure =
        recognize_effect_sequence_with_context(&mut cursor, &[], None, &source_context(true))
            .expect_err("half of a literal is outside the source-cast-X grammar");

    assert_eq!(failure.actual.as_deref(), Some("3"));
    assert_eq!(failure.expected, "the source cast X value");
}
