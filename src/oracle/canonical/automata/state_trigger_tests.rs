//! State triggers (CR 603.8) through the one event clause reader: the shared
//! condition language after `when`, tried after every occurrence.
use super::super::{EventClauseAttempt, GenericTriggerEventNode, read_event_clause};
use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn clause(text: &str) -> (EventClauseAttempt, Option<String>) {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("state-trigger-test", &tokens);
    let attempt = read_event_clause(&mut cursor, &[], None, ",");
    (attempt, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn a_state_after_when_is_a_state_trigger() {
    for (text, next) in [
        // Serendib Djinn, Sea Serpent, Gorilla Pack, Tethered Griffin.
        (
            "When you control no lands, sacrifice this creature.",
            "sacrifice",
        ),
        (
            "When you control no Islands, sacrifice this creature.",
            "sacrifice",
        ),
        (
            "When you control no Forests, sacrifice this creature.",
            "sacrifice",
        ),
        (
            "When you control no enchantments, sacrifice this creature.",
            "sacrifice",
        ),
        // Endangered Armodon: presence instead of absence, same atom.
        (
            "When you control a creature with toughness 2 or less, sacrifice this creature.",
            "sacrifice",
        ),
        // Immortal Coil: a located quantity, no new code.
        (
            "When there are no cards in your graveyard, you lose the game.",
            "you",
        ),
    ] {
        let (attempt, after) = clause(text);
        assert!(
            matches!(
                attempt,
                EventClauseAttempt::Recognized(GenericTriggerEventNode::StateCondition(_))
            ),
            "{text}"
        );
        assert_eq!(after.as_deref(), Some(next), "{text}");
    }
}

#[test]
fn a_known_occurrence_is_never_reread_as_a_state() {
    let (attempt, _) = clause("When you gain life, draw a card.");
    assert!(matches!(
        attempt,
        EventClauseAttempt::Recognized(GenericTriggerEventNode::Local(_))
    ));
    // `whenever` introduces an occurrence, not a state (CR 603.8).
    let (attempt, _) = clause("Whenever you control no Islands, draw a card.");
    assert!(!matches!(attempt, EventClauseAttempt::Recognized(_)));
}

#[test]
fn a_state_with_a_modifier_or_a_history_fails_explicitly() {
    // An event modifier on a state.
    let (attempt, _) = clause(
        "When you control no Islands for the first time each turn, sacrifice this creature.",
    );
    let (EventClauseAttempt::Rejected(failure) | EventClauseAttempt::Committed(failure)) = attempt
    else {
        panic!("a modified state is refused")
    };
    assert_eq!(failure.state, "event.state-trigger.modifier");
    // An unsupported trailing qualifier stops at the trigger's boundary.
    let (attempt, _) = clause("When you control no Islands this turn, sacrifice this creature.");
    assert!(!matches!(attempt, EventClauseAttempt::Recognized(_)));
    // What happened earlier is a history, not a state the engine rechecks.
    let (attempt, _) = clause("When you gained life this turn, draw a card.");
    assert!(!matches!(attempt, EventClauseAttempt::Recognized(_)));
}
