use super::*;

fn cost(symbols: &[&str]) -> ManaCostNode {
    ManaCostNode {
        symbols: symbols.iter().map(|symbol| (*symbol).to_owned()).collect(),
    }
}

#[test]
fn executable_fixed_symbols_preserve_bounded_complete_mana_value() {
    for (symbols, value) in [
        (vec!["{0}"], 0),
        (vec!["{2147483647}"], i32::MAX),
        (vec!["{2}", "{R}"], 3),
        (vec!["{S}", "{W/U}", "{B/P}", "{C}"], 4),
        (vec!["{1}", "{2}", "{w/u}", "{g/p}"], 5),
    ] {
        let node = cost(&symbols);
        assert_eq!(node.fixed_runtime_mana_value(), Ok(value), "{symbols:?}");
        assert_eq!(node.as_oracle_cost(), symbols.concat());
    }
}

#[test]
fn variable_unknown_and_overflow_costs_have_distinct_typed_failures() {
    use FixedRuntimeManaCostError::*;
    for (symbols, error) in [
        (vec!["{X}"], Variable),
        (vec!["{Y}"], UnsupportedSymbol),
        (vec!["{NOPE}"], UnsupportedSymbol),
        (vec!["{2/W}"], UnsupportedSymbol),
        (vec!["{C/W}"], UnsupportedSymbol),
        (vec!["{-1}"], UnsupportedSymbol),
        (vec!["{+1}"], UnsupportedSymbol),
        (vec!["{2147483648}"], AmountOutOfRange),
        (vec!["{2147483647}", "{1}"], AmountOutOfRange),
        (vec!["{2147483647}", "{R}"], AmountOutOfRange),
    ] {
        assert_eq!(
            cost(&symbols).fixed_runtime_mana_value(),
            Err(error),
            "{symbols:?}"
        );
    }
}
