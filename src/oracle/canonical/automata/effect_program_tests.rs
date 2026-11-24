use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn optional_is_a_program_operator_and_connectors_stay_in_its_body() {
    let tokens = recognize_entities("you may destroy target artifact and draw a card.");
    let mut cursor = OracleTokenCursor::new("optional-program-test", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    let EffectProgramNode::Optional { player, body } = program else {
        panic!("permission must be a program node")
    };
    assert_eq!(player, LocalPlayerReference::Controller);
    let EffectProgramNode::Sequence(sequence) = *body else {
        panic!("ordinary body must remain shared sequence")
    };
    assert_eq!(sequence.effects.len(), 2);
    assert_eq!(sequence.connectors, vec![EffectConnector::And]);
    cursor.finish().unwrap();
}

#[test]
fn optional_scope_is_its_own_sentence_and_later_sentences_are_independent_steps() {
    // CR 608.2c: without an outcome connector, the next sentence runs
    // whether or not the permission was used; it is never absorbed.
    let tokens = recognize_entities("you may destroy target artifact. Then draw a card.");
    let mut cursor = OracleTokenCursor::new("optional-scope-steps", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    cursor.finish().unwrap();
    let EffectProgramNode::Steps(steps) = program else {
        panic!("a later sentence must become an independent program step")
    };
    let [
        EffectProgramNode::Optional { body, .. },
        EffectProgramNode::Sequence(after),
    ] = steps.as_slice()
    else {
        panic!("the permission owns only its first sentence: {steps:?}")
    };
    assert_eq!(body.action_count(), 1);
    assert_eq!(after.effects.len(), 1);
}

#[test]
fn optional_scope_rejects_a_second_permission() {
    let text = "you may destroy target artifact and you may draw a card.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("optional-scope-cap", &tokens);
    let failure = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap_err();
    assert_eq!(failure.state, "program.optional.scope", "{text}");
}

#[test]
fn a_postposed_condition_in_a_permission_is_read_by_the_shared_sequence() {
    let text = "you may destroy target artifact if you control a creature. Draw a card.";
    let tokens = recognize_entities(text);
    // A parent that lowers no sentence conditional still refuses it, in the
    // sequence that reads the condition rather than in the permission scope.
    let mut cursor = OracleTokenCursor::new("optional-scope-condition", &tokens);
    let failure = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap_err();
    assert_ne!(failure.state, "program.optional.scope", "{text}");
    let mut cursor = OracleTokenCursor::new("optional-scope-condition", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default().with_sentence_conditionals(),
    )
    .expect("the permission body reads its postposed condition");
    let EffectProgramNode::Steps(steps) = program else {
        panic!("the permission and the next sentence are independent steps")
    };
    let EffectProgramNode::Optional { body, .. } = &steps[0] else {
        panic!("the permission comes first")
    };
    assert_eq!(body.inner_sequence().conditionals.len(), 1);
}

#[test]
fn established_optional_draw_action_keeps_its_existing_sentence_scope() {
    let tokens = recognize_entities("you may draw a card. You gain 2 life.");
    let mut cursor = OracleTokenCursor::new("optional-action-compatibility", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    assert!(matches!(&program, EffectProgramNode::Sequence(_)));
    assert_eq!(program.action_count(), 2);
}
