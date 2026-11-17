use crate::oracle::canonical::LocalEffectNode;
use crate::oracle::syntax::recognize_oracle_entities;

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
    assert!(matches!(
        node.effects.effects.as_slice(),
        [
            LocalEffectNode::TargetPlayerLife(_),
            LocalEffectNode::GainLife(_)
        ]
    ));
}

#[test]
fn missing_separator_is_a_committed_parent_failure() {
    let failure = parse("Whenever another creature dies target player loses 1 life.")
        .expect_err("missing separator must remain visible");
    assert_eq!(failure.state, "trigger.permanent-died.separator");
    assert_eq!(failure.actual.as_deref(), Some("target"));
}
