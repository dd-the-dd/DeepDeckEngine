use crate::oracle::syntax::recognize_entities;

use super::super::{
    ActivatedEffectSequenceParseAttempt, EffectRecognitionContext,
    try_recognize_activated_effect_sequence_with_annotation,
};
use super::super::{LocalEffectNode, NumericAtom, NumericExpression};
use super::{OracleTokenCursor, recognize_effect_sequence};

fn sequence(text: &str) -> crate::oracle::canonical::MachineResult<Vec<LocalEffectNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("surveil-reminder-sequence", &tokens);
    recognize_effect_sequence(&mut cursor, &[], None).map(|node| node.effects)
}

fn integer(value: i64) -> NumericExpression {
    NumericExpression::Atom(NumericAtom::Integer(value))
}

#[test]
fn singular_surveil_reminder_after_the_sentence_is_validated() {
    // Shattered Wings / Starfighter Pilot / Forum of Amity wording.
    let effects = sequence(
        "Surveil 1. (Look at the top card of your library. You may put it into your graveyard.)",
    )
    .expect("typed surveil reminder parses");
    let [LocalEffectNode::Surveil(surveil)] = effects.as_slice() else {
        panic!("expected one surveil");
    };
    assert_eq!(surveil.count, integer(1));
}

#[test]
fn surveil_reminder_follows_the_same_count_for_any_literal_and_after_other_actions() {
    for text in [
        "Surveil 2. (Look at the top two cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
        "Surveil 3. (Look at the top three cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
        "Surveil 2. (To surveil 2, look at the top two cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
        "Draw a card, then surveil 1. (To surveil 1, look at the top card of your library. You may put it into your graveyard.)",
    ] {
        sequence(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
    }
}

#[test]
fn activated_ability_validates_the_surveil_reminder_before_the_annotation_stop() {
    // An activated surveil with its reminder (Fields of Strife wording).
    let tokens = recognize_entities(
        "{1}, {T}: Surveil 1. (Look at the top card of your library. You may put it into your graveyard.)",
    );
    let mut cursor = OracleTokenCursor::new("surveil-reminder-activation", &tokens);
    let attempt = try_recognize_activated_effect_sequence_with_annotation(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    );
    let activation = match attempt {
        ActivatedEffectSequenceParseAttempt::Parsed(activation) => activation,
        ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure)
        | ActivatedEffectSequenceParseAttempt::FailedCommitted(failure) => {
            panic!("activated surveil with a typed reminder must parse: {failure:?}")
        }
    };
    assert!(activation.timing_annotation.is_none());
    assert!(matches!(
        activation.effects.effects.as_slice(),
        [LocalEffectNode::Surveil(_)]
    ));
}

#[test]
fn a_scry_worded_reminder_after_surveil_fails_in_the_surveil_destination_state() {
    let failure = sequence(
        "Surveil 1. (Look at the top card of your library. You may put that card on the bottom.)",
    )
    .expect_err("scry wording must not validate a surveil");
    assert_eq!(failure.state, "effect.surveil.reminder.destination");
}

#[test]
fn a_surveil_worded_reminder_after_scry_fails_in_the_scry_destination_state() {
    let failure = sequence(
        "Scry 1. (Look at the top card of your library. You may put it into your graveyard.)",
    )
    .expect_err("surveil wording must not validate a scry");
    assert_eq!(failure.state, "effect.scry.reminder.destination");
}

#[test]
fn a_reminder_naming_the_other_action_or_another_count_is_rejected() {
    let wrong_verb = sequence("Surveil 1. (To scry 1, look at the top card of your library.)")
        .expect_err("`To scry` cannot introduce a surveil reminder");
    assert_eq!(wrong_verb.state, "effect.surveil.reminder.open");

    let wrong_count = sequence(
        "Surveil 2. (Look at the top three cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
    )
    .expect_err("the reminder quantity must match the action");
    assert_eq!(
        wrong_count.expected,
        "the same literal count as the surveil effect"
    );
}

#[test]
fn older_surveil_wording_names_the_looked_at_card_as_that_card() {
    // The Empower Jace token prints the older `that card` anaphor.
    sequence(
        "Surveil 1. (Look at the top card of your library. You may put that card into your graveyard.)",
    )
    .expect("`that card` and `it` name the same looked-at card");
}
