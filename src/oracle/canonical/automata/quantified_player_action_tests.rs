use super::super::{
    EffectRecognitionContext, LocalEffectNode, recognize_effect_sequence_with_context,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn quantified_scope_ends_before_explicit_controller_subject() {
    let tokens = recognize_entities("Each opponent loses 2 life and you gain 2 life.");
    let mut cursor = OracleTokenCursor::new("quantified-player-scope", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .unwrap();
    assert!(matches!(
        sequence.effects.as_slice(),
        [
            LocalEffectNode::QuantifiedPlayerAction(_),
            LocalEffectNode::GainLife(_)
        ]
    ));
    let LocalEffectNode::QuantifiedPlayerAction(first) = &sequence.effects[0] else {
        unreachable!()
    };
    assert_eq!(
        first.players,
        PlayerSetReferenceNode::OpponentsOfAbilityController
    );
    assert!(matches!(
        first.actions.as_slice(),
        [QuantifiedPlayerAction::Life(LifeActionNode {
            action: super::super::TargetPlayerLifeAction::Lose,
            ..
        })]
    ));
}

#[test]
fn discard_and_life_are_independent_subjectless_action_children() {
    for text in [
        "Each opponent discards a card.",
        "Each opponent discards two cards at random.",
        "Each opponent gains three life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("quantified-player-siblings", &tokens);
        recognize_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .unwrap();
    }
}

#[test]
fn singular_subject_rejects_bad_agreement_and_implicit_scope_extension() {
    for text in [
        "Each opponents loses 2 life.",
        "Each opponent lose 2 life.",
        "Each opponent loses 2 life and gains.",
        "Each opponent loses 2 life and sings a song.",
        "Each opponent discards a card and, loses 2 life.",
        "Each opponent discards two card.",
        "Each opponent sings a song.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("quantified-player-near-miss", &tokens);
        assert!(
            recognize_effect_sequence_with_context(
                &mut cursor,
                &[],
                None,
                &EffectRecognitionContext::default()
            )
            .is_err(),
            "{text}"
        );
    }
}
