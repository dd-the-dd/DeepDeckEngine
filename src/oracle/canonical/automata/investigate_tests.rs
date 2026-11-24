use crate::oracle::syntax::recognize_entities;

use super::super::{InvestigateEffectNode, LocalPlayerReference};
use super::{OracleTokenCursor, recognize_investigate_effect};

#[test]
fn investigate_is_an_independent_controller_effect() {
    let tokens = recognize_entities("investigate");
    let mut cursor = OracleTokenCursor::new("investigate-test", &tokens);

    let effect = recognize_investigate_effect(&mut cursor).expect("investigate parses");

    assert_eq!(
        effect,
        InvestigateEffectNode {
            player: LocalPlayerReference::Controller,
            times: super::super::NumericExpression::from(1_i64),
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn a_different_action_is_not_silently_treated_as_investigate() {
    let tokens = recognize_entities("surveil");
    let mut cursor = OracleTokenCursor::new("investigate-test", &tokens);

    let failure = recognize_investigate_effect(&mut cursor)
        .expect_err("unsupported actions must retain a committed diagnostic");

    assert_eq!(failure.actual.as_deref(), Some("surveil"));
}
