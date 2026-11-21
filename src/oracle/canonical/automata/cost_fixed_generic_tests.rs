use super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn parsed_cost(text: &str) -> ManaCostNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("fixed-generic-cost-test", &tokens);
    recognize_mana_cost(&mut cursor).expect("test mana cost parses")
}

#[test]
fn one_generic_symbol_returns_a_bounded_integer() {
    assert_eq!(parsed_cost("{0}").fixed_generic_amount(), Some(0));
    assert_eq!(parsed_cost("{12}").fixed_generic_amount(), Some(12));
    assert_eq!(
        parsed_cost("{2147483647}").fixed_generic_amount(),
        Some(i32::MAX)
    );
}

#[test]
fn variable_colored_multi_symbol_and_overflow_are_not_fixed_generic_amounts() {
    for cost in ["{X}", "{R}", "{R/G}", "{1}{R}", "{2147483648}"] {
        assert_eq!(parsed_cost(cost).fixed_generic_amount(), None, "{cost}");
    }
}
