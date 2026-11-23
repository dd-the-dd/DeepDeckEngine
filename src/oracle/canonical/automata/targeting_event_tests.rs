use crate::oracle::canonical::MachineFailure;
use crate::oracle::syntax::recognize_entities;

use super::{
    BecameTargetEventNode, ObjectControllerScope, OracleTokenCursor, TargetedObjectDomain,
    TargetingControllerPolicy, TargetingStackKind, recognize_became_target_event,
    recognize_targeting_stack_selector,
};

/// The clause child starts after the trigger word, which the event clause
/// owns, and leaves the separator to it.
fn recognize(
    text: &str,
) -> (
    Result<BecameTargetEventNode, MachineFailure>,
    Option<String>,
) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("became-target-test", &tokens);
    cursor.consume("whenever").expect("trigger word");
    let result = recognize_became_target_event(&mut cursor);
    (result, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn permanent_and_spell_subjects_share_one_targeting_event() {
    let (node, next) = recognize(
        "Whenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.",
    );
    let node = node.expect("targeting event parses");
    assert_eq!(next.as_deref(), Some(","), "the clause owns the separator");

    assert_eq!(node.objects.len(), 2);
    assert_eq!(node.objects[0].domain, TargetedObjectDomain::Permanent);
    assert_eq!(node.objects[1].domain, TargetedObjectDomain::Spell);
    assert!(
        node.objects
            .iter()
            .all(|selector| { selector.controller == ObjectControllerScope::AbilityController })
    );
    assert_eq!(
        node.targeting_stack.kinds,
        [
            TargetingStackKind::Spell,
            TargetingStackKind::ActivatedAbility,
            TargetingStackKind::TriggeredAbility,
        ]
    );
    assert_eq!(
        node.targeting_stack.controller,
        Some(ObjectControllerScope::OpponentOfAbilityController)
    );
}

#[test]
fn artifact_permanent_only_and_controlled_source_are_generic_variants() {
    let (node, _) = recognize(
        "Whenever an artifact you control becomes the target of an ability you control, draw two cards.",
    );
    let node = node.expect("artifact targeting event parses");

    assert_eq!(node.objects.len(), 1);
    assert_eq!(node.objects[0].domain, TargetedObjectDomain::Permanent);
    assert_eq!(
        node.targeting_stack.controller,
        Some(ObjectControllerScope::AbilityController)
    );
}

#[test]
fn dangling_subject_union_is_a_failure_at_the_missing_selector() {
    let (failure, _) = recognize(
        "Whenever a creature you control or becomes the target of a spell an opponent controls, draw a card.",
    );
    let failure = failure.expect_err("dangling subject union must fail");

    assert_eq!(failure.state, "event.became-target.subject.article");
    assert_eq!(failure.actual.as_deref(), Some("becomes"));
}

#[test]
fn object_selectors_require_a_targeting_controller_relation() {
    let (failure, _) =
        recognize("Whenever a creature you control becomes the target of a spell, draw a card.");
    let failure = failure.expect_err("unscoped targeting source must fail");

    assert_eq!(
        failure.state,
        "event.became-target.source-controller.subject"
    );
    assert_eq!(failure.actual.as_deref(), Some(","));
}

#[test]
fn the_source_observer_reads_an_optional_controller() {
    for (text, controller) in [
        ("a spell or ability, sacrifice it.", None),
        (
            "a spell or ability an opponent controls, draw a card.",
            Some(ObjectControllerScope::OpponentOfAbilityController),
        ),
        (
            "a spell you control, draw a card.",
            Some(ObjectControllerScope::AbilityController),
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("targeting-stack-test", &tokens);
        let node =
            recognize_targeting_stack_selector(&mut cursor, TargetingControllerPolicy::Optional)
                .expect(text);
        assert_eq!(node.controller, controller, "{text}");
        assert_eq!(cursor.peek(), Some(","), "{text}");
    }
}
