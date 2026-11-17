use super::super::{NumericAtom, NumericExpression};
use super::*;
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> PowerToughnessDeltaNode<NumericExpression> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("numeric-stat-unit", &tokens);
    let delta = recognize_signed_numeric_power_toughness_delta(&mut cursor).unwrap();
    cursor.finish().unwrap();
    delta
}

#[test]
fn symbolic_components_share_arithmetic_precedence_and_negation() {
    let x = NumericExpression::Atom(NumericAtom::VariableX);
    assert_eq!(
        parse("+X/+0"),
        PowerToughnessDeltaNode {
            power: x.clone(),
            toughness: NumericExpression::from(0)
        }
    );
    assert_eq!(
        parse("+0/-X").toughness,
        NumericExpression::subtract(NumericExpression::from(0), x.clone())
    );
    assert_eq!(
        parse("+(X plus two multiplied by three)/-0").power,
        NumericExpression::add(
            x,
            NumericExpression::multiply(NumericExpression::from(2), NumericExpression::from(3))
        )
    );
    // Literal pairs retain signed integer atoms, not 0-minus wrappers.
    assert_eq!(
        parse("-2/+1"),
        PowerToughnessDeltaNode {
            power: NumericExpression::from(-2),
            toughness: NumericExpression::from(1)
        }
    );
}

#[test]
fn pair_delimiter_and_child_failure_are_transactional_and_span_preserving() {
    let text = "+ X plus / +0";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("numeric-stat-unit", &tokens);
    cursor.consume("+").unwrap();
    let checkpoint = cursor.position();
    let failure = cursor
        .bounded(3, |child| recognize_numeric_expression(child, None))
        .unwrap_err();
    assert_eq!(cursor.position(), checkpoint);
    assert_eq!(failure.token_index, 3);
    assert_eq!(failure.span_start, tokens[2].span_end);
    assert_eq!(cursor.peek(), Some("X"));
    for text in ["X/+0", "+X/0", "+X//+0", "+(X plus)/+0", "+X/+", "+X/-Q"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("numeric-stat-unit", &tokens);
        assert!(
            recognize_signed_numeric_power_toughness_delta(&mut cursor).is_err(),
            "{text}"
        );
    }
}
