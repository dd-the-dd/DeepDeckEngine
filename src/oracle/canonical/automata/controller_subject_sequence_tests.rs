use crate::oracle::canonical::LocalPlayerReference;
use crate::oracle::syntax::recognize_entities;

use super::super::life_action::LifeActionNode;
use super::super::player_action::{PlayerActionNode, SingularPlayerAction};
use super::super::source_sacrifice::atomic_action::player_subject::{
    PlayerBinding, PlayerEffectSubject,
};
use super::super::{TargetPlayerLifeAction, TargetPlayerLifeEffectNode};
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

    // A coordinated third-person predicate shares its declared subject: the
    // second verb is bound to the same target player, never defaulted to the
    // controller (CR 115.10 target identity, carried by the shared binding).
    let tokens = recognize_entities("target player gains 1 life and loses 1 life.");
    let mut cursor = OracleTokenCursor::new("third-person-subject-test", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
    .expect("coordinated third-person life predicate parses");
    let [
        LocalEffectNode::TargetPlayerLife(TargetPlayerLifeEffectNode {
            player: target,
            declares_target: true,
            action: TargetPlayerLifeAction::Gain,
            ..
        }),
        LocalEffectNode::PlayerAction(PlayerActionNode {
            player: PlayerEffectSubject::Bound(PlayerBinding(bound)),
            action:
                SingularPlayerAction::Life(LifeActionNode {
                    action: TargetPlayerLifeAction::Lose,
                    ..
                }),
        }),
    ] = sequence.effects.as_slice()
    else {
        panic!(
            "expected the second verb bound to the declared target player: {:?}",
            sequence.effects
        );
    };
    assert_eq!(*bound, target.effect_index);
}
