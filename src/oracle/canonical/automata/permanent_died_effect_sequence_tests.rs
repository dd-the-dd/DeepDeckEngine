use crate::oracle::canonical::LocalEffectNode;
use crate::oracle::syntax::recognize_oracle_entities;

use super::super::life_clause::LifeActionNode;
use super::super::player_action::{PlayerActionNode, SingularPlayerAction};
use super::super::source_sacrifice::atomic_action::player_subject::PlayerEffectSubject;
use super::super::{TargetPlayerLifeAction, TargetPlayerLifeEffectNode};
use super::{OracleTokenCursor, recognize_permanent_died_effect_sequence};

fn parse(text: &str) -> super::MachineResult<super::PermanentDiedEffectSequenceNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("permanent-died-effect-sequence-test", &tokens);
    recognize_permanent_died_effect_sequence(&mut cursor, &[], None)
}

#[test]
fn delegates_death_event_and_independent_life_actions_to_shared_children() {
    let node = parse(
        "Whenever this creature or another creature dies, target player loses 1 life and you gain 1 life.",
    )
    .expect("death parent should compose event and sequence children");
    assert_eq!(node.event.alternatives.len(), 2);
    // Each clause keeps its own subject: the declared target player loses
    // life and the explicit `you` subject is the shared controller player
    // action, not a second target or a combined life operation.
    assert!(
        matches!(
            node.effects.effects.as_slice(),
            [
                LocalEffectNode::TargetPlayerLife(TargetPlayerLifeEffectNode {
                    declares_target: true,
                    action: TargetPlayerLifeAction::Lose,
                    ..
                }),
                LocalEffectNode::PlayerAction(PlayerActionNode {
                    player: PlayerEffectSubject::Controller,
                    action: SingularPlayerAction::Life(LifeActionNode {
                        action: TargetPlayerLifeAction::Gain,
                        ..
                    }),
                })
            ]
        ),
        "{:?}",
        node.effects.effects
    );
}

#[test]
fn missing_separator_is_a_committed_parent_failure() {
    let failure = parse("Whenever another creature dies target player loses 1 life.")
        .expect_err("missing separator must remain visible");
    assert_eq!(failure.state, "trigger.permanent-died.separator");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}
