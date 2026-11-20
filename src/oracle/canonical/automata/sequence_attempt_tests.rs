use crate::oracle::syntax::recognize_entities;

use super::super::EffectRecognitionContext;
use super::{
    EffectSequenceParseAttempt, OracleTokenCursor, try_recognize_effect_sequence_with_context,
};

fn try_sequence<'a>(
    tokens: &'a [crate::oracle::model::RecognizedEntity],
) -> (EffectSequenceParseAttempt, OracleTokenCursor<'a>) {
    let mut cursor = OracleTokenCursor::new("effect-sequence-attempt-test", tokens);
    let outcome = try_recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    );
    (outcome, cursor)
}

#[test]
fn rejected_candidate_rolls_back_after_a_successful_first_effect() {
    let text = "draw a card. incubate 2.";
    let tokens = recognize_entities(text);
    let (outcome, cursor) = try_sequence(&tokens);

    assert!(matches!(
        outcome,
        EffectSequenceParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
}

#[test]
fn successful_draw_cut_does_not_leak_to_an_unsupported_following_child() {
    let text = "draw a card and incubate 2.";
    let tokens = recognize_entities(text);
    let (outcome, cursor) = try_sequence(&tokens);

    assert!(matches!(
        outcome,
        EffectSequenceParseAttempt::RejectedCandidate(_)
    ));
    assert_eq!(cursor.position(), 0);
}

#[test]
fn punctuation_and_supported_draw_qualifier_remain_tentative() {
    for text in ["draw two additional cards."] {
        let tokens = recognize_entities(text);
        let (outcome, cursor) = try_sequence(&tokens);

        assert!(
            matches!(outcome, EffectSequenceParseAttempt::RejectedCandidate(_)),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
}

#[test]
fn postposed_variable_definition_binds_the_sentence_actions() {
    // CR 107.3: the clause is part of the sentence, not an unsupported tail.
    let text = "draw X cards, where X is the number of creatures you control.";
    let tokens = recognize_entities(text);
    let (outcome, cursor) = try_sequence(&tokens);

    let EffectSequenceParseAttempt::Parsed(sequence) = outcome else {
        panic!("a where-X sentence must parse as one sequence");
    };
    assert_eq!(cursor.position(), tokens.len());
    assert_eq!(sequence.effects.len(), 1);
    assert_eq!(sequence.where_x.len(), 1);
    assert_eq!(
        (
            sequence.where_x[0].first_effect,
            sequence.where_x[0].end_effect
        ),
        (0, 1)
    );
}

#[test]
fn malformed_draw_is_a_committed_failure_without_diagnostic_rewriting() {
    let text = "draw target card.";
    let tokens = recognize_entities(text);
    let (outcome, _) = try_sequence(&tokens);

    let EffectSequenceParseAttempt::FailedCommitted(failure) = outcome else {
        panic!("malformed draw must remain a committed native failure");
    };
    assert_eq!(failure.automaton, "effect-sequence-attempt-test");
    assert_eq!(failure.state, "numeric.atom.start");
    assert_eq!(failure.actual.as_deref(), Some("target"));
    assert!(
        failure
            .trace
            .iter()
            .any(|transition| { transition.state == "effect.draw-cards.count" })
    );
}
