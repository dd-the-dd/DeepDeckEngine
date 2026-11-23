use super::super::{ObjectControllerScope, PermanentDiedEventBranchNode};
use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn envelope(text: &str) -> (GenericTriggerEnvelopeAttempt, Option<String>) {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("generic-trigger-envelope-test", &tokens);
    let result = try_recognize_generic_trigger_envelope(&mut cursor, &[], None);
    (result, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn event_families_return_the_cursor_before_guard_or_permission() {
    for text in [
        "When this creature enters, if you control a Forest, draw a card.",
        "Whenever you cast a spell, if you control a Forest, draw a card.",
        "Whenever a player sacrifices a permanent, if you control a Forest, draw a card.",
        "At the beginning of your upkeep, if you control a Forest, draw a card.",
        "Whenever a Forest you control enters, if you control a Forest, draw a card.",
        "Whenever another creature dies, if you control a Forest, draw a card.",
        "Whenever this creature attacks, if you control a Forest, draw a card.",
        "Whenever this creature deals combat damage to a player, if you control a Forest, draw a card.",
    ] {
        let (attempt, next) = envelope(text);
        assert!(
            matches!(attempt, GenericTriggerEnvelopeAttempt::Recognized(_)),
            "{text}"
        );
        assert_eq!(next.as_deref(), Some("if"), "{text}");
    }
    for text in [
        "Whenever a creature enters, you may draw a card.",
        "At the beginning of your upkeep, you may draw a card.",
    ] {
        let (attempt, next) = envelope(text);
        assert!(
            matches!(attempt, GenericTriggerEnvelopeAttempt::Recognized(_)),
            "{text}"
        );
        assert_eq!(
            next.as_deref(),
            Some("you"),
            "permission belongs to the program: {text}"
        );
    }
}

#[test]
fn shared_subject_query_retains_controller_exclusion_and_event_kind() {
    let (attempt, _) = envelope("Whenever another creature you control dies, draw a card.");
    let GenericTriggerEnvelopeAttempt::Recognized(node) = attempt else {
        panic!("death envelope")
    };
    let GenericTriggerEventNode::PermanentTransition { kind, subjects, .. } = node.event else {
        panic!("typed transition")
    };
    assert_eq!(kind, PermanentTransitionKind::Died);
    let [
        PermanentDiedEventBranchNode::Filtered {
            controller,
            exclude_source,
            ..
        },
    ] = subjects.alternatives.as_slice()
    else {
        panic!("one filtered subject")
    };
    assert_eq!(*controller, Some(ObjectControllerScope::AbilityController));
    assert!(*exclude_source);

    let (attempt, _) = envelope("Whenever a creature enters under your control, draw a card.");
    let GenericTriggerEnvelopeAttempt::Recognized(node) = attempt else {
        panic!("entry envelope")
    };
    let GenericTriggerEventNode::PermanentTransition { kind, subjects, .. } = node.event else {
        panic!("typed transition")
    };
    assert_eq!(kind, PermanentTransitionKind::Entered);
    let [PermanentDiedEventBranchNode::Filtered { controller, .. }] =
        subjects.alternatives.as_slice()
    else {
        panic!("one filtered subject")
    };
    assert_eq!(*controller, Some(ObjectControllerScope::AbilityController));
}

#[test]
fn numeric_event_binding_is_retained_but_not_assumed_by_the_body() {
    let (attempt, _) =
        envelope("Whenever this creature deals combat damage to a player, draw two cards.");
    let GenericTriggerEnvelopeAttempt::Recognized(node) = attempt else {
        panic!("combat event")
    };
    assert_eq!(
        node.event.numeric_binding(),
        Some(EventNumericBinding::DamageAmount)
    );
    assert_eq!(node.event.source(), LocalObjectReference::SourceSelf);
}

#[test]
fn malformed_committed_envelopes_and_extra_relations_are_explicit() {
    for text in [
        "Whenever a creature enters draw a card.",
        "Whenever a red enters, draw a card.",
        "Whenever a creature you control enters under an opponent's control, draw a card.",
        "Whenever a creature enters under its owner's control, draw a card.",
    ] {
        assert!(
            matches!(
                envelope(text).0,
                GenericTriggerEnvelopeAttempt::FailedCommitted(_)
            ),
            "{text}"
        );
    }
}

#[test]
fn no_event_match_rolls_back_without_looking_inside_quoted_abilities() {
    for text in [
        "This creature has flying.",
        "Enchanted creature has \"Whenever a creature enters, draw a card.\"",
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("generic-trigger-no-match", &tokens);
        assert!(matches!(
            try_recognize_generic_trigger_envelope(&mut cursor, &[], None),
            GenericTriggerEnvelopeAttempt::NotClaimed
        ));
        assert_eq!(cursor.position(), 0);
    }
}
