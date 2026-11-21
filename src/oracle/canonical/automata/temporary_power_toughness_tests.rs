use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{
    EffectDurationNode, GrantedKeyword, NumericExpression, PowerToughnessDeltaNode,
    recognize_temporary_power_toughness,
};

fn parse(text: &str) -> super::TemporaryPowerToughnessNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.temporary-power-toughness", &tokens);
    let fragment =
        recognize_temporary_power_toughness(&mut cursor).expect("temporary modifier parses");
    assert_eq!(fragment.end_position, cursor.position());
    assert_eq!(cursor.peek(), Some("."));
    fragment.node
}

#[test]
fn dismember_shape_composes_target_delta_and_duration() {
    let node = parse("Target creature gets -5/-5 until end of turn.");

    assert_eq!(node.targets.slots.len(), 1);
    assert_eq!(
        node.delta,
        PowerToughnessDeltaNode {
            power: NumericExpression::from(-5),
            toughness: NumericExpression::from(-5),
        }
    );
    assert!(node.keywords.is_empty());
    assert_eq!(node.duration, EffectDurationNode::UntilEndOfCurrentTurn);
}

#[test]
fn keyword_atoms_stay_independent_from_the_stat_delta() {
    let node =
        parse("Target creature gets +2/+2 and gains flying and vigilance until end of turn.");

    assert_eq!(node.delta.power, NumericExpression::from(2));
    assert_eq!(node.delta.toughness, NumericExpression::from(2));
    assert_eq!(
        node.keywords,
        vec![GrantedKeyword::Flying, GrantedKeyword::Vigilance]
    );
}

#[test]
fn another_target_retains_source_exclusion_as_target_metadata() {
    let node = parse("Another target creature gets -1/-1 until end of turn.");

    assert!(node.exclude_source_from_first_target);
}

#[test]
fn missing_duration_fails_in_the_duration_child() {
    let tokens = recognize_entities("Target creature gets -2/-2.");
    let mut cursor = OracleTokenCursor::new("test.missing-modifier-duration", &tokens);
    let failure = recognize_temporary_power_toughness(&mut cursor)
        .expect_err("temporary modifier requires an explicit duration");

    assert_eq!(failure.state, "duration.until-end-of-turn.boundary");
    assert_eq!(failure.actual.as_deref(), Some("."));
}

#[test]
fn following_action_is_left_for_the_parent_sequence() {
    let tokens = recognize_entities("Target creature gets +3/+3 until end of turn. Draw a card.");
    let mut cursor = OracleTokenCursor::new("test.trailing-modifier-text", &tokens);
    let fragment = recognize_temporary_power_toughness(&mut cursor)
        .expect("the atomic modifier stops before its sentence terminator");

    assert_eq!(fragment.end_position, cursor.position());
    assert_eq!(cursor.peek(), Some("."));
    assert_eq!(cursor.peek_ahead(1), Some("Draw"));
}
