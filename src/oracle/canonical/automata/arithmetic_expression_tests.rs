use crate::oracle::syntax::recognize_entities;

use super::super::{
    LocalObjectReference, NumericAtom, NumericExpression, NumericObjectProperty, NumericRounding,
    TargetBinding,
};
use super::{OracleTokenCursor, recognize_arithmetic_expression};

fn parse(text: &str) -> super::MachineResult<NumericExpression> {
    parse_with_antecedent(text, None)
}

fn parse_with_antecedent(
    text: &str,
    antecedent: Option<TargetBinding>,
) -> super::MachineResult<NumericExpression> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("arithmetic-expression-test", &tokens);
    let expression = recognize_arithmetic_expression(&mut cursor, antecedent)?;
    cursor.finish()?;
    Ok(expression)
}

fn integer(value: i64) -> NumericExpression {
    NumericExpression::Atom(NumericAtom::Integer(value))
}

#[test]
fn multiplication_binds_more_tightly_than_addition() {
    assert_eq!(
        parse("one plus two times three").expect("word arithmetic parses"),
        NumericExpression::Add {
            left: Box::new(integer(1)),
            right: Box::new(NumericExpression::Multiply {
                left: Box::new(integer(2)),
                right: Box::new(integer(3)),
            }),
        }
    );
}

#[test]
fn additive_symbols_are_left_associative_when_lexically_distinct() {
    assert_eq!(
        parse("1 + 2 - 3").expect("symbol arithmetic parses"),
        NumericExpression::Subtract {
            left: Box::new(NumericExpression::Add {
                left: Box::new(integer(1)),
                right: Box::new(integer(2)),
            }),
            right: Box::new(integer(3)),
        }
    );
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(
        parse("( one plus two ) times three").expect("parenthesized arithmetic parses"),
        NumericExpression::Multiply {
            left: Box::new(NumericExpression::Add {
                left: Box::new(integer(1)),
                right: Box::new(integer(2)),
            }),
            right: Box::new(integer(3)),
        }
    );
}

#[test]
fn division_retains_an_explicit_rounding_policy() {
    assert_eq!(
        parse("twenty divided by three, rounded up").expect("rounded division parses"),
        NumericExpression::Divide {
            left: Box::new(integer(20)),
            right: Box::new(integer(3)),
            rounding: NumericRounding::Up,
        }
    );
    assert_eq!(
        parse("twenty divided by four").expect("exactly expressed division parses"),
        NumericExpression::Divide {
            left: Box::new(integer(20)),
            right: Box::new(integer(4)),
            rounding: NumericRounding::Down,
        }
    );
}

#[test]
fn literal_zero_divisor_is_a_structured_failure() {
    let failure = parse("ten divided by 0").expect_err("literal zero must be rejected");

    assert_eq!(failure.state, "numeric.arithmetic.division.nonzero");
    assert!(failure.expected.contains("positive literal divisor"));
}

#[test]
fn dangling_operator_fails_at_the_missing_right_operand() {
    let failure = parse("one plus").expect_err("a missing operand must fail");

    assert_eq!(failure.state, "numeric.arithmetic.sum.right");
    assert_eq!(
        failure.expected,
        "a right operand after the additive operator"
    );
    assert!(failure.actual.is_none());
}

#[test]
fn variable_x_is_an_ordinary_typed_atom() {
    assert_eq!(
        parse("X multiplied by two").expect("variable arithmetic parses"),
        NumericExpression::Multiply {
            left: Box::new(NumericExpression::Atom(NumericAtom::VariableX)),
            right: Box::new(integer(2)),
        }
    );
}

#[test]
fn power_toughness_counter_token_is_never_claimed_as_arithmetic() {
    let tokens = recognize_entities("+1/+1");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].token_type, "counterValue");
    let mut cursor = OracleTokenCursor::new("arithmetic-expression-test", &tokens);
    let failure = recognize_arithmetic_expression(&mut cursor, None)
        .expect_err("a power/toughness value is not a numeric atom");

    assert_eq!(failure.state, "numeric.atom.start");
    assert_eq!(failure.actual.as_deref(), Some("+1/+1"));
}

#[test]
fn multiplicative_symbols_remain_unclaimed_until_the_tokenizer_distinguishes_them() {
    for text in ["2 * 3", "8 / 2"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("arithmetic-expression-test", &tokens);
        let result = recognize_arithmetic_expression(&mut cursor, None).and_then(|expression| {
            cursor.finish()?;
            Ok(expression)
        });
        assert!(
            result.is_err(),
            "{text} must remain outside the native family"
        );
    }
}

#[test]
fn arithmetic_reuses_the_existing_bound_object_property_leaf() {
    let binding = TargetBinding {
        effect_index: 1,
        slot_index: 0,
    };

    assert_eq!(
        parse_with_antecedent("one plus equal to that card's power", Some(binding))
            .expect("the bound property remains an arithmetic leaf"),
        NumericExpression::Add {
            left: Box::new(integer(1)),
            right: Box::new(NumericExpression::QualifiedTargetProperty {
                object: binding,
                required_nominal: super::super::ObjectPredicate::ObjectKind(
                    super::super::ObjectKind::Card
                ),
                property: super::super::SelectorNumericProperty::Power
            }),
        }
    );
}

#[test]
fn arithmetic_reuses_the_shared_source_property_leaf() {
    assert_eq!(
        parse("one plus this creature's toughness")
            .expect("the source property remains an arithmetic leaf"),
        NumericExpression::Add {
            left: Box::new(integer(1)),
            right: Box::new(NumericExpression::ObjectProperty {
                object: LocalObjectReference::SourceSelf,
                property: NumericObjectProperty::Toughness,
            }),
        }
    );
}
