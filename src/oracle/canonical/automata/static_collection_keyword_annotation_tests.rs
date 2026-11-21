use super::*;
use crate::oracle::syntax::recognize_entities;

fn recognition(text: &str) -> StaticCollectionKeywordRecognition {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("keyword-annotation-test", &tokens);
    try_recognize_static_collection_keyword_grant(&mut cursor)
}

#[test]
fn trample_annotation_attaches_to_the_bound_creature_collection() {
    assert!(matches!(
        recognition(
            "Creatures you control have trample. (Each of those creatures can deal excess combat damage to the player or planeswalker it's attacking.)"
        ),
        StaticCollectionKeywordRecognition::Parsed(_)
    ));
}

#[test]
fn another_keyword_and_intersected_collection_reuse_the_subject_child() {
    assert!(matches!(
        recognition(
            "Artifact creatures you control have reach. (Each of those creatures can block creatures with flying.)"
        ),
        StaticCollectionKeywordRecognition::Parsed(_)
    ));
    assert!(matches!(
        recognition(
            "Artifact creatures you control have trample and reach. (Each of those creatures can block creatures with flying.)"
        ),
        StaticCollectionKeywordRecognition::Parsed(_)
    ));
}

#[test]
fn inline_self_subject_reminders_are_preserved() {
    assert!(matches!(
        recognition(
            "Creatures you control have reach (This creature can block creatures with flying.)."
        ),
        StaticCollectionKeywordRecognition::Parsed(_)
    ));
}

#[test]
fn changed_predicate_or_wrong_keyword_never_becomes_arbitrary_annotation() {
    for text in [
        "Creatures you control have trample. (Each of those creatures can deal all combat damage to the player or planeswalker it's attacking.)",
        "Creatures you control have trample. (Each of those creatures can block creatures with flying.)",
        "Creatures you control have trample. (Draw three cards.)",
        "Creatures you control have trample. (Each of those creatures can deal excess combat damage to the player or planeswalker it's attacking.",
    ] {
        assert!(
            matches!(
                recognition(text),
                StaticCollectionKeywordRecognition::Failed(_)
            ),
            "{text}"
        );
    }
}

#[test]
fn annotation_subject_must_be_plural_and_implied_by_every_collection_branch() {
    for text in [
        "Artifacts you control have trample. (Each of those creatures can deal excess combat damage to the player or planeswalker it's attacking.)",
        "Artifacts or creatures you control have trample. (Each of those creatures can deal excess combat damage to the player or planeswalker it's attacking.)",
        "Creatures you control have trample. (Each of those creature can deal excess combat damage to the player or planeswalker it's attacking.)",
    ] {
        assert!(
            matches!(
                recognition(text),
                StaticCollectionKeywordRecognition::Failed(_)
            ),
            "{text}"
        );
    }
}

#[test]
fn conditional_and_later_effect_envelopes_are_not_unconditional_grants() {
    for text in [
        "Creatures you control have trample until end of turn.",
        "Creatures you control have trample as long as you control an artifact.",
        "Creatures you control have trample. You gain 3 life.",
        "Creatures you control have trample. (Each of those creatures can deal excess combat damage to the player or planeswalker it's attacking.) You gain 3 life.",
    ] {
        assert!(
            matches!(
                recognition(text),
                StaticCollectionKeywordRecognition::UnsupportedEnvelope(_)
            ),
            "{text}"
        );
    }
}
