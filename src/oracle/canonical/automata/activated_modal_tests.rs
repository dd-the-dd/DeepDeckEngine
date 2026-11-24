use crate::oracle::syntax::recognize_entities;

use super::super::{ActivationTiming, LocalActivationCostNode, LocalEffectNode};
use super::{
    ActivatedModalParseAttempt, EffectRecognitionContext, OracleTokenCursor,
    try_recognize_activated_modal_with_cost_context,
};

fn attempt(text: &str) -> (ActivatedModalParseAttempt, usize, usize) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.activated-modal", &tokens);
    let result = try_recognize_activated_modal_with_cost_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
        |_, base| base.clone(),
    );
    (result, cursor.position(), tokens.len())
}

/// Positive card shape (Cankerbloom): costs, a dash header and three modes,
/// each an ordinary shared effect program.
#[test]
fn activated_costs_own_a_modal_container_with_shared_mode_programs() {
    let (result, position, length) = attempt(
        "{1}, Sacrifice this creature: Choose one \u{2014} \u{2022} Destroy target artifact. \u{2022} Destroy target enchantment. \u{2022} Draw a card.",
    );
    let ActivatedModalParseAttempt::Parsed(node) = result else {
        panic!("a modal activated body is claimed after its costs");
    };
    assert_eq!(position, length);
    assert!(matches!(
        node.costs.costs.as_slice(),
        [
            LocalActivationCostNode::PayMana(_),
            LocalActivationCostNode::SacrificeSelf(_)
        ]
    ));
    assert_eq!(node.modal.modes.len(), 3);
    assert_eq!(node.modal.cardinality.minimum, 1);
    assert_eq!(node.modal.cardinality.maximum, 1);
    assert!(node.restrictions.is_empty());
    assert!(matches!(
        node.modal.modes[2].inner_sequence().effects.as_slice(),
        [LocalEffectNode::DrawCards(_)]
    ));
}

/// Sibling: the header's own sentence is the ability's restriction
/// (`Choose one. Activate only as a sorcery.`), never a mode.
#[test]
fn a_restriction_between_the_header_and_the_modes_belongs_to_the_ability() {
    let (result, position, length) = attempt(
        "{4}, {T}: Choose one. Activate only as a sorcery. \u{2022} Draw a card. \u{2022} You gain 3 life.",
    );
    let ActivatedModalParseAttempt::Parsed(node) = result else {
        panic!("the header sentence is the activation restriction");
    };
    assert_eq!(position, length);
    assert_eq!(node.restrictions.timing, Some(ActivationTiming::Sorcery));
    assert_eq!(node.modal.modes.len(), 2);
    assert!(!node.modal.cardinality.allow_repeated);
}

/// Near-misses: a nonmodal body is not claimed (the cursor is restored),
/// and a mode-history header stays an explicit failure of its own state.
#[test]
fn nonmodal_bodies_are_released_and_history_headers_fail_explicitly() {
    let (result, position, _) = attempt("{T}: Draw a card.");
    assert!(matches!(result, ActivatedModalParseAttempt::NotClaimed));
    assert_eq!(position, 0);

    let (result, _, _) = attempt(
        "{1}: Choose one that hasn't been chosen this turn \u{2014} \u{2022} Draw a card. \u{2022} You gain 3 life.",
    );
    let ActivatedModalParseAttempt::FailedCommitted(failure) = result else {
        panic!("a mode-history header is committed and fails explicitly");
    };
    assert_eq!(failure.state, "modal.header.qualifier");

    // `Choose one —` without bullets is no modal container at all.
    let (result, position, _) = attempt("{2}: Choose one \u{2014} draw a card.");
    assert!(matches!(result, ActivatedModalParseAttempt::NotClaimed));
    assert_eq!(position, 0);
}
