use crate::oracle::canonical::MachineFailure;
use crate::oracle::syntax::recognize_entities;

use super::{
    BecameTargetTriggeredAbilityNode, ObjectControllerScope, OracleTokenCursor,
    TargetedObjectDomain, TargetingStackKind, recognize_became_target_triggered_ability,
};

fn recognize(text: &str) -> Result<BecameTargetTriggeredAbilityNode, MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("became-target-test", &tokens);
    recognize_became_target_triggered_ability(&mut cursor, &[], None)
}

#[test]
fn permanent_and_spell_subjects_share_one_targeting_event() {
    let node = recognize(
        "Whenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.",
    )
    .expect("targeting trigger parses");

    assert_eq!(node.event.objects.len(), 2);
    assert_eq!(
        node.event.objects[0].domain,
        TargetedObjectDomain::Permanent
    );
    assert_eq!(node.event.objects[1].domain, TargetedObjectDomain::Spell);
    assert!(
        node.event
            .objects
            .iter()
            .all(|selector| { selector.controller == ObjectControllerScope::AbilityController })
    );
    assert_eq!(
        node.event.targeting_stack.kinds,
        [
            TargetingStackKind::Spell,
            TargetingStackKind::ActivatedAbility,
            TargetingStackKind::TriggeredAbility,
        ]
    );
    assert_eq!(
        node.event.targeting_stack.controller,
        ObjectControllerScope::OpponentOfAbilityController
    );
}

#[test]
fn artifact_permanent_only_and_controlled_source_are_generic_variants() {
    let node = recognize(
        "Whenever an artifact you control becomes the target of an ability you control, draw two cards.",
    )
    .expect("artifact targeting trigger parses");

    assert_eq!(node.event.objects.len(), 1);
    assert_eq!(
        node.event.objects[0].domain,
        TargetedObjectDomain::Permanent
    );
    assert_eq!(
        node.event.targeting_stack.controller,
        ObjectControllerScope::AbilityController
    );
}

#[test]
fn dangling_subject_union_is_a_committed_failure() {
    let failure = recognize(
        "Whenever a creature you control or becomes the target of a spell an opponent controls, draw a card.",
    )
    .expect_err("dangling subject union must fail");

    assert_eq!(failure.state, "event.became-target.subject.article");
    assert_eq!(failure.actual.as_deref(), Some("becomes"));
}

#[test]
fn targeting_source_requires_a_controller_relation() {
    let failure =
        recognize("Whenever a creature you control becomes the target of a spell, draw a card.")
            .expect_err("unscoped targeting source must fail");

    assert_eq!(
        failure.state,
        "event.became-target.source-controller.subject"
    );
    assert_eq!(failure.actual.as_deref(), Some(","));
}
