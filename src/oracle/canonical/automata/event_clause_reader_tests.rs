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
    let (attempt, _) = clause("Whenever you gain life during your turn, draw a card.");
    let EventClauseAttempt::Committed(failure) = attempt else {
        panic!("committed engine gap")
    };
    assert_eq!(failure.state, "engine.gap.event-matcher");
    assert!(failure.expected.contains("during your turn"));
}

#[test]
fn an_embedded_clause_reads_its_event_with_the_same_reader() {
    // The boundary is the caller's: an embedded trigger reuses the reader.
    let (attempt, next) = clause("When this creature dies, return it to its owner's hand.");
    assert!(matches!(attempt, EventClauseAttempt::Recognized(_)));
    assert_eq!(next.as_deref(), Some("return"));
}
