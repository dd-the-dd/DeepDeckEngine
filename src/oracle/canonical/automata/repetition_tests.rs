use super::super::super::OracleTokenCursor;
use super::super::NumericExpression;
use super::{recognize_repetition_adjunct, repetition_adjunct_starts};
use crate::oracle::syntax::recognize_entities;

fn read(
    text: &str,
) -> (
    Result<NumericExpression, super::super::super::MachineFailure>,
    usize,
    usize,
) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("repetition-unit", &tokens);
    let read = recognize_repetition_adjunct(&mut cursor);
    (read, cursor.position(), tokens.len())
}

#[test]
fn twice_thrice_and_counted_times_are_one_adjunct() {
    for (text, count) in [("twice", 2_i64), ("thrice", 3_i64), ("three times", 3_i64)] {
        let (read, position, length) = read(text);
        assert_eq!(read.expect(text), NumericExpression::from(count), "{text}");
        assert_eq!(position, length, "{text}");
    }
    // `X times`: the amount stops before `times`, never the multiplication.
    let (read, position, length) = read("X times");
    assert!(read.is_ok());
    assert_eq!(position, length);
}

#[test]
fn a_times_after_a_connector_or_boundary_is_no_adjunct_here() {
    for text in ["and draw three times", ". Twice", ", then two times"] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("repetition-unit", &tokens);
        assert!(!repetition_adjunct_starts(&cursor), "{text}");
    }
}
