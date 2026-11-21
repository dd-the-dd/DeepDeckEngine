use super::*;
use crate::oracle::canonical::automata::{
    EffectProgramNode, EffectRecognitionContext, recognize_effect_program_with_context,
};
use crate::oracle::syntax::recognize_entities;

fn program(text: &str) -> MachineResult<EffectProgramNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("optional-cost-test", &tokens);
    let program = recognize_effect_program_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )?;
    cursor.finish()?;
    Ok(program)
}

#[test]
fn each_cost_atom_composes_with_each_outcome_connector() {
    for (text, outcome) in [
        (
            "you may pay {1}. If you do, draw a card.",
            CostOutcomeContinuation::IfPaid,
        ),
        (
            "you may pay 2 life. If you don't, draw a card.",
            CostOutcomeContinuation::IfDeclined,
        ),
        (
            "you may discard a card. If you do, draw two cards.",
            CostOutcomeContinuation::IfPaid,
        ),
        (
            "you may sacrifice another creature or artifact. When you do, destroy target creature.",
            CostOutcomeContinuation::WhenPaid,
        ),
        (
            "you may sacrifice an artifact. When you do, you gain 3 life.",
            CostOutcomeContinuation::WhenPaid,
        ),
    ] {
        let EffectProgramNode::OptionalCost {
            outcome: parsed, ..
        } = program(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
        else {
            panic!("{text}: optional cost must be a program node");
        };
        assert_eq!(parsed, outcome, "{text}");
    }
}

#[test]
fn another_excludes_the_source_and_keeps_the_shared_query() {
    let EffectProgramNode::OptionalCost { cost, .. } =
        program("you may sacrifice another creature. If you do, draw a card.").unwrap()
    else {
        panic!("optional cost");
    };
    let ResolutionCostNode::Sacrifice {
        exclude_source: true,
        object,
    } = cost
    else {
        panic!("sacrifice cost excluding the source");
    };
    assert!(object.predicate.has_nominal_head());
}

#[test]
fn a_later_sentence_is_an_independent_step_outside_the_outcome() {
    // CR 608.2c: the outcome connector governs its own sentence only.
    let EffectProgramNode::Steps(steps) =
        program("you may pay {1}. If you do, draw a card. You gain 2 life.").unwrap()
    else {
        panic!("an optional-cost block followed by an independent step");
    };
    assert_eq!(steps.len(), 2);
    let EffectProgramNode::OptionalCost { body, .. } = &steps[0] else {
        panic!("the outcome block comes first");
    };
    assert_eq!(body.action_count(), 1);
    assert!(
        matches!(&steps[1], EffectProgramNode::Sequence(sequence) if sequence.effects.len() == 1)
    );
}

#[test]
fn outcome_scope_and_unsupported_costs_fail_explicitly() {
    for (text, state) in [
        // A target is not a cost selection.
        (
            "you may sacrifice target creature. If you do, draw a card.",
            "cost.payment.quantity",
        ),
        (
            "you may discard two cards. If you do, draw a card.",
            "cost.resolution.discard.quantity",
        ),
        // Old Man Willow: `another` scopes only the first branch.
        (
            "you may sacrifice another creature or a token. When you do, draw a card.",
            "cost.payment.branch-article",
        ),
    ] {
        assert_eq!(cost_failure(text).state, state, "{text}");
    }
}

/// The resolution-cost child's own failure after `you may`. A sentence the
/// cost child rejects may still be an ordinary optional instruction.
fn cost_failure(text: &str) -> crate::oracle::canonical::MachineFailure {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("optional-cost-test", &tokens);
    cursor.consume_phrase(&["you", "may"]).unwrap();
    recognize_resolution_cost(&mut cursor).unwrap_err()
}

#[test]
fn optional_cost_without_outcome_keeps_the_ordinary_optional_scope() {
    // No outcome connector: the cost reader is never consulted.
    let failure = program("you may pay {1}. Draw a card.").unwrap_err();
    assert_ne!(failure.state, "cost.resolution.verb");
}

#[test]
fn tapped_permanents_are_an_optional_cost_with_their_own_quantity_and_exclusion() {
    for (text, count, exclude_source) in [
        (
            "you may tap three untapped creatures you control. If you do, draw a card.",
            3,
            false,
        ),
        (
            "you may tap another untapped creature you control. If you do, draw a card.",
            1,
            true,
        ),
        (
            "you may tap two untapped creatures and/or artifacts you control. When you do, draw a card.",
            2,
            false,
        ),
    ] {
        let EffectProgramNode::OptionalCost {
            cost: ResolutionCostNode::TapObjects(tap),
            ..
        } = program(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
        else {
            panic!("{text}: tap-objects optional cost");
        };
        assert_eq!(
            (tap.count, tap.exclude_source),
            (count, exclude_source),
            "{text}"
        );
    }
    // Near miss: a target is not a cost selection (CR 601.2h); the sentence
    // is an ordinary optional instruction instead of an optional cost.
    let text = "you may tap target creature you control. If you do, draw a card.";
    assert_eq!(cost_failure(text).state, "cost.payment.quantity");
    assert!(matches!(
        program(text).unwrap(),
        EffectProgramNode::OptionalAction { .. }
    ));
}
