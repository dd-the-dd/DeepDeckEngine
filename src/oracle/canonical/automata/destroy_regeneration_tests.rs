use crate::oracle::syntax::recognize_entities;

use super::super::{
    ObjectCardType, TargetCardinality, ZoneChangeAction, ZoneChangeRegenerationPolicy,
};
use super::{OracleTokenCursor, recognize_destroy_regeneration_prohibition};

#[test]
fn destroy_and_regeneration_amendment_keep_independent_typed_concepts() {
    let tokens = recognize_entities("Destroy target creature. It can't be regenerated.");
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration", &tokens);
    let node = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect("destroy plus regeneration prohibition parses");
    assert_eq!(node.action, ZoneChangeAction::Destroy);
    assert_eq!(node.target.slots.len(), 1);
    assert_eq!(
        node.target.slots[0].cardinality,
        TargetCardinality::EXACTLY_ONE
    );
    assert_eq!(
        node.regeneration,
        ZoneChangeRegenerationPolicy::Prohibited {
            referenced_type: None,
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn selector_is_delegated_to_the_shared_target_automaton() {
    let tokens =
        recognize_entities("Destroy target nonartifact creature. It can't be regenerated.");
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration", &tokens);
    let node = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect("generalized permanent selector parses");
    assert_eq!(node.action, ZoneChangeAction::Destroy);
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn destroyed_type_this_way_uses_shared_card_type_vocabulary() {
    let tokens = recognize_entities(
        "Destroy target nonartifact creature. A creature destroyed this way can't be regenerated.",
    );
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration", &tokens);
    let node = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect("typed result anaphor parses");
    assert_eq!(
        node.regeneration,
        ZoneChangeRegenerationPolicy::Prohibited {
            referenced_type: Some(ObjectCardType::Creature),
        }
    );
}

#[test]
fn destroyed_type_this_way_rejects_a_mismatched_target() {
    let tokens = recognize_entities(
        "Destroy target artifact. A creature destroyed this way can't be regenerated.",
    );
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration", &tokens);
    let failure = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect_err("the typed result anaphor cannot change target domain");
    assert_eq!(
        failure.state,
        "selector.predicate-atom.complete"
    );
    assert!(failure.expected.contains("referenced card type"));
}

#[test]
fn malformed_regeneration_verb_fails_at_the_amendment_child() {
    let tokens = recognize_entities("Destroy target creature. It can't be restored.");
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration", &tokens);
    let failure = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect_err("unknown amendment verb must fail visibly");
    assert_eq!(failure.state, "sequence.destroy-regeneration.regeneration");
    assert_eq!(failure.actual.as_deref(), Some("restored"));
    assert!(failure.expected.contains("regenerated"));
}

#[test]
fn unrelated_second_sentence_is_not_claimed_as_an_amendment() {
    let tokens = recognize_entities("Destroy target creature. It deals 2 damage to any target.");
    let mut cursor = OracleTokenCursor::new("test.destroy-not-amendment", &tokens);
    let node = super::recognize_zone_change_with_regeneration_amendment(&mut cursor)
        .expect("ordinary destroy remains an ordinary zone-change action");
    let super::super::LocalEffectNode::ZoneChange(node) = node else {
        panic!("expected an ordinary zone change");
    };
    assert_eq!(node.regeneration, ZoneChangeRegenerationPolicy::Allowed);
    assert_eq!(cursor.peek(), Some("."));
}

#[test]
fn duration_after_regeneration_is_not_silently_accepted() {
    let tokens = recognize_entities("Destroy target creature. It can't be regenerated this turn.");
    let mut cursor = OracleTokenCursor::new("test.destroy-regeneration-duration", &tokens);
    let failure = recognize_destroy_regeneration_prohibition(&mut cursor)
        .expect_err("duration changes the policy semantics and must remain unsupported");
    assert_eq!(
        failure.state,
        "sequence.destroy-regeneration.amendment-terminator"
    );
    assert_eq!(failure.actual.as_deref(), Some("this"));
}
