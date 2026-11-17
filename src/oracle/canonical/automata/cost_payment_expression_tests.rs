use super::*;

fn cost(symbols: &[&str]) -> ManaCostNode {
    ManaCostNode {
        symbols: symbols.iter().map(|symbol| (*symbol).to_string()).collect(),
    }
}

#[test]
fn preserved_generic_symbols_project_to_one_bound_numeric_expression() {
    assert_eq!(
        cost(&["{X}"]).generic_payment_expression(),
        Some(NumericExpression::Atom(NumericAtom::VariableX))
    );
    assert_eq!(
        cost(&["{0}"]).generic_payment_expression(),
        Some(NumericExpression::Atom(NumericAtom::Integer(0)))
    );
    assert_eq!(
        cost(&["{1}", "{X}", "{2}", "{X}"]).generic_payment_expression(),
        Some(NumericExpression::add(
            NumericExpression::Atom(NumericAtom::Integer(3)),
            NumericExpression::multiply(
                NumericExpression::Atom(NumericAtom::Integer(2)),
                NumericExpression::Atom(NumericAtom::VariableX)
            ),
        ))
    );
}

#[test]
fn generic_projection_does_not_erase_colored_unknown_or_overflow_symbols() {
    for symbols in [
        vec!["{X}", "{U}"],
        vec!["{NOPE}"],
        vec!["{2147483647}", "{1}"],
        vec!["{2/W}"],
        vec!["{+1}"],
    ] {
        assert_eq!(
            cost(&symbols).generic_payment_expression(),
            None,
            "{symbols:?}"
        );
    }
}
