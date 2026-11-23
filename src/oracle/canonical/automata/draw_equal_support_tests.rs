use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn unsupported_equal_count_families_reject_and_roll_back_the_shared_sequence() {
    // The count is the shared `equal to` quantity: each unimplemented family
    // reports its own numeric state, and the draw candidate stays tentative.
    for text in [
        "Draw cards equal to the least power among creatures you control.",
        "Draw cards equal to the number of cards in target player's hand.",
        "Draw cards equal to the number of cards named Rite of Flame in all graveyards.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("draw-equal-support-test", &tokens);
        let outcome = super::super::try_recognize_effect_sequence_with_context(
            &mut cursor,
            &[],
            None,
            &EffectRecognitionContext::default(),
        );
        let super::super::EffectSequenceParseAttempt::RejectedCandidate(failure) = outcome else {
            panic!("an unimplemented numeric family must remain tentative: {text}");
        };
        assert_eq!(cursor.position(), 0, "{text}: {}", failure.state);
    }
}

#[test]
fn supported_half_library_count_keeps_its_typed_value() {
    for direction in ["up", "down"] {
        let text = format!(
            "Draw cards equal to half the number of cards in your library, rounded {direction}"
        );
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("draw-equal-support-test", &tokens);
        let DrawCardsEffectParseAttempt::Parsed(node) =
            try_recognize_draw_cards_effect_with_context(
                &mut cursor,
                &EffectRecognitionContext::default(),
            )
        else {
            panic!("implemented half family must still parse");
        };
        assert!(matches!(node.count, NumericExpression::Divide { .. }));
        cursor
            .finish()
            .expect("complete supported numeric expression");
    }
}

#[test]
fn malformed_supported_half_family_keeps_its_original_committed_diagnostic() {
    for (text, state, found) in [
        // `half` is now the shared half of any operand (CR 107.1a): its
        // operand and stated rounding fail at their own states.
        (
            "Draw cards equal to half a library",
            "numeric.arithmetic.half.rounding",
            "library",
        ),
        (
            "Draw cards equal to half the number of cards in your hand, rounded up",
            "zone-list.zone",
            "rounded",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("draw-equal-support-test", &tokens);
        let DrawCardsEffectParseAttempt::FailedCommitted(failure) =
            try_recognize_draw_cards_effect_with_context(
                &mut cursor,
                &EffectRecognitionContext::default(),
            )
        else {
            panic!("malformed implemented family must stay committed: {text}");
        };
        assert_eq!(failure.state, state);
        assert_eq!(failure.actual.as_deref(), Some(found));
        assert_eq!(failure.automaton, "draw-equal-support-test");
    }
}
