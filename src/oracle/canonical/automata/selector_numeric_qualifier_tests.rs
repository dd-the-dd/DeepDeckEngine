use super::*;
use crate::oracle::canonical::{NumericAtom, NumericExpression, ObjectQueryNode};
use crate::oracle::syntax::recognize_entities;

fn recognize_complete_query(text: &str) -> MachineResult<ObjectQueryNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("selector-numeric-qualifier-test", &tokens);
    let query = recognize_object_query(&mut cursor)?;
    cursor.finish()?;
    Ok(query)
}

#[test]
fn power_and_toughness_reuse_the_numeric_comparison_qualifier() {
    for (text, property, operator, right) in [
        (
            "creature with power 2 or less",
            SelectorNumericProperty::Power,
            NumericComparisonOperator::LessThanOrEqual,
            NumericAtom::Integer(2),
        ),
        (
            "creature with toughness X or greater",
            SelectorNumericProperty::Toughness,
            NumericComparisonOperator::GreaterThanOrEqual,
            NumericAtom::VariableX,
        ),
    ] {
        let query = recognize_complete_query(text).expect("numeric property qualifier parses");
        assert!(matches!(
            query.predicate,
            SelectorPredicate::Intersection(ref operands)
                if operands.iter().any(|operand| operand == &SelectorPredicate::NumericComparison {
                    left: property,
                    operator,
                    right: Box::new(NumericExpression::Atom(right.clone())),
                })
        ));
    }
}

#[test]
fn keyword_qualifier_keeps_its_existing_route() {
    let query = recognize_complete_query("creature with flying")
        .expect("keyword qualifier remains independent");
    assert!(matches!(
        query.predicate,
        SelectorPredicate::Intersection(ref operands)
            if operands.iter().any(|operand| matches!(operand, SelectorPredicate::HasKeyword(_)))
    ));
}

#[test]
fn malformed_property_and_threshold_report_the_numeric_child_state() {
    let tokens = recognize_entities("with strength 2");
    let mut cursor = OracleTokenCursor::new("selector-numeric-property-near-miss", &tokens);
    let property_failure =
        recognize_numeric_comparison_qualifier(&mut cursor, ComparisonRhsAdmission::Literal)
            .expect_err("unknown numeric property must fail");
    assert_eq!(
        property_failure.state,
        "selector.numeric-comparison.property"
    );
    assert_eq!(property_failure.actual.as_deref(), Some("strength"));

    let tokens = recognize_entities("with power enormous");
    let mut cursor = OracleTokenCursor::new("selector-numeric-threshold-near-miss", &tokens);
    let threshold_failure =
        recognize_numeric_comparison_qualifier(&mut cursor, ComparisonRhsAdmission::Literal)
            .expect_err("unknown threshold must fail");
    assert_eq!(threshold_failure.state, "numeric.atom.start");
    assert_eq!(threshold_failure.actual.as_deref(), Some("enormous"));
}
