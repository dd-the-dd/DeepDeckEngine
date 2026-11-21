use super::*;
use crate::oracle::canonical::automata::{
    EffectRecognitionContext, EffectSequenceNode, LocalConditionNode, ResourcePaymentNode,
    recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::recognize_entities;

fn sequence_in(
    text: &str,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("unless-clause-test", &tokens);
    recognize_effect_sequence_with_context(&mut cursor, &[], None, context)
}

fn sequence(text: &str) -> MachineResult<EffectSequenceNode> {
    sequence_in(
        text,
        &EffectRecognitionContext::default().with_unless_frames(),
    )
}

fn frame(text: &str) -> UnlessFrameNode {
    let parsed = sequence(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
    let [frame] = parsed.unless.as_slice() else {
        panic!("{text}: one unless frame, got {:?}", parsed.unless);
    };
    frame.clone()
}

#[test]
fn each_payer_binds_its_reference_and_conjugates_the_cost() {
    let controller_of = |effect_index| {
        UnlessPayerNode::ControllerOf(TargetBinding {
            effect_index,
            slot_index: 0,
        })
    };
    for (text, payer) in [
        // `you`: imperative payment by the ability controller.
        (
            "sacrifice this creature unless you pay {1}.",
            UnlessPayerNode::Controller,
        ),
        (
            "sacrifice this creature unless you discard a card.",
            UnlessPayerNode::Controller,
        ),
        // `its controller`: the guarded action's single target, third person.
        (
            "counter target spell unless its controller pays {2}.",
            controller_of(0),
        ),
        (
            "tap target creature unless its controller pays 2 life.",
            controller_of(0),
        ),
        (
            "destroy target artifact unless its controller sacrifices a creature of their choice.",
            controller_of(0),
        ),
    ] {
        let UnlessClauseNode::Payment { payer: parsed, .. } = frame(text).clause else {
            panic!("{text}: a payment clause");
        };
        assert_eq!(parsed, payer, "{text}");
    }
    let UnlessClauseNode::Payment { cost, .. } =
        frame("return target creature to its owner's hand unless its controller pays {1}.").clause
    else {
        panic!("a payment clause");
    };
    assert!(matches!(
        cost,
        ResolutionCostNode::Payment(ResourcePaymentNode::Mana(_))
    ));
}

#[test]
fn the_frame_guards_the_actions_after_the_last_then_of_its_sentence() {
    // CR 118.12 guards the clause the exception follows, not an earlier step.
    for (text, first, end) in [
        ("sacrifice this creature unless you pay {1}.", 0, 1),
        (
            "draw a card, then sacrifice this creature unless you discard a card.",
            1,
            2,
        ),
        (
            "draw two cards. Then discard two cards unless you discard an artifact card.",
            1,
            2,
        ),
    ] {
        let frame = frame(text);
        assert_eq!(
            (frame.first_effect, frame.end_effect),
            (first, end),
            "{text}"
        );
    }
}

#[test]
fn a_you_control_condition_is_the_shared_boolean_atom() {
    let UnlessClauseNode::Condition(BooleanConditionNode::Atom(atom)) =
        frame("you lose 2 life unless you control an artifact.").clause
    else {
        panic!("a condition clause");
    };
    assert!(matches!(
        atom,
        LocalConditionNode::ControlledPermanentCount(_)
    ));
}

#[test]
fn a_parent_without_shared_lowering_keeps_rejecting_unless() {
    let failure = sequence_in(
        "sacrifice this creature unless you pay {1}.",
        &EffectRecognitionContext::default(),
    )
    .unwrap_err();
    assert_eq!(failure.state, "sequence.connector-or-end");
}

#[test]
fn unsupported_payers_scopes_and_costs_fail_at_their_own_state() {
    for (text, state) in [
        (
            "sacrifice this enchantment unless any player pays {3}.",
            "sequence.unless.payer.any-player",
        ),
        // `its` needs a target declared by the guarded action.
        (
            "sacrifice this creature unless its controller pays {1}.",
            "reference.anaphora.resolve-target",
        ),
        // A state condition is not a payment.
        (
            "sacrifice this creature unless it escaped.",
            "sequence.unless.payer",
        ),
        (
            "you lose 4 life unless you attacked this turn.",
            "sequence.unless.payment-action",
        ),
        // `you control` names the ability controller, not this payer.
        (
            "destroy target artifact unless its controller sacrifices a creature you control.",
            "sequence.unless.payer-relative-objects",
        ),
        // No resolution cost program for several sacrificed permanents.
        (
            "sacrifice this creature unless you sacrifice two lands.",
            "cost.resolution.sacrifice.quantity",
        ),
        // The exception closes its sentence.
        (
            "sacrifice this creature unless you pay {1}, then draw a card.",
            "sequence.unless.sentence-end",
        ),
        (
            "sacrifice this creature unless you pay {G} for each creature you control.",
            "sequence.unless.sentence-end",
        ),
    ] {
        let failure = sequence(text).unwrap_err();
        assert_eq!(failure.state, state, "{text}: {failure:?}");
    }
}
