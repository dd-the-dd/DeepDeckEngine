//! Lexical location of replacing (`instead`) sentences and of the active
//! action-result clause. Card-level proofs (Oracle -> IR) live in the parser
//! fixture `program-control-outcomes.json`.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn locate(text: &str) -> Option<ReplacingSentence> {
    let tokens = recognize_entities(text);
    let cursor = OracleTokenCursor::new("replacing-conditional-test", &tokens);
    replacing_sentence(&cursor)
}

#[test]
fn postposed_instead_replaces_the_previous_sentence() {
    let sentence = locate("Draw two cards. If you control a Wizard, draw three cards instead.")
        .expect("a replacing sentence");
    assert_eq!(sentence.replaced_start, 0);
    assert!(sentence.start > sentence.replaced_start);
    assert!(matches!(sentence.instead, InsteadPosition::Postposed(_)));
}

#[test]
fn medial_instead_is_located_after_the_condition_separator() {
    let sentence = locate("You gain 2 life. If this spell was kicked, instead you gain 5 life.")
        .expect("a replacing sentence");
    assert_eq!(sentence.instead, InsteadPosition::Medial);
}

#[test]
fn the_sentence_after_a_reminder_is_still_located() {
    let sentence = locate(
        "Amass Goblins 1. (Put a +1/+1 counter on an Army you control.) If this spell was cast from a graveyard, amass Goblins 3 instead.",
    )
    .expect("a replacing sentence after a reminder");
    assert_eq!(sentence.replaced_start, 0);
}

#[test]
fn other_instead_constructs_are_not_replacing_sentences() {
    // A created replacement effect (CR 614.1a) is another construct.
    assert!(locate("Draw a card. If a creature would die this turn, exile it instead.").is_none());
    // `instead of` amends a destination.
    assert!(
        locate("Draw a card. If this spell was kicked, put it onto the battlefield instead of into your hand.")
            .is_none()
    );
    // Without an earlier sentence nothing is replaced.
    assert!(locate("If you control a Wizard, draw three cards instead.").is_none());
}

#[test]
fn active_action_result_clause_needs_this_way() {
    let starts = |text: &str| {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("replacing-conditional-test", &tokens);
        super::super::active_action_result_condition_starts(&cursor)
    };
    assert!(starts("you discard a creature card this way"));
    assert!(starts("you exiled a land card this way"));
    assert!(!starts("you discard a creature card"));
    assert!(!starts("you discarded two or more cards this turn"));
}
