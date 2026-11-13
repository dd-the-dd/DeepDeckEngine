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
