use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn independent_card_and_spell_queries_share_the_same_zone_union() {
    for text in [
        "Creature cards in graveyards and libraries can't enter the battlefield.",
        "Players can't cast spells from graveyards or libraries.",
        "Each player cannot cast artifact or creature spells from graveyards, libraries, or exile.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-prohibition", &tokens);
        assert!(
            matches!(
                try_recognize_zone_prohibition(&mut cursor),
                ZoneProhibitionAttempt::Parsed(_)
            ),
            "{text}"
        );
        assert!(cursor.peek().is_none());
    }
}
#[test]
fn existing_prohibitions_quotas_and_broader_sentences_are_not_intercepted() {
    for text in [
        "Your opponents can't cast spells during your turn.",
        "Each player can't cast more than one spell each turn.",
        "Creature cards in graveyards and libraries can't enter the battlefield. You gain 1 life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-envelope", &tokens);
        assert!(
            matches!(
                try_recognize_zone_prohibition(&mut cursor),
                ZoneProhibitionAttempt::NotClaimed | ZoneProhibitionAttempt::UnsupportedEnvelope(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0);
    }
}
