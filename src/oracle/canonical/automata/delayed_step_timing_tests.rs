use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::super::DelayedTurnStep;
use super::{
    DelayedStepTimingNode, DelayedStepTurnNode, delayed_step_timing_starts,
    recognize_delayed_step_timing,
};

#[test]
fn step_and_turn_owner_are_independent_axes() {
    for (text, step, turn) in [
        (
            "at the beginning of the next end step",
            DelayedTurnStep::EndStep,
            DelayedStepTurnNode::AnyTurn,
        ),
        (
            "at the beginning of the next upkeep",
            DelayedTurnStep::Upkeep,
            DelayedStepTurnNode::AnyTurn,
        ),
        (
            "at the beginning of your next upkeep",
            DelayedTurnStep::Upkeep,
            DelayedStepTurnNode::ControllerTurn,
        ),
        (
            "at the beginning of your next end step",
            DelayedTurnStep::EndStep,
            DelayedStepTurnNode::ControllerTurn,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("delayed-timing-test", &tokens);
        assert!(delayed_step_timing_starts(&cursor), "{text}");
        assert_eq!(
            recognize_delayed_step_timing(&mut cursor).expect(text),
            DelayedStepTimingNode { step, turn }
        );
        cursor.finish().expect("the timing is consumed");
    }
}

#[test]
fn other_steps_and_turn_references_fail_explicitly() {
    for (text, state) in [
        (
            "at the beginning of the next combat",
            "delayed-step.timing.step",
        ),
        (
            "at the beginning of each end step",
            "delayed-step.timing.turn",
        ),
        (
            "at the beginning of the end step",
            "delayed-step.timing.next",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("delayed-timing-test", &tokens);
        let failure = recognize_delayed_step_timing(&mut cursor).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
    // `at end of combat` is the end of combat step (CR 511).
    let tokens = recognize_entities("at end of combat");
    let mut cursor = OracleTokenCursor::new("delayed-timing-test", &tokens);
    assert!(delayed_step_timing_starts(&cursor));
    let timing = recognize_delayed_step_timing(&mut cursor).expect("end of combat");
    assert_eq!(timing.step, DelayedTurnStep::EndOfCombat);
    assert_eq!(timing.turn, DelayedStepTurnNode::AnyTurn);
}
