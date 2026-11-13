use crate::oracle::syntax::recognize_entities;

use super::super::{PlayerSetReferenceNode, TurnContextNode, recognize_cast_prohibition};
use crate::oracle::canonical::OracleTokenCursor;

fn parse(text: &str) -> super::super::CastProhibitionNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("cast-prohibition-test", &tokens);
    recognize_cast_prohibition(&mut cursor).expect("cast prohibition parses")
}

#[test]
fn opponent_set_and_controller_turn_are_typed_children() {
    let node = parse("Your opponents can't cast spells during your turn.");
    assert_eq!(
        node.players,
        PlayerSetReferenceNode::OpponentsOfAbilityController
    );
    assert_eq!(node.turn, TurnContextNode::AbilityControllerTurn);

    assert_eq!(
        parse("Your opponents cannot cast spells during your turn."),
        node
    );
}

#[test]
fn unsupported_turn_context_reports_its_local_state() {
    let tokens = recognize_entities("Your opponents can't cast spells during each turn.");
    let mut cursor = OracleTokenCursor::new("cast-prohibition-test", &tokens);
    let failure = recognize_cast_prohibition(&mut cursor).expect_err("near miss must fail");
    assert_eq!(failure.state, "turn-context.possessor");
    assert_eq!(failure.actual.as_deref(), Some("each"));
}
