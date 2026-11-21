use crate::oracle::syntax::recognize_entities;

use super::{
    OracleTokenCursor, PowerToughnessDeltaNode, PowerToughnessNode,
    recognize_literal_power_toughness, recognize_signed_power_toughness_delta,
};

#[test]
fn literal_power_toughness_is_typed_once() {
    let tokens = recognize_entities("4/4");
    let mut cursor = OracleTokenCursor::new("test.power-toughness", &tokens);
    assert_eq!(
        recognize_literal_power_toughness(&mut cursor).expect("statistics parse"),
        PowerToughnessNode {
            power: 4,
            toughness: 4,
        }
    );
    cursor.finish().expect("statistics consume their token");
}

#[test]
fn negative_statistics_are_rejected() {
    let tokens = recognize_entities("-1/4");
    let mut cursor = OracleTokenCursor::new("test.invalid-power-toughness", &tokens);
    let failure = recognize_literal_power_toughness(&mut cursor)
        .expect_err("negative literal statistics are unsupported");
    assert!(failure.expected.contains("nonnegative"));
}

#[test]
fn signed_delta_accepts_independent_positive_and_negative_components() {
    for (text, expected) in [
        (
            "-5/-5",
            PowerToughnessDeltaNode {
                power: -5,
                toughness: -5,
            },
        ),
        (
            "+3/+2",
            PowerToughnessDeltaNode {
                power: 3,
                toughness: 2,
            },
        ),
        (
            "+2/-1",
            PowerToughnessDeltaNode {
                power: 2,
                toughness: -1,
            },
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.power-toughness-delta", &tokens);
        assert_eq!(
            recognize_signed_power_toughness_delta(&mut cursor).expect("signed delta parses"),
            expected,
            "{text}"
        );
        cursor.finish().expect("signed delta consumes its token");
    }
}

#[test]
fn unsigned_base_statistics_are_not_a_delta() {
    let tokens = recognize_entities("4/4");
    let mut cursor = OracleTokenCursor::new("test.invalid-power-toughness-delta", &tokens);
    let failure = recognize_signed_power_toughness_delta(&mut cursor)
        .expect_err("base statistics are not a modifier");
    assert!(failure.expected.contains("signed literal"));
}
