use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{
    AddedPhaseAnchor, AdditionalPhasesNode, additional_phases_start, recognize_additional_phases,
};

/// The anchor and the following main phase are independent attributes, in
/// either written order (preposed or postposed anchor).
#[test]
fn anchor_and_following_main_phase_are_independent_axes() {
    for (text, anchor, followed_by_main_phase) in [
        (
            "After this main phase, there is an additional combat phase followed by an additional main phase.",
            AddedPhaseAnchor::CurrentMainPhase,
            true,
        ),
        (
            "After this phase, there is an additional combat phase.",
            AddedPhaseAnchor::CurrentPhase,
            false,
        ),
        (
            "After this combat phase, there is an additional combat phase.",
            AddedPhaseAnchor::CurrentCombatPhase,
            false,
        ),
        (
            "there is an additional combat phase after this phase.",
            AddedPhaseAnchor::CurrentPhase,
            false,
        ),
        (
            "There is an additional combat phase after this one.",
            AddedPhaseAnchor::CurrentPhase,
            false,
        ),
        (
            "there is an additional combat phase after this phase, followed by an additional main phase.",
            AddedPhaseAnchor::CurrentPhase,
            true,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("additional-phase-test", &tokens);
        assert!(additional_phases_start(&cursor), "{text}");
        assert_eq!(
            recognize_additional_phases(&mut cursor).expect(text),
            AdditionalPhasesNode {
                anchor,
                followed_by_main_phase,
            },
            "{text}"
        );
        assert_eq!(cursor.peek(), Some("."), "{text}");
    }
}

/// Other phrases starting with `after this` or `there is` are not claimed.
#[test]
fn unrelated_after_and_existential_phrases_are_not_claimed() {
    for text in [
        "After this creature attacks, draw a card.",
        "There is no maximum hand size.",
        "After the second main phase this turn, there's an additional combat phase.",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("additional-phase-near-miss", &tokens);
        assert!(!additional_phases_start(&cursor), "{text}");
    }
}
