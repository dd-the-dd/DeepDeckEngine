use crate::oracle::canonical::MachineFailure;
use crate::oracle::syntax::recognize_entities;

use super::super::ObjectControllerScope;
use super::{
    EventMultiplicity, ObjectActionEventNode, ObjectActionVerb, OracleTokenCursor,
    recognize_object_action_event,
};

/// The clause child starts after the trigger word, which the event clause
/// owns, and leaves the separator to it.
fn recognize(
    text: &str,
) -> (
    Result<ObjectActionEventNode, MachineFailure>,
    Option<String>,
) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-action-test", &tokens);
    cursor.consume("whenever").expect("trigger word");
    let result = recognize_object_action_event(&mut cursor);
    (result, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn a_controlled_creature_exploiting_a_nontoken_creature_is_one_object_action() {
    let (node, next) = recognize(
        "Whenever a creature you control exploits a nontoken creature, create a 2/2 black Zombie creature token.",
    );
    let node = node.expect("Skull Skaab's event parses");
    assert_eq!(next.as_deref(), Some(","), "the clause owns the separator");
    assert_eq!(node.verb, ObjectActionVerb::Exploit);
    assert_eq!(node.subject.multiplicity, EventMultiplicity::PerObject);
    assert_eq!(
        node.subject.query.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(node.counterpart.controller, None);
}

#[test]
fn another_opponent_creature_is_the_same_frame() {
    // Unused combination: an opponent's creature, the source excluded.
    let (node, _) = recognize(
        "Whenever another creature an opponent controls exploits a creature, draw a card.",
    );
    let node = node.expect("the sibling subject parses");
    assert!(node.subject.exclude_source);
    assert_eq!(
        node.subject.query.controller,
        Some(ObjectControllerScope::OpponentOfAbilityController)
    );
}

#[test]
fn another_verb_after_the_subject_is_a_mismatch_at_the_verb() {
    let (failure, _) =
        recognize("Whenever a creature you control fights, put a +1/+1 counter on it.");
    let failure = failure.expect_err("`fights` is no object-action verb");
    assert_eq!(failure.state, "event.object-action.verb");
}

#[test]
fn a_plural_subject_is_not_one_acting_permanent() {
    let (failure, _) =
        recognize("Whenever one or more creatures you control exploit a creature, draw a card.");
    let failure = failure.expect_err("a plural subject fails");
    assert_eq!(failure.state, "event.object-action.subject");
}
