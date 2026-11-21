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
fn optional_scope_cannot_absorb_later_sentences_or_conditional_outcomes() {
    for text in [
        "you may destroy target artifact. Draw a card.",
        "you may destroy target artifact and you may draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("optional-scope-cap", &tokens);
        let failure = recognize_effect_program_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .unwrap_err();
        assert_eq!(failure.state, "program.optional.scope");
    }
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
