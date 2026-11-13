use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{TargetBinding, recognize_untap_target_effect};

#[test]
fn preserves_the_parent_target_binding_and_stops_before_punctuation() {
    let tokens = recognize_entities("Untap it.");
    let mut cursor = OracleTokenCursor::new("test.untap-target", &tokens);
    let binding = TargetBinding {
        effect_index: 2,
        slot_index: 1,
    };

    let fragment = recognize_untap_target_effect(&mut cursor, Some(binding))
        .expect("bound target untap parses");

    assert_eq!(fragment.node.permanent, binding);
    assert_eq!(fragment.end_position, cursor.position());
    assert_eq!(cursor.peek(), Some("."));
}

#[test]
fn rejects_anaphora_without_a_target_antecedent() {
    let tokens = recognize_entities("Untap it.");
    let mut cursor = OracleTokenCursor::new("test.unbound-untap-target", &tokens);

    let failure = recognize_untap_target_effect(&mut cursor, None)
        .expect_err("unbound target anaphora must remain visible");

    assert_eq!(failure.state, "effect.untap-target.reference");
    assert_eq!(failure.actual.as_deref(), Some("it"));
}
