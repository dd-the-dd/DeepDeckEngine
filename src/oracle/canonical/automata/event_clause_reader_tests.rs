//! The one event clause reader owns the trigger word and the boundary; each
//! child reads only the occurrence (S5).
use super::super::{EventTurnOwner, TargetingStackKind};
use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn clause(text: &str) -> (EventClauseAttempt, Option<String>) {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("event-clause-test", &tokens);
    let attempt = read_event_clause(&mut cursor, &[], None, ",");
    (attempt, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn every_subject_kind_returns_after_the_boundary() {
    for text in [
        "When this creature enters, draw a card.",
        "Whenever you cast a spell, draw a card.",
        "Whenever a player sacrifices a permanent, draw a card.",
        "Whenever you create or sacrifice a token, draw a card.",
        "At the beginning of your upkeep, draw a card.",
        "Whenever a Forest you control enters, draw a card.",
        "Whenever this creature attacks, draw a card.",
        "Whenever a creature you control becomes the target of a spell or ability an opponent controls, draw a card.",
        "Whenever this creature becomes the target of a spell or ability, draw a card.",
        "Whenever a creature you control attacks alone, draw a card.",
    ] {
        let (attempt, next) = clause(text);
        assert!(
            matches!(attempt, EventClauseAttempt::Recognized(_)),
            "{text}"
        );
        assert_eq!(next.as_deref(), Some("draw"), "{text}");
    }
}

#[test]
fn no_introducer_is_not_claimed_and_keeps_the_cursor() {
    let tokens = recognize_oracle_entities("Draw a card.");
    let mut cursor = OracleTokenCursor::new("event-clause-test", &tokens);
    assert!(matches!(
        read_event_clause(&mut cursor, &[], None, ","),
        EventClauseAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), 0);
}

#[test]
fn the_source_targeting_frame_is_a_typed_source_verb() {
    let (attempt, _) = clause(
        "Whenever this creature becomes the target of a spell an opponent controls, draw a card.",
    );
    let EventClauseAttempt::Recognized(GenericTriggerEventNode::SourceEvent(node)) = attempt else {
        panic!("source targeting event")
    };
    assert_eq!(node.verbs, [SourceEventVerb::BecameTarget]);
    assert!(node.modifiers.iter().any(|modifier| matches!(
        modifier,
        SourceEventModifier::TargetingStack(stack)
            if stack.kinds == [TargetingStackKind::Spell]
                && stack.controller == Some(ObjectControllerScope::OpponentOfAbilityController)
    )));
}

#[test]
fn shared_modifiers_are_typed_on_the_subject_kinds_that_absorb_them() {
    let (attempt, _) =
        clause("Whenever a creature you control attacks during your turn, draw a card.");
    let EventClauseAttempt::Recognized(GenericTriggerEventNode::Object(node)) = attempt else {
        panic!("object event")
    };
    assert_eq!(
        node.modifiers.turn_owner,
        Some(EventTurnOwner::ControllerTurn)
    );
    let (attempt, _) =
        clause("Whenever this creature blocks during an opponent's turn, draw a card.");
    let EventClauseAttempt::Recognized(GenericTriggerEventNode::SourceEvent(node)) = attempt else {
        panic!("source event")
    };
    assert!(node.modifiers.contains(&SourceEventModifier::DuringTurn(
        EventTurnOwner::OpponentTurn
    )));
}

#[test]
fn a_modifier_on_an_occurrence_without_its_matcher_is_an_engine_gap() {
    let (attempt, _) = clause("Whenever you gain life for the first time each turn, draw a card.");
    let EventClauseAttempt::Committed(failure) = attempt else {
        panic!("committed engine gap")
    };
    assert_eq!(failure.state, "engine.gap.event-matcher");
    assert!(failure.expected.contains("for the first time each turn"));
}

#[test]
fn an_unmatched_turn_owner_and_a_while_condition_are_the_event_time_condition() {
    let (attempt, next) = clause("Whenever you gain life during your turn, draw a card.");
    let EventClauseAttempt::Recognized(GenericTriggerEventNode::Conditioned { condition, .. }) =
        attempt
    else {
        panic!("conditioned occurrence")
    };
    assert_eq!(condition.turn_owner, Some(EventTurnOwner::ControllerTurn));
    assert!(condition.condition.is_none());
    assert_eq!(next.as_deref(), Some("draw"));
    let (attempt, next) =
        clause("Whenever this creature attacks while you control an artifact, draw a card.");
    let EventClauseAttempt::Recognized(GenericTriggerEventNode::Conditioned { event, condition }) =
        attempt
    else {
        panic!("conditioned occurrence")
    };
    assert!(matches!(*event, GenericTriggerEventNode::SourceEvent(_)));
    assert!(condition.condition.is_some() && condition.turn_owner.is_none());
    assert_eq!(next.as_deref(), Some("draw"));
}

#[test]
fn a_trigger_condition_list_reads_every_member_with_its_own_introducer() {
    for (text, members) in [
        (
            "When this creature enters and at the beginning of your upkeep, surveil 1.",
            2,
        ),
        (
            "Whenever an opponent casts a spell during your turn and when this creature dies, draw a card.",
            2,
        ),
        (
            "When this creature enters and whenever you cast a spell with mana value 4 or greater, draw a card.",
            2,
        ),
        // A previously unused combination: three members, a comma before the
        // list connector and an event-time condition on the first member.
        (
            "Whenever you attack with two or more creatures while you control an artifact, and whenever a creature you control is turned face up, and at the beginning of your end step, draw a card.",
            3,
        ),
    ] {
        let (attempt, next) = clause(text);
        let EventClauseAttempt::Recognized(GenericTriggerEventNode::Compound(list)) = attempt
        else {
            panic!("{text}: a trigger condition list")
        };
        assert_eq!(list.len(), members, "{text}");
        assert!(
            list.iter()
                .all(|member| !matches!(member, GenericTriggerEventNode::Compound(_))),
            "{text}"
        );
        assert_eq!(next.as_deref(), Some(","), "{text}");
    }
}

#[test]
fn and_without_a_trigger_word_is_not_a_condition_list() {
    // `enters and attacks` coordinates verbs, not trigger conditions; no
    // reader owns it, so the claimed clause fails typed at its boundary.
    let (attempt, _) = clause("When this creature enters and attacks, draw a card.");
    let (EventClauseAttempt::Committed(failure) | EventClauseAttempt::Rejected(failure)) = attempt
    else {
        panic!("never recognized")
    };
    assert!(!failure.is_engine_gap(), "{failure}");
    // `and at least` after a source is a coordinated attack, never a list.
    let (attempt, _) =
        clause("Whenever this creature and at least two other creatures attack, draw a card.");
    assert!(matches!(
        attempt,
        EventClauseAttempt::Recognized(GenericTriggerEventNode::Local(
            LocalTriggerEventNode::AttackDeclaration(_)
        ))
    ));
}

#[test]
fn an_embedded_clause_reads_its_event_with_the_same_reader() {
    // The boundary is the caller's: an embedded trigger reuses the reader.
    let (attempt, next) = clause("When this creature dies, return it to its owner's hand.");
    assert!(matches!(attempt, EventClauseAttempt::Recognized(_)));
    assert_eq!(next.as_deref(), Some("return"));
}

#[test]
fn an_entering_controller_scope_does_not_leak_into_a_transition_union() {
    // `under your control` is the entering branch's post-event controller;
    // the union would apply it to `or dies` as a pre-event relation.
    let (attempt, _) =
        clause("Whenever a creature enters under your control or dies, draw a card.");
    let (EventClauseAttempt::Committed(failure) | EventClauseAttempt::Rejected(failure)) = attempt
    else {
        panic!("an entering controller with a union is never recognized")
    };
    assert_eq!(failure.state, "trigger.envelope.transition-union");
    // Near miss: a pre-event controller on the subject binds both branches.
    let (attempt, next) = clause("Whenever a creature you control enters or dies, draw a card.");
    assert!(matches!(attempt, EventClauseAttempt::Recognized(_)));
    assert_eq!(next.as_deref(), Some("draw"));
}
