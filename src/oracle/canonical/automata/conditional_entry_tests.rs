use crate::oracle::{canonical::OracleTokenCursor, syntax::recognize_entities};

use super::recognize_controlled_permanent_conditional_entry;

#[test]
fn selector_is_typed_and_not_reparsed() {
    let tokens = recognize_entities("This land enters tapped unless you control a basic land.");
    let mut cursor = OracleTokenCursor::new("conditional-entry-test", &tokens);
    recognize_controlled_permanent_conditional_entry(&mut cursor, &[], None)
        .expect("conditional entry parses");
}

#[test]
fn missing_selector_reports_local_state() {
    let tokens = recognize_entities("This land enters tapped unless you control a.");
    let mut cursor = OracleTokenCursor::new("conditional-entry-test", &tokens);
    let failure = recognize_controlled_permanent_conditional_entry(&mut cursor, &[], None)
        .expect_err("missing selector fails");
    assert_eq!(failure.state, "selector.predicate.start");
}

#[test]
fn implicit_battlefield_domain_accepts_subtypes_without_weakening_incompatible_domains() {
    for text in [
        "This land enters tapped unless you control an Island or a Swamp.",
        "This land enters tapped unless you control a Plains or an Island or a Swamp.",
        "This land enters tapped unless you control an artifact or a creature.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("conditional-entry-test", &tokens);
        recognize_controlled_permanent_conditional_entry(&mut cursor, &[], None).expect(text);
    }
    for text in [
        "This land enters tapped unless you control a card.",
        "This land enters tapped unless you control a spell.",
        "This land enters tapped unless you control a player.",
        "This land enters tapped unless you control an Island or a spell.",
        "This land enters tapped unless you control an Island or a creature card.",
        "This land enters tapped unless you control an Island or a Swamp you control.",
        "This land enters tapped unless you control an Island or a Swamp an opponent controls.",
        "This land enters tapped unless you control an Island or a Swamp. Draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("conditional-entry-test", &tokens);
        assert!(
            recognize_controlled_permanent_conditional_entry(&mut cursor, &[], None).is_err(),
            "{text}"
        );
    }
}
