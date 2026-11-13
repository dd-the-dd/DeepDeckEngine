use super::recognize_counter_name;
use crate::oracle::canonical::OracleTokenCursor;

fn resolved_tokens(text: &str) -> Vec<crate::oracle::model::RecognizedEntity> {
    let mut tokens = crate::oracle::syntax::recognize_entities(text);
    crate::oracle::syntax::resolve_counter_name_spans(&mut tokens);
    tokens
}

#[test]
fn consumes_one_or_more_parts_and_stops_before_the_noun() {
    for (text, expected, position) in [
        ("+1/+1 counter", "+1/+1", 1),
        ("flying counter", "flying", 1),
        (
            "flux capacitor calibration counters",
            "flux capacitor calibration",
            3,
        ),
    ] {
        let tokens = resolved_tokens(text);
        let mut cursor = OracleTokenCursor::new("counter-name-test", &tokens);
        let name = recognize_counter_name(&mut cursor).expect("counter name parses");
        assert_eq!(name.as_str(), expected);
        assert_eq!(cursor.position(), position);
    }
}

#[test]
fn rejects_a_dangling_or_unresolved_name() {
    let tokens = crate::oracle::syntax::recognize_entities("mystery value");
    let mut cursor = OracleTokenCursor::new("counter-name-test", &tokens);
    let failure = recognize_counter_name(&mut cursor).expect_err("unbounded UNKNOWN must fail");
    assert_eq!(failure.state, "counter-name.first-part");
}
