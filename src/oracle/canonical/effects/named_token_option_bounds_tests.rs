use super::*;

fn spec(quantity: i64, name: &str) -> NamedTokenSpec {
    NamedTokenSpec {
        quantity: NumericAtom::Integer(quantity),
        token_name: name.into(),
        tapped: false,
    }
}

#[test]
fn every_alternative_has_a_positive_i32_quantity_before_lowering() {
    let max = i64::from(i32::MAX);
    for (first, last, accepted) in [
        (max, 1, true),
        (1, max, true),
        (max + 1, 1, false),
        (1, max + 1, false),
        (0, 1, false),
        (1, -1, false),
    ] {
        let mut cursor = OracleTokenCursor::new("test.token-option-bounds", &[]);
        let pattern =
            NamedTokenCreationPattern::OneOf(vec![spec(first, "Food"), spec(last, "Treasure")]);
        let result = lower_named_token_creation(&mut cursor, &pattern, controller(), false);
        assert_eq!(result.is_ok(), accepted, "first={first} last={last}");
        if accepted {
            let rule = result.unwrap();
            assert_eq!(rule["options"][0]["effects"][0]["quantity"]["value"], first);
            assert_eq!(rule["options"][1]["effects"][0]["quantity"]["value"], last);
        } else {
            assert_eq!(
                result.unwrap_err().state,
                "lower.named-token-alternatives.quantity"
            );
        }
    }
}

#[test]
fn old_single_large_integer_ir_remains_historical_debt_without_truncation() {
    let n = i64::from(i32::MAX) + 1;
    let mut cursor = OracleTokenCursor::new("test.token-single-compat", &[]);
    let single = NamedTokenCreationPattern::Single(spec(n, "Food"));
    let rule = lower_named_token_creation(&mut cursor, &single, controller(), false).unwrap();
    assert_eq!(
        rule,
        json!({"kind":"createTokens","controller":controller(),
        "quantity":{"kind":"integer","value":n},"token":{"kind":"namedToken","name":"Food"}})
    );
}
