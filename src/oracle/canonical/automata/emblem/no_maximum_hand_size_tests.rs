use crate::oracle::syntax::recognize_entities;

use super::super::super::super::OracleTokenCursor;
use super::super::super::LocalPlayerReference;
use super::super::PlayerModifierNode;
use super::recognize_static_no_maximum_hand_size;

fn recognize(text: &str) -> super::super::super::super::MachineResult<PlayerModifierNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("no-maximum-hand-size-test", &tokens);
    recognize_static_no_maximum_hand_size(&mut cursor)
}

#[test]
fn controller_modifier_is_typed_and_consumed_completely() {
    assert_eq!(
        recognize("You have no maximum hand size.").expect("canonical modifier parses"),
        PlayerModifierNode::NoMaximumHandSize {
            player: LocalPlayerReference::Controller,
        }
    );
}

#[test]
fn case_variation_reuses_the_same_token_machine() {
    recognize("YOU HAVE NO MAXIMUM HAND SIZE.").expect("token matching is case insensitive");
}

#[test]
fn near_miss_reports_the_missing_maximum_concept() {
    let failure = recognize("You have no minimum hand size.")
        .expect_err("minimum hand size is a different rule");

    assert_eq!(failure.state, "modifier.no-maximum-hand-size.maximum");
    assert_eq!(failure.expected, "maximum");
    assert_eq!(failure.actual.as_deref(), Some("minimum"));
}

#[test]
fn another_player_reference_is_not_approximated_as_controller() {
    let failure = recognize("Your opponents have no maximum hand size.")
        .expect_err("opponents require a different typed player reference");

    assert_eq!(failure.state, "modifier.no-maximum-hand-size.player");
    assert_eq!(failure.expected, "you");
}
