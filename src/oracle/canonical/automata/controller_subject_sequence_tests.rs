use crate::oracle::canonical::LocalPlayerReference;
use crate::oracle::syntax::recognize_entities;

use super::{
    EffectRecognitionContext, LocalEffectNode, OracleTokenCursor,
    recognize_effect_sequence_with_context,
};

#[test]
fn imperative_life_action_defaults_to_controller_but_third_person_does_not() {
    let tokens = recognize_entities("gain 1 life.");
    let mut cursor = OracleTokenCursor::new("controller-subject-test", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .expect("imperative controller life effect parses");
    let [LocalEffectNode::GainLife(life)] = sequence.effects.as_slice() else {
        panic!("expected one life effect");
    };
    assert_eq!(life.player, LocalPlayerReference::Controller);

    let tokens = recognize_entities("target player gains 1 life and loses 1 life.");
    let mut cursor = OracleTokenCursor::new("third-person-subject-test", &tokens);
    assert!(
        recognize_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        )
        .is_err()
    );
}
