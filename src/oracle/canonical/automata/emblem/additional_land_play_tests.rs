use crate::oracle::syntax::recognize_entities;

use super::super::super::super::OracleTokenCursor;
use super::super::super::{LocalPlayerReference, NumericAtom, NumericExpression};
use super::super::PlayerModifierNode;
use super::recognize_static_additional_land_play;

fn recognize(text: &str) -> super::super::super::super::MachineResult<PlayerModifierNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("additional-land-play-test", &tokens);
    recognize_static_additional_land_play(&mut cursor)
}

#[test]
fn controller_one_land_modifier_is_typed_and_consumed_completely() {
    assert_eq!(
        recognize("You may play an additional land on each of your turns.")
            .expect("canonical modifier parses"),
        PlayerModifierNode::AdditionalLandPlay {
            player: LocalPlayerReference::Controller,
            count: NumericExpression::Atom(NumericAtom::Integer(1)),
        }
    );
}

#[test]
fn numeric_sibling_reuses_the_shared_expression_child() {
    assert_eq!(
        recognize("You may play two additional lands on each of your turns.")
            .expect("plural numeric sibling parses"),
        PlayerModifierNode::AdditionalLandPlay {
            player: LocalPlayerReference::Controller,
            count: NumericExpression::Atom(NumericAtom::Integer(2)),
        }
    );
}

#[test]
fn temporary_duration_does_not_become_a_static_permission() {
    let failure = recognize("You may play an additional land this turn.")
        .expect_err("temporary permission requires a different parent");

    assert_eq!(failure.state, "modifier.additional-land-play.period.on");
    assert_eq!(failure.actual.as_deref(), Some("this"));
    assert_eq!(failure.expected, "on");
}

#[test]
fn singular_plural_disagreement_is_a_structured_failure() {
    let failure = recognize("You may play two additional land on each of your turns.")
        .expect_err("two requires the plural land noun");

    assert_eq!(failure.state, "modifier.additional-land-play.land");
    assert_eq!(failure.actual.as_deref(), Some("land"));
    assert_eq!(failure.expected, "lands");
}

#[test]
fn another_player_is_not_approximated_as_the_controller() {
    let failure = recognize("Each player may play an additional land on each of their turns.")
        .expect_err("shared-player permission needs a distinct player-set parent");

    assert_eq!(failure.state, "reference.anaphora.start");
    assert_eq!(failure.actual.as_deref(), Some("Each"));
}
